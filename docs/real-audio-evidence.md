# Real speech and offline timing evidence

The offline replay exercises the public 160-byte μ-law packet API with one deterministic input stream. Its speech source is [OSR_us_000_0010_8k.wav](https://www.voiptroubleshooter.com/open_speech/american/OSR_us_000_0010_8k.wav), credited to **Open Speech Repository**. The [catalog](https://www.voiptroubleshooter.com/open_speech/american.html) identifies it as 8 kHz 16-bit PCM. The retrieved file is RIFF/WAVE, mono, 8 kHz, signed 16-bit little-endian PCM, 268,985 samples (33.623125 seconds), 538,014 bytes, SHA-256 `a4bf9becd046d7aedb6d05b6e12347a6294a44f74d263089c636fb0a2b1e6561`. The source clip and all generated audio stay outside Git.

## Reproduce the replay

Download the linked WAV outside the checkout, then verify its bytes and run the feature-gated example:

```sh
shasum -a 256 /private/tmp/OSR_us_000_0010_8k.wav
cargo run --release --locked -p noise-oxydation-pipeline --features performance-analysis --example wav_replay -- /private/tmp/OSR_us_000_0010_8k.wav /private/tmp/OSR_us_000_0010_8k-prepared.mulaw /private/tmp/oxydation-replay
shasum -a 256 /private/tmp/OSR_us_000_0010_8k-prepared.mulaw
```

On systems with `sha256sum` instead of `shasum`, use `sha256sum` for the two hash commands. The example accepts any 8 kHz mono 16-bit PCM WAV longer than 1,600 samples. Its second and third arguments are optional local destinations for the prepared μ-law stream and per-estimator μ-law output. The WAV parser rejects unsupported or malformed format and chunk metadata. Compare the source hash with the value above before treating a run as this representative experiment.

Preparation uses LCG seed `0x5eed1234`, recurrence `state = state × 1664525 + 1013904223` modulo 2³², and noise `((state >> 16) as i16)/32768 × 0.03`. Samples `[0,40000)` contain noise alone during the five-second calibration. Samples `[40000,48000)` contain noise alone after calibration. The source speech plus continued noise begins at sample 48,000. The last partial 160-sample input packet is padded with μ-law silence; the clip's 268,985 speech samples produce 317,120 packet input samples in all. The prepared input SHA-256 is `ac50c854bbc0500ee478376a1bc74e339d53257170974685403a1674430a082f`. All three estimators receive those same bytes through `Pipeline::process_packet` and `finish`.

The noise window is `[40800,47200)`, 6,400 samples after the 40,064-sample frame cutoff and before speech. The speech window is `[48800,48000+N-800) = [48800,316185)`, where `N = 268985` source samples. An arbitrary source with `N ≤ 1600` is rejected so that this speech window cannot be empty.

All metric samples are decoded μ-law values divided by 32,768. For a window `W`, `RMS(x,W) = sqrt(Σ(i∈W) xᵢ² / |W|)`. The noise ratio is `RMS(output,W) / RMS(input,W)` on the noise window; lower means less residual noise, but says nothing alone about speech quality. The clean speech reference is `cᵢ = decode(encode(sourceᵢ)) / 32768`, so it includes μ-law quantization. On the speech window, correlation is `Σ((yᵢ−ȳ)(cᵢ−c̄)) / sqrt(Σ(yᵢ−ȳ)² × Σ(cᵢ−c̄)²)`. Projection gain is `Σ(yᵢcᵢ) / Σ(cᵢ²)`; unlike correlation, it reveals attenuation along the clean speech reference.

| Estimator | Valid output samples | Noise RMS ratio | Speech correlation | Speech projection gain |
| --- | ---: | ---: | ---: | ---: |
| SPP-MMSE | 317,120 | 0.1040 | 0.8902 | 0.5910 |
| MCRA | 317,120 | 0.1070 | 0.9634 | 0.8383 |
| Minimum | 317,120 | 0.1563 | 0.9586 | 0.8303 |

SPP-MMSE suppressed noise slightly more than MCRA here, but retained less clean speech projection. Inspection of its probability-smoothed noise update and a 40,000-sample-by-40,000-sample projection check showed attenuation across the clip, not a packet-count or one-time onset failure. Its observed projection ranged roughly 0.50–0.74 by segment against the original PCM reference, compared with roughly 0.79–0.88 for MCRA. This evidence does not isolate a defect in the supported DSP path, so the estimator formula was not changed solely to improve one clip. The metric windows exclude the intro bypass and leading/trailing speech edges; one clip, one noise seed and one amplitude do not establish broad quality or exact Go numerical parity. The [reference log](reference-observations.md) retains the unresolved Go formula questions.

## Explicit status logging

Build with `--features logging` to call `Pipeline::write_status(&mut sink)` outside the audio thread. The caller owns the `std::fmt::Write` sink. A focused test compares the same packet stream with and without status writes and checks active, finished and reset counters. Run it with `cargo test --locked -p noise-oxydation-pipeline --all-targets --features logging`; `cargo test --locked --workspace --all-targets --all-features` also exercises the counting allocator and packet lifecycle under both features. Default builds have no diagnostic callback or dependency.

## Packet timing

Run the scalar benchmark separately from the replay so they do not contend for the CPU:

```sh
cargo run --release --locked -p noise-oxydation-pipeline --features performance-analysis --example packet_bench
```

This run used VincentacStudio, macOS arm64 (Apple Silicon), Rust 1.98.1, and a release build. Each estimator receives 2,000 identical `[0x80; 160]` packets in a complete warmup call, then a reset and a measured 2,000-packet call. Timing wraps only each `process_packet` call; `finish` is timed separately. Packets 0–249 are the calibration input group, 250–299 are omitted as a cutoff transition, and 300–1999 are the suppression group. Each group's p50 and p95 are nearest-rank values after sorting individual packet nanoseconds. These are local scalar measurements, not a target-platform latency guarantee.

| Estimator | Learning p50 / p95 (µs) | Suppression p50 / p95 (µs) | Finish (µs) |
| --- | ---: | ---: | ---: |
| SPP-MMSE | 6.12 / 11.58 | 17.42 / 34.92 | 19.00 |
| MCRA | 6.12 / 11.58 | 19.00 / 38.79 | 20.75 |
| Minimum | 6.21 / 11.62 | 18.50 / 37.75 | 20.92 |

The stdlib `Instant` measurement distinguishes calibration, suppression, and finish on this full packet path. It has not demonstrated a per-stage tracing need, so this checkpoint adds no `minitrace` dependency. SIMD assessment remains in the next checkpoint; this run alone does not establish a useful vector speedup.
