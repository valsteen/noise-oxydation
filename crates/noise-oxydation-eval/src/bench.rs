//! `bench`: packet-path latency, heap activity, per-call memory and concurrent-call throughput in release builds.
//!
//! The input is a headerless μ-law file (by default the office-5db noisy input written by `replay`). Every evaluated
//! configuration is measured:
//!
//! - heap activity of construction, and of streaming, draining and resetting (which must be zero), counted by the
//!   binary's per-thread allocator;
//! - per-call memory: `size_of::<CallEnhancer>()` plus the heap bytes construction leaves allocated;
//! - the latency of every `process_packet` call over several passes on one thread, against the 20 ms packet cadence;
//! - wall time and real-time factor of many concurrent calls on separate threads, each looping the input for the
//!   call duration;
//! - with the `stage-timing` feature, the per-stage breakdown, aggregated here after the call threads have joined.

use std::{
    hint::black_box,
    mem::size_of,
    path::PathBuf,
    sync::Barrier,
    thread,
    time::{Duration, Instant},
};

use noise_oxydation::{CallConfig, CallEnhancer, DELAY_PACKETS, PACKET_SAMPLES, Packet};

use crate::{
    audio_io::read_mulaw_packets,
    error::EvalError,
    heap::{HeapActivity, HeapCounter, measure},
    replay::configurations,
};

/// The packet cadence of a live call.
const PACKET_CADENCE: Duration = Duration::from_millis(20);
const PACKETS_PER_SECOND: usize = 50;

#[derive(Debug, Clone)]
pub(crate) struct BenchOptions {
    pub(crate) input: PathBuf,
    pub(crate) passes: usize,
    pub(crate) concurrent_calls: Vec<usize>,
    pub(crate) call_seconds: usize,
}

pub(crate) fn default_concurrent_calls() -> Vec<usize> {
    let cores = thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let mut calls = vec![1, cores, 100];
    calls.dedup();
    calls
}

pub(crate) fn bench(options: &BenchOptions, heap: &dyn HeapCounter) -> Result<(), EvalError> {
    let bytes = read_mulaw_packets(&options.input)?;
    let packets: Vec<Packet> = bytes.as_chunks::<PACKET_SAMPLES>().0.to_vec();
    println!("input {} ({} packets, {} s)", options.input.display(), packets.len(), packets.len() / PACKETS_PER_SECOND);
    println!(
        "build: {}, stage-timing feature {}; size_of::<CallEnhancer>() = {} bytes",
        if cfg!(debug_assertions) { "DEBUG (numbers are not representative)" } else { "release" },
        if cfg!(feature = "stage-timing") { "on" } else { "off" },
        size_of::<CallEnhancer>()
    );

    println!("\nheap activity and per-call memory");
    println!(
        "  {:<26} {:>12} {:>12} {:>12} {:>16} {:>22}",
        "configuration", "constr. allocs", "constr. bytes", "live bytes", "per-call memory", "stream+drain+reset"
    );
    for (name, config) in configurations() {
        memory(name, &config, &packets, heap)?;
    }

    println!(
        "\nprocess_packet latency on one thread, {} passes of {} packets (budget {} ms per packet)",
        options.passes,
        packets.len(),
        PACKET_CADENCE.as_millis()
    );
    println!(
        "  {:<26} {:>8} {:>8} {:>8} {:>8} {:>8} {:>9} {:>9} {:>10} {:>11}",
        "configuration (µs)", "min", "p50", "p90", "p99", "p99.9", "max", "mean", "drain max", "max/budget"
    );
    let mut breakdowns = Vec::new();
    for (name, config) in configurations() {
        breakdowns.push((name, latency(name, &config, &packets, options.passes)?));
    }

    let call_packets = options.call_seconds * PACKETS_PER_SECOND;
    println!("\nconcurrent calls of {} s each (input looped), one thread per call", options.call_seconds);
    println!(
        "  {:<26} {:>6} {:>10} {:>18} {:>16}",
        "configuration", "calls", "wall (s)", "per call-equiv. (ms)", "real-time factor"
    );
    let mut concurrent_breakdowns = Vec::new();
    for (name, config) in configurations() {
        for &calls in &options.concurrent_calls {
            let timings = throughput(name, &config, &packets, calls, call_packets, options.call_seconds)?;
            if calls == *options.concurrent_calls.iter().max().unwrap_or(&calls) {
                concurrent_breakdowns.push((name, calls, timings));
            }
        }
    }

    print_breakdowns(&breakdowns, &concurrent_breakdowns);
    Ok(())
}

