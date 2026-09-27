//! Unwinding containment: no panic crosses the ABI, and a handle whose operation panicked is poisoned.

use std::panic::{self, AssertUnwindSafe};

use noise_oxydation::CallEnhancer;

use crate::abi::NOX_STATUS_PANIC;

/// Runs `operation`, turning a panic into `fallback`.
pub(crate) fn guard<T>(fallback: T, operation: impl FnOnce() -> T) -> T {
    // `AssertUnwindSafe`: after a panic nothing observes state the closure may have left half-updated. Handle
    // operations go through `run_on_enhancer`, which poisons the handle; the other closures own no shared state.
    panic::catch_unwind(AssertUnwindSafe(operation)).unwrap_or(fallback)
}

/// The enhancer behind one handle and whether one of its operations panicked.
#[derive(Debug)]
pub(crate) struct GuardedEnhancer {
    enhancer: CallEnhancer,
    poisoned: bool,
}

impl GuardedEnhancer {
    pub(crate) fn new(enhancer: CallEnhancer) -> Self {
        Self { enhancer, poisoned: false }
    }

    /// Runs `operation` on the enhancer and returns its status. A poisoned handle answers [`NOX_STATUS_PANIC`] without
    /// running it; a panic poisons the handle and also answers [`NOX_STATUS_PANIC`].
    pub(crate) fn run(&mut self, operation: impl FnOnce(&mut CallEnhancer) -> u32) -> u32 {
        if self.poisoned {
            return NOX_STATUS_PANIC;
        }
        let enhancer = &mut self.enhancer;
        if let Ok(status) = panic::catch_unwind(AssertUnwindSafe(|| operation(enhancer))) {
            status
        } else {
            self.poisoned = true;
            NOX_STATUS_PANIC
        }
    }

    /// Runs a read-only `operation` unless the handle is poisoned.
    pub(crate) fn read(&self, operation: impl FnOnce(&CallEnhancer) -> u32) -> u32 {
        if self.poisoned {
            return NOX_STATUS_PANIC;
        }
        guard(NOX_STATUS_PANIC, || operation(&self.enhancer))
    }
}

#[cfg(test)]
#[path = "../tests/unit/guard.rs"]
mod tests;
