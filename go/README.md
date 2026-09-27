# Try it from Go

The fastest check is to replay one of your own calls through both modes. From a checkout of this repository, with Rust stable, Go 1.23+, and a C linker available, run:

```sh
sh go/try.sh /path/to/one-call.mulaw /tmp/noise-try-001
```

The input is **raw, concatenated 160-byte G.711 μ-law packets** from one 8 kHz mono call: base64 decode any transport payload first, then append each packet in arrival order. Do not include JSON, RTP headers, or a WAV header. Capture from the start of the call; the default first five seconds must be quiet enough to learn noise. The output directory must not already exist. The command builds the Rust static library, links the Go example, and writes `conservative.mulaw` and `experimental.mulaw` with exactly the input's valid sample count. Feed either result into your existing μ-law playback or transport path to compare. If FFmpeg is already installed, `ffmpeg -f mulaw -ar 8000 -ac 1 -i /tmp/noise-try-001/conservative.mulaw /tmp/noise-try-001/conservative.wav` makes a WAV for listening; the project does not require FFmpeg.

This is a local trial, not a claim of source compatibility. The [pinned Go reference](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/processor.go) exports `dsp.Processor` with `Process([]float32) ([]float32, error)` and `Reset()`. Its README sketches a complete `pipeline.New`, but that package is absent from the pinned source. Your colleague's application decides where those stages are assembled. The two paths below cover the usual PCM call loop and the more efficient μ-law packet boundary; the exact edit depends on that call site.

## Closest swap: 160 normalized PCM samples

If the existing Go call loop already gives the enhancement pipeline **160 normalized `float32` samples per call**, replace the construction of the whole enhancement chain with `noise.NewPCMProcessor`. The adapter implements the reference `dsp.Processor` method shape, so the processing call remains `processor.Process(input)`. Remove the old enhancement stages from that call to avoid processing the audio twice. Add `Finish` at call end to recover delayed samples, and `Close` to release the native handle:

```go
import noise "github.com/valsteen/noise-oxydation/go"

config := noise.DefaultConfig() // conservative
processor, err := noise.NewPCMProcessor(config)
if err != nil { return err }
defer processor.Close()

// Inside the existing call loop, where pcm160 is one []float32 packet:
enhanced, err := processor.Process(pcm160)
if err != nil { return err }
if err := sendEnhancedPCM(enhanced); err != nil { return err }

// At the existing end-of-call point:
tail, err := processor.Finish()
if err != nil { return err }
if err := sendEnhancedPCM(tail); err != nil { return err }
```

`pcm160` and `sendEnhancedPCM` are your application's existing packet and consumer. `Process` returns an owned slice containing zero, 160, or 320 samples; treat successive results and the tail as one ordered audio stream. For the experimental mode, set `config.Mode = noise.ExperimentalLowDelay` before construction. `Reset()` keeps the chosen mode and satisfies the original interface. This route converts normalized PCM to μ-law and back and allocates an owned output slice, so it is a compatibility path. It accepts exactly 160 samples per call and is not a drop-in for a differently sized or non-telephony PCM stream.

## Faster swap: keep the μ-law packet boundary

If the application has the **160 decoded μ-law bytes before it converts them to PCM**, call `noise.New(config)` there. Pass one packet to `Call.Process`, then send every returned packet in order using the last packet's `FinalValid` count. Call `Finish` once when the stream ends and send its returned packets too. This keeps Go-side μ-law conversion and output-slice allocation out of the packet loop. The runnable [`cmd/replay` example](cmd/replay/main.go) shows the complete batch handling and both `-mode` values. The [mode comparison](../docs/processing-modes.md) documents the experimental quality and CPU tradeoff. Neither mode promises hard real-time Go scheduling.

Import `github.com/valsteen/noise-oxydation/go` for either path. The Go package hides cgo, owns one native handle per `Call`, and serializes each call's `Process`, `Finish`, `Reset`, and `Close` methods. Separate calls can run concurrently. `Close` is safe against a concurrent `Process` and is idempotent. Keep packet processing away from a timing-sensitive Go audio callback: Go, cgo, and the OS scheduler provide no hard real-time guarantee.

## Build and link

Rust 1.98 or newer, Go 1.23 or newer, a C linker, and `CGO_ENABLED=1` are required. Build a static library for the same OS and architecture as the Go program. The archive is an explicit local prerequisite; it is not committed or downloaded by the Go module.

From the repository root:

```sh
cargo build --release --locked -p noise-oxydation-ffi
export NOISE_ROOT="$(pwd)"
export CGO_ENABLED=1
```

On macOS:

```sh
export CGO_LDFLAGS="$NOISE_ROOT/target/release/libnoise_oxydation_ffi.a"
```

On Linux:

```sh
export CGO_LDFLAGS="$NOISE_ROOT/target/release/libnoise_oxydation_ffi.a -ldl -lpthread -lm"
```

The native archive must be passed through `CGO_LDFLAGS` for **every** Go build, test, or run that imports the package. If cross-compiling, build the Rust archive for the target and supply a matching C linker.

