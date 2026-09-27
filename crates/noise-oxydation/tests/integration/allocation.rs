//! `process_packet`, `drain` and `reset` never touch the heap after construction.
//!
//! This test binary installs a counting global allocator. Counters are per thread and each test measures only its
//! own thread, so tests running in parallel do not disturb each other. The gates run it with `--all-features`, which
//! includes the `stage-timing` feature, and CI also runs it without optional features.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
    time::Duration,
};

use noise_oxydation::{
    CallConfig, CallEnhancer, DELAY_PACKETS, InterferenceConfig, McraConfig, MinimumConfig, NoiseEstimatorConfig,
    PACKET_SAMPLES, Packet, SppMmseConfig, TonalTransientConfig,
};

thread_local! {
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
    static REALLOCATIONS: Cell<u64> = const { Cell::new(0) };
    static DEALLOCATIONS: Cell<u64> = const { Cell::new(0) };
}

fn increment(counter: &'static std::thread::LocalKey<Cell<u64>>) {
    // `try_with` tolerates allocator calls during thread teardown; the const-initialized counters have no destructor.
    let _ = counter.try_with(|count| count.set(count.get() + 1));
}

struct CountingAllocator;

// SAFETY: every method forwards its arguments unchanged to `System`, which upholds the `GlobalAlloc` contract; the
// only addition is a thread-local counter update, which never allocates.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        increment(&ALLOCATIONS);
        // SAFETY: the caller upholds `GlobalAlloc::alloc`'s contract for `layout`.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        increment(&ALLOCATIONS);
        // SAFETY: the caller upholds `GlobalAlloc::alloc_zeroed`'s contract for `layout`.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        increment(&DEALLOCATIONS);
        // SAFETY: `pointer` was allocated by `System` through this allocator with `layout`.
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        increment(&REALLOCATIONS);
        // SAFETY: `pointer` was allocated by `System` through this allocator with `layout`, and the caller upholds
        // `GlobalAlloc::realloc`'s contract for `new_size`.
        unsafe { System.realloc(pointer, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HeapActivity {
    allocations: u64,
    reallocations: u64,
    deallocations: u64,
}

fn heap_activity() -> HeapActivity {
    HeapActivity {
        allocations: ALLOCATIONS.with(Cell::get),
        reallocations: REALLOCATIONS.with(Cell::get),
        deallocations: DEALLOCATIONS.with(Cell::get),
    }
}

/// Heap activity on the current thread while `work` runs.
fn measure(work: impl FnOnce()) -> HeapActivity {
    let before = heap_activity();
    work();
    let after = heap_activity();
    HeapActivity {
        allocations: after.allocations - before.allocations,
        reallocations: after.reallocations - before.reallocations,
        deallocations: after.deallocations - before.deallocations,
    }
}

const NONE: HeapActivity = HeapActivity { allocations: 0, reallocations: 0, deallocations: 0 };

/// Deterministic noise packets with a slow level change, so calibration, enhancement and silence all occur.
fn packets(count: usize) -> Vec<Packet> {
    let mut state = 0x1234_5678_u32;
    (0..count)
        .map(|index| {
            let mut packet = [0xFF; PACKET_SAMPLES];
            let loud = index % 100 > 60;
            for byte in &mut packet {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let [_, _, _, draw] = state.to_le_bytes();
                *byte = if loud { draw } else { draw | 0x60 };
            }
            packet
        })
        .collect()
}

fn stream_and_drain(enhancer: &mut CallEnhancer, input: &[Packet]) {
    let mut output = [0; PACKET_SAMPLES];
    for packet in input {
        let outcome = enhancer.process_packet(packet, &mut output);
        assert!(outcome.is_ok());
        black_box(&output);
    }
    let mut tail = [[0; PACKET_SAMPLES]; DELAY_PACKETS];
    assert!(enhancer.drain(&mut tail).is_ok());
    black_box(&tail);
    // With the `stage-timing` feature the packet path also accumulates stage timings, and reading them is a copy.
    #[cfg(feature = "stage-timing")]
    black_box(enhancer.stage_timings());
}

#[test]
fn the_counter_observes_heap_activity_on_this_thread() {
    let activity = measure(|| {
        let mut buffer = black_box(Vec::<u8>::with_capacity(8));
        buffer.extend_from_slice(&[0; 64]);
        drop(black_box(buffer));
    });
    assert!(activity.allocations >= 1 && activity.reallocations >= 1 && activity.deallocations >= 1, "{activity:?}");
}

/// Every noise estimator with interference suppression enabled and disabled; the minimum estimator's history is
/// sized from its window at construction, so a non-default window is included too.
fn configurations(calibration: Duration) -> Vec<CallConfig> {
    let estimators = [
        NoiseEstimatorConfig::SppMmse(SppMmseConfig::default()),
        NoiseEstimatorConfig::Mcra(McraConfig::default()),
        NoiseEstimatorConfig::Minimum(MinimumConfig::default()),
        NoiseEstimatorConfig::Minimum(MinimumConfig { window_frames: 7, ..MinimumConfig::default() }),
    ];
    let interference =
        [InterferenceConfig::TonalTransient(TonalTransientConfig::default()), InterferenceConfig::Disabled];
    estimators
        .into_iter()
        .flat_map(|noise_estimator| {
            interference.into_iter().map(move |interference| CallConfig {
                calibration_duration: calibration,
                noise_estimator,
                interference,
                ..CallConfig::default()
            })
        })
        .collect()
}

#[test]
fn packet_path_drain_and_reset_do_not_allocate() {
    let input = packets(400);
    for calibration in [Duration::ZERO, Duration::from_secs(1), Duration::from_secs(5)] {
        for config in configurations(calibration) {
            let mut enhancer = CallEnhancer::new(&config).expect("valid config");
            let activity = measure(|| {
                stream_and_drain(&mut enhancer, &input);
                enhancer.reset();
                stream_and_drain(&mut enhancer, &input[..123]);
                enhancer.reset();
                stream_and_drain(&mut enhancer, &[]);
            });
            assert_eq!(activity, NONE, "{:?} with {:?}", config.noise_estimator, config.interference);
        }
    }
}
