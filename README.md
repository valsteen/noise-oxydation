# Noise Oxydation

Noise Oxydation enhances 8 kHz mono telephony audio carried in 160-byte G.711 μ-law packets. One `Pipeline` owns one call, buffers FFT frames, and returns zero or more ordered complete output packets for each input packet. `finish` returns the remaining audio once and reports the final packet's valid sample count. With fixed-size inputs and length-preserving output, that count is currently 160 whenever a packet is returned. `reset` prepares the same instance for another call.

Go programs can import [`github.com/valsteen/noise-oxydation/go`](go/README.md) to use the same Rust packet pipeline without writing cgo. The [Go quick trial and swap guide](go/README.md) starts with one command to compare both modes on a recorded μ-law call, then shows the closest `[]float32` processor swap and the faster direct packet path. It also gives macOS/Linux build and link commands, an external-module import check, and packet timing measurements. Each Go `Call` owns a native pipeline and serializes its lifecycle, including `Close`. The Rust process/finish core remains allocation-free and lock-free; Go/cgo scheduling has no hard real-time guarantee.

The default **conservative** mode preserves the existing audio output and first returns a complete packet with input packet three. An **experimental lower-delay** mode returns the first complete packet with input packet two, at a higher processing cost and with possible quality differences. Both modes are available per call in the same binary. [Processing modes and comparison](docs/processing-modes.md) shows how to select each mode, reproduces the A/B measurements, and describes the experimental risks.

```rust
use noise_oxydation_pipeline::{Config, Pipeline};

let mut call = Pipeline::new(Config::default())?;
let input = [0xff; 160];
let batch = call.process_packet(&input)?;
for index in 0..batch.len() {
    let (packet, valid_samples) = batch.packet(index).unwrap();
    send_valid_audio(&packet[..valid_samples]);
}
let tail = call.finish()?;
for index in 0..tail.len() {
    let (packet, valid_samples) = tail.packet(index).unwrap();
    send_valid_audio(&packet[..valid_samples]);
}
```

The example's `send_valid_audio` stands for the caller's transport. This crate does not provide one. Processing after `finish` returns `AlreadyFinished` until `reset`. Configuration accepts a runtime `learning_duration` and a `noise_estimator` selected from `NoiseEstimator::{SppMmse, Mcra, Minimum}`. SPP-MMSE and five seconds are the defaults. For example, use `Config { noise_estimator: NoiseEstimator::Mcra, ..Config::default() }`. The minimum learning interval is 256 samples in conservative mode and 320 samples in experimental mode, so each accepts at least one complete learning frame; shorter intervals return a typed DSP cause.

For the experimental mode, construct the call with `Pipeline::new_with_mode(config, ProcessingMode::ExperimentalLowDelay)`. The usual `Pipeline::new(config)` remains conservative. Go callers can set `Config.Mode` to `ExperimentalLowDelay`; the zero value stays conservative.

The first five seconds of audio are audibly passed through while the pipeline learns stationary noise. The frame scheduler rounds the first suppressed sample to the next 128-sample hop: with the default, suppression begins at sample 40,064 (5.008 seconds). **A quiet call intro is assumed.** Speech during learning can contaminate the noise estimate and reduce later suppression. Choose another duration if the call setup provides a different quiet interval.

The conservative path is μ-law decode, a high-pass filter, 256-point Hann STFT with a 128-sample hop, selected noise estimation, decision-directed SNR, Log-MMSE gain, tonal transient gain, normalized ISTFT, and μ-law encode. The learning interval emits the original decoded audio rather than the filtered reconstruction. The first whole output packet arrives with the third 20 ms input packet in the packet timing test. The experimental path keeps the 256-point spectral processing but uses an 80-sample hop and shorter asymmetric synthesis; see the comparison guide for its exact timing and limits.

In conservative mode, the simple minimum and MCRA estimators smooth each bin's power and search a fixed 50-frame minimum history. Experimental mode uses 80 frames to retain the same 800 ms history at its shorter hop. MCRA uses that minimum to estimate speech presence, then updates noise with `αₙ = 0.95 + 0.05p`. High posterior SNR also protects sustained narrowband voice when the window turns over. The simple minimum remains a lower-envelope estimator and may suppress continuous foreground sound.