fn memory(
    name: &'static str,
    config: &CallConfig,
    packets: &[Packet],
    heap: &dyn HeapCounter,
) -> Result<(), EvalError> {
    let (enhancer, construction) = measure(heap, || CallEnhancer::new(config));
    let mut enhancer = enhancer?;
    let (streamed, processing) = measure(heap, || -> Result<(), EvalError> {
        stream(&mut enhancer, packets)?;
        enhancer.reset();
        stream(&mut enhancer, &packets[..packets.len() / 3])?;
        enhancer.reset();
        Ok(())
    });
    streamed?;
    let (Some(construction), Some(processing)) = (construction, processing) else {
        println!("  {name:<26} (no counting allocator installed)");
        return Ok(());
    };
    let live = construction.live_bytes();
    println!(
        "  {:<26} {:>12} {:>12} {:>12} {:>16} {:>22}",
        name,
        construction.allocations,
        construction.bytes_allocated,
        live,
        u64::try_from(size_of::<CallEnhancer>()).expect("fits u64") + live,
        describe_processing(processing)
    );
    if processing.is_none() {
        Ok(())
    } else {
        Err(EvalError::PacketPathAllocated { configuration: name, activity: processing })
    }
}

fn describe_processing(activity: HeapActivity) -> String {
    if activity.is_none() {
        "0 allocations".to_owned()
    } else {
        format!("{} allocs, {} deallocs", activity.allocations, activity.deallocations)
    }
}

/// Streams every packet and drains, without collecting output.
fn stream(enhancer: &mut CallEnhancer, packets: &[Packet]) -> Result<(), EvalError> {
    let mut output = [0; PACKET_SAMPLES];
    for packet in packets {
        black_box(enhancer.process_packet(packet, &mut output)?);
        black_box(&output);
    }
    let mut tail = [[0; PACKET_SAMPLES]; DELAY_PACKETS];
    black_box(enhancer.drain(&mut tail)?);
    black_box(&tail);
    Ok(())
}

fn latency(
    name: &'static str,
    config: &CallConfig,
    packets: &[Packet],
    passes: usize,
) -> Result<StageTotals, EvalError> {
    let mut enhancer = CallEnhancer::new(config)?;
    stream(&mut enhancer, packets)?;
    enhancer.reset();
    let mut durations = Vec::with_capacity(passes * packets.len());
    let mut drain_max = Duration::ZERO;
    let mut totals = StageTotals::default();
    let mut output = [0; PACKET_SAMPLES];
    let mut tail = [[0; PACKET_SAMPLES]; DELAY_PACKETS];
    for _ in 0..passes {
        for packet in packets {
            let start = Instant::now();
            let outcome = enhancer.process_packet(packet, &mut output);
            durations.push(start.elapsed());
            black_box(outcome?);
            black_box(&output);
        }
        let start = Instant::now();
        let drained = enhancer.drain(&mut tail);
        drain_max = drain_max.max(start.elapsed());
        black_box(drained?);
        totals.add(&enhancer);
        enhancer.reset();
    }
    durations.sort_unstable();
    let percentile = |per_mille: usize| durations[(durations.len() - 1) * per_mille / 1000];
    let micros = |duration: Duration| format!("{:.2}", duration.as_secs_f64() * 1e6);
    let total: Duration = durations.iter().sum();
    let count = u32::try_from(durations.len()).expect("fewer than 2^32 packets");
    let max = durations.last().copied().unwrap_or_default();
    println!(
        "  {:<26} {:>8} {:>8} {:>8} {:>8} {:>8} {:>9} {:>9} {:>10} {:>10.3}%",
        name,
        micros(durations.first().copied().unwrap_or_default()),
        micros(percentile(500)),
        micros(percentile(900)),
        micros(percentile(990)),
        micros(percentile(999)),
        micros(max),
        micros(total / count.max(1)),
        micros(drain_max),
        100.0 * max.as_secs_f64() / PACKET_CADENCE.as_secs_f64()
    );
    totals.packets = durations.len();
    totals.measured = total;
    Ok(totals)
}

