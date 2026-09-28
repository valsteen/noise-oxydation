# Noise Oxydation Core

This crate enhances an 8 kHz stream carried as G.711 μ-law packets. A caller creates one `Processor` per call, reuses its input and output storage, pushes packets in order, and drains once the stream ends.

## Processing path

The processor decodes each 160-byte packet to PCM, applies an 80 Hz high-pass filter, and analyzes 256-sample Hann windows with a 128-sample hop. Frame power feeds one selected noise estimator and the tonal-transient detector. After the quiet intro, observed power and the selected noise estimate feed decision-directed SNR and Log-MMSE suppression; the detector then applies its gain to the suppressed spectrum. Inverse transforms are overlap-added, and the result is encoded back to μ-law bytes.

The default estimator is the README-specified minimum-noise moving-window estimator with smoothing 0.8, a 50-frame minimum window, and a 1e-12 power floor. `Processor::with_estimator` selects MCRA or SPP-MMSE instead. A selection remains fixed for the processor's call.

The first complete analysis window requires 256 samples, or 32 ms at 8 kHz. The first output becomes available on the second 160-byte packet push. This is buffering arithmetic, not a measured end-to-end latency.

## Packet API

`Processor::push_packet` accepts exactly 160 μ-law bytes and writes zero to 256 ordered μ-law sample bytes into caller-owned storage. Output length varies with the current frame phase. A buffer that is too small returns `ProcessorError::OutputTooSmall` with the exact required capacity and leaves processor state unchanged.

`Processor::drain` writes zero to 255 remaining valid samples, then closes the stream. A repeated drain returns zero. Pushing after drain returns `ProcessorError::StreamDrained`. `Processor::reset` discards a pending tail and starts a fresh stream, including filter and estimator history. It retains the selected estimator and quiet-intro duration.

## Calibration

The default quiet intro is exactly 40,000 samples (five seconds). `Processor::with_quiet_intro` changes the duration while keeping the default estimator; `Processor::with_estimator_and_quiet_intro` selects both. Durations are floored to 8 kHz samples. Only complete 256-sample windows wholly inside the interval train the selected estimator, so the default contains 311 training windows. Frames ending within the interval bypass suppression; the first frame ending beyond it enters the shared suppression path. Speech during calibration can affect the learned estimate and later be attenuated; provide a quiet intro when possible.

## Resource behavior

Create the processor before entering the packet-processing path. Construction prepares FFT plans and reusable buffers and may allocate. Initialized pushes and drains use bounded preallocated storage without blocking or allocating. Each processor owns its mutable signal state; use separate processors for independent calls.

## Checks

The repository pins stable Rust 1.98.1 for build, test, and Clippy. Rust formatting uses nightly-2026-09-25 with the settings recorded in `rustfmt.toml`.

```sh
cargo +nightly-2026-09-25 fmt --all -- --check
cargo +1.98.1 clippy --workspace --all-targets --locked -- -D warnings -D clippy::pedantic
cargo +1.98.1 test --workspace --locked
cargo +1.98.1 build --workspace --locked
```

The noise estimators and suppression stages are implementation evidence, not a claim of Go numerical parity or measured speech quality. Real-speech replay and quality evidence remain later accepted work; Go integration remains separate.
