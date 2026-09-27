# Performance

This page records measured packet latency, heap activity, per-call memory and concurrent-call throughput of the
library, the same measurements through the Go binding, a comparison with the Go reference on the same machine and
audio, the per-stage cost, the vectorization investigation and the `minitrace` decision. Every number comes from release builds of the scalar code as it stands
in this repository.

In short, on an Apple M1 Ultra:

- A packet takes 14–17 µs on average and p99.9 stays below 0.09 ms, against a 20 ms packet cadence. The worst single
  packet observed in any run took 1.48 ms, which is still 7.4 % of the cadence.
- After construction, `process_packet`, `drain` and `reset` never touch the heap. A call holds 18–44 KB of state.
- 100 concurrent calls of 120 s each finish in 0.54–0.72 s of wall time, a real-time factor of about 17 000–22 000.
  The Go reference composition needs 2.2–2.3 s for the same work on the same machine and audio.
- Called from Go through the `noiseox` package, a packet costs the same within measurement spread: one Go-to-Rust
  crossing costs about 28 ns, the call loop performs no heap allocation, and 100 concurrent calls in goroutines reach
  real-time factors of 17 000–20 000 ([Go Binding](#go-binding)).
- Log-MMSE, specifically the exponential integral evaluated per bin in `f64`, takes about two thirds of the packet
  time. No vectorization candidate gave a measured benefit while keeping the output byte-identical, so none was
  adopted, and `minitrace` was not adopted either.

## Machine, Toolchains And Commands

| Item | Value |
| --- | --- |
| Machine | Mac Studio, Apple M1 Ultra (20 cores: 16 performance, 4 efficiency), 64 GB, macOS 15.5 |
| Rust | 1.98.1 (`rust-toolchain.toml`), target `aarch64-apple-darwin`, default release profile |
| Go | 1.27.1 `darwin/arm64`, `GOMAXPROCS` 20, reference module `v0.0.0-20260920200827-cfc7520a0625` |
| Input | `audio/out/office-5db/noisy.ul` from the replay ([evaluation.md](evaluation.md)): 2647 packets, 52.9 s |

```bash
scripts/fetch-evaluation-audio.sh
cargo run --locked --release -p noise-oxydation-eval -- replay           # writes the bench input
cargo run --locked --release -p noise-oxydation-eval -- bench
cargo run --locked --release -p noise-oxydation-eval --features stage-timing -- bench
(cd tools/go-parity && go run . bench -in ../../audio/out/office-5db/noisy.ul)
```

`bench` accepts `--input`, `--passes` (default 20), `--calls` (default 1, the number of cores, and 100) and
`--call-seconds` (default 120). The machine was otherwise idle but not isolated. The build without stage timing ran
four times and the build with it three times; ranges below span those runs, and single values come from the first
run.

## Method

`bench` measures the four configurations of the evaluation: SPP-MMSE, MCRA and the minimum estimator with tonal
transient suppression, and SPP-MMSE without it, all with default parameters.

- **Heap activity.** The `noise-oxydation-eval` binary installs a counting global allocator that delegates to
  `std::alloc::System` and counts calls and bytes per thread. `bench` counts construction, then streams the whole
  input, drains, resets, streams a third of it again, drains and resets. Any heap operation in the second phase
  fails the run.
- **Per-call memory.** `size_of::<CallEnhancer>()` plus the heap bytes construction leaves allocated.
- **Latency.** One call on one thread streams the input once to warm up, then 20 more times with a reset between
  passes. Each `process_packet` call is timed with `Instant::now` around it, so every figure includes about two clock
  reads.
- **Throughput.** Each call runs on its own OS thread and loops the input for 120 s of audio (6000 packets), then
  drains. The threads construct their enhancers, wait on a barrier, and the wall time runs from the barrier to the last
  join. The real-time factor is calls × 120 s / wall time.
- **Go reference.** `tools/go-parity` composes the reference's public packages in the order of its end-to-end
  benchmark (SPP-MMSE or MCRA, tonal suppression on). Its `bench` measures per-packet latency the same way, and
  runs 100 goroutines of 120 s looping the same input.

## Latency

`process_packet` latency in µs, one thread, 20 passes of 2647 packets (52 940 packets), build without stage timing:

| Configuration | min | p50 | p90 | p99 | p99.9 | max | mean | drain max |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `spp-mmse` | 0.38 | 14.96 | 29.38 | 41.79 | 86.08 | 145.88 | 16.69 | 31.33 |
| `mcra` | 0.33 | 12.96 | 25.71 | 40.12 | 77.83 | 136.29 | 14.49 | 34.08 |
| `minimum` | 0.33 | 14.75 | 28.75 | 43.96 | 80.29 | 153.62 | 16.66 | 28.04 |
| `spp-mmse-no-interference` | 0.33 | 13.96 | 27.38 | 39.83 | 83.29 | 161.50 | 15.51 | 65.08 |

Across the runs of this build the means varied by less than 0.2 µs and p50 by less than 0.1 µs; the maxima varied
between 133 µs and 198 µs. The distribution has three steps because a 160-sample packet completes zero (the first
packet), one or two 128-sample hops: most packets run one frame, every fourth runs two, and calibration frames skip
Log-MMSE and tonal detection.

Against the 20 ms cadence, p99.9 uses under 0.5 % of the budget. Maxima vary from run to run, which points to the
operating system rather than the audio: the largest value in any run, 1.48 ms, appeared once in 52 940 packets of an
MCRA pass in a stage-timing build, while that configuration's maximum was 0.15–0.45 ms in the other runs.

## Heap Activity And Per-Call Memory

| Configuration | Construction allocations | Construction bytes | Processing allocations | Per-call memory |
| --- | ---: | ---: | ---: | ---: |
| `spp-mmse` | 0 | 0 | 0 | 18 352 B |
| `mcra` | 1 | 3 648 | 0 | 22 000 B |
| `minimum` | 1 | 25 800 | 0 | 44 152 B |
| `spp-mmse-no-interference` | 0 | 0 | 0 | 18 352 B |

`size_of::<CallEnhancer>()` is 18 352 bytes; SPP-MMSE needs no heap block at all. MCRA's boxed state (3648 bytes) and
the minimum estimator's history (50 frames × 129 bins × 4 bytes = 25 800 bytes by default) are the only construction
allocations, as [ARCHITECTURE.md](../ARCHITECTURE.md#per-call-ownership) describes. With the `stage-timing` feature
the struct grows by 280 bytes (seven accumulators of 40 bytes). 100 SPP-MMSE calls therefore hold 1.8 MB of enhancer
state, and 100 minimum-estimator calls 4.4 MB.

Streaming, draining and resetting allocated nothing in every configuration and in both builds. The library's
allocation integration test enforces the same property in CI.

## Concurrent-Call Throughput

Calls of 120 s each (6000 packets, looping the input), one thread per call, build without stage timing:

| Configuration | 1 call | 20 calls | 100 calls | 100 calls, real-time factor |
| --- | ---: | ---: | ---: | ---: |
| `spp-mmse` | 0.104 s | 0.147 s | 0.61–0.71 s | 17 000–19 500× |
| `mcra` | 0.092 s | 0.140 s | 0.54–0.72 s | 16 800–22 200× |
| `minimum` | 0.107 s | 0.143 s | 0.61–0.66 s | 18 100–19 600× |
| `spp-mmse-no-interference` | 0.095 s | 0.133 s | 0.54–0.59 s | 20 300–22 100× |

The 1-call and 20-call columns are from the first run. One call of 120 s costs about 0.1 s of one core. Twenty calls
on twenty cores take 1.3–1.5 times as long as one call, most likely because four of the cores are slower efficiency
cores and the cores share caches. At 100 calls the threads outnumber the cores five to one, and the wall time grows to
four to five times the 20-call time, as expected when the work is compute-bound.

The benchmark does not measure CPU time or power, only wall time.

## Comparison With The Go Reference

Same machine, same input, same 20 passes and 100 × 120 s workload:

| Measure | Rust `spp-mmse` | Go SPP-MMSE | Rust `mcra` | Go MCRA |
| --- | ---: | ---: | ---: | ---: |
| Latency p50 | 14.96 µs | 29.67 µs | 12.96 µs | 27.75 µs |
| Latency p99 | 41.79 µs | 85.38 µs | 40.12 µs | 83.08 µs |
| Latency p99.9 | 86.08 µs | 160.79 µs | 77.83 µs | 169.62 µs |
| Latency max | 145.88 µs | 341.96 µs | 136.29 µs | 342.04 µs |
| Latency mean | 16.69 µs | 34.91 µs | 14.49 µs | 32.98 µs |
| 100 calls × 120 s, wall | 0.61–0.71 s | 2.316 s | 0.54–0.72 s | 2.158 s |
| Real-time factor | 17 000–19 500× | 5 181× | 16 800–22 200× | 5 562× |

The Rust packet path takes about half the Go time per packet, and 100 concurrent calls finish 3.0–4.0 times faster.
Both implementations run the same stages on the same analysis grid and produce nearly identical output (see
[reference-log.md](reference-log.md#measured-parity)). The measurements do not isolate the causes. The visible
differences are that the Go composition allocates new spectra and output slices for every frame, which also brings
garbage-collector work, while Rust reuses fixed storage; and that the two FFTs differ in twiddle generation.

For context, the reference's own benchmark (`go test ./benchmark -run '^$' -bench
BenchmarkEndToEndPipeline100ConcurrentTwoMinutes -benchtime=1x -count=3`, synthetic tones plus noise instead of the
office input) took 1.95–1.98 s on this machine. It reported 2867 MiB of cumulative allocations in 4.29 million
allocations, 15.0–16.3 MiB of peak live heap and 39 CPU-seconds (about 20 cores busy). Its README figure of 1.291 s
comes from an Apple M5 Pro and is not comparable with this machine.

## Go Binding

The Go package `noiseox` in `go/` calls the library through the C ABI crate `noise-oxydation-capi`
([go-integration.md](go-integration.md)). These measurements answer what a Go call-handling service pays on top of the
Rust packet cost: per-packet latency seen from Go, heap allocations of the Go call loop, the cost of one cgo crossing,
and the throughput of many calls in parallel goroutines.

### Setup And Commands

Same machine and input as above (`audio/out/office-5db/noisy.ul`, 2647 packets), Go 1.27.1 `darwin/arm64` with
`GOMAXPROCS` 20, the static library built in release mode by Rust 1.98.1, and the call-stream example compiled once.
The example is the production pattern of [go-integration.md](go-integration.md#the-call-stream-example): it reads each
packet from a buffered reader into one reused array, calls `ProcessPacket`, and writes emitted packets to a buffered
writer.

```bash
cargo build --locked --release -p noise-oxydation-capi
cd go
go build -o /tmp/callstream ./examples/callstream
for calls in 1 20 100; do
  /tmp/callstream -in ../audio/out/office-5db/noisy.ul -estimator spp-mmse -passes 20 -parallel "$calls"
done                                     # likewise -estimator mcra, -estimator minimum, -interference=false
go test -run '^$' -bench . -count 5 ./...
go test -race ./...                      # includes the testing.AllocsPerRun checks
```

- **Latency.** One Go call, 20 passes over the input with `Reset` between them (52 940 packets), `time.Now` read
  before and after each `ProcessPacket`, like `bench` does around `process_packet`. The Rust library itself is the
  same code measured above.
- **Allocations.** `runtime.MemStats` `Mallocs` and `TotalAlloc`, read after a `runtime.GC()` before the first pass
  and after the last drain, with no other goroutine running; the parallel runs happen afterwards. The package tests
  also assert `testing.AllocsPerRun` = 0 for `ProcessPacket` and for a process–drain–phase–reset cycle, with and
  without the race detector, and the benchmarks report allocations per operation.
- **Crossing cost.** `BenchmarkPhase` calls `Call.Phase`, which crosses into Rust, checks its pointers, enters the
  panic guard, reads one field and returns: an upper bound of the fixed cost every Go call into the library pays.
- **Throughput.** After the measured loop, the example creates N `Call`s in N goroutines, releases them together, and
  lets each stream 120 s of audio (6000 packets, looping the input) and drain. Wall time runs from the release to the
  last goroutine's end; every call must produce the same output digest.

### Results

`ProcessPacket` latency from Go in µs, ranges over four runs per configuration; the last column is the mean of
`process_packet` measured by `noise-oxydation-eval bench` in the same session:

| Configuration | min | p50 | p99 | p99.9 | max | mean | Rust mean, same session |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `spp-mmse` | 0.42 | 15.12–15.21 | 43.08–44.00 | 82.33–85.71 | 128.04–141.46 | 16.89–17.03 | 17.10 |
| `mcra` | 0.42 | 13.08–13.21 | 40.75–41.92 | 79.79–85.08 | 133.71–287.92 | 14.62–14.77 | 14.62 |
| `minimum` | 0.42 | 14.88–14.92 | 45.58–46.17 | 79.88–88.58 | 138.21–189.25 | 16.91–17.00 | 16.97 |
| `spp-mmse-no-interference` | 0.42 | 13.96–14.04 | 40.33–42.04 | 81.54–84.75 | 150.54–174.00 | 15.61–15.72 | 15.86 |

The Go-side means lie within 0.25 µs of the Rust means, above or below them, which is inside the run-to-run spread of
either tool: the crossing cost does not show at this resolution. It is measured directly instead: `BenchmarkPhase`
takes 28.0–28.3 ns per call (five runs), 0.17 % of a mean packet, and `BenchmarkProcessPacket` (synthetic input,
default configuration) 17.22–17.37 µs per packet. The slowest packet of any run took 0.29 ms, 1.4 % of the 20 ms
cadence.

Heap allocations: the call loop allocated nothing (0 allocations, 0 bytes) in every run of every configuration, over
52 940 `ProcessPacket` calls, 20 drains and 19 resets each. `testing.AllocsPerRun` reports 0 for `ProcessPacket` and
for the lifecycle cycle, also under `-race`, and both benchmarks report 0 B/op and 0 allocs/op. Without the
`#cgo noescape` directives the allocation test fails, because the arguments passed to C then escape to the heap.

Concurrent calls of 120 s each, one goroutine and one `Call` per call:

| Configuration | 1 call | 20 calls | 100 calls | 100 calls, real-time factor | Rust `bench`, 100 calls, same session |
| --- | ---: | ---: | ---: | ---: | ---: |
| `spp-mmse` | 0.106 s | 0.138 s | 0.699–0.702 s | 17 100–17 200× | 0.714 s |
| `mcra` | 0.092 s | 0.125 s | 0.594–0.602 s | 19 900–20 200× | 0.600 s |
| `minimum` | 0.108 s | 0.143 s | 0.687–0.701 s | 17 100–17 500× | 0.852 s |
| `spp-mmse-no-interference` | 0.098 s | 0.132 s | 0.622–0.631 s | 19 000–19 300× | 0.741 s |

Goroutines scale like the Rust threads of `bench`: 100 calls on 20 cores take about five times as long as 20 calls.
The Go runtime schedules them on `GOMAXPROCS` (20) threads; a goroutine inside `ProcessPacket` keeps its thread for the
duration of the Rust call.

The Go output is the Rust output: on the office input, `-out` of the example is byte-identical to
`audio/out/office-5db/enhanced-spp-mmse.ul` written by the replay, and the package tests compare three configurations
with digests that the C ABI crate's tests check against the Rust API.

### Limits Of The Go Measurements

- One machine, macOS on Apple silicon. The Linux CI job checks correctness, allocation and packet counts, not timing.
- `time.Now` on this machine advances in steps of about 42 ns, visible in the 0.42 µs minimum; each sample includes
  two clock reads.
- The loop does nothing but read, process and write packets through buffered files; a service adds network, codec and
  scheduling work around it. With `-pace` the example waits for a 20 ms ticker, and the process-wide allocation
  count then also includes the Go runtime's own background allocations while it waits, so paced runs report the count
  without failing on it.
- The rest of a Go service can still allocate and trigger garbage collection; the enhancer adds no garbage of its own,
  and a goroutine inside `ProcessPacket` does not hold up a collection because the runtime treats a cgo call like a
  system call.

## Stage Breakdown

Built with `--features stage-timing`, one thread, 20 passes (µs per packet; shares of the summed stage time):

| Stage | `spp-mmse` | `mcra` | `minimum` | `spp-mmse-no-interference` |
| --- | ---: | ---: | ---: | ---: |
| decode + high-pass | 0.38 (2.3 %) | 0.39 (2.7 %) | 0.39 (2.3 %) | 0.39 (2.5 %) |
| analysis (window, FFT, power) | 1.57 (9.3 %) | 1.58 (10.8 %) | 1.59 (9.4 %) | 1.59 (10.2 %) |
| noise estimation | 0.97 (5.8 %) | 0.14 (1.0 %) | 0.62 (3.7 %) | 0.97 (6.2 %) |
| tonal detection | 1.24 (7.4 %) | 1.28 (8.7 %) | 1.30 (7.8 %) | — |
| Log-MMSE (with tonal gain application) | 10.66 (63.4 %) | 9.31 (63.3 %) | 10.91 (64.9 %) | 10.68 (68.4 %) |
| synthesis (inverse FFT, overlap-add) | 1.57 (9.4 %) | 1.59 (10.8 %) | 1.59 (9.5 %) | 1.58 (10.1 %) |
| encode + queue | 0.41 (2.4 %) | 0.41 (2.8 %) | 0.41 (2.4 %) | 0.41 (2.6 %) |
| sum of stages | 16.80 | 14.70 | 16.81 | 15.62 |
| measured `process_packet` mean | 16.89 | 14.79 | 16.90 | 15.71 |

Per frame, Log-MMSE costs about 9.4 µs with SPP-MMSE: 129 exponential integrals in `f64`, each a power series or a
continued fraction iterated to convergence, plus a logarithm or exponential. The two FFTs cost about 1.2 µs each.
The stages account for all but 0.1 µs per packet of the measured time.

`bench` also prints the breakdown aggregated over the 100 concurrent calls, summed by the tool after the threads join.
With five threads per core, those durations include time a thread spent descheduled in the middle of a stage (for
example 48 µs per Log-MMSE frame instead of 9.4 µs), so only the one-thread breakdown measures cost.

### Overhead Of Stage Timing

The feature reads `Instant::now` once per stage boundary: two reads per packet plus seven per enhanced frame (five per
calibration frame), about eleven per packet while enhancing. Three runs of each build:

| Measure | Without | With | Change |
| --- | ---: | ---: | ---: |
| `spp-mmse` mean latency | 16.58–16.61 µs | 16.78–16.95 µs | +1.0 to +2.2 % |
| `mcra` mean latency | 14.38–14.40 µs | 14.70–14.77 µs | +2.1 to +2.7 % |
| `spp-mmse` 100 calls × 120 s | 0.614–0.642 s | 0.625–0.653 s | within run-to-run spread |
| `CallEnhancer` size | 18 352 B | 18 632 B | +280 B |

Every replay output (all 16 enhanced files and 4 inputs) is byte-identical with and without the feature, and the
allocation test passes in both builds.

## Vectorization Investigation

The baseline is the scalar Rust code compiled by LLVM for `aarch64-apple-darwin`. The acceptance rule for any change
was a measured benefit and byte-identical replay output for every scenario and configuration. The replay output
hashes of the baseline were recorded for that comparison.

| Candidate | Measurement | Output | Decision |
| --- | --- | --- | --- |
| Element-wise loops: power spectrum, windowing, gain application, overlap-add and normalization, μ-law decode | Release assembly of the library: the inlined frame path (`process_frame`) holds 1071 NEON vector float instructions and 196 scalar ones; `Analyzer::transform` is fully vectorized (50 vector, 0 scalar) | unchanged | Already autovectorized; nothing to add |
| `-C target-cpu=native` | Adds no target feature on this machine: the target's default CPU is already `apple-m1` | — | Nothing to adopt here; x86-64 targets were not measured |
| Split real and imaginary arrays in the FFT, identical butterfly arithmetic | 1576–1585 ns vs 1410–1428 ns per 256-point transform (0.89–0.90×) | bit-identical on 10 000 random frames | Rejected: slower |
| Split FFT with per-stage contiguous twiddles, so the butterfly loop is a zip over slices | 1404–1413 ns vs 1410–1428 ns (1.00–1.02×) | bit-identical on 10 000 random frames | Rejected: no measurable benefit |
| Optimized FFT crates, `rustfft` 6 and `realfft` 3, measured in a throwaway crate outside the workspace | 409 ns (complex) and 246 ns (real input) vs 1410–1428 ns (3.4× and 5.8×) | 9 of 129 bins bit-identical, largest difference 2.9e−6 | Rejected: changes the output and adds a dependency; with 2.5 transforms per packet the best case would save about 13–18 % of packet time |
| Log-MMSE exponential integral, the dominant cost | Per-bin iterative `f64` series or continued fraction plus `exp` and `ln` | A vector version needs vector transcendental functions whose rounding differs from `std` | Not attempted: it cannot stay byte-identical |

The largest remaining lever is the exponential integral. A faster evaluation (for example a table or a rational
approximation) would change output bits and is a different kind of change from vectorization, so it is left for a
decision that accepts measured output differences. The library still contains no `unsafe` code and no explicit SIMD.

## Minitrace Decision

`minitrace` is not adopted. The question it would answer, where a packet's time goes, is answered by the
`stage-timing` feature at 1–3 % overhead, without allocation, locks or a dependency: one stage, Log-MMSE, takes about
two thirds of the time, and every stage's maximum stayed below 0.3 ms in the one-thread breakdown. A tracing
framework would add a dependency and a process-wide span collector that the per-call ownership rules would have to
accommodate, for no additional question this project needs answered. `minitrace` itself was not measured. The
decision should be revisited if calls need to be traced across application components (for example from network
receive to send), which per-stage accumulation cannot show.

## Limits

- One machine, one operating system and one CPU architecture. x86-64 and Linux, which CI builds on, were not
  measured.
- Wall time only: no CPU time, power or frequency control. Other processes were not stopped.
- One input: the office scene from the replay. Packet cost depends a little on content because the exponential
  integral converges at different rates.
- Latency is measured around `process_packet` on a thread that does nothing else, without network or codec work.
- The Go comparison uses a harness composed like the reference benchmark, not a production Go service.
- The Go binding has its own limits, listed in [Limits Of The Go Measurements](#limits-of-the-go-measurements).