fn throughput(
    name: &'static str,
    config: &CallConfig,
    packets: &[Packet],
    calls: usize,
    call_packets: usize,
    call_seconds: usize,
) -> Result<StageTotals, EvalError> {
    let barrier = Barrier::new(calls + 1);
    let (wall, results) = thread::scope(|scope| {
        let handles: Vec<_> = (0..calls)
            .map(|_| {
                scope.spawn(|| -> Result<StageTotals, EvalError> {
                    let enhancer = CallEnhancer::new(config);
                    // Every thread reaches the barrier, even when construction failed, so the start stays aligned.
                    barrier.wait();
                    let mut enhancer = enhancer?;
                    let mut output = [0; PACKET_SAMPLES];
                    for packet in packets.iter().cycle().take(call_packets) {
                        black_box(enhancer.process_packet(packet, &mut output)?);
                        black_box(&output);
                    }
                    let mut tail = [[0; PACKET_SAMPLES]; DELAY_PACKETS];
                    black_box(enhancer.drain(&mut tail)?);
                    let mut totals = StageTotals::default();
                    totals.add(&enhancer);
                    Ok(totals)
                })
            })
            .collect();
        barrier.wait();
        let start = Instant::now();
        let results: Vec<_> = handles.into_iter().map(|handle| handle.join().expect("call thread")).collect();
        (start.elapsed(), results)
    });
    let mut totals = StageTotals::default();
    for result in results {
        totals.merge(&result?);
    }
    let audio_seconds = f64::from(u32::try_from(calls * call_seconds).expect("small"));
    let call_count = f64::from(u32::try_from(calls).expect("small"));
    println!(
        "  {:<26} {:>6} {:>10.3} {:>18.3} {:>15.0}x",
        name,
        calls,
        wall.as_secs_f64(),
        wall.as_secs_f64() * 1000.0 / call_count,
        audio_seconds / wall.as_secs_f64()
    );
    totals.packets = calls * call_packets;
    Ok(totals)
}

/// Stage timings summed over calls by this tool (the library keeps them per call).
#[derive(Debug, Clone, Default)]
struct StageTotals {
    #[cfg(feature = "stage-timing")]
    stages: [(u64, Duration, Duration); 7],
    packets: usize,
    measured: Duration,
}

impl StageTotals {
    /// Adds one call's stage timings (nothing without the `stage-timing` feature).
    fn add(&mut self, enhancer: &CallEnhancer) {
        #[cfg(feature = "stage-timing")]
        {
            let timings = enhancer.stage_timings();
            for (total, stage) in self.stages.iter_mut().zip(noise_oxydation::Stage::ALL) {
                let timing = timings.get(stage);
                total.0 += timing.invocations;
                total.1 += timing.total;
                total.2 = total.2.max(timing.max);
            }
        }
        #[cfg(not(feature = "stage-timing"))]
        let _ = (self, enhancer);
    }

    fn merge(&mut self, other: &Self) {
        #[cfg(feature = "stage-timing")]
        for (total, other) in self.stages.iter_mut().zip(&other.stages) {
            total.0 += other.0;
            total.1 += other.1;
            total.2 = total.2.max(other.2);
        }
        self.measured += other.measured;
    }
}

#[cfg(feature = "stage-timing")]
fn print_breakdowns(single: &[(&str, StageTotals)], concurrent: &[(&str, usize, StageTotals)]) {
    for (name, totals) in single {
        print_breakdown(&format!("{name}, one thread (latency passes)"), totals);
    }
    for (name, calls, totals) in concurrent {
        print_breakdown(&format!("{name}, {calls} concurrent calls"), totals);
    }
}

#[cfg(feature = "stage-timing")]
fn print_breakdown(title: &str, totals: &StageTotals) {
    let stage_sum: Duration = totals.stages.iter().map(|stage| stage.1).sum();
    let packets = u32::try_from(totals.packets).expect("fewer than 2^32 packets").max(1);
    println!("\nstage breakdown: {title}");
    println!(
        "  {:<20} {:>12} {:>14} {:>12} {:>15} {:>8}",
        "stage", "invocations", "mean (ns)", "max (µs)", "per packet (µs)", "share"
    );
    for (stage, (invocations, total, max)) in noise_oxydation::Stage::ALL.iter().zip(&totals.stages) {
        let mean = if *invocations == 0 {
            0.0
        } else {
            total.as_secs_f64() * 1e9 / f64::from(u32::try_from(*invocations).unwrap_or(u32::MAX))
        };
        println!(
            "  {:<20} {:>12} {:>14.1} {:>12.2} {:>15.3} {:>7.1}%",
            stage.name(),
            invocations,
            mean,
            max.as_secs_f64() * 1e6,
            (*total / packets).as_secs_f64() * 1e6,
            100.0 * total.as_secs_f64() / stage_sum.as_secs_f64().max(f64::MIN_POSITIVE)
        );
    }
    let staged = (stage_sum / packets).as_secs_f64() * 1e6;
    if totals.measured > Duration::ZERO {
        let measured = (totals.measured / packets).as_secs_f64() * 1e6;
        println!("  sum of stages {staged:.3} µs per packet; measured process_packet mean {measured:.3} µs per packet");
    } else {
        println!("  sum of stages {staged:.3} µs per packet");
    }
}

#[cfg(not(feature = "stage-timing"))]
fn print_breakdowns(_single: &[(&str, StageTotals)], _concurrent: &[(&str, usize, StageTotals)]) {
    println!("\nstage breakdown: build with `--features stage-timing` to measure it");
}
