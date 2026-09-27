//! Heap activity as observed by the binary's counting global allocator.
//!
//! The counters are per thread, so a measurement covers exactly the work done on the measuring thread.

use std::ops::Sub;

/// Cumulative heap operations and bytes on the current thread.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HeapActivity {
    /// Calls to `alloc` and `alloc_zeroed`.
    pub allocations: u64,
    /// Calls to `realloc`.
    pub reallocations: u64,
    /// Calls to `dealloc`.
    pub deallocations: u64,
    /// Bytes requested by allocations and by the new size of reallocations.
    pub bytes_allocated: u64,
    /// Bytes released by deallocations and by the old size of reallocations.
    pub bytes_deallocated: u64,
}

impl HeapActivity {
    /// Whether no heap operation happened.
    #[must_use]
    pub fn is_none(&self) -> bool {
        self.allocations == 0 && self.reallocations == 0 && self.deallocations == 0
    }

    /// Bytes still held: allocated minus released (saturating at zero).
    #[must_use]
    pub fn live_bytes(&self) -> u64 {
        self.bytes_allocated.saturating_sub(self.bytes_deallocated)
    }
}

impl Sub for HeapActivity {
    type Output = Self;

    fn sub(self, earlier: Self) -> Self {
        Self {
            allocations: self.allocations - earlier.allocations,
            reallocations: self.reallocations - earlier.reallocations,
            deallocations: self.deallocations - earlier.deallocations,
            bytes_allocated: self.bytes_allocated - earlier.bytes_allocated,
            bytes_deallocated: self.bytes_deallocated - earlier.bytes_deallocated,
        }
    }
}

/// Reads the calling thread's cumulative heap activity.
pub trait HeapCounter: Sync {
    /// The calling thread's counters, or `None` when no counting allocator is installed.
    fn current_thread(&self) -> Option<HeapActivity>;
}

/// A counter for contexts without a counting allocator (for example unit tests).
#[cfg(test)]
pub(crate) struct NoHeapCounter;

#[cfg(test)]
impl HeapCounter for NoHeapCounter {
    fn current_thread(&self) -> Option<HeapActivity> {
        None
    }
}

/// Heap activity on the current thread while `work` runs, or `None` without a counting allocator.
pub(crate) fn measure<T>(counter: &dyn HeapCounter, work: impl FnOnce() -> T) -> (T, Option<HeapActivity>) {
    let before = counter.current_thread();
    let result = work();
    let after = counter.current_thread();
    (result, before.zip(after).map(|(before, after)| after - before))
}