Tonal gain is enabled for all three estimators. Local prominence, positive flux, frequency movement, persistent tonal evidence, and harmonic support determine a conservative target gain. Attack/release smoothing and a two-bin neighbor spread follow.

## Diagnostics and offline evidence

Enable `logging` to request a read-only call snapshot with `call.write_status(&mut sink)`, where `sink` implements `std::fmt::Write` (for example, `String`). It reports active/finished phase and input, output and pending sample counts. Call it outside the audio processing thread; `process_packet` and `finish` never write to a sink. The default build has no logging dependency or callback work. Run `cargo test --locked --workspace --all-targets --all-features` to check both opt-in features with the packet tests.

The `performance-analysis` feature enables two offline examples. The packet benchmark takes a prepared μ-law stream as an argument, warms a complete call, resets, then measures each packet and `finish`. For the credited OSR stream described below, run `cargo run --release --locked -p noise-oxydation-pipeline --features performance-analysis --example packet_bench -- /private/tmp/OSR_us_000_0010_8k-prepared.mulaw` after verifying its SHA-256. Its 1,982 packets include 250 learning packets, 50 excluded cutoff packets, and 1,682 suppression packets. The [portable versus native codegen assessment](docs/real-audio-evidence.md#packet-timing-and-codegen) reports five repeated full-call runs per mode, exact output equivalence, and generated-code evidence. Native codegen did not show a useful end-to-end improvement on this host, so no explicit SIMD path was added. These local timings do not promise target-platform latency; stdlib timing has not shown a need for `minitrace`.

For real-speech replay, download [OSR_us_000_0010_8k.wav](https://www.voiptroubleshooter.com/open_speech/american/OSR_us_000_0010_8k.wav) from **Open Speech Repository** outside Git. The verified 8 kHz mono 16-bit PCM source has 268,985 samples and SHA-256 `a4bf9becd046d7aedb6d05b6e12347a6294a44f74d263089c636fb0a2b1e6561`. Run `cargo run --release --locked -p noise-oxydation-pipeline --features performance-analysis --example wav_replay -- /private/tmp/OSR_us_000_0010_8k.wav /private/tmp/OSR_us_000_0010_8k-prepared.mulaw`, then hash the prepared stream. The clip and generated audio are not committed.

The conservative replay puts fixed-seed noise in calibration `[0,40000)` and post-calibration `[40000,48000)`, then adds the recorded speech from sample 48,000. All estimators receive identical μ-law bytes and return 317,120 valid samples. Noise RMS output/input is measured only on `[40800,47200)`. Pearson correlation and projection gain `Σ(output × clean) / Σ(clean²)` use `[48800,48000+N-800)`, with `N` equal to the source sample count and a μ-law-quantized clean reference. Add `experimental` as the final replay argument and use a separate output directory to compare the second mode on identical input.

| Estimator | Noise RMS ratio | Speech correlation | Speech projection gain |
| --- | ---: | ---: | ---: |
| SPP-MMSE | 0.1040 | 0.8902 | 0.5910 |
| MCRA | 0.1070 | 0.9634 | 0.8383 |
| Minimum | 0.1563 | 0.9586 | 0.8303 |

The [replay evidence](docs/real-audio-evidence.md) gives the preparation, full formulas, hashes and limits.

The Rust implementation uses the [Go project's documented behavior at revision `cfc7520`](https://github.com/sghaida/noise-cancelation/commit/cfc7520a0625da90e4ad4699541a6ffe98e7c637) as a reference, without copying Go source. The selected MCRA coefficient, sustained-voice safeguard, and tonal edge, movement, harmonic, and overlapping-spread rules are Rust interpretations where the README is incomplete; [reference observations](docs/reference-observations.md) keeps those questions open. On identical synthetic input, composed public Go components retained 68.6% of the sustained 250 Hz fundamental in the final second. That repository has no shipped top-level MCRA pipeline, so this is component evidence, not end-to-end numerical parity. Synthetic Rust checks cover estimator arithmetic, packet behavior, stationary-noise attenuation, and sustained voice mixed with a tone. The credited real-speech replay is Rust path evidence, not verified Go numerical parity.

See the [illustrated how-it-works guide](HOW_IT_WORKS.md) for call ownership and signal flow, and [Architecture](ARCHITECTURE.md) for the detailed timing contract. Licensed under [MIT](LICENSE).
