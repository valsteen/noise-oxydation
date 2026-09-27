# Real speech and offline timing evidence

This page records the conservative mode's original replay and portable/native codegen comparison. The [two-mode comparison](processing-modes.md) adds experimental lower-delay timing, a second voice, and the observed quality tradeoff.

The offline replay exercises the public 160-byte μ-law packet API with one deterministic input stream. Its speech source is [OSR_us_000_0010_8k.wav](https://www.voiptroubleshooter.com/open_speech/american/OSR_us_000_0010_8k.wav), credited to **Open Speech Repository**. The [catalog](https://www.voiptroubleshooter.com/open_speech/american.html) identifies it as 8 kHz 16-bit PCM. The retrieved file is RIFF/WAVE, mono, 8 kHz, signed 16-bit little-endian PCM, 268,985 samples (33.623125 seconds), 538,014 bytes, SHA-256 `a4bf9becd046d7aedb6d05b6e12347a6294a44f74d263089c636fb0a2b1e6561`. The source clip and all generated audio stay outside Git.

## Reproduce the replay

Download the linked WAV outside the checkout, then verify its bytes and run the feature-gated example:

```sh
shasum -a 256 /private/tmp/OSR_us_000_0010_8k.wav
cargo run --release --locked -p noise-oxydation-pipeline --features performance-analysis --example wav_replay -- /private/tmp/OSR_us_000_0010_8k.wav /private/tmp/OSR_us_000_0010_8k-prepared.mulaw /private/tmp/oxydation-replay
shasum -a 256 /private/tmp/OSR_us_000_0010_8k-prepared.mulaw
```

On systems with `sha256sum` instead of `shasum`, use `sha256sum` for the two hash commands. The example accepts any 8 kHz mono 16-bit PCM WAV longer than 1,600 samples. Its second and third arguments are optional local destinations for the prepared μ-law stream and per-estimator μ-law output; a fourth optional argument selects `conservative` (the default) or `experimental`. The WAV parser rejects unsupported or malformed format and chunk metadata. Compare the source hash with the value above before treating a run as this representative experiment.

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

## Packet timing and codegen

The final comparison used the same 317,120-byte prepared OSR μ-law stream above for both builds and every estimator. Its SHA-256 was verified as `ac50c854bbc0500ee478376a1bc74e339d53257170974685403a1674430a082f` before timing. The stream contains 1,982 whole 160-byte packets. `packet_bench` reads it before the clock starts, warms a complete call per estimator, calls `finish`, resets, and then times a second complete call. Packets 0–249 form the learning group, 250–299 are excluded around the cutoff, and 300–1981 form the suppression group. Each packet group's p50 and p95 use nearest rank on sorted `Instant` durations; `finish` is timed separately. Every measured call returned 317,120 valid samples.

Both builds used the same checkout, Rust 1.98.1 (LLVM 22.1.8), locked dependencies, release profile, `performance-analysis` feature, and macOS arm64 host. The portable build used the default `aarch64-apple-darwin` codegen with `RUSTFLAGS` unset. The native build differed only by `-C target-cpu=native`, which this host resolves to `apple-m1`. To reproduce, first prepare and hash the stream as above, then build both examples into separate directories:

```sh
shasum -a 256 /private/tmp/OSR_us_000_0010_8k-prepared.mulaw
CARGO_TARGET_DIR=/private/tmp/noise-oxydation-ab/portable cargo build --release --locked -p noise-oxydation-pipeline --features performance-analysis --examples
CARGO_TARGET_DIR=/private/tmp/noise-oxydation-ab/native RUSTFLAGS='-C target-cpu=native' cargo build --release --locked -p noise-oxydation-pipeline --features performance-analysis --examples
for run in 1 2 3 4 5; do
  /private/tmp/noise-oxydation-ab/portable/release/examples/packet_bench /private/tmp/OSR_us_000_0010_8k-prepared.mulaw > "/private/tmp/noise-oxydation-ab/portable-${run}.txt"
  /private/tmp/noise-oxydation-ab/native/release/examples/packet_bench /private/tmp/OSR_us_000_0010_8k-prepared.mulaw > "/private/tmp/noise-oxydation-ab/native-${run}.txt"
done
```

Five independent portable/native process pairs were run in that order. Values below are the median of the five run-level results, followed by the full run-level range, all in microseconds. The alternating runs expose variation rather than claiming a precise platform latency.

| Estimator | Metric | Portable median [range] | Native median [range] |
| --- | --- | ---: | ---: |
| SPP-MMSE | Learning p50 | 6.17 [6.17–6.29] | 6.17 [6.17–6.25] |
| SPP-MMSE | Learning p95 | 11.58 [11.54–12.42] | 11.71 [11.58–11.92] |
| SPP-MMSE | Suppression p50 | 21.79 [21.67–21.88] | 21.79 [21.71–21.88] |
| SPP-MMSE | Suppression p95 | 44.50 [43.58–44.71] | 44.17 [43.83–44.54] |
| SPP-MMSE | Finish | 36.92 [36.79–39.96] | 36.67 [36.50–48.96] |
| MCRA | Learning p50 | 6.17 [6.17–6.25] | 6.17 [6.17–6.54] |
| MCRA | Learning p95 | 11.67 [11.62–11.71] | 11.71 [11.67–12.54] |
| MCRA | Suppression p50 | 22.96 [22.79–23.08] | 22.88 [22.83–23.08] |
| MCRA | Suppression p95 | 46.71 [46.42–47.04] | 46.46 [46.17–47.38] |
| MCRA | Finish | 40.58 [40.46–41.04] | 43.21 [40.33–70.50] |
| Minimum | Learning p50 | 6.21 [6.17–6.29] | 6.17 [6.17–6.25] |
| Minimum | Learning p95 | 11.71 [11.62–12.17] | 11.67 [11.62–11.92] |
| Minimum | Suppression p50 | 23.79 [23.58–23.83] | 23.96 [23.67–24.38] |
| Minimum | Suppression p95 | 48.42 [48.38–48.62] | 48.33 [48.17–49.08] |
| Minimum | Finish | 40.21 [39.92–42.29] | 40.38 [40.04–42.92] |

The native suppression p95 differences are smaller than or overlap the observed run variation, and its finish timing includes outliers. There is no useful end-to-end improvement here to justify a separate explicit SIMD path.

For an output check, the two `wav_replay` binaries independently regenerated the prepared stream from the hash-verified source WAV. Each prepared stream matched the input SHA-256 above. Each estimator produced 317,120 valid output bytes in both modes, and the portable/native μ-law files compared byte-for-byte. The SPP-MMSE output hash was `d1c85b74f385e53a66c565096d182f33a445f214721c5a7274f9437983c5ff17`, MCRA was `d1d930735a10fe340c9b559cdbe7384adf3c9fac5272f4686449677ed23e2a5f`, and minimum was `537425a9daf0fff4efecba51fecfa3a78ebdedf5a2cadb5c131aec10aefcc150`. The allowed decoded error was zero, and observed error was zero. The benchmark and replay use the same public packet path and build settings.

Disassembly was inspected for `noise_oxydation_dsp::fft` and `<noise_oxydation_dsp::Enhancer>::push` in both `packet_bench` binaries with `xcrun llvm-objdump --disassemble --demangle --disassemble-symbols='noise_oxydation_dsp::fft,<noise_oxydation_dsp::Enhancer>::push'`. The selected instruction streams were identical after the binary-path header. Both contain four-lane NEON arithmetic, including `fmul.4s` and `fmla.4s` in the frame-processing path, and scalar `fmadd s` in FFT butterflies. The inspected paths therefore already use a mix of vector and scalar instructions in both modes; neither build is a scalar-only baseline, and the timing difference cannot be credited to new vectorization.

The previous 2,000-packet timing used repeated `[0x80; 160]`, which decodes to near-full-scale +32,124 PCM and is poor representative audio. Its suppression p50/p95 values were 17.42/34.92 µs for SPP-MMSE, 19.00/38.79 µs for MCRA, and 18.50/37.75 µs for minimum on this host. Those historical values demonstrate the earlier timing tool, not OSR-stream or SIMD behavior. Neither measurement establishes general speech quality, another device's latency, or Go numerical parity. `Instant` has not shown a per-stage tracing need, so no `minitrace` dependency was added.
