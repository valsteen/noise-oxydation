# How Noise Oxydation works

Noise Oxydation enhances one 8 kHz mono G.711 μ-law call through a `Pipeline`. The caller supplies 160-byte packets, receives ordered complete output packets, and calls `finish` once to drain the final valid samples. A quiet call intro lets the pipeline learn a noise baseline before suppression begins.

## Ownership

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/how-it-works/crates-night.svg">
  <img src="assets/how-it-works/crates-day.svg" alt="Pipeline owns packet and call state and depends on separate codec and DSP crates.">
</picture>

The Rust 2024 workspace has three crates. `pipeline` owns public configuration, packet buffering, and call lifecycle. It calls `codec` for μ-law conversion and `dsp` for fixed-frame enhancement. Neither lower crate depends on `pipeline`. Each `Pipeline` contains its own estimator, FFT, tonal, and output state.

Independent calls can run at the same time when the caller schedules separate `Pipeline` instances. Within one call, packets, FFT hops, estimator updates, and `finish` advance that instance in order. There is no internal worker or shared DSP state.

## One call's audio flow

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/how-it-works/audio-night.svg">
  <img src="assets/how-it-works/audio-day.svg" alt="Decoded packets enter high-pass filtering and 256-point STFT. After the quiet intro, selected noise estimation, Log-MMSE, tonal gain, and synthesis lead to encoded output. During the intro, original decoded audio bypasses suppression while complete frames learn noise.">
</picture>

`process_packet(&[u8; 160])` decodes each 20 ms packet. A first-order high-pass filter feeds a periodic Hann short-time Fourier transform (STFT). The first 256 samples complete a frame; subsequent frames start every 128 samples. One input packet can complete zero, one, or two hops, so its result can contain zero, one, or two **whole** output packets.

The default `learning_duration` is five seconds and assumes a quiet intro. Complete FFT windows wholly inside the interval update the noise baseline. Frames starting before the cutoff emit the original decoded PCM instead of filtered, reconstructed audio. With the default 40,000-sample interval, the first suppressed frame starts at sample 40,064 (5.008 seconds). A different call setup can configure another duration; at least one complete 256-sample frame is required.

After the intro, the selected estimator updates one call-owned noise spectrum:

- **SPP-MMSE** is the default. It smooths speech-presence probability for a conditional noise update.
- **MCRA** compares smoothed power with a fixed 50-frame minimum history, then uses speech probability to slow noise updates. A Rust safeguard protects sustained narrowband voice after the window turns over.
- **Minimum estimation** uses the same history as a lower-envelope estimate. It can learn continuous foreground energy.

All three alternatives feed the same decision-directed prior SNR and Log-MMSE gain. Tonal gain then attenuates isolated transient peaks while accounting for neighboring and harmonic evidence. The modified spectrum passes through inverse FFT, Hann synthesis, and window-square overlap normalization. `codec` encodes the valid output samples back to μ-law.

The formulas and safeguards chosen where the Go documentation is incomplete are [recorded separately](docs/reference-observations.md). The [real-speech replay](docs/real-audio-evidence.md) measures this Rust path on one credited clip; it does not establish universal speech quality or exact Go numerical parity.

## End of call and another call

`finish` pads the DSP just enough to emit the remaining valid audio and reports the last packet's valid sample count. With fixed 160-sample inputs, a returned final packet currently has 160 valid samples. A second `finish` or a `process_packet` after finishing returns `AlreadyFinished`. `reset` clears the filter, estimator, tonal, FFT, overlap, pending-packet, and finish state while retaining configuration.

Processing and finishing allocate no memory after construction and use no blocking synchronization. The opt-in `logging` feature only writes a status snapshot when the caller explicitly invokes `write_status` outside the audio thread. The opt-in `performance-analysis` examples run offline.

See [Architecture](ARCHITECTURE.md) for the detailed timing and ownership contract and [README](README.md) for a caller example. The project is [MIT licensed](LICENSE).
