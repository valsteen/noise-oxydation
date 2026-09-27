# Real-Speech Evaluation

This page shows how Noise Oxydation performs on real recorded speech mixed with real recorded noise, and how to
reproduce the result. The replay feeds each scenario to `CallEnhancer` packet by packet, exactly as a live call would,
writes the clean, noisy and enhanced audio as WAV files you can listen to, and reports objective noise-reduction and
speech-preservation metrics for every noise estimator.

In short, with 5 dB speech-to-noise ratio:

- In speech pauses the enhancer removes 25–34 dB of office noise and 8–13 dB of cafeteria noise. Part of the office
  figure comes from the 80 Hz high-pass filter, because the office recording carries strong low-frequency rumble.
- Segmental SNR improves by 4.3–5.6 dB in the office scenes and by 2.6–2.7 dB in the cafeteria scene.
- Speech loses 0.3–0.7 dB of level in the office scenes and 0.9–1.7 dB in the cafeteria scene.
- The log-spectral distance to the clean speech improves in the cafeteria scene (from 13.1 dB to 10.6–11.3 dB) and
  worsens in the office scenes (from 6.1–6.5 dB to 6.8–8.8 dB). The metric counts low-level spectral detail that
  suppression removes as distortion.
- Speech during the quiet intro makes the first second after calibration up to 5.1 dB quieter than in an undisturbed
  call (MCRA). The enhanced output matches the undisturbed run within 1 dB from 1 s after calibration for SPP-MMSE
  and the minimum estimator, and from 3 s for MCRA.

