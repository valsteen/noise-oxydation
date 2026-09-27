# Architecture

## Product and crate boundary

One `noise-oxydation-pipeline::Pipeline` owns one 8 kHz mono call. The caller provides fixed 160-byte G.711 μ-law packets and receives owned, fixed-capacity packet batches. Independent calls use independent instances; there is no microphone, Twilio connection, UI, worker pool, or shared mutable DSP state.

The Rust 2024 workspace has three crates. `codec` performs μ-law conversion without dependencies. `dsp` owns the high-pass filter, frame analyzer, FFT, three selectable noise estimators, Log-MMSE and tonal gains, and synthesis without depending on codec or pipeline. `pipeline` depends on both and owns public estimator selection, call lifecycle, packet conversion, and output buffering. The fixed 256-point FFT is implemented in `dsp`; this path needs no runtime dependency or FFT scratch allocation.

```text
telephony caller → pipeline → codec
                       └────→ dsp
```

## Audio flow and timing

Each sample is decoded and fed through a first-order high-pass filter. After the first 256 samples, every additional 128 samples produces one spectral frame. The analyzer uses a periodic Hann window. The suppressor estimates initial noise power from the quiet intro.

The default SPP-MMSE estimator uses smoothed speech-presence probability for its conditional noise update. The simple minimum estimator tracks the smallest smoothed power in a 50-frame ring. MCRA uses the same ring for a speech-presence ratio and a probability-dependent recursive update. Both rings start with the quiet-intro baseline so the first speech frame is compared with learned noise.

For MCRA alone, high posterior SNR also counts as speech evidence when fewer than half the positive-frequency bins have a fivefold rise. This protects sustained narrowband voice after the minimum turns over while allowing a broad rise to enter the ordinary MCRA update. The simple minimum is unchanged and can learn continuous foreground energy. All variants feed one decision-directed prior SNR and Log-MMSE gain. The MCRA safeguard, the SPP path's smoothed probability, and noise-overestimation factor β = 1 are Rust choices, not verified Go numerical rules.

After Log-MMSE, a default tonal stage evaluates each original spectral power bin. Side bins three through six positions away determine local prominence; positive flux and prior-frame peak movement identify new or moving events, while weighted persistent prominence retains stationary tones. Related harmonic peaks protect voiced speech; subharmonic support requires a local peak whose multiplied frequency is within one bin of the candidate. The combined score sets a gain no lower than 0.5; attack and release smooth it, and attenuations spread two bins on either side. That gain multiplies the Log-MMSE spectrum before inverse FFT, Hann synthesis, and window-square normalization. No tonal state advances the gain during quiet-intro bypass, though the previous spectrum is observed. The precise edge, overlap, movement, and harmonic choices are recorded in [reference observations](docs/reference-observations.md), not claimed as verified Go numerical behavior.

During learning, frame output takes the original decoded PCM rather than the filtered or FFT-reconstructed signal. Only complete FFT windows wholly inside the configured interval update the noise estimate. Frames starting before the cutoff bypass suppression, including any that straddle it. With the default 40,000 samples, suppression begins with frame start 40,064 (5.008 seconds). This assumes a quiet call intro; speech during that interval can contaminate the estimate. The runtime duration changes the cutoff without changing storage.

A 160-sample input can complete zero, one, or two 128-sample DSP hops. `process_packet` returns only whole 160-sample output packets, so it may return none. The first full packet is observed on input packet three. An input packet contributes at most 256 output samples, and fewer than 160 may already be pending: at most two whole packets can be returned during processing. At `finish`, the DSP can lag by less than 256 samples and the output remainder is less than 160. Their sum is a multiple of 160 because every input packet contains 160 samples and valid output length equals input length; the sum is below 416, so it is at most 320. The stack-owned batch has two slots and reports the valid count of its last packet. The current final packet is whole. A second finish fails. `reset` clears the high-pass, analyzer, selected estimator's baseline and fixed history, tonal history, synthesis, packet remainder, and finished state while retaining the configuration.