To verify import and link from a separate local Go module without fetching this unpublished module, run `sh go/integration.sh` from the repository root. It creates a temporary external consumer, tests both Go APIs and both modes, then runs the replay command on a deterministic packet-aligned stream. In your existing Go application's module, add the local replacement:

```sh
cd /path/to/your-go-app
go mod edit -require=github.com/valsteen/noise-oxydation/go@v0.0.0
go mod edit -replace=github.com/valsteen/noise-oxydation/go="$NOISE_ROOT/go"
```

Then import `github.com/valsteen/noise-oxydation/go`, with the same `CGO_LDFLAGS` set, and run `go run .` or `go test ./...`. Once a version of the Go module is published, consumers can select that version in place of the local replacement; they still build and supply the matching native archive.

## Process a prepared call

`DefaultConfig()` selects a five-second quiet intro and SPP-MMSE. Set `Config.LearningDuration` and `Config.Estimator` to choose another duration and `Mcra` or `Minimum`. Zero duration and intervals shorter than the selected mode's learning minimum return typed `*noiseoxydation.Error` values; `StatusDSPInit` retains the supplied and minimum sample counts. The minimum is 256 samples (32 ms) for conservative mode and 320 samples (40 ms) for experimental mode. The call consumes exactly 160 bytes at a time and writes zero to two ordered packets into a caller-owned 320-byte array. Earlier returned packets contain 160 valid bytes; the last has `Batch.FinalValid` valid bytes. Call `Finish` once to drain the stream, then `Reset` before a new stream or `Close` to release it. A finished or closed call returns a typed error.

`Config.Mode` selects `Conservative` (the zero-value default) or `ExperimentalLowDelay` once per call. Both are present in the same Rust archive and Go binary. The experimental mode emits its first complete packet with input packet two instead of three, but changes audio quality and increases frame processing. Keep the conservative default for production until the [mode comparison and risks](../docs/processing-modes.md) fit your call conditions.

`cmd/replay` uses buffered input and output and reuses one input packet and one output batch. It verifies that valid output bytes equal input bytes. With a prepared stream:

```sh
cd "$NOISE_ROOT/go"
go run ./cmd/replay -input /tmp/prepared.mulaw -output /tmp/enhanced.mulaw -estimator mcra
go run ./cmd/replay -input /tmp/prepared.mulaw -output /tmp/experimental.mulaw -estimator mcra -mode experimental
```

For a realistic call, download the [credited OSR 8 kHz mono WAV](https://www.voiptroubleshooter.com/open_speech/american/OSR_us_000_0010_8k.wav) outside Git and verify SHA-256 `a4bf9becd046d7aedb6d05b6e12347a6294a44f74d263089c636fb0a2b1e6561`. Prepare it with the existing Rust replay tool:

```sh
cd "$NOISE_ROOT"
cargo run --release --locked -p noise-oxydation-pipeline --features performance-analysis --example wav_replay -- /tmp/OSR_us_000_0010_8k.wav /tmp/prepared.mulaw
shasum -a 256 /tmp/prepared.mulaw
```

The expected prepared hash is `ac50c854bbc0500ee478376a1bc74e339d53257170974685403a1674430a082f`; it contains 317,120 bytes of five-second quiet-intro noise, another second of noise, and noisy recorded speech. On Linux, use `sha256sum` instead of `shasum`. The [preparation details](../docs/real-audio-evidence.md) describe the exact seed, windows, and limits. Neither audio file is committed.

## Packet latency and allocations

After preparing and hashing the same full-call stream, run the reproducible Go/cgo benchmark with the release staticlib and `CGO_LDFLAGS` above:

```sh
cd "$NOISE_ROOT/go"
go run ./cmd/bench -input /tmp/prepared.mulaw
go run ./cmd/bench -input /tmp/prepared.mulaw -mode experimental
```

The benchmark reads the file before timing, warms a full call for each estimator, resets, then times only `Call.Process` per packet. It counts valid bytes including `Finish`, reports nearest-rank p50/p95 over all packets, and measures Go malloc count across the measured call. It excludes file I/O and construction; the malloc count includes Go runtime activity during the call and can vary. It does not measure scheduling delay before the call.

On VincentacStudio (macOS arm64, Darwin 24.5.0, Rust 1.98.1 release staticlib, Go 1.27.1), the hash-verified 317,120-byte OSR-derived input produced 1,982 packets with five-second intro:

| Estimator | Process p50 | Process p95 | Go mallocs per call | Valid bytes |
| --- | ---: | ---: | ---: | ---: |
| SPP-MMSE | 21.708 µs | 43.792 µs | 0 | 317,120 |
| MCRA | 22.625 µs | 45.375 µs | 0 | 317,120 |
| Minimum | 23.625 µs | 48.333 µs | 0 | 317,120 |

These observed p95 values are below the 20 ms packet cadence on this host. They are sample distributions, not worst-case bounds or a portable latency promise. The Go mutex protects `Close` against use-after-free; cgo and the Go/OS schedulers can still add unmeasured delay in a live call.

The Go binding follows the Rust pipeline's packet behavior. The pinned Go reference has no shipped top-level MCRA pipeline, so neither source/API compatibility nor exact numerical parity with it is claimed.
