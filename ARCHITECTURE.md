# Architecture

## Current packet path

One `noise-oxydation-core::Processor` owns one call's state. Its public entry point accepts one fixed-size G.711 μ-law packet and a caller-owned output buffer. The implementation decodes the packet, applies the 8 kHz high-pass filter, fills Hann-windowed 256-sample frames at a 128-sample hop, and transforms each frame. Frame power feeds the selected noise estimator and the tonal-transient detector. After calibration, observed power and the selected noise PSD feed decision-directed SNR and Log-MMSE; tonal gain then multiplies the Log-MMSE spectrum. The processor inverse-transforms, overlap-adds, and encodes ordered μ-law output.

A complete first frame needs 256 samples. At 8 kHz that is 32 ms, and the fixed 160-byte packet API makes the first frame available on the second push. Each push emits at most 256 bytes; final drain emits at most 255 bytes. Drain emits only valid samples and closes the stream. Reset intentionally discards the pending tail and resets all call state.

## Ownership and effects

The core crate owns packet framing, codec conversion, filter state, the fixed per-processor estimator choice and history, decision-directed SNR, Log-MMSE and tonal state, transform plans and scratch buffers, calibration, overlap buffers, stream counters, and lifecycle errors. It depends on `realfft` for real-valued transforms. No application host, transport, file access, logging, or cross-call state enters the packet path.

Processor construction may allocate while it prepares transform plans and buffers. After construction, push and drain use fixed-size state and caller-owned output storage. They perform no file or network I/O, wait on no synchronization, and allocate no memory. Independent calls use independent processors and share no mutable signal state.

The caller owns packet order, output storage, processor lifetime, and the choice of quiet-intro duration. A capacity error reports the exact requirement before input or state is changed. The public lifecycle rejects pushes after drain, returns an empty result on repeated drain, and lets reset begin a new call while deliberately dropping any unreturned tail.

## Calibration and estimator selection

`Processor::with_quiet_intro` floors its `Duration` to 8 kHz samples; the default interval is exactly 40,000 samples. Only complete 256-sample windows wholly within the interval train the selected estimator. The default minimum-noise estimator uses the README-derived smoothing of 0.8, 50-frame minimum window, and 1e-12 floor; MCRA and SPP-MMSE are selectable alternatives with parameters recorded in `docs/REFERENCE_NOTES.md`. The default interval contains 311 complete training windows. Frames ending within the interval bypass suppression. The first frame ending beyond it enters the shared path, even when it crosses the configured boundary. Reset clears temporal state and restarts calibration while retaining the estimator choice and configured duration. Calibration assumes a quiet intro: speech in that interval can affect the estimate and later be attenuated.

## Current boundary

The workspace contains one production crate with selectable minimum-noise, MCRA, and SPP-MMSE estimation followed by shared decision-directed Log-MMSE and tonal-transient suppression. Real-speech replay and quality evidence, performance analysis, vectorization, and application or Go integration remain separate accepted work. The Rust API preserves valid input sample cardinality; no numerical parity with the pinned Go implementation is claimed.
