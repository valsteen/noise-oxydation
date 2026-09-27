# Architecture

Noise Oxydation is a Rust library for real-time enhancement of 8 kHz mono telephony audio carried in 160-byte G.711
μ-law packets. One enhancer instance owns one call. This document owns the crate map, dependency direction, per-call
state ownership, the packet timing contract, and the audio-critical constraints. Algorithm equations, defaults, and
units live in [docs/algorithms.md](docs/algorithms.md); differences from the Go reference live in
[docs/reference-log.md](docs/reference-log.md).

The behavior reference is [sghaida/noise-cancelation at `cfc7520`](https://github.com/sghaida/noise-cancelation/tree/cfc7520a0625da90e4ad4699541a6ffe98e7c637).
This is an independently written implementation; no Go code is copied.

## Crate Map And Dependency Direction

Crates are grouped by dependency surface first, then by coherent responsibility. Dependencies point downward only.

| Crate | Role | Dependencies | Status |
| --- | --- | --- | --- |
| `crates/noise-oxydation` | Library: μ-law codec, high-pass filter, STFT/ISTFT, noise estimators, decision-directed SNR, Log-MMSE, tonal transient suppression, and the per-call `CallEnhancer` | `std`; optional `log` facade | Complete documented processing path implemented: SPP-MMSE, MCRA, and minimum noise estimators, decision-directed SNR, Log-MMSE, tonal transient suppression |
| `crates/noise-oxydation-eval` | Offline evaluation binary `noise-oxydation-eval`: real-speech replay with quality metrics, byte comparison of μ-law outputs, and packet latency, allocation, memory and throughput benchmarks | `noise-oxydation`, `hound` (WAV I/O) | Implemented: `replay`, `compare`, `bench` ([docs/evaluation.md](docs/evaluation.md), [docs/performance.md](docs/performance.md)) |
| `crates/how-it-works` | Project-owned renderer for `HOW_IT_WORKS.md` diagrams in day and night palettes, with a freshness check | `std` only | Planned (visual guide) |

Outside the Cargo workspace, [`tools/go-parity`](tools/go-parity/main.go) is a Go module that drives the pinned Go
reference as a dependency for parity, timing and reference-concern measurements. It is an evidence tool: nothing in
the workspace depends on it, and CI does not run it.

The evaluation crate is split into a library, which holds every workflow and forbids `unsafe` code, and a thin binary
that adds the allocation-counting global allocator (see [Audio-Critical Constraints](#audio-critical-constraints)).
Its `stage-timing` feature forwards to the library's feature so that `bench` can print the stage breakdown.

The library's public API is the per-call packet interface: `CallEnhancer` (`new`, `process_packet`, `drain`,
`reset`, `phase`, `config`), `Packet` (`[u8; 160]`), `PacketOutcome`, `CallPhase`, `CallConfig` with its per-stage
parameter structs and the `NoiseEstimatorConfig` and `InterferenceConfig` choices, and the typed errors `ConfigError`
and `StreamError`. With the `stage-timing` feature it adds `CallEnhancer::stage_timings` and the `Stage`,
`StageTiming` and `StageTimings` types. Stages are private modules of the one crate:

| Module | Responsibility |
| --- | --- |
| `enhancer` | `CallEnhancer`: stage order, calibration clock, packet timing, drain and reset |
| `config`, `error` | Configuration defaults and validation; typed errors |
| `mulaw`, `highpass` | G.711 μ-law codec; DC-blocking high-pass filter |
| `window`, `fft`, `analysis`, `synthesis` | Symmetric Hann window; 256-point FFT; STFT framing; weighted overlap-add |
| `noise_estimator` | The call's noise estimator: static dispatch over the three estimators |
| `spp_mmse`, `mcra`, `minimum` | SPP-MMSE; minimum-controlled recursive averaging; exact sliding-window minimum |
| `power` | Power flooring and the `f64` calibration mean shared by the estimators |
| `decision_directed`, `log_mmse` | A priori SNR estimation; Log-MMSE gain and the exponential integral |
| `tonal` | Tonal transient detector: per-bin interference gains applied after Log-MMSE |
| `output_queue` | Bounded FIFO of finalized encoded samples with exact sample accounting |
| `stage_timing` | Opt-in per-call stage accumulators; zero-sized without the `stage-timing` feature |
| `convert` | The audited lossy numeric conversions |
| `geometry` | The fixed telephony constants |

Rules:

- The library crate has no dependency on evaluation, rendering, WAV, or CLI crates. Tools depend on the library, never
  the reverse.
- Do not add a generic `utils` crate or split the library into per-algorithm crates: every DSP stage shares one
  dependency surface (`std`) and one lifecycle owner (the call), so they stay modules of one crate.
- A new crate needs an independent dependency, lifecycle, or reuse boundary (for example a C ABI for Go integration).
- Error types stay owned by the crate that recognizes the condition. A consumer crate maps them at its boundary into its
  own typed error while preserving the source (`std::error::Error::source`) so callers do not inherit dependencies.

## Fixed Telephony Geometry

The product scope is 8 kHz mono G.711 μ-law telephony, so the geometry is a set of compile-time constants rather than
runtime configuration:

| Quantity | Value |
| --- | --- |
| Sample rate | 8000 Hz |
| Packet | 160 μ-law bytes = 160 samples = 20 ms |
| FFT size / analysis window | 256 samples (32 ms), symmetric Hann |
| Hop | 128 samples (16 ms), 50 % overlap |
| One-sided bins | 129, bin width 31.25 Hz |

Algorithm parameters (cutoff, smoothing constants, gains, thresholds, window lengths, calibration duration, noise
estimator choice, interference setting) are runtime configuration validated at construction. Invalid values are
rejected with typed errors; they are never silently replaced by defaults.

## Per-Call Ownership

`CallEnhancer` is the composition root for one call. It exclusively owns every piece of temporal state for that call:
high-pass filter memory, the analysis input buffer, the window, FFT scratch and twiddles, the noise estimator's state
(SPP-MMSE, MCRA, or the minimum estimator), decision-directed SNR history, Log-MMSE scratch, the tonal transient
detector's previous power and gains, the synthesis overlap buffers, the calibration clock, the encoded output
queue, and, with the `stage-timing` feature, the call's stage timing accumulators.

The chosen noise estimator is an enum and the tonal detector an optional field inside the instance, so every stage is
dispatched statically: no trait objects, no shared tables. Storage is fixed-size arrays inside the instance, with two
blocks allocated once by `CallEnhancer::new`: the minimum estimator's history of `window_frames` smoothed spectra (the
only buffer whose size depends on configuration), and MCRA's state, which is about 1.5 KB larger than the other
estimators' and lives in one fixed-size heap block so that the estimator enum stays small. `reset` clears every stage,
including the estimator history and the tonal detector, in place.

Nothing is shared between calls: the library owns no global mutable state, no shared caches, and no locks. (The
optional `log` facade keeps its own process-wide logger registration; the library only reads it at construction.)

- Processing within a call is strictly sequential. Each packet runs the stages in order on the caller's thread.
- Independent calls run in parallel by giving each call its own `CallEnhancer` on whatever thread or task drives that
  call. `CallEnhancer` is `Send` (it may move between threads between calls to its methods) and its mutating methods
  take `&mut self`, so the compiler rejects concurrent use of one instance. An integration test runs several calls on
  parallel threads and checks their output is byte-identical to sequential processing.
- Stage timings are per-call state too. A tool that wants totals across concurrent calls reads each call's
  `StageTimings` snapshot and adds them up itself after its call threads have finished, as `noise-oxydation-eval bench`
  does.
- There is no cross-thread boundary inside the library, so it uses no atomics or locks. If a future feature adds one
  (for example publishing statistics to a monitoring thread), values that must be observed together are published as
  one coherent snapshot, never as independently updated atomics.

## Packet Timing Contract

Input arrives as whole 160-byte packets. Output leaves as whole 160-byte packets, in order, with a fixed algorithmic
delay of two packets (40 ms):

- Packets 1 and 2 of a call produce no output: `process_packet` returns `PacketOutcome::Priming`.
- From packet 3 on, every input packet produces exactly one output packet (`PacketOutcome::Emitted`): output packet *n*
  carries the enhanced samples of input packet *n − 2*.
- `drain` ends the call. It processes the buffered tail with one zero-padded analysis frame plus the synthesis tail and
  returns exactly the withheld packets (`min(packets_in, 2)`), so the total output sample count equals the total input
  sample count. Samples beyond the input end (analysis padding) are never emitted. Draining a call that received no
  packet processes no frame and returns zero packets.
- After `drain`, further packets and drains are rejected with `StreamError::Drained` until `reset`.
- `reset` starts a new call on the same instance. It discards any buffered input and withheld output, restores the
  calibration clock, and reuses all storage.

Why two packets: frame *t* covers input samples `[128t, 128t + 256)` and finalizes output samples `[128t, 128t + 128)`
once overlap-add with frame *t − 1* is complete. After *p ≥ 2* packets, `128·⌊(160p − 128)/128⌋ ≥ 160p − 255` samples
are final, which is always at least `160(p − 2)`. One packet of delay is not enough: after two packets only 128 samples
are final, fewer than one packet.

Timing mirrors the reference analysis grid exactly (see [docs/reference-log.md](docs/reference-log.md) for the Go
behavior and every deliberate difference).

## Quiet-Intro Calibration

A call is assumed to begin with a quiet intro: environment noise without wanted speech. The default calibration
duration is 5 s and is configurable (zero disables calibration).

- A spectral frame is a calibration frame when it ends at or before the calibration duration:
  `128t + 256 ≤ calibration_samples`. With the 5 s default that is frames 0–310; frame 311 (starting at 4.976 s) is the
  first enhanced frame.
- During calibration the noise estimator accumulates the mean noise power spectrum, and the audio passes through
  un-enhanced (high-pass filtered only). The tonal transient detector does not run on calibration frames.
- At the first non-calibration frame the estimator switches to adaptive tracking, initialized from the calibration
  mean. All three estimators follow this rule.
- The call phase (`CallEnhancer::phase`) is `CallPhase::Calibrating` from construction until the first
  non-calibration frame has been processed, then `CallPhase::Enhancing`; after `drain` it is `CallPhase::Drained`.
  When the duration is shorter than one frame (256 samples), no calibration frame exists and the phase starts as
  `Enhancing`; the estimator then initializes from the first frame.
- Speech during calibration is passed through un-enhanced, and it is learned as noise. The inflated noise estimate
  over-suppresses speech bands right after calibration until the adaptive estimator tracks back down. The replay
  measured the recovery by enhancing the same speech and noise with the talker starting at 0 s and at 6 s
  ([docs/evaluation.md](docs/evaluation.md#speech-during-calibration)). With 5 dB SNR office noise, the first second
  after calibration was up to 1.1 dB (SPP-MMSE), 2.1 dB (minimum estimator) and 5.1 dB (MCRA) quieter than in the
  undisturbed call. The speech level matched the undisturbed call within 1 dB from 1 s after calibration for SPP-MMSE
  and the minimum estimator, and from 3 s for MCRA. Louder or longer speech in the intro can take longer.

## Audio-Critical Constraints

`process_packet`, `drain`, and `reset` are audio-critical:

- no heap allocation or deallocation after construction;
- no locks, blocking calls, I/O, or logging;
- bounded, preallocated storage sized by the fixed geometry (input buffer, overlap buffers, output queue), plus the
  minimum estimator's history sized from its validated window at construction;
- deterministic output for identical input and configuration.

Construction (`CallEnhancer::new`) is the only place that may allocate or log. Integration tests enforce the allocation
claim with a per-thread counting global allocator for every estimator with interference suppression enabled and
disabled, in builds with and without the `stage-timing` feature, the lifecycle claim with sample-exact accounting, and
the concurrency claim by running independent calls with mixed configurations on parallel threads and comparing their
output with sequential runs. `noise-oxydation-eval bench` repeats the allocation check in release builds on real audio
and measures latency against the 20 ms cadence ([docs/performance.md](docs/performance.md)).

The library crate forbids `unsafe` code (`#![forbid(unsafe_code)]`), and so does the evaluation crate's library. The
workspace contains exactly two `unsafe` sites, both allocation-counting `GlobalAlloc` implementations that delegate
every call to `std::alloc::System` and count per thread: the library's `allocation` test harness and the
`noise-oxydation-eval` binary (`crates/noise-oxydation-eval/src/main.rs`). No vectorization or other optimization may
add `unsafe` to the library; the vectorization investigation adopted no explicit SIMD.

## Observability

- Logging uses the `log` facade behind the default `log` Cargo feature. The library emits one debug record only from
  construction (`CallEnhancer::new`), naming the noise estimator and the interference setting, which is initialization
  rather than the packet path; `process_packet`, `drain`, and `reset` never log and report lifecycle facts through their
  return values and `phase()`. Disable at compile time with `default-features = false` or at runtime by installing no
  logger or filtering the level. Applications and tools log their own call-level events; the evaluation tool prints
  plain reports to stdout.
- The `stage-timing` Cargo feature (off by default) is the opt-in performance analysis. Each call accumulates, per
  stage, the invocation count and the total and maximum duration, read with `CallEnhancer::stage_timings` as a `Copy`
  snapshot and cleared by `reset`. The stages are decode and high-pass, analysis, noise estimation, tonal detection,
  Log-MMSE (including applying the tonal gains), synthesis, and encode and queue. The call reads `Instant::now` at
  stage boundaries and adds to fixed accumulators: no allocation, locks, atomics or I/O. Measured overhead is 1–3 % of
  packet time, and the enhanced output is byte-identical with and without the feature. Without the feature the clock
  is zero-sized and never reads the time.
- `minitrace` is not adopted. Per-stage accounting located the cost (Log-MMSE takes about two thirds of packet time),
  and a tracing framework would add a dependency and a process-wide collector without answering a further question
  ([docs/performance.md](docs/performance.md#minitrace-decision)).

## Numeric Conversions

Clippy pedantic runs with warnings as errors. Lossy numeric conversions that cannot be expressed with a lossless std
conversion (f64 → f32 narrowing, float → PCM16 quantization) are confined to one audited conversion module per crate:
`crates/noise-oxydation/src/convert.rs` in the library and `crates/noise-oxydation-eval/src/convert.rs` in the
evaluation crate. Every lint expectation is narrow, carries a `reason`, and is listed in
[docs/lint-exceptions.md](docs/lint-exceptions.md).

## Limitations

- Fixed 8 kHz / 256 / 128 geometry: other sample rates or FFT sizes are out of scope.
- Classical single-channel enhancement: it cannot separate a second talker, and strong foreground sounds that do not
  match the tonal-transient pattern are treated as foreground.
- Tonal transient suppression analyzes only bins at or above `min_frequency_bin` (2 kHz by default) and attenuates by
  at most `min_gain` (−6.02 dB by default): tones below that bin pass as foreground, and louder tones are reduced
  rather than removed.
- Calibration assumes a quiet intro; speech during calibration degrades early enhancement for the measured 1–3 s
  described above.
