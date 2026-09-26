# Noise Oxydation

Noise Oxydation enhances 8 kHz mono telephony audio carried in 160-byte G.711 μ-law packets. The first working Rust path owns one call per `Pipeline`, buffers FFT frames, and returns zero or more ordered complete output packets for each input packet. `finish` returns the remaining audio once and reports the final packet's valid sample count. With fixed-size inputs and length-preserving output, that count is currently 160 whenever a packet is returned. `reset` prepares the same instance for another call.

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

The example's `send_valid_audio` stands for the caller's transport. This crate does not provide one. Processing after `finish` returns `AlreadyFinished` until `reset`. Configuration accepts a runtime `learning_duration`; the default is five seconds, and intervals shorter than one 256-sample FFT window are rejected with a typed DSP cause.

The first five seconds of audio are audibly passed through while the pipeline learns stationary noise. The frame scheduler rounds the first suppressed sample to the next 128-sample hop: with the default, suppression begins at sample 40,064 (5.008 seconds). **A quiet call intro is assumed.** Speech during learning can contaminate the noise estimate and reduce later suppression. Choose another duration if the call setup provides a different quiet interval.

The implemented path is μ-law decode, a high-pass filter, 256-point Hann STFT with a 128-sample hop, SPP-MMSE noise estimation, decision-directed SNR, Log-MMSE gain, normalized ISTFT, and μ-law encode. The learning interval emits the original decoded audio rather than the filtered reconstruction. The first whole output packet arrives with the third 20 ms input packet in the packet timing test. A release benchmark on the local Apple Silicon host processed 2,000 packets in 19.90 ms (9.9 µs per input packet); run `cargo run --release -p noise-oxydation-pipeline --example packet_bench` on a target host to measure its own timing.

The Rust implementation uses the [Go project's documented behavior at revision `cfc7520`](https://github.com/sghaida/noise-cancelation/commit/cfc7520a0625da90e4ad4699541a6ffe98e7c637) as a reference, without copying Go source. Its SPP and Log-MMSE formula ambiguities remain open in [reference observations](docs/reference-observations.md). MCRA, the documented minimum-noise alternative, tonal transient suppression, real-audio validation, opt-in logging and tracing, SIMD assessment, and final visual explanations are required later work.

See [Architecture](ARCHITECTURE.md) for the current ownership and timing contract. Licensed under [MIT](LICENSE).