These are objective proxies measured on one talker per scene, not perceptual ratings. See
[Measurement limits](#measurement-limits) before drawing conclusions.

## Reproduce

From a fresh clone, with the pinned Rust toolchain (`rust-toolchain.toml`), `curl` and `python3`:

```bash
scripts/fetch-evaluation-audio.sh
cargo run --locked --release -p noise-oxydation-eval -- replay
```

The fetch script downloads the four pinned recordings into `audio/sources/`, verifies every file by SHA-256 (and each
DEMAND channel additionally by the zip member's CRC32), and stops with an error on any mismatch. It is idempotent:
verified files are kept on later runs. The replay reads `audio/sources/`, writes everything under `audio/out/`, and
prints the tables below. It takes about 3 s on the measurement machine described in
[performance.md](performance.md).

Both directories are under `/audio/`, which `.gitignore` excludes. Never commit downloaded or rendered audio; the
DEMAND license forbids redistributing derived mixes under other terms. The fetch, replay, benchmark and Go parity runs
are manual evidence runs; CI never downloads audio.

The optional comparison with the Go reference implementation (see
[reference-log.md](reference-log.md#measured-parity)) needs Go 1.27:

```bash
(cd tools/go-parity && for scenario in office-5db cafeteria-5db speech-after-calibration speech-during-calibration; do
  for estimator in spp-mmse mcra; do
    go run . enhance -estimator "$estimator" -in "../../audio/out/$scenario/noisy.ul" \
      -out "../../audio/out/$scenario/go-enhanced-$estimator.ul"
  done
done)
cargo run --locked --release -p noise-oxydation-eval -- compare \
  audio/out/office-5db/go-enhanced-spp-mmse.ul audio/out/office-5db/enhanced-spp-mmse.ul
```

## Sources And Licenses

| File in `audio/sources/` | Source | License and credit | SHA-256 |
| --- | --- | --- | --- |
| `OSR_us_000_0030_8k.wav` | [Open Speech Repository](https://www.voiptroubleshooter.com/open_speech/american.html), American English, Harvard sentences, 8 kHz, 46.9 s | Free use with the attribution "Open Speech Repository" | `d3e1cfba…43b622` |
| `OSR_us_000_0031_8k.wav` | Open Speech Repository, same set, 42.1 s, with a 0.68 s lead-in | Free use with the attribution "Open Speech Repository" | `dff349ae…57a43c` |
| `DEMAND_OOFFICE_ch01.wav` | [DEMAND](https://doi.org/10.5281/zenodo.1227121) (Thiemann, Ito and Vincent), `OOFFICE_16k.zip`, channel 1, 16 kHz, 300 s | CC BY-SA 3.0 | `a831879b…bc2f2` |
| `DEMAND_PCAFETER_ch01.wav` | DEMAND, `PCAFETER_16k.zip`, channel 1, 16 kHz, 300 s | CC BY-SA 3.0 | `854dd0e5…2550b` |

The DEMAND record's description states CC BY-SA 3.0, while its metadata field says CC BY 4.0. This project treats the
recordings as CC BY-SA 3.0, the more restrictive reading, and keeps every derived mix out of the repository. Only one
channel of each recording is needed, so the script fetches just the byte range holding that channel's compressed zip
member and inflates it locally. The script holds the full hashes, URLs and byte ranges.

## Scenarios

Every scenario is 8 kHz audio built the same way:

```text
noisy = [noise-only intro] ++ [speech + noise]
clean = [zeros]            ++ [speech]
```

Both are zero-padded at the end to a whole number of 160-sample packets.

- **Noise.** DEMAND is recorded at 16 kHz. It is decimated to 8 kHz with a 121-tap Blackman-windowed sinc low-pass
  (3850 Hz cutoff, unit DC gain) applied with zero phase before dropping every second sample. By design its response
  is flat within 0.01 dB below 3.4 kHz and at least 70 dB down above 4.3 kHz; the unit tests measure synthetic tones
  and assert at most 0.1 dB of passband deviation and at least 60 dB of stopband attenuation.
- **One noise excerpt.** Each scenario takes one contiguous excerpt starting 10 s into the decimated recording. It
  covers the intro and the speech, so the noise the enhancer learns during calibration is the noise that continues
  under the speech.
- **Target SNR.** The noise is scaled so that the speech-to-noise ratio over the samples of the speech's active frames
  (defined under [Metrics](#metrics)) equals the target. The replay then checks the peak: if the mix would exceed
  −1 dBFS, speech and noise are attenuated together. None of the current scenarios needed this headroom gain.
- **μ-law input.** The noisy mix is quantized to 16-bit PCM and μ-law encoded. The decoded μ-law bytes are both the
  enhancer's input and the noisy baseline of every metric.

| Scenario | Speech | Noise | Intro | SNR | Length | Noise gain |
| --- | --- | --- | --- | --- | --- | --- |
| `office-5db` | OSR 0030 | OOFFICE | 6 s | 5 dB | 52.9 s | +15.28 dB |
| `cafeteria-5db` | OSR 0030 | PCAFETER | 6 s | 5 dB | 52.9 s | +15.83 dB |
| `speech-after-calibration` | OSR 0031, first 4640 samples (0.58 s) trimmed | OOFFICE | 6 s | 5 dB | 47.6 s | +15.02 dB |
| `speech-during-calibration` | `speech-after-calibration` with its first 6 s removed | same | none | 5 dB | 41.6 s | same |

The trim of OSR 0031 is a whole number of frames, so it keeps the recording's frame grid, and it leaves 480 samples
(60 ms) before the first active frame. The replay refuses to run if fewer than 400 samples (50 ms) would remain.

The calibration pair is built so that only the timing differs. `speech-during-calibration` is exactly the
`speech-after-calibration` clean and noisy signal with the 6 s intro cut off. Every speech sample therefore has the
same noise under it in both runs, but in the during run the talker starts at 0 s, inside the 5 s calibration.

## Configurations

Each scenario is enhanced four times, always with the default 5 s calibration:

| Name | Noise estimator | Tonal transient suppression |
| --- | --- | --- |
| `spp-mmse` | SPP-MMSE (default) | on (default) |
| `mcra` | MCRA | on |
| `minimum` | Minimum estimator | on |
| `spp-mmse-no-interference` | SPP-MMSE | off |

All other parameters are the defaults in [algorithms.md](algorithms.md). No default was tuned for these results.

## Metrics

Metrics use 20 ms frames aligned with the input packets. The packet API preserves sample alignment: after the two
priming packets and the drain, output sample *n* is the enhanced input sample *n*. Unless stated otherwise, metrics
cover only frames that start at least 1 s after calibration ends, that is from 6 s.

Frame classes come from the clean reference. A frame is **active** when its energy is at least the loudest clean
frame's energy minus 30 dB, and a **pause** when it is at most that maximum minus 45 dB. Frames in between belong to
neither class. The clean signal is zero during the intro, so intro frames are pauses.

| Metric | Definition | Frames | Better |
| --- | --- | --- | --- |
| Noise attenuation | `10·log10(Σ noisy² / Σ enhanced²)` | pause | higher |
| Segmental SNR | mean of `clamp(10·log10(Σ clean² / Σ (y − clean)²), −10, 35)` for `y` = noisy or enhanced | active | higher |
| SNR improvement | segmental SNR of the enhanced output minus that of the noisy input | active | higher |
| Speech level change | `10·log10(Σ y² / Σ clean²)` | active | closer to 0 |
| Log-spectral distance (LSD) | mean of `sqrt(mean_k (10·log10 P_y[k] − 10·log10 P_clean[k])²)` | active | lower |

The LSD spectra `P` are 256-point Hann-windowed power spectra of the 256 samples centered on each frame. Each spectrum
is floored 60 dB below its own peak (and at 1e−20, so that an all-zero frame stays finite).

## Results

Numbers come from one replay on the measurement machine. They are deterministic for this machine and toolchain. The
noisy row describes the μ-law input; its speech level is above 0 dB because the noise adds energy.

### `office-5db`

1255 active and 406 pause frames.

| Signal | Noise attenuation | Segmental SNR | SNR improvement | Speech level | LSD |
| --- | ---: | ---: | ---: | ---: | ---: |
| noisy input | 0.00 | 3.00 | | +1.20 | 6.50 |
| `spp-mmse` | 26.10 | 8.24 | 5.24 | −0.29 | 7.13 |
| `mcra` | 32.13 | 7.44 | 4.44 | −0.52 | 8.54 |
| `minimum` | 25.24 | 7.53 | 4.53 | −0.53 | 7.77 |
| `spp-mmse-no-interference` | 26.10 | 8.29 | 5.30 | −0.28 | 7.13 |

### `cafeteria-5db`

1255 active and 406 pause frames.

| Signal | Noise attenuation | Segmental SNR | SNR improvement | Speech level | LSD |
| --- | ---: | ---: | ---: | ---: | ---: |
| noisy input | 0.00 | 1.04 | | +1.20 | 13.05 |
| `spp-mmse` | 11.04 | 3.77 | 2.73 | −1.39 | 10.65 |
| `mcra` | 12.58 | 3.66 | 2.61 | −1.74 | 11.31 |
| `minimum` | 8.38 | 3.66 | 2.62 | −0.90 | 10.61 |
| `spp-mmse-no-interference` | 11.03 | 3.78 | 2.74 | −1.38 | 10.64 |

### `speech-after-calibration`

1150 active and 374 pause frames.

| Signal | Noise attenuation | Segmental SNR | SNR improvement | Speech level | LSD |
| --- | ---: | ---: | ---: | ---: | ---: |
| noisy input | 0.00 | 2.98 | | +1.19 | 6.26 |
| `spp-mmse` | 28.79 | 8.51 | 5.53 | −0.30 | 6.78 |
| `mcra` | 32.91 | 7.67 | 4.69 | −0.51 | 8.30 |
| `minimum` | 26.99 | 7.80 | 4.82 | −0.50 | 7.74 |
| `spp-mmse-no-interference` | 28.79 | 8.56 | 5.59 | −0.29 | 6.77 |

### `speech-during-calibration`

968 active and 322 pause frames. Only frames from 6 s on count here, so the speech that overlapped calibration is
excluded; the next section measures that part.

| Signal | Noise attenuation | Segmental SNR | SNR improvement | Speech level | LSD |
| --- | ---: | ---: | ---: | ---: | ---: |
| noisy input | 0.00 | 3.01 | | +1.22 | 6.12 |
| `spp-mmse` | 29.45 | 8.42 | 5.41 | −0.34 | 6.79 |
| `mcra` | 33.77 | 7.30 | 4.29 | −0.68 | 8.80 |
| `minimum` | 27.80 | 7.72 | 4.71 | −0.54 | 7.77 |
| `spp-mmse-no-interference` | 29.45 | 8.48 | 5.47 | −0.33 | 6.77 |

### Reading the results

- **The office noise attenuation includes the high-pass filter.** The office intro measures −31.0 dBFS before and
  −44.2 dBFS after the enhancer's 80 Hz high-pass, so about 13 dB of that noise is low-frequency rumble the filter
  removes. That is why office attenuation can exceed the −26 dB Log-MMSE gain floor (`min_gain` 0.05). The
  cafeteria noise loses only 2 dB to the filter (−30.6 to −32.6 dBFS).
- **MCRA suppresses hardest.** It gives the most attenuation in every scenario, but also the largest speech level loss
  and LSD. SPP-MMSE, the default, gives the best segmental SNR in every scenario.
- **Babble is harder than office noise.** Cafeteria noise contains speech-like, non-stationary energy that the
  estimators partly track as speech, so attenuation and SNR gains are smaller and speech loses more level.
- **Tonal transient suppression is nearly inert on this material.** Neither recording contains beeps or whistles.
  With suppression on, segmental SNR drops by at most 0.06 dB and the other metrics move by at most 0.02 dB.

## Speech During Calibration

The quiet-intro assumption says a call starts with noise only (see
[ARCHITECTURE.md](../ARCHITECTURE.md#quiet-intro-calibration)). When the talker speaks during calibration, that speech
is learned as noise and the inflated estimate over-suppresses speech once enhancement starts.

To measure the recovery, the replay compares the two calibration-pair runs on identical content. For *k* = 0…9, the
during run's window [5 + *k*, 6 + *k*) s holds exactly the same speech and noise as the after run's window
[11 + *k*, 12 + *k*) s. In each window it reports the speech level change over active frames and the noise attenuation
over pause frames. The **recovery time** is the smallest *k* from which every window with speech has a during-run
speech level within 1 dB of the after-run level. Calibration ends at 5 s, so *k* counts seconds after calibration.

Speech level change in dB, during run / after run:

| *k* (s after calibration) | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | Recovery |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `spp-mmse` | −1.34 / −0.25 | −0.64 / −0.55 | −0.82 / −0.77 | −4.15 / −4.09 | −0.24 / −0.24 | −0.09 / −0.09 | −0.36 / −0.36 | −0.89 / −0.89 | −0.04 / −0.04 | −0.28 / −0.28 | 1 s |
| `mcra` | −5.62 / −0.47 | −3.31 / −1.30 | −2.95 / −1.83 | −7.91 / −7.43 | −0.29 / −0.29 | −0.14 / −0.13 | −0.83 / −0.80 | −1.88 / −1.90 | −0.06 / −0.06 | −0.42 / −0.31 | 3 s |
| `minimum` | −2.39 / −0.27 | −0.86 / −0.86 | −1.76 / −1.76 | −5.24 / −5.24 | −0.22 / −0.22 | −0.44 / −0.44 | −0.41 / −0.41 | −1.29 / −1.29 | −0.04 / −0.04 | −0.52 / −0.52 | 1 s |
| `spp-mmse-no-interference` | −1.33 / −0.24 | −0.63 / −0.53 | −0.82 / −0.77 | −4.15 / −4.09 | −0.23 / −0.23 | −0.08 / −0.08 | −0.35 / −0.35 | −0.89 / −0.89 | −0.04 / −0.04 | −0.23 / −0.23 | 1 s |

Window 3 is low in both runs, so that stretch of speech is attenuated regardless of calibration; only the difference
between the runs measures recovery. The inflated estimate also raises noise attenuation briefly: in window 1 the
during run attenuates pauses by 26.0 dB against 22.6 dB with SPP-MMSE, and by 29.7 dB against 22.3 dB with MCRA. From
window 3 on, the runs agree within 1.1 dB on noise attenuation for every estimator.

In this scene, speech during the intro costs SPP-MMSE and the minimum estimator about one second of reduced speech
level, at most 2.1 dB. It costs MCRA three seconds, with the first second 5.1 dB quieter than it should be. These
recovery times belong to this talker, noise and SNR; louder or longer speech in the intro would inflate the estimate
further.

The full per-window numbers are in `audio/out/speech-during-calibration/metrics.json` under `calibration_recovery`.

## Listening

Each `audio/out/<scenario>/` directory holds:

| File | Content |
| --- | --- |
| `clean.wav` | The clean speech reference, silent during the intro |
| `noisy.wav` | The μ-law input the enhancer received, decoded to 16-bit PCM |
| `enhanced-<configuration>.wav` | The enhanced output, decoded to 16-bit PCM |
| `noisy.ul`, `enhanced-<configuration>.ul` | The same audio as headerless 8 kHz μ-law, for `compare` and the Go harness |
| `metrics.json` | The metrics above, with more decimals |

All WAV files are 8 kHz mono 16-bit and play in any audio player. Listen to `noisy.wav` and
`enhanced-spp-mmse.wav` of the same scenario back to back, starting a few seconds before the speech (6 s). The first
5 s of every enhanced file is the un-enhanced calibration pass-through. In `speech-during-calibration`, the first
sentence starts during calibration: compare its enhanced second sentence with the same sentence in
`speech-after-calibration` to hear the recovery.

## Measurement Limits

- **Objective proxies only.** No PESQ, POLQA, STOI or other perceptual model was run, and no listening panel rated
  the audio. Segmental SNR and LSD reward and penalize different things, and neither is a perceived-quality score.
- **Mixed, not recorded, calls.** Speech and noise are added digitally at a fixed SNR, without room acoustics, a
  telephone channel or a codec other than G.711 μ-law.
- **One talker per scene** and read Harvard sentences, not conversation. The two speech recordings are from the same
  set, and only two noise types are covered.
- **The clean reference is not high-pass filtered.** Speech energy below 80 Hz that the enhancer removes counts
  against its segmental SNR and LSD.
- **μ-law quantization floor.** The input and output are μ-law, so the metrics include quantization noise of about
  −38 dB relative to the signal, and very quiet enhanced output quantizes coarsely.
- **Single machine and toolchain.** The outputs are deterministic on the measurement machine; other platforms may
  differ in the last bits of floating-point library functions.
- **Fixed scenario choices.** The noise offset (10 s), the 6 s intro, the 5 dB SNR and the frame-class thresholds are
  documented choices, not tuned values. Other choices move the numbers.