The output sample sequence has the same valid length as the input sequence after `finish`. Tests cover short streams, packet timing, exact hop alignment, ordered sample counts, finish-once, and reset for all estimators. The 50-by-129 power history and tonal prior-frame arrays are owned by each `Enhancer`; no call shares them. An allocation-counting test observes zero allocations after construction through `process_packet` and `finish` for each selection. The path has no locks or atomics in production; callers should keep each instance on one processing owner. Transferring an instance between threads is the caller's policy.

## Diagnostics and offline evidence

The opt-in `logging` feature adds `Pipeline::write_status(&mut impl fmt::Write)`. The caller owns the sink and invokes this read-only status write away from the audio thread. It reports active/finished phase and input, output and pending sample counters. No sink is retained, and neither `process_packet` nor `finish` formats, allocates for diagnostics, calls a callback or acquires a lock. The counting allocator runs with default and all features for every estimator. Both feature modes preserve the same packet and reset tests. CI runs locked build, tests and pedantic Clippy for default and all features, plus nightly rustfmt.

The opt-in `performance-analysis` feature compiles two offline examples without adding processing-path instrumentation or a dependency. Run `cargo run --release --locked -p noise-oxydation-pipeline --features performance-analysis --example packet_bench` for warmed, identical-input scalar timing. The benchmark warms one 2,000-packet call, resets, measures packets 0–249 for learning, excludes 250–299 around the cutoff, and measures 300–1999 for suppression. It times `finish` separately and computes p50/p95 by nearest rank on sorted packet durations. This run used VincentacStudio, macOS arm64, Rust 1.98.1, release:

| Estimator | Learning p50 / p95 (µs) | Suppression p50 / p95 (µs) | Finish (µs) |
| --- | ---: | ---: | ---: |
| SPP-MMSE | 6.12 / 11.58 | 17.42 / 34.92 | 19.00 |
| MCRA | 6.12 / 11.58 | 19.00 / 38.79 | 20.75 |
| Minimum | 6.21 / 11.62 | 18.50 / 37.75 | 20.92 |

These local measurements are not a target-platform guarantee; no per-stage need for `minitrace` was demonstrated.

The replay example accepts a caller-supplied 8 kHz mono PCM WAV. Run `cargo run --release --locked -p noise-oxydation-pipeline --features performance-analysis --example wav_replay -- /private/tmp/OSR_us_000_0010_8k.wav /private/tmp/OSR_us_000_0010_8k-prepared.mulaw`. The representative source is [OSR_us_000_0010_8k.wav](https://www.voiptroubleshooter.com/open_speech/american/OSR_us_000_0010_8k.wav), credited to **Open Speech Repository**. Its verified SHA-256 is `a4bf9becd046d7aedb6d05b6e12347a6294a44f74d263089c636fb0a2b1e6561`, with `N = 268985` samples. The deterministic prepared μ-law stream hashes to `ac50c854bbc0500ee478376a1bc74e339d53257170974685403a1674430a082f`. Neither the source clip nor generated audio is committed.

Noise-only calibration occupies `[0,40000)`, post-calibration noise `[40000,48000)`, and noisy recorded speech follows. Each estimator received 317,120 input samples and returned exactly 317,120 valid samples. RMS output/input uses only decoded μ-law noise in `[40800,47200)`. Pearson correlation and gain-sensitive projection `Σ(output × clean)/Σ(clean²)` use `[48800,48000+N-800)` against the μ-law-quantized clean WAV.

| Estimator | Noise RMS ratio | Speech correlation | Speech projection gain |
| --- | ---: | ---: | ---: |
| SPP-MMSE | 0.1040 | 0.8902 | 0.5910 |
| MCRA | 0.1070 | 0.9634 | 0.8383 |
| Minimum | 0.1563 | 0.9586 | 0.8303 |

The [replay evidence](docs/real-audio-evidence.md) gives the exact seed, preparation, formulas, hash commands and limitations.

Numerical and synthetic tests still cover estimator and tonal arithmetic, intro bypass, finite output, stationary-noise attenuation, voice with tonal interference, and reset. The pinned [Go reference observations](docs/reference-observations.md) record a 68.6% component-level Go sustained-voice result, the absence of a shipped top-level Go MCRA pipeline, and unresolved formula and flush behavior questions. The real-speech replay measures this Rust path only; it does not prove broad speech quality or Go numerical parity. The continuing item retains scalar-versus-SIMD investigation and the final light/dark guide.
