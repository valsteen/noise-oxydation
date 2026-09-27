//! `noise-oxydation-eval`: replay, compare and bench (see `docs/evaluation.md` and `docs/performance.md`).
//!
//! This file holds the workspace's second and last permitted `unsafe` code: a counting global allocator that delegates
//! every call to `std::alloc::System` and counts per thread, so `bench` can prove the packet path allocation-free.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    process::ExitCode,
};

use noise_oxydation_eval::{HeapActivity, HeapCounter};

thread_local! {
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
    static REALLOCATIONS: Cell<u64> = const { Cell::new(0) };
    static DEALLOCATIONS: Cell<u64> = const { Cell::new(0) };
    static BYTES_ALLOCATED: Cell<u64> = const { Cell::new(0) };
    static BYTES_DEALLOCATED: Cell<u64> = const { Cell::new(0) };
}

fn add(counter: &'static std::thread::LocalKey<Cell<u64>>, amount: usize) {
    let amount = u64::try_from(amount).unwrap_or(u64::MAX);
    // `try_with` tolerates allocator calls during thread teardown; the const-initialized counters have no destructor.
    let _ = counter.try_with(|count| count.set(count.get().saturating_add(amount)));
}

struct CountingAllocator;

// SAFETY: every method forwards its arguments unchanged to `System`, which upholds the `GlobalAlloc` contract; the
// only addition is a thread-local counter update, which never allocates.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        add(&ALLOCATIONS, 1);
        add(&BYTES_ALLOCATED, layout.size());
        // SAFETY: the caller upholds `GlobalAlloc::alloc`'s contract for `layout`.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        add(&ALLOCATIONS, 1);
        add(&BYTES_ALLOCATED, layout.size());
        // SAFETY: the caller upholds `GlobalAlloc::alloc_zeroed`'s contract for `layout`.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        add(&DEALLOCATIONS, 1);
        add(&BYTES_DEALLOCATED, layout.size());
        // SAFETY: `pointer` was allocated by `System` through this allocator with `layout`.
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        add(&REALLOCATIONS, 1);
        add(&BYTES_DEALLOCATED, layout.size());
        add(&BYTES_ALLOCATED, new_size);
        // SAFETY: `pointer` was allocated by `System` through this allocator with `layout`, and the caller upholds
        // `GlobalAlloc::realloc`'s contract for `new_size`.
        unsafe { System.realloc(pointer, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

struct ThreadCounters;

impl HeapCounter for ThreadCounters {
    fn current_thread(&self) -> Option<HeapActivity> {
        Some(HeapActivity {
            allocations: ALLOCATIONS.with(Cell::get),
            reallocations: REALLOCATIONS.with(Cell::get),
            deallocations: DEALLOCATIONS.with(Cell::get),
            bytes_allocated: BYTES_ALLOCATED.with(Cell::get),
            bytes_deallocated: BYTES_DEALLOCATED.with(Cell::get),
        })
    }
}

fn main() -> ExitCode {
    match noise_oxydation_eval::run(std::env::args().skip(1).collect(), &ThreadCounters) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            let mut source = std::error::Error::source(&error);
            while let Some(cause) = source {
                eprintln!("  caused by: {cause}");
                source = cause.source();
            }
            ExitCode::FAILURE
        }
    }
}
