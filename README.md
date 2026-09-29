# Noise Oxydation

Noise Oxydation enhances 8 kHz mono audio carried as G.711 μ-law packets. The Rust `Processor` owns one call's signal state. Go applications use `bindings/go` to send the same ordered packets through a Rust static C ABI while reusing input and output storage, then drain once the stream ends.

Follow the [How It Works guide](HOW_IT_WORKS.md) for the crate boundary, packet path, and per-call sequencing.

## Processing path

The processor decodes each 160-byte packet to PCM, applies an 80 Hz high-pass filter, and analyzes 256-sample Hann windows with a 128-sample hop. Frame power feeds one selected noise estimator and the tonal-transient detector. After the quiet intro, observed power and the selected noise estimate feed decision-directed SNR and Log-MMSE suppression; the detector then applies its gain to the suppressed spectrum. Inverse transforms are overlap-added, and the result is encoded back to μ-law bytes.

The default estimator is the README-specified minimum-noise moving-window estimator with smoothing 0.8, a 50-frame minimum window, and a 1e-12 power floor. `Processor::with_estimator` selects MCRA or SPP-MMSE instead. A selection remains fixed for the processor's call.

The first complete analysis window requires 256 samples, or 32 ms at 8 kHz. The first output becomes available on the second 160-byte packet push. This is buffering arithmetic, not a measured end-to-end latency.

## Packet API

`Processor::push_packet` accepts exactly 160 μ-law bytes and writes zero to 256 ordered μ-law sample bytes into caller-owned storage. Output length varies with the current frame phase. A buffer that is too small returns `ProcessorError::OutputTooSmall` with the exact required capacity and leaves processor state unchanged.

`Processor::drain` writes zero to 255 remaining valid samples, then closes the stream. A repeated drain returns zero. Pushing after drain returns `ProcessorError::StreamDrained`. `Processor::reset` discards a pending tail and starts a fresh stream, including filter and estimator history. It retains the selected estimator and quiet-intro duration.

## Go integration

The package import path is `github.com/valsteen/noise-oxydation/bindings/go`. It exposes `New`, `Push`, `Drain`, `Reset`, and `Close`; it keeps the C handle private and returns typed errors, including the exact output capacity needed for a retry. For each call, create one processor, push 160-byte packets in order with reusable buffers, write each produced byte range before continuing, and write the single drained tail last. A processor is used by one goroutine; independent processors may run concurrently.

Build the static archive from the repository root before building or testing the Go module. The cgo package links that archive from the source checkout on macOS and Linux.

```sh
cargo +1.98.1 build --release --locked --package noise-oxydation-ffi
cd bindings/go
CGO_ENABLED=1 go test ./...
CGO_ENABLED=1 go build ./...
```

Run the complete raw μ-law streaming example with `go run ./examples/stream /path/to/input.ulaw`. It reuses a 160-byte input packet and 256-byte output buffer, pads only the last partial input packet with μ-law silence, writes output in order, and trims that padding from the final stream. Provide a quiet intro when using the default five-second calibration with speech.

The measurement command accepts an external raw μ-law sample and reports Go allocations per packet, Push latency percentiles, Drain latency, host, Go version, and sample hash against the 20 ms packet cadence. Convert the documented clean replay with FFmpeg as follows:

```sh
ffmpeg -i /path/to/clean.wav -ar 8000 -ac 1 -c:a pcm_mulaw -f mulaw /path/to/clean.ulaw
go run ./cmd/measure /path/to/clean.ulaw
```

The command prepends the default 40,000-byte quiet calibration interval and pads its final packet with μ-law silence. The measurement on one host is call-level evidence for that sample; it is not an end-to-end scheduling or audio-quality guarantee. See [`docs/REPLAY.md`](docs/REPLAY.md) for the recorded run and [`docs/REFERENCE_NOTES.md`](docs/REFERENCE_NOTES.md) for the selected Go boundary.

## Calibration

The default quiet intro is exactly 40,000 samples (five seconds). `Processor::with_quiet_intro` changes the duration while keeping the default estimator; `Processor::with_estimator_and_quiet_intro` selects both. Durations are floored to 8 kHz samples. Only complete 256-sample windows wholly inside the interval train the selected estimator, so the default contains 311 training windows. Frames ending within the interval bypass suppression; the first frame ending beyond it enters the shared suppression path. Speech during calibration can affect the learned estimate and later be attenuated; provide a quiet intro when possible.

## Resource behavior

Create the Rust processor before entering the packet-processing path. Construction prepares FFT plans and reusable buffers and may allocate. Initialized Rust pushes and drains use bounded preallocated storage without blocking or allocating. Each processor owns its mutable signal state; use separate processors for independent calls. The cgo wrapper has separate Go heap-allocation evidence recorded under Go integration; it does not change the Rust core's allocator behavior.

## Offline replay

The `noise-oxydation-core` replay example reads a PCM16 mono 8 kHz WAV converted from the attributed OpenSLR SLR31 utterance documented in [`docs/REPLAY.md`](docs/REPLAY.md). It generates a fixed-seed synthetic-noise prefix and aligned noisy speech, then sends ordered 160-byte μ-law packets through the public default `Processor` and drains once. The example writes noisy and enhanced WAVs under the OS temporary directory and prints input and output speech metrics. Source and rendered audio remain outside the checkout. File access, logging, timing, allocation counting and metrics stay in the example, outside initialized `push_packet` and `drain`.

Run ordinary replay with `cargo +1.98.1 run --release --locked --package noise-oxydation-core --example replay -- /path/to/clean.wav`; add `--quiet` after the WAV path to suppress progress messages. The opt-in `performance-analysis` feature adds `--profile` for release push/drain timing and a separate allocation-counting pass. The one-sample metrics and host-specific timings in the replay guide are evidence for this synthetic mix and host, not general speech-quality or real-time guarantees.

## Checks

The repository pins stable Rust 1.98.1 for build, test, and Clippy. Rust formatting uses nightly-2026-09-25 with the settings recorded in `rustfmt.toml`.

```sh
cargo +nightly-2026-09-25 fmt --all -- --check
cargo +1.98.1 clippy --workspace --all-targets --locked -- -D warnings -D clippy::pedantic
cargo +1.98.1 test --workspace --locked
cargo +1.98.1 build --workspace --locked
```

The noise estimators and suppression stages are implementation evidence, not a claim of Go numerical parity. The offline replay provides one attributed utterance's synthetic-noise metrics; it does not establish general speech quality, natural-noise robustness, or numerical parity with Go. The Go latency observation is one-host call timing and does not establish scheduling behavior on another machine.
