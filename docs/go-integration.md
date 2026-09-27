# Go Integration

A Go program uses the enhancer through the Go package `noiseox`, module `github.com/valsteen/noise-oxydation/go`
in [`go/`](../go). The package links the Rust library as a static library through a small C ABI crate,
[`crates/bindings/noise-oxydation-capi`](../crates/bindings/noise-oxydation-capi), and hides every cgo detail: a Go
caller sees Go types, one `Call` per telephone call, typed errors, and packet methods that allocate nothing. The output
is byte-identical to the Rust library's.

This page explains how the reference Go project is meant to be consumed and how this package relates to it, why the
boundary is a static C ABI, the Go API with its errors, lifecycle and concurrency rules, the exact macOS and Linux build
and link steps, the call-stream example, and what the Go side costs. The binding rules themselves are recorded in
[ARCHITECTURE.md](../ARCHITECTURE.md#go-integration-boundary).

## How The Reference Is Consumed

The Go reference, [sghaida/noise-cancelation](https://github.com/sghaida/noise-cancelation), is a library of ordinary Go
packages (μ-law codec, high-pass filter, STFT, noise estimators, SNR, suppressor, interference detector). Its README
tells an application to `go get` the module, create one stateful processor per call, pass every incoming chunk to it,
forward whatever it returns, prefer a reusable destination buffer so that the steady state allocates nothing, and reset
the processor between calls. The processor itself is not shipped at the pinned revision; the end-to-end benchmark
composes the packages by hand, one call per goroutine ([reference-log.md](reference-log.md#r15-the-go-package-is-the-per-call-processor-the-readme-describes)).

`noiseox` keeps that consumption model and supplies the missing processor, backed by the Rust implementation:

| Reference model | `noiseox` |
| --- | --- |
| `go get` the module, import its packages | `go get` (or `replace`) the module, import `noiseox`; link the static library once |
| One stateful processor per call | One `*noiseox.Call` per call, created with `noiseox.New(config)` |
| `Process(dst, src)` returns 0, 128 or 256 float samples | `ProcessPacket(&in, &out)` takes one 160-byte μ-law packet and writes one packet after the two-packet delay |
| Reusable destination buffer for zero allocations | Caller-owned fixed arrays; the Rust side writes into them |
| Flush at the end of the call | `Drain(&tail)` returns the withheld packets; output length equals input length |
| Reset between calls | `Reset()` on the same `Call`; `Close()` when done |
| One goroutine per call | One goroutine per `Call`; calls share nothing |

## Boundary Choice

The Go service reaches Rust through a C ABI crate built as a static library and linked by cgo, wrapped by a Go package
that hides cgo. The alternatives were weighed against the 20 ms packet cadence, the zero-allocation packet path and
deployment:

| Option | Assessment |
| --- | --- |
| Static C ABI library linked through cgo (chosen) | One self-contained Go binary, no runtime library path, about 28 ns per call crossing, and the caller's arrays are passed by pointer without copies |
| Rewrite in Go | Duplicates the implementation and loses the measured Rust behavior and parity evidence |
| Subprocess or socket service | Adds IPC latency and a second process lifecycle per call for every 20 ms packet |
| WebAssembly in a Go runtime (for example wazero) | Pure Go, but slower, a runtime dependency, and a copy into and out of guest memory per packet |
| Rust `cdylib` | Works, but the Go binary then depends on a shared library found at run time |

The C ABI crate mirrors the Rust per-call API (create, process one packet, drain, reset, phase, free) and is the only
crate whose code handles raw pointers. Its `unsafe` blocks each state their safety argument, and the crate denies
undocumented `unsafe` blocks. The library itself keeps `#![forbid(unsafe_code)]` and is used without its default `log`
feature, so the static library carries no logging dependency. [`go/noise_oxydation.h`](../go/noise_oxydation.h) is
the single C declaration of the ABI; the Rust crate pins the same sizes, offsets and constants at compile time and its
tests compare them with the header, whose `_Static_assert`s make the C compiler check the other side.

## Build And Link

Requirements: the Rust toolchain from `rust-toolchain.toml` (installed by `rustup` on first use), Go 1.24 or later
(verified with Go 1.27), and a C toolchain for cgo: the Xcode Command Line Tools on macOS, or GCC with the glibc
development files on Linux. macOS on Apple silicon and Linux x86-64 are the verified platforms.

Always build the static library for its own package. A workspace-wide build unifies features and would link the
library's `log` feature into it.

```bash
cargo build --locked --release -p noise-oxydation-capi    # writes target/release/libnoise_oxydation_capi.a
```

The package's cgo directives link `-lnoise_oxydation_capi` plus the system libraries that
`rustc --print native-static-libs` lists for the target, so a consumer only has to say where the library is:

| Platform | Libraries the package links |
| --- | --- |
| macOS | `-lc -lm` (rustc lists `-lSystem -lc -lm`; Go's linker already passes `-lSystem`) |
| Linux (glibc) | `-lgcc_s -lutil -lrt -lpthread -lm -ldl -lc` |

The Linux list was read on macOS with `rustup target add x86_64-unknown-linux-gnu` and
`cargo rustc --release --target x86_64-unknown-linux-gnu -p noise-oxydation-capi --crate-type staticlib -- --print
native-static-libs`, and the hosted Linux CI job links with it.

### Inside the repository

The package searches `${SRCDIR}/../target/release`, so after the build above the tests and the example work directly:

```bash
cd go
go test -race ./...
go run ./examples/callstream -synthetic 3000
```

### From another Go program

The code is published on the `claude` branch of the public repository `github.com/valsteen/noise-oxydation`. The
shortest path uses a checkout of that branch and a `replace` directive. It works the same on macOS and Linux:

```bash
git clone --branch claude https://github.com/valsteen/noise-oxydation.git
cd noise-oxydation
cargo build --locked --release -p noise-oxydation-capi
cd ../your-service
go mod edit -require=github.com/valsteen/noise-oxydation/go@v0.0.0 \
  -replace=github.com/valsteen/noise-oxydation/go=../noise-oxydation/go
CGO_ENABLED=1 go build ./...
```

With the replace directive the package finds the library in the checkout's `target/release` by itself. CI builds a
separate consumer module against a copy of the package placed outside the checkout, where that built-in path finds
nothing and only `CGO_LDFLAGS`, set as below, locates the library: the same situation as a module in the module cache.

To fetch the module with `go get` instead, name the `claude` branch, where this code is published, or a commit on it.
The module cache holds only the Go sources, so the static library still comes from a checkout of the same commit, and
`CGO_LDFLAGS` points at it:

```bash
go get github.com/valsteen/noise-oxydation/go@claude   # or @<commit>
CGO_ENABLED=1 CGO_LDFLAGS="-L/path/to/noise-oxydation/target/release" go build ./...
```

Build the library from the commit the module version names: the header in the module and the library must describe
the same ABI. On macOS the linker then warns that the package's default search path inside the module cache does not
exist; the warning is harmless.

`CGO_ENABLED=1` is the default for native builds when a C compiler is found; it is spelled out because cgo is required.
Cross-compiling needs the static library built for the target (`cargo build --target ...`) and a C cross toolchain;
it is not covered here.

## API

```go
import noiseox "github.com/valsteen/noise-oxydation/go"

func DefaultConfig() Config
func New(config Config) (*Call, error)
func (c *Call) ProcessPacket(in, out *[PacketSize]byte) (emitted bool, err error)
func (c *Call) Drain(out *[DelayPackets][PacketSize]byte) (int, error)
func (c *Call) Reset() error
func (c *Call) Phase() (Phase, error)
func (c *Call) Close() error
```

- `PacketSize` (160) and `DelayPackets` (2) are constants. `Phase` is `PhaseCalibrating`, `PhaseEnhancing` or
  `PhaseDrained`.
- `Config` mirrors the Rust `CallConfig` field by field: `CalibrationDuration` as a `time.Duration`, `HighPass`, the
  `NoiseEstimator` selector (`NoiseEstimatorSppMmse`, `NoiseEstimatorMcra`, `NoiseEstimatorMinimum`) with the
  parameters of all three estimators in `SppMmse`, `Mcra` and `Minimum`, `DecisionDirected`, `LogMmse`, and the
  `Interference` selector (`InterferenceTonalTransient`, `InterferenceDisabled`) with `TonalTransient`. Only the
  selected parameters are validated. Units, ranges and defaults are those of [algorithms.md](algorithms.md).
- `DefaultConfig` comes from the Rust `Default` implementations through the C ABI, so the defaults have one source.
  Start from it: the zero `Config` is invalid.
- No cgo type appears in the exported API.

## Errors

| Condition | Go error | Match with |
| --- | --- | --- |
| Invalid configuration | `*ConfigError` | `errors.As(err, &cfgErr)`; `errors.Is(err, ErrInvalidConfig)` |
| Packet or drain after `Drain`, until `Reset` | `ErrDrained` | `errors.Is` |
| Any method after `Close` | `ErrClosed` (Rust is not called) | `errors.Is` |
| A nil packet array | `ErrNilArgument` | `errors.Is` |
| A panic inside Rust | `ErrPanic`; the `Call` answers every later method with it and should be closed | `errors.Is` |

`ConfigError.Kind` names the Rust `ConfigError` variant, and the other fields carry all of its data:

| Kind | Fields set |
| --- | --- |
| `KindOutOfRange` | `Field`, `Value` (may be NaN), `Constraint`, `Bounds` |
| `KindMinGainAboveMaxGain` | `Value` (min gain), `SecondValue` (max gain) |
| `KindCountOutOfRange` | `Field`, `Count`, `CountMin`, `CountMax` |
| `KindStartNotBelowFull` | `Field` and `Value` of the start threshold, `SecondField` and `SecondValue` of the full one |
| `KindCalibrationTooLong` | `CalibrationSeconds`, `CalibrationNanos` (unreachable from Go: no `time.Duration` is that long) |
| `KindUnknownSelector` | `Selector`, `SelectorValue` |
| `KindNegativeCalibrationDuration` | `CalibrationDuration`, checked in Go before Rust is called |
| `KindOther` | a Rust variant this version of the binding does not describe |

`ConfigField` values such as `FieldSppMmseSpeechPrior` name every validated field; `String` returns the Rust name
(`spp_mmse.speech_prior`), and `Constraint.String` the Rust description (`in (0, 1)`). The message of a
`ConfigError` is formatted by the Rust library from these fields, for example
`noiseox: spp_mmse.speech_prior must be in (0, 1), got 1.5`.

Across the ABI, every function returns a status. Creation also fills a `nox_error` detail in caller-owned storage, so
no error text is allocated or has to be freed, and every exported function catches unwinding instead of letting a
panic cross into Go.

## Lifecycle And Concurrency

```go
call, err := noiseox.New(noiseox.DefaultConfig())
if err != nil {
	return err
}
defer call.Close()

var in, out [noiseox.PacketSize]byte
for receive(&in) {
	emitted, err := call.ProcessPacket(&in, &out)
	if err != nil {
		return err
	}
	if emitted {
		send(&out)
	}
}
var tail [noiseox.DelayPackets][noiseox.PacketSize]byte
n, err := call.Drain(&tail)
if err != nil {
	return err
}
for i := range n {
	send(&tail[i])
}
```

- The first two packets of a call return `emitted == false`; every later packet writes the enhanced packet received two
  packets earlier. `Drain` returns `min(packets, 2)` packets, so the output has the input's length. After `Drain`,
  `ProcessPacket` and `Drain` return `ErrDrained` until `Reset`, which starts a new call on the same `Call` without
  allocating.
- `New` is the only call that allocates in Rust (the enhancer's state, 18–44 KB depending on the estimator). The packet
  methods allocate nothing on either side: they pass the caller's arrays to Rust, which writes into them. The input
  and output may be the same array.
- `Close` frees the enhancer and is idempotent; afterwards every method returns `ErrClosed` without calling Rust. A
  `Call` that becomes unreachable without `Close` is freed by a runtime cleanup after a garbage collection, so close
  calls explicitly.
- A `Call` is not safe for concurrent use: drive it from one goroutine at a time, and do not `Close` it while another
  goroutine is inside a method. Independent calls share nothing and run in parallel, one `Call` per goroutine; the
  tests check that parallel calls produce byte-identical output to sequential ones under the race detector.

## The Call-Stream Example

[`go/examples/callstream`](../go/examples/callstream/main.go) is a complete call loop to copy into a service. It
reads a headerless μ-law file or standard input packet by packet into one reused input array, processes each packet
into one reused output array, writes every emitted packet in order to a buffered writer, drains and writes the
withheld packets, and closes the call. It optionally paces packets at 20 ms (`-pace`), and it reports the packet
counts, `ProcessPacket` latency percentiles recorded into a preallocated slice, and the heap allocations of the loop.
`-parallel N` then runs N independent calls in parallel goroutines outside the measured loop.

```bash
cd go
go run ./examples/callstream -in ../audio/out/office-5db/noisy.ul -out enhanced.ul
go run ./examples/callstream -synthetic 3000
```

It exits with status 1 if the output packet count differs from the input packet count, if the unpaced loop allocated,
or if parallel calls disagree. On the office recording of the replay its output is byte-identical to the Rust
replay's `enhanced-spp-mmse.ul`.

## Measurements

Measured on an Apple M1 Ultra with Go 1.27.1 on the office recording
([performance.md](performance.md#go-binding) records the setup, commands, all results and the limits):

- Per-packet latency seen from Go matches the Rust packet cost within run-to-run spread: with the default
  configuration the mean is 16.9–17.0 µs and p99.9 below 0.09 ms, and the slowest packet of any configuration and run
  took 0.29 ms, 1.4 % of the 20 ms cadence.
- One Go-to-Rust crossing costs 28 ns, about 0.17 % of a packet.
- The Go call loop allocates nothing: 0 allocations over 52 940 packets per run, and `testing.AllocsPerRun` reports 0,
  also under the race detector.
- 100 concurrent calls of 120 s each in goroutines finish in 0.59–0.70 s, a real-time factor of 17 000–20 000,
  as fast as the Rust threads of `noise-oxydation-eval bench` in the same session.

The limits: one machine and operating system for timing (Linux is checked for correctness in CI, not timed), wall
time only, a loop without network or codec work around it, and an allocation count that is process-wide.

## Verification

- Rust tests of the C ABI crate: every `ConfigError` variant's kind and payload, null arguments, panic containment and
  poisoning, the packet lifecycle, the header's constants and layout, and output equal to the Rust API on synthetic
  calls.
- Go tests, run with `-race`: lifecycle and timing contract, each error kind through `errors.As` and `errors.Is`, every
  one of the 41 fields reaching Rust, use after `Close`, reset equivalence, parallel calls equal to sequential ones,
  output digests equal to those of the Rust tests, zero allocations, and benchmarks.
- CI runs a Go integration job on Linux and macOS: it builds the static library, checks `gofmt`, runs `go vet` and
  `go test -race`, runs the example on synthetic input, and builds and runs a separate consumer module that imports a
  copy of the package outside the checkout through a `replace` directive, linked by `CGO_LDFLAGS` alone.
