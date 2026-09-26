# Architecture

## Product and crate boundary

One `noise-oxydation-pipeline::Pipeline` owns one 8 kHz mono call. The caller provides fixed 160-byte G.711 μ-law packets and receives owned, fixed-capacity packet batches. Independent calls use independent instances; there is no microphone, Twilio connection, UI, worker pool, or shared mutable DSP state.

The Rust 2024 workspace has three crates. `codec` performs μ-law conversion without dependencies. `dsp` owns the high-pass filter, frame analyzer, FFT, SPP noise estimator, gain, and synthesis without depending on codec or pipeline. `pipeline` depends on both and owns call lifecycle, packet conversion, and output buffering. The fixed 256-point FFT is implemented in `dsp`; this path needs no runtime dependency or FFT scratch allocation.

```text
telephony caller → pipeline → codec
                       └────→ dsp
```

## Audio flow and timing

Each sample is decoded and fed through a first-order high-pass filter. After the first 256 samples, every additional 128 samples produces one spectral frame. The analyzer uses a periodic Hann window. The suppressor estimates noise power from the quiet intro, then uses smoothed speech-presence probability for the conditional noise update, a decision-directed prior SNR, and a Log-MMSE gain. The selected interpretation uses the smoothed probability and noise-overestimation factor β = 1. These are Rust design choices, not claims of exact Go numerical parity. An inverse FFT, Hann synthesis window, and window-square normalization produce output samples.

During learning, frame output takes the original decoded PCM rather than the filtered or FFT-reconstructed signal. Only complete FFT windows wholly inside the configured interval update the noise estimate. Frames starting before the cutoff bypass suppression, including any that straddle it. With the default 40,000 samples, suppression begins with frame start 40,064 (5.008 seconds). This assumes a quiet call intro; speech during that interval can contaminate the estimate. The runtime duration changes the cutoff without changing storage.

A 160-sample input can complete zero, one, or two 128-sample DSP hops. `process_packet` returns only whole 160-sample output packets, so it may return none. The first full packet is observed on input packet three. An input packet contributes at most 256 output samples, and fewer than 160 may already be pending: at most two whole packets can be returned during processing. At `finish`, the DSP can lag by less than 256 samples and the output remainder is less than 160. Their sum is a multiple of 160 because every input packet contains 160 samples and valid output length equals input length; the sum is below 416, so it is at most 320. The stack-owned batch has two slots and reports the valid count of its last packet. The current final packet is whole. A second finish fails. `reset` clears the high-pass, analyzer, estimator, synthesis, packet remainder, and finished state.

The output sample sequence has the same valid length as the input sequence after `finish`. Tests cover short streams, packet timing, exact hop alignment, ordered sample counts, finish-once, and reset. An allocation-counting test observes zero allocations after construction through `process_packet` and `finish`. The path has no locks or atomics in production; callers should keep each instance on one processing owner. Transferring an instance between threads is the caller's policy.

## Evidence and remaining stages

Stable Rust builds, tests, and Clippy with pedantic warnings denied run in CI; nightly rustfmt checks the shared 2024 style. Synthetic tests check finite output, intro bypass, stationary-noise attenuation, and retained speech at both default and alternate learning intervals. The release packet benchmark measures a local scalar baseline; it is not a target-platform latency guarantee. The pinned [Go reference observations](docs/reference-observations.md) retain unresolved formula and flush behavior questions.

Later checkpoints must add MCRA, the documented minimum-noise estimator, tonal transient suppression, real-audio evidence, opt-in logging and performance tracing, scalar versus SIMD measurements, and final light/dark visual explanations. No current result establishes full Go pipeline parity.
