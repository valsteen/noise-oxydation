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

The implemented path is μ-law decode, a high-pass filter, 256-point Hann STFT with a 128-sample hop, selected noise estimation, decision-directed SNR, Log-MMSE gain, tonal transient gain, normalized ISTFT, and μ-law encode. The simple minimum and MCRA estimators smooth each bin's power and search a fixed 50-frame minimum history. MCRA uses that minimum to estimate speech presence, then updates noise with `αₙ = 0.95 + 0.05p`. Tonal gain is enabled for all three estimators: local prominence, positive flux, frequency movement, persistent tonal evidence, and harmonic support determine a conservative target gain; attack/release smoothing and a two-bin neighbor spread follow. The learning interval emits the original decoded audio rather than the filtered reconstruction. The first whole output packet arrives with the third 20 ms input packet in the packet timing test.

On the local Apple Silicon host, a release run of 2,000 packets measured 18.2 µs per packet for SPP-MMSE, 20.5 µs for MCRA, and 19.9 µs for minimum estimation. The earlier SPP-only path measured 10.1 µs per packet on the same host before tonal gain was added. These are local comparisons, not target-platform latency promises. Run `cargo run --release --locked -p noise-oxydation-pipeline --example packet_bench` on the target host.

The Rust implementation uses the [Go project's documented behavior at revision `cfc7520`](https://github.com/sghaida/noise-cancelation/commit/cfc7520a0625da90e4ad4699541a6ffe98e7c637) as a reference, without copying Go source. The selected MCRA coefficient and tonal edge, movement, harmonic, and overlapping-spread rules are Rust interpretations where the README is incomplete; [reference observations](docs/reference-observations.md) keeps those questions open. Synthetic checks cover estimator arithmetic, packet behavior, and speech mixed with a tone. Representative real-audio validation, opt-in logging and tracing, SIMD assessment, and final visual explanations remain required later work.

See [Architecture](ARCHITECTURE.md) for the current ownership and timing contract. Licensed under [MIT](LICENSE).
