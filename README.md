# Noise Oxydation

Noise Oxydation is a Rust library that reduces background noise in live telephone calls. It works on 8 kHz mono audio
carried in 160-byte G.711 μ-law packets (20 ms each): one enhancer instance per call takes each incoming packet and
returns an enhanced packet after a fixed 40 ms delay, without allocating memory on the packet path.

It is an independently written port of the Go project
[sghaida/noise-cancelation](https://github.com/sghaida/noise-cancelation), used as a behavior reference at commit
[`cfc7520`](https://github.com/sghaida/noise-cancelation/tree/cfc7520a0625da90e4ad4699541a6ffe98e7c637). No Go code is
copied; every known difference from the reference is recorded in [docs/reference-log.md](docs/reference-log.md), and no
numerical parity with it is claimed yet.

## Status

The default processing path works end to end:

μ-law decode → 80 Hz high-pass → STFT (256-sample Hann frames, 128-sample hop) → SPP-MMSE noise estimation with a
5-second quiet-intro calibration → decision-directed SNR → Log-MMSE suppression → ISTFT → μ-law encode.

Still to come: the MCRA and minimum-statistics noise estimators, tonal transient (beep and click) suppression, a
real-speech evaluation with quality metrics, latency measurements, and an illustrated guide. Until the evaluation
exists, enhancement quality on real speech has not been measured.

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

[ARCHITECTURE.md](ARCHITECTURE.md) states the full timing contract and constraints; [docs/algorithms.md](docs/algorithms.md)
gives the equations and defaults of every stage.

## Quick Start

The crate is not published to crates.io. Depend on a checkout of this repository by path (or by its Git URL):

```toml
[dependencies]
noise-oxydation = { path = "../noise-oxydation/crates/noise-oxydation" }
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

`CallConfig` exposes the calibration duration and every stage parameter; invalid values are rejected by
`CallEnhancer::new` with a `ConfigError` naming the field. Logging uses the [`log`](https://crates.io/crates/log)
facade behind the default `log` feature and only emits a debug record when an enhancer is created; build with
`default-features = false` to remove it.

## Example: Enhance a μ-law File

The `enhance_mulaw` example enhances a headerless 8 kHz μ-law file (for example a `.ul` export or a raw capture of a
call) and reports the packet counts:

```bash
cargo run --locked --release -p noise-oxydation --example enhance_mulaw -- input.ul output.ul
```

A final partial packet is padded with μ-law silence and reported. For a quick smoke test with random bytes:

```bash
mkdir -p target && head -c 480000 /dev/urandom > target/noise.ul
cargo run --locked --release -p noise-oxydation --example enhance_mulaw -- target/noise.ul target/noise.enhanced.ul
```

This reports 3000 input and 3000 output packets and writes a 480 000-byte file.

## Development

The toolchain is pinned to Rust 1.98.1 in `rust-toolchain.toml`; formatting uses the pinned nightly rustfmt because
`rustfmt.toml` uses unstable options. Builds are locked. The CI gates are:

```bash
cargo +nightly-2026-09-25 fmt --all --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
cargo test --locked -p noise-oxydation --no-default-features
cargo build --locked --workspace --release
```

The tests include per-stage unit tests against independently derived values, a packet lifecycle test over every call
length from 1 to 600 packets, the calibration boundary, reset equivalence, byte-identical parallel calls, and a
counting global allocator that asserts the packet path, `drain`, and `reset` never allocate. Contributor rules are in
[AGENTS.md](AGENTS.md).

## License

MIT. See [LICENSE](LICENSE).
