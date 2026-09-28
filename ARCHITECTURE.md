# Architecture

## Current packet path

One `noise-oxydation-core::Processor` owns one call's state. Its public entry point accepts one fixed-size G.711 μ-law packet and a caller-owned output buffer. The implementation decodes the packet, applies the 8 kHz high-pass filter, fills Hann-windowed 256-sample frames at a 128-sample hop, transforms each frame, updates or applies the per-bin minimum-noise gate, inverse-transforms it, overlap-adds the result, and encodes ordered μ-law output.

A complete first frame needs 256 samples. At 8 kHz that is 32 ms, and the fixed 160-byte packet API makes the first frame available on the second push. Each push emits at most 256 bytes; final drain emits at most 255 bytes. Drain emits only valid samples and closes the stream. Reset intentionally discards the pending tail and resets all call state.

## Ownership and effects

The core crate owns packet framing, codec conversion, filter state, transform plans and scratch buffers, the calibration estimate, overlap buffers, stream counters, and lifecycle errors. It depends on `realfft` for real-valued transforms. No application host, transport, file access, logging, or cross-call state enters the packet path.

Processor construction may allocate while it prepares transform plans and buffers. After construction, push and drain use fixed-size state and caller-owned output storage. They perform no file or network I/O, wait on no synchronization, and allocate no memory. Independent calls use independent processors and share no mutable signal state.

The caller owns packet order, output storage, processor lifetime, and the choice of quiet-intro duration. A capacity error reports the exact requirement before input or state is changed. The public lifecycle rejects pushes after drain, returns an empty result on repeated drain, and lets reset begin a new call while deliberately dropping any unreturned tail.

## Calibration and gate

The default calibration interval is exactly 40,000 samples. Only complete 256-sample windows wholly within the configured interval contribute to the per-bin minimum-power estimate. The gate applies after calibration and attenuates bins below twice the estimated minimum power to 10% magnitude. Calibration assumes a quiet intro: speech in that interval can affect the estimate and later be attenuated.

## Current boundary

The workspace contains one production crate and one baseline gate. Additional estimators and suppressors, real-speech replay and quality evidence, performance analysis, vectorization, and application or Go integration are outside this crate's current packet path and need their accepted follow-on work. The Rust API preserves valid input sample cardinality; no numerical parity with the pinned Go implementation is claimed.
