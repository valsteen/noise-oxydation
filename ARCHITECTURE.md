# Architecture

## Current packet path

One `noise-oxydation-core::Processor` owns one call's state. Its public entry point accepts one fixed-size G.711 μ-law packet and a caller-owned output buffer. The implementation decodes the packet, applies the 8 kHz high-pass filter, fills Hann-windowed 256-sample frames at a 128-sample hop, and transforms each frame. Frame power feeds the selected noise estimator and the tonal-transient detector. After calibration, observed power and the selected noise PSD feed decision-directed SNR and Log-MMSE; tonal gain then multiplies the Log-MMSE spectrum. The processor inverse-transforms, overlap-adds, and encodes ordered μ-law output.

A complete first frame needs 256 samples. At 8 kHz that is 32 ms, and the fixed 160-byte packet API makes the first frame available on the second push. Each push emits at most 256 bytes; final drain emits at most 255 bytes. Drain emits only valid samples and closes the stream. Reset intentionally discards the pending tail and resets all call state.

## Ownership and effects

The core crate owns packet framing, codec conversion, filter state, the fixed per-processor estimator choice and history, decision-directed SNR, Log-MMSE and tonal state, transform plans and scratch buffers, calibration, overlap buffers, stream counters, and lifecycle errors. It depends on `realfft` for real-valued transforms. No application host, transport, file access, logging, or cross-call state enters the packet path.

Processor construction may allocate while it prepares transform plans and buffers. After construction, push and drain use fixed-size state and caller-owned output storage. They perform no file or network I/O, wait on no synchronization, and allocate no memory. Independent calls use independent processors and share no mutable signal state.

The standard-library `examples/replay.rs` owner handles offline WAV parsing and writing, PCM-to-μ-law conversion, deterministic noise generation, source/output alignment, metrics, and optional timing and allocation instrumentation. It processes the same public 160-byte packet API as a caller, in order, and drains once; all diagnostics and file effects remain outside initialized `Processor` calls. Its outputs live in a newly created directory under the OS temporary root, and it rejects a temporary root that resolves inside the Git checkout. The optional `performance-analysis` Cargo feature instruments the example only and adds no runtime dependency or processor callback.

The caller owns packet order, output storage, processor lifetime, and the choice of quiet-intro duration. A capacity error reports the exact requirement before input or state is changed. The public lifecycle rejects pushes after drain, returns an empty result on repeated drain, and lets reset begin a new call while deliberately dropping any unreturned tail.

## Calibration and estimator selection

`Processor::with_quiet_intro` floors its `Duration` to 8 kHz samples; the default interval is exactly 40,000 samples. Only complete 256-sample windows wholly within the interval train the selected estimator. The default minimum-noise estimator uses the README-derived smoothing of 0.8, 50-frame minimum window, and 1e-12 floor; MCRA and SPP-MMSE are selectable alternatives with parameters recorded in `docs/REFERENCE_NOTES.md`. The default interval contains 311 complete training windows. Frames ending within the interval bypass suppression. The first frame ending beyond it enters the shared path, even when it crosses the configured boundary. Reset clears temporal state and restarts calibration while retaining the estimator choice and configured duration. Calibration assumes a quiet intro: speech in that interval can affect the estimate and later be attenuated.

## Current boundary

The workspace contains one production crate with selectable minimum-noise, MCRA, and SPP-MMSE estimation followed by shared decision-directed Log-MMSE and tonal-transient suppression. Offline replay and host-specific performance evidence are provided by a dependency-free example, not by the installed library. The replay covers one attributed speech utterance with synthetic noise; it does not establish general speech quality, natural-noise robustness, end-to-end scheduling performance, or numerical parity with the pinned Go implementation. SIMD, tracing, and application or Go integration remain outside the current implementation.
