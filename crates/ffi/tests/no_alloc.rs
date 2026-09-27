use noise_oxydation_ffi::{NoCall, no_create, no_destroy, no_finish, no_process};
use std::alloc::{GlobalAlloc, Layout, System};
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct Counting;
static COUNTING: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.realloc(pointer, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn main() {
    for estimator in 0..3 {
        let mut handle: *mut NoCall = ptr::null_mut();
        // SAFETY: Valid storage and exclusive handle access.
        unsafe {
            assert_eq!(
                no_create(5_000_000_000, estimator, &raw mut handle).status,
                0
            );
            let input = [0x80; 160];
            let mut output = [0; 320];
            ALLOCATIONS.store(0, Ordering::Relaxed);
            COUNTING.store(true, Ordering::Relaxed);
            for _ in 0..400 {
                assert_eq!(
                    no_process(handle, input.as_ptr(), 160, output.as_mut_ptr(), 320).status,
                    0
                );
            }
            assert_eq!(no_finish(handle, output.as_mut_ptr(), 320).status, 0);
            COUNTING.store(false, Ordering::Relaxed);
            assert_eq!(
                ALLOCATIONS.load(Ordering::Relaxed),
                0,
                "estimator {estimator}"
            );
            assert_eq!(no_destroy(handle).status, 0);
        }
    }
}
