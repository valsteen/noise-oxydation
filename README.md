# Noise Oxydation

Noise Oxydation is a Rust library that reduces background noise in live telephone calls. It works on 8 kHz mono audio
carried in 160-byte G.711 μ-law packets (20 ms each): one enhancer instance per call takes each incoming packet and
returns an enhanced packet after a fixed 40 ms delay, without allocating memory on the packet path.

It is an independently written port of the Go project
[sghaida/noise-cancelation](https://github.com/sghaida/noise-cancelation), used as a behavior reference at commit
[`cfc7520`](https://github.com/sghaida/noise-cancelation/tree/cfc7520a0625da90e4ad4699541a6ffe98e7c637). No Go code is
copied; every known difference from the reference is recorded in [docs/reference-log.md](docs/reference-log.md), which
also holds the measured numeric comparison: on the evaluation recordings, with SPP-MMSE and MCRA, more than
99.998 % of the output bytes are identical to the reference's.

## Status

The complete documented processing path works end to end. The default call runs:

μ-law decode → 80 Hz high-pass → STFT (256-sample Hann frames, 128-sample hop) → SPP-MMSE noise estimation with a
5-second quiet-intro calibration → decision-directed SNR → Log-MMSE suppression → tonal transient suppression →
ISTFT → μ-law encode.

Each call can instead estimate noise with MCRA or with the simple minimum estimator, and can turn tonal transient
suppression off. Tonal transient suppression attenuates narrow tones such as beeps and whistles between 2 and 4 kHz
by up to about 6 dB, while sparing peaks with harmonic support (voiced speech).

Enhancement quality on real speech and packet-path performance are measured; see
[Evaluation and Performance](#evaluation-and-performance). [HOW_IT_WORKS.md](HOW_IT_WORKS.md) is an illustrated guide
to the crates, the path of a packet through a call, what runs sequentially and in parallel, and the call timeline.

## How a Call Works

- **Two-packet delay.** The first two packets of a call return `PacketOutcome::Priming`; every later packet returns one
  enhanced packet carrying the audio of the packet received two packets earlier.
- **Exact drain.** `drain` returns the withheld packets (at most two), so a call's output has exactly as many samples
  as its input, in order.
- **Quiet intro.** The first 5 seconds (configurable, zero to disable) are assumed to contain only background noise.
  That audio passes through un-enhanced while the noise spectrum is learned. Speech during the intro is learned as
  noise, which over-suppresses speech for a while after calibration ends.
- **One instance per call.** Instances share nothing, so independent calls run in parallel on separate threads. `reset`
  reuses an instance for a new call.

[ARCHITECTURE.md](ARCHITECTURE.md) states the full timing contract and constraints;
[docs/algorithms.md](docs/algorithms.md) gives the equations and defaults of every stage.

## Quick Start

The crate is not published to crates.io. Depend on a checkout of this repository by path (or by its Git URL):

```toml
[dependencies]
noise-oxydation = { path = "../noise-oxydation/crates/core/noise-oxydation" }
```

Stream a call:

```rust
use noise_oxydation::{CallConfig, CallEnhancer, DELAY_PACKETS, Packet, PacketOutcome};

fn enhance_call(
    incoming: impl Iterator<Item = Packet>,
    mut send: impl FnMut(&Packet),
) -> Result<(), Box<dyn std::error::Error>> {
    let mut enhancer = CallEnhancer::new(&CallConfig::default())?;
    let mut output = [0; 160];
    for packet in incoming {
        if enhancer.process_packet(&packet, &mut output)? == PacketOutcome::Emitted {
            send(&output);
        }
    }
    let mut tail = [[0; 160]; DELAY_PACKETS];
    let withheld = enhancer.drain(&mut tail)?;
    tail[..withheld].iter().for_each(|packet| send(packet));
    Ok(())
}
```

`CallConfig` exposes the calibration duration, the noise estimator, the interference setting, and every stage
parameter; invalid values are rejected by `CallEnhancer::new` with a `ConfigError` naming the field. For example, to
use MCRA with a shorter minimum window and no tonal transient suppression:

```rust
use std::time::Duration;

use noise_oxydation::{CallConfig, CallEnhancer, ConfigError, InterferenceConfig, McraConfig, NoiseEstimatorConfig};

fn mcra_enhancer() -> Result<CallEnhancer, ConfigError> {
    CallEnhancer::new(&CallConfig {
        calibration_duration: Duration::from_secs(3),
        noise_estimator: NoiseEstimatorConfig::Mcra(McraConfig { window_frames: 30, ..McraConfig::default() }),
        interference: InterferenceConfig::Disabled,
        ..CallConfig::default()
    })
}
```

Logging uses the [`log`](https://crates.io/crates/log) facade behind the default `log` feature and only emits one
debug record, naming the noise estimator and the interference setting, when an enhancer is created; build with
`default-features = false` to remove it. The opt-in `stage-timing` feature makes each call count and time its
processing stages without allocating; read the totals with `CallEnhancer::stage_timings`.

## Using It With Twilio Media Streams

A Twilio Media Streams media message carries 20 ms of 8 kHz mono G.711 μ-law audio: 160 bytes before base64
encoding, which is exactly one `Packet`. For each call:

1. Create one `CallEnhancer` when the stream starts (or `reset` a pooled one), so no caller's noise statistics, SNR
   history or tonal history can leak into another call.
2. For every media message, base64-decode the payload into a `[u8; 160]` and pass it to `process_packet`. Forward the
   enhanced packet, or nothing while the first two packets prime the enhancer.
3. When the stream stops, call `drain` and forward the two withheld packets.

The 160-sample packets do not need to line up with the 256-sample analysis frames: the enhancer buffers internally and
keeps its output packet-aligned. Remember that the first 5 seconds are treated as a quiet intro (see
[How a Call Works](#how-a-call-works)).

This section adapts the Twilio Media Streams and reset guidance of the Go project's README (MIT License, Copyright (c)
2026 Saddam Abu Ghaida; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)).

## Try It

Every command runs from the repository root with the pinned toolchain from `rust-toolchain.toml`, which `rustup`
installs on first use.

**Build and test the library:**

```bash
cargo build --locked --release -p noise-oxydation
cargo test --locked -p noise-oxydation
```

**Enhance a μ-law file.** The `enhance_mulaw` example enhances a headerless 8 kHz μ-law file (for example a `.ul`
export or a raw capture of a call) and reports the packet counts. A final partial packet is padded with μ-law silence
and reported.

```bash
cargo run --locked --release -p noise-oxydation --example enhance_mulaw -- input.ul output.ul
```

To try it on any recording, convert it to 8 kHz μ-law and back with [FFmpeg](https://ffmpeg.org/), then listen to both
WAV files:

```bash
ffmpeg -i recording.wav -ar 8000 -ac 1 -f mulaw input.ul
cargo run --locked --release -p noise-oxydation --example enhance_mulaw -- input.ul output.ul
ffmpeg -f mulaw -ar 8000 -ac 1 -i input.ul before.wav
ffmpeg -f mulaw -ar 8000 -ac 1 -i output.ul after.wav
```

The first 5 seconds of `after.wav` are the un-enhanced calibration pass-through, so pick a recording that starts with a
few seconds of background noise. For a quick smoke test without a recording, random bytes work too:

```bash
mkdir -p target && head -c 480000 /dev/urandom > target/noise.ul
cargo run --locked --release -p noise-oxydation --example enhance_mulaw -- target/noise.ul target/noise.enhanced.ul
```

This reports 3000 input and 3000 output packets and writes a 480 000-byte file.

**Replay real speech and listen.** Download the pinned evaluation recordings (needs `curl` and `python3`), then
replay them through the enhancer:

```bash
scripts/fetch-evaluation-audio.sh
cargo run --locked --release -p noise-oxydation-eval -- replay
```

The replay prints the metrics and writes WAV files under the Git-ignored `audio/out/`. Listen to
`audio/out/office-5db/noisy.wav` and then `audio/out/office-5db/enhanced-spp-mmse.wav`, starting at 6 s where the
speech begins. [docs/evaluation.md](docs/evaluation.md#listening) lists every file.

**Reproduce the performance measurements** (after the replay, which writes the benchmark input):

```bash
cargo run --locked --release -p noise-oxydation-eval -- bench
cargo run --locked --release -p noise-oxydation-eval --features stage-timing -- bench
```

The first run reports allocations, per-call memory, packet latency and concurrent-call throughput; the second adds the
per-stage breakdown. [docs/performance.md](docs/performance.md) describes the method, the machine and the comparison
with the Go reference, which needs Go (see [docs/evaluation.md](docs/evaluation.md#reproduce)).

## Evaluation and Performance

The `noise-oxydation-eval` tool replays real recorded speech (Open Speech Repository) mixed with real office and
cafeteria noise (DEMAND) at 5 dB SNR through `CallEnhancer`, packet by packet, and writes WAV files to listen to plus
objective metrics. With the default settings the enhancer removes about 26 dB of office noise and 11 dB of cafeteria
noise in speech pauses and improves segmental SNR by 5.2 and 2.7 dB, while speech loses 0.3 and 1.4 dB of level.
[docs/evaluation.md](docs/evaluation.md) has every scenario and estimator, the effect of speech during calibration, and
the measurement limits.

On an Apple M1 Ultra a packet takes about 17 µs on average (p99.9 under 0.09 ms) against its 20 ms cadence, the
packet path never allocates, a call holds 18–44 KB, and 100 concurrent calls of 120 s finish in about 0.6–0.7 s,
3–4 times faster than the Go reference on the same machine and audio. [docs/performance.md](docs/performance.md) has
the method, the Go comparison, the stage breakdown and the vectorization investigation.

The evidence runs download audio into the Git-ignored `audio/` directory and are not part of CI; [Try It](#try-it)
lists the commands.

## Limitations

- **Strong foreground sounds.** The noise estimators recognize what matches the learned background, not what is wanted
  speech. A loud foreground sound such as a bird chirp looks like speech and passes; tonal transient suppression only
  attenuates narrow tones between 2 and 4 kHz, by up to about 6 dB.
- **A second talker** cannot be separated from the wanted one by a single-channel suppressor; that needs speaker
  extraction, source separation or several microphones.
- **8 kHz bandwidth.** Telephone audio carries nothing above 4 kHz, and no processing restores it.
- **Quiet intro.** Speech during the first 5 seconds is learned as noise; in the measured office scene it cost 1–3 s
  of reduced speech level afterwards, depending on the estimator.

The first three points adapt the limitations section of the Go project's README (MIT License, Copyright (c) 2026
Saddam Abu Ghaida; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)). [ARCHITECTURE.md](ARCHITECTURE.md#limitations)
lists every limitation.

## Documentation

| Document | Content |
| --- | --- |
| [HOW_IT_WORKS.md](HOW_IT_WORKS.md) | Illustrated guide: crate map, processing flow, sequential and parallel work, call timeline |
| [ARCHITECTURE.md](ARCHITECTURE.md) | Crate map, per-call ownership, buffers and synchronization, packet timing, calibration, constraints |
| [docs/algorithms.md](docs/algorithms.md) | Equations, defaults and valid ranges of every stage, and why each stage exists |
| [docs/evaluation.md](docs/evaluation.md) | Real-speech replay: sources, scenarios, metrics, results, listening, limits |
| [docs/performance.md](docs/performance.md) | Latency, allocation, memory, throughput, Go comparison, stage breakdown, vectorization |
| [docs/reference-log.md](docs/reference-log.md) | Every difference from the Go reference and the measured parity |
| [docs/design-principles.md](docs/design-principles.md) | Which engineering principles are carried, adapted or rejected, and why |
| [docs/how-it-works.md](docs/how-it-works.md) | The diagram grammar and how to regenerate the guide |
| [docs/lint-exceptions.md](docs/lint-exceptions.md) | The audited lint expectations |
| [AGENTS.md](AGENTS.md) | Contributor and coding-agent rules |

## Development

The workspace groups its crates by role: the library in `crates/core/noise-oxydation`, and the tools
`crates/tools/noise-oxydation-eval` and `crates/tools/how-it-works` (the guide renderer). The toolchain is pinned to
Rust 1.98.1 in `rust-toolchain.toml`; formatting uses the pinned nightly rustfmt because `rustfmt.toml` uses unstable
options. Builds are locked. The CI gates are:

```bash
cargo +nightly-2026-09-25 fmt --all --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
cargo test --locked -p noise-oxydation --no-default-features
cargo build --locked --workspace --release
cargo run --locked -p how-it-works -- --check
```

The tests include per-stage unit tests against independently derived values, a packet lifecycle test over every call
length from 1 to 600 packets, the calibration boundary, and, for every noise estimator with tonal transient
suppression on and off: packet timing, reset equivalence, bounded output, stationary-noise attenuation,
byte-identical parallel calls, and a counting global allocator that asserts the packet path, `drain`, and `reset`
never allocate, with and without the `stage-timing` feature. A sweeping-tone test checks that tonal transient
suppression attenuates a foreground tone by more than 0.5 dB and at most its 6.02 dB limit. The evaluation crate's
unit tests cover its file handling, resampler, scenario mixing, metrics and comparison statistics, and the renderer's
cover its diagram grammar, theme rendering, freshness check and crate-map drift check.

`HOW_IT_WORKS.md` and its diagrams are generated: change the seeds in `crates/tools/how-it-works` and run
`cargo run --locked -p how-it-works` ([docs/how-it-works.md](docs/how-it-works.md)). Contributor rules are in
[AGENTS.md](AGENTS.md).

## License

MIT. See [LICENSE](LICENSE). [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) holds the notice for the documentation
adapted from the Go reference.
