# Noise Oxydation

Noise Oxydation enhances 8 kHz mono telephony audio carried in 160-byte G.711 μ-law packets. One `Pipeline` owns one call, buffers FFT frames, and returns zero or more ordered complete output packets for each input packet. `finish` returns the remaining audio once and reports the final packet's valid sample count. With fixed-size inputs and length-preserving output, that count is currently 160 whenever a packet is returned. `reset` prepares the same instance for another call.

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

The example's `send_valid_audio` stands for the caller's transport. This crate does not provide one. Processing after `finish` returns `AlreadyFinished` until `reset`. Configuration accepts a runtime `learning_duration` and a `noise_estimator` selected from `NoiseEstimator::{SppMmse, Mcra, Minimum}`. SPP-MMSE and five seconds are the defaults. For example, use `Config { noise_estimator: NoiseEstimator::Mcra, ..Config::default() }`. Intervals shorter than one 256-sample FFT window are rejected with a typed DSP cause.

The first five seconds of audio are audibly passed through while the pipeline learns stationary noise. The frame scheduler rounds the first suppressed sample to the next 128-sample hop: with the default, suppression begins at sample 40,064 (5.008 seconds). **A quiet call intro is assumed.** Speech during learning can contaminate the noise estimate and reduce later suppression. Choose another duration if the call setup provides a different quiet interval.

The implemented path is μ-law decode, a high-pass filter, 256-point Hann STFT with a 128-sample hop, selected noise estimation, decision-directed SNR, Log-MMSE gain, tonal transient gain, normalized ISTFT, and μ-law encode. The learning interval emits the original decoded audio rather than the filtered reconstruction. The first whole output packet arrives with the third 20 ms input packet in the packet timing test.

The simple minimum and MCRA estimators smooth each bin's power and search a fixed 50-frame minimum history. MCRA uses that minimum to estimate speech presence, then updates noise with `αₙ = 0.95 + 0.05p`. High posterior SNR also protects sustained narrowband voice when the window turns over. The simple minimum remains a lower-envelope estimator and may suppress continuous foreground sound.

Tonal gain is enabled for all three estimators. Local prominence, positive flux, frequency movement, persistent tonal evidence, and harmonic support determine a conservative target gain. Attack/release smoothing and a two-bin neighbor spread follow.

## Diagnostics and offline evidence

Enable `logging` to request a read-only call snapshot with `call.write_status(&mut sink)`, where `sink` implements `std::fmt::Write` (for example, `String`). It reports active/finished phase and input, output and pending sample counts. Call it outside the audio processing thread; `process_packet` and `finish` never write to a sink. The default build has no logging dependency or callback work. Run `cargo test --locked --workspace --all-targets --all-features` to check both opt-in features with the packet tests.

The `performance-analysis` feature enables two offline examples. Run `cargo run --release --locked -p noise-oxydation-pipeline --features performance-analysis --example packet_bench` for warmed, identical-input packet timing. It measures 250 learning packets, omits 50 cutoff packets, then measures 1,700 suppression packets and `finish` separately. On VincentacStudio (macOS arm64, Apple Silicon, Rust 1.98.1, release), nearest-rank suppression p50/p95 was 17.42/34.92 µs for SPP-MMSE, 19.00/38.79 µs for MCRA and 18.50/37.75 µs for minimum. The [full timing evidence](docs/real-audio-evidence.md#packet-timing) includes learning and finish results. These local timings do not promise target-platform latency; stdlib timing has not shown a need for `minitrace`.

For real-speech replay, download [OSR_us_000_0010_8k.wav](https://www.voiptroubleshooter.com/open_speech/american/OSR_us_000_0010_8k.wav) from **Open Speech Repository** outside Git. The verified 8 kHz mono 16-bit PCM source has 268,985 samples and SHA-256 `a4bf9becd046d7aedb6d05b6e12347a6294a44f74d263089c636fb0a2b1e6561`. Run `cargo run --release --locked -p noise-oxydation-pipeline --features performance-analysis --example wav_replay -- /private/tmp/OSR_us_000_0010_8k.wav /private/tmp/OSR_us_000_0010_8k-prepared.mulaw`, then hash the prepared stream. The clip and generated audio are not committed.

The replay puts fixed-seed noise in calibration `[0,40000)` and post-calibration `[40000,48000)`, then adds the recorded speech from sample 48,000. All estimators receive identical μ-law bytes and return 317,120 valid samples. Noise RMS output/input is measured only on `[40800,47200)`. Pearson correlation and projection gain `Σ(output × clean) / Σ(clean²)` use `[48800,48000+N-800)`, with `N` equal to the source sample count and a μ-law-quantized clean reference.

| Estimator | Noise RMS ratio | Speech correlation | Speech projection gain |
| --- | ---: | ---: | ---: |
| SPP-MMSE | 0.1040 | 0.8902 | 0.5910 |
| MCRA | 0.1070 | 0.9634 | 0.8383 |
| Minimum | 0.1563 | 0.9586 | 0.8303 |

The [replay evidence](docs/real-audio-evidence.md) gives the preparation, full formulas, hashes and limits.

The Rust implementation uses the [Go project's documented behavior at revision `cfc7520`](https://github.com/sghaida/noise-cancelation/commit/cfc7520a0625da90e4ad4699541a6ffe98e7c637) as a reference, without copying Go source. The selected MCRA coefficient, sustained-voice safeguard, and tonal edge, movement, harmonic, and overlapping-spread rules are Rust interpretations where the README is incomplete; [reference observations](docs/reference-observations.md) keeps those questions open. On identical synthetic input, composed public Go components retained 68.6% of the sustained 250 Hz fundamental in the final second. That repository has no shipped top-level MCRA pipeline, so this is component evidence, not end-to-end numerical parity. Synthetic Rust checks cover estimator arithmetic, packet behavior, stationary-noise attenuation, and sustained voice mixed with a tone. The credited real-speech replay is Rust path evidence, not verified Go numerical parity. SIMD assessment and final visual explanations remain later work.

See [Architecture](ARCHITECTURE.md) for the current ownership and timing contract. Licensed under [MIT](LICENSE).
