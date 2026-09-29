# How Go packets reach the Rust processor

Noise Oxydation processes 8 kHz mono audio carried as fixed 160-byte G.711 μ-law packets. A Go application creates one `bindings/go` `Processor` per call, reuses its packet and output buffers, pushes packets in order, and drains once. The Go wrapper owns the opaque handle; the Rust core owns every call's mutable signal state. Separate processors can run concurrently.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/images/how-it-works-night.svg">
  <img alt="The Go binding, Rust static C ABI, core processor, and ordered audio path" src="docs/images/how-it-works-day.svg">
</picture>

Open the [day SVG](docs/images/how-it-works-day.svg) or [night SVG](docs/images/how-it-works-night.svg) directly.

## Package and crate boundary

`bindings/go` presents the importable Go API without exposing cgo types. Its `Processor` calls the Rust static C ABI in process. `noise-oxydation-ffi` is the ABI owner and depends on `noise-oxydation-core`; the core depends on `realfft` for real-valued transforms. The core example target handles offline WAV access and measurements and is not another production crate.

Build `noise-oxydation-ffi` as a release static archive from the repository root before building the Go package. The cgo link paths use that checkout-local archive and support macOS and Linux. On Linux, cgo also links `dl`, pthread, and math; on macOS, it links math.

## Packet path

Each packet follows this ordered path from the Go method through Rust and back:

1. `bindings/go.Processor.Push` validates that the Go slice is exactly 160 bytes and passes the packet and reusable output buffer to the C ABI.
2. `noise-oxydation-ffi` copies the packet into Rust-owned fixed-size storage before passing it to the core processor.
3. The core decodes the 160 μ-law bytes to PCM and applies the 80 Hz high-pass filter.
4. It fills 256-sample Hann-windowed frames at a 128-sample hop, then runs the real FFT.
5. It computes original frame power, which feeds both the selected noise estimator and tonal-transient detector.
6. After calibration, it uses observed frame power and the selected noise PSD for decision-directed SNR and shared Log-MMSE suppression.
7. It applies tonal gain to the Log-MMSE spectrum, inverse-transforms, overlap-adds, and encodes ordered μ-law bytes back through the ABI to the caller's output buffer.

The estimator is fixed for a processor's call. The default minimum-noise estimator uses smoothing 0.8, a 50-frame minimum window, and a 1e-12 power floor. `Processor::with_estimator` selects MCRA or SPP-MMSE instead.

## Ordering and buffering

One processor handles packet pushes and frames in order. Independent `Processor` instances own separate mutable signal state and may run concurrently; the packet path does not synchronize shared signal state.

A complete first frame needs 256 samples, or 32 ms at 8 kHz. The fixed packet size makes that frame available on the second push. This is buffering arithmetic, not a measured end-to-end latency. A push writes zero to 256 ordered bytes. A final drain writes zero to 255 remaining valid samples and closes the stream. Reset discards any pending tail and starts a fresh call.

## Quiet-intro calibration

The default calibration interval is 40,000 samples, or five seconds. Durations are floored to 8 kHz samples. Only complete 256-sample windows wholly inside the interval train the estimator; the default contains 311 training windows. Frames ending within the interval bypass suppression. The first frame ending beyond it enters the shared suppression path, even if it crosses the boundary. Speech during the intro can affect the learned noise estimate and later be attenuated.

## Offline replay

The dependency-free replay example reads one attributed OpenSLR SLR31 utterance, mixes deterministic synthetic noise, and streams it through the public packet API. Its file access, metrics, timing, and allocation instrumentation stay outside initialized `Processor` calls. The recorded sample-stream metrics describe that one synthetic mix. Playback of both rendered files worked. The user reported a saturation-like voice artifact around “Bengal light” and chose to preserve post-calibration minimum-noise adaptation while deferring its correction to a later audio-quality task. This one-utterance report does not establish general speech quality, natural-noise robustness, end-to-end scheduling performance, or numerical parity with Go. See [docs/REPLAY.md](docs/REPLAY.md) for attribution, reproduction steps, and measurement limits.

## Go example and build

Build the Rust archive first with `cargo +1.98.1 build --release --locked --package noise-oxydation-ffi`, then run Go checks from `bindings/go` with cgo enabled. `go run ./examples/stream /path/to/input.ulaw` reads exact packet groups, pads only a partial final packet with μ-law silence, writes each produced range before continuing, and writes the final drained bytes once. It trims padded bytes so output length matches valid input length. Use a quiet intro with the default five-second calibration.

`go run ./cmd/measure /path/to/input.ulaw` prepends 40,000 bytes of μ-law silence and reports Go allocations per packet plus Push latency percentiles relative to 20 ms. It identifies the Go version, host, and input hash; one-host timings do not guarantee scheduling or audio quality. See [docs/REPLAY.md](docs/REPLAY.md) for the recorded input and result.
