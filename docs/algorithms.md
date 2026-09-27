# Algorithms

This document describes the signal processing that `noise-oxydation` implements today, stage by stage, with the
equations, defaults, and units the code uses. Packet timing, calibration timing, and ownership live in
[ARCHITECTURE.md](../ARCHITECTURE.md); differences from the Go reference live in [reference-log.md](reference-log.md).

**Status.** The complete documented processing path is implemented: μ-law decode, high-pass filter, STFT, noise
estimation with quiet-intro calibration (SPP-MMSE by default, MCRA or the minimum estimator on request),
decision-directed SNR, Log-MMSE suppression, tonal transient suppression (enabled by default), ISTFT, and μ-law
encode. Enhancement quality on real speech has not been measured yet; that is the planned audio evidence.

## Signal Flow

For every 160-byte packet of a call, in order:

1. Decode each μ-law byte to a normalized sample.
2. High-pass filter the samples (state carries across packets).
3. Append them to the analysis buffer. Each time 256 samples are buffered, analyze one frame and advance by 128.
4. For each frame `t`:
   1. compute the power spectrum `P`;
   2. update the noise estimate with the configured estimator (calibration mean during calibration frames);
   3. if the frame is a calibration frame, keep the spectrum unchanged. Otherwise:
      1. if interference suppression is enabled, run the tonal transient detector on the original `P`;
      2. apply the Log-MMSE gain, which updates the decision-directed state from its own clean-speech power;
      3. multiply each bin by the tonal gain, clamped to `[0, 1]`;
   4. overlap-add the inverse transform and finalize 128 output samples.
5. Encode the finalized samples to μ-law and queue them; emit one packet per input packet after the two-packet
   delay.

The tonal detector never runs on calibration frames, so its first run (which only initializes it) is on the first
enhanced frame. It reads the power before suppression, and the tonal gain never feeds back into the decision-directed
recursion.

At drain, the buffered remainder is analyzed once as a zero-padded frame (classified like any other frame), the
synthesis tail finalizes the last 128 overlap samples, and finalized samples beyond the end of the input are dropped.

## μ-law Codec

G.711 μ-law with bias 132 (`0x84`) and clip level 32635.

- **Decode.** A byte is the one's complement of `sign | segment | mantissa` (1, 3, 4 bits). The 16-bit magnitude is
  `((8·mantissa + 132) · 2^segment) − 132`, negated when the sign bit is set, then divided by 32768. Decoded values
  lie in `[−32124, 32124] / 32768`.
- **Encode.** The sample is clamped to `[−1, 1]`, multiplied by 32767, and truncated toward zero to 16-bit PCM. The
  magnitude is clipped to 32635 and biased by 132; the segment is the position of its highest set bit minus 7, the
  mantissa is the next four bits, and the result is complemented.

The decode and encode scales differ (32768 versus 32767), as in the reference. Every code survives a decode–encode
round trip except `0x7F` (negative zero), which re-encodes as `0xFF`.

## High-Pass Filter

```text
y[n] = x[n] − x[n−1] + r·y[n−1],   r = exp(−2π·fc / 8000)
```

| Parameter | Default | Valid range |
| --- | --- | --- |
| `high_pass.cutoff_hz` (`fc`) | 80 Hz | `(0, 4000)` Hz |

`fc` is the pole parameter, not the −3 dB point. With the default, `r ≈ 0.9391` and the magnitude response
`|1 − e^{−jω}| / |1 − r·e^{−jω}|` is −3 dB near 75.3 Hz, −2.74 dB at 80 Hz, and +0.27 dB at 4 kHz. DC is removed.

## Short-Time Fourier Transform

- **Frames.** 256 samples every 128 samples (50 % overlap); frame `t` covers input samples `[128t, 128t + 256)`.
- **Window.** Symmetric Hann, `w[n] = 0.5 − 0.5·cos(2πn / 255)`, zero at both ends; used for analysis and synthesis.
- **Transform.** In-place radix-2 complex FFT of size 256 with twiddles `exp(−2πik/256)` computed in `f64` and stored
  as `f32`. The one-sided spectrum has 129 bins of 31.25 Hz; the power spectrum is `P[k] = |X[k]|²`.
- **Synthesis.** Bins 1–127 are mirrored as complex conjugates to rebuild the full spectrum. The inverse FFT (scaled by
  1/256) is multiplied by the window; the windowed samples and the squared window weights are overlap-added. The first
  128 accumulated samples are then final and are emitted as `sum / weight` where `weight > 1e−8`, else 0.

For unmodified spectra the synthesis reconstructs the input exactly, except that output sample 0 is always 0 (its
weight is 0) and output sample 1 is divided by `w[1]² ≈ 2.3e−8`, which amplifies single-precision FFT rounding by about
`1/w[1] ≈ 6600`.

## SPP-MMSE Noise Estimation

Per bin `k`, with observed power `P` and previous estimate `N` (both replaced by `floor` when below it, NaN, or
infinite):

```text
γ  = P / N
p  = 1 / (1 + ((1 − q) / q) · (1 + ξH1) · exp(−γ · ξH1 / (1 + ξH1)))    (f64; non-finite → 0; clamped to [0, 1])
p̄  = αP · p̄ + (1 − αP) · p
p  = pmax                                   if p̄ > stagnation threshold and p > pmax
N_mmse = (1 − p) · P + p · N
N  = αN · N + (1 − αN) · N_mmse             (then floored)
```

`p` is the posterior probability that speech is present, assuming a fixed a priori SNR `ξH1` under speech presence.
Loud frames yield `p ≈ 1`, which freezes the estimate; the stagnation cap releases it when speech-like power persists
(with the defaults, after about 44 consecutive frames), so the estimator can follow a genuine rise in noise.

| Parameter | Default | Valid range |
| --- | --- | --- |
| `spp_mmse.noise_smoothing` (`αN`) | 0.8 | `(0, 1)` |
| `spp_mmse.spp_smoothing` (`αP`) | 0.9 | `(0, 1)` |
| `spp_mmse.speech_prior` (`q`) | 0.5 | `(0, 1)` |
| `spp_mmse.fixed_prior_snr` (`ξH1`) | 31.622 776 6 (15 dB) | finite, `> 0` |
| `spp_mmse.stagnation_threshold` | 0.99 | `(0, 1]` |
| `spp_mmse.max_speech_probability` (`pmax`) | 0.99 | `(0, 1]` |
| `spp_mmse.floor` | 1e−12 | finite, `> 0` |

### Quiet-Intro Calibration

A frame is a calibration frame when `128t + 256 ≤ floor(calibration_duration · 8000)`. Every estimator handles
calibration the same way: during calibration frames the estimate is the running mean of the floored observed power per
bin, accumulated in `f64`, and the spectrum passes through unchanged. At the first non-calibration frame the adaptive
update starts from that mean. If the duration is shorter than one frame, there is no calibration frame and the first
frame initializes the estimator from its own floored power (and is enhanced). For SPP-MMSE that frame's estimate is
`N = P`; MCRA and the minimum estimator also run their first update on it, which with `S = N = P` again yields
`N = P`.

| Parameter | Default | Valid range |
| --- | --- | --- |
| `calibration_duration` | 5 s | any `Duration` whose sample count fits `u64`; zero disables calibration |

The estimator assumes the intro contains only noise. Speech during calibration is passed through un-enhanced and is
averaged into the noise estimate, so the estimate is too high when enhancement starts and speech bands are
over-suppressed until the adaptive update tracks back down. How long that takes has not been measured yet; it is part
of the planned audio evidence.

## MCRA Noise Estimation

Minimum-controlled recursive averaging. Per bin, with the observed power `P` floored (values below `floor`, NaN, and
infinities become `floor`):

```text
S  = αs · S + (1 − αs) · P                           smoothed power
Mc = min(Mc, S)                                       current-window minimum
R  = S / max(min(Mp, Mc), floor)                      ratio to the local minimum (Mp: previous-window minimum)
I  = 1 if R > δ, else 0                               speech indicator
p  = αp · p + (1 − αp) · I                            speech presence probability
a  = αd + (1 − αd) · p                                adaptive noise smoothing
N  = max(a · N + (1 − a) · P, floor)
```

At the first non-calibration frame `N`, `S`, `Mc`, and `Mp` start from the calibration mean (or from the first frame's
floored power without calibration), `p` from 0, and the window counter from 0. After every `W` updated frames `Mp`
takes the value of `Mc` and `Mc` restarts at `+∞`, so a genuine rise in the noise floor stops counting as speech
within two windows. When `p` is near 1 the estimate freezes; when it is 0 the estimate follows `P` with smoothing `αd`.

| Parameter | Default | Valid range |
| --- | --- | --- |
| `mcra.smoothing` (`αs`) | 0.8 | `[0, 1)` |
| `mcra.speech_smoothing` (`αp`) | 0.2 | `[0, 1)` |
| `mcra.noise_smoothing` (`αd`) | 0.95 | `[0, 1)` |
| `mcra.ratio_threshold` (`δ`) | 5 | finite, `> 1` |
| `mcra.window_frames` (`W`) | 50 frames (0.8 s) | `[1, 1000]` |
| `mcra.floor` | 1e−12 | finite, `> 0` |

## Minimum Noise Estimation

The simple estimator documented by the reference: the noise is the lower envelope of the smoothed power. Per bin,
with `P` floored as above:

```text
S = αs · S + (1 − αs) · P
N = max(min of S over the most recent W frames (including this one), floor)
```

The minimum is exact over a sliding window: the smoothed spectra of the last `W` frames are kept in a history
allocated at construction from `W`. At the first non-calibration frame `S` and every history slot start from the
calibration mean (or the first frame's floored power). Because the minimum of a fluctuating spectrum lies below its
mean, this estimate is biased low; it tracks rising noise only after `W` frames.

| Parameter | Default | Valid range |
| --- | --- | --- |
| `minimum.smoothing` (`αs`) | 0.8 | `[0, 1)` |
| `minimum.window_frames` (`W`) | 50 frames (0.8 s) | `[1, 1000]` |
| `minimum.floor` | 1e−12 | finite, `> 0` |

## Decision-Directed SNR

The suppressor works with an overestimated noise power `N_eff = β · N`, where `β` is the Log-MMSE noise
overestimation and a negative or non-finite `N` counts as 0. Per bin:

```text
γ      = max(P, 0) / max(N_eff, floor)                  a posteriori SNR
ξ_inst = max(γ − 1, 0)
ξ      = ξ_inst                                         first enhanced frame of a call
ξ      = α · ξ_clean_prev + (1 − α) · ξ_inst            later frames
```

After the gain is applied, `ξ_clean_prev = max(S_hat, 0) / max(N_eff, floor)` is stored for the next frame, where
`S_hat` is the estimated clean-speech power below.

| Parameter | Default | Valid range |
| --- | --- | --- |
| `decision_directed.alpha` (`α`) | 0.98 | `[0, 1)` |
| `decision_directed.floor` | 1e−12 | finite, `> 0` |

## Log-MMSE Suppression

```text
v     = max(γ · ξ / (1 + ξ), floor)
G     = ξ / (1 + ξ) · exp(E1(v) / 2)       (f64; non-finite → 0; then clamped to [min_gain, max_gain])
Y[k]  = G · X[k]
S_hat = G² · P
```

`E1(x) = ∫ₓ^∞ e^{−t}/t dt` is evaluated in `f64` with its power series `−γ_E − ln x − Σ (−x)^k / (k·k!)` for
`x ≤ 1` and a continued fraction (modified Lentz method) for `x > 1`. At high SNR `E1(v) → 0` and `G` approaches the
Wiener gain `ξ / (1 + ξ)`; when `ξ = 0` the gain is 0 before clamping, so pure noise is attenuated to `min_gain`
(−26 dB with the default).

| Parameter | Default | Valid range |
| --- | --- | --- |
| `log_mmse.min_gain` | 0.05 | `[0, 1]`, at most `max_gain` |
| `log_mmse.max_gain` | 1 | `(0, 1]` |
| `log_mmse.floor` | 1e−12 | finite, `> 0` |
| `log_mmse.noise_overestimation` (`β`) | 1.25 | finite, `> 0` |

## Tonal Transient Suppression

A per-bin attenuation of narrow foreground tones (beeps, whistles, chirps) that appear suddenly, move in frequency,
or persist, while protecting peaks with harmonic support as voiced speech. It is enabled by default
(`InterferenceConfig::TonalTransient`) and can be disabled (`InterferenceConfig::Disabled`).

The detector keeps the previous frame's floored power. Its first run (the first enhanced frame of a call) only stores
that power and yields unit gains. On each later frame every target gain starts at 1, and each bin `k` from
`max(local_radius, min_frequency_bin)` to `128 − local_radius` whose floored power `c` is not exceeded by any bin
within `local_radius` is scored (sums in `f64`):

```text
T      = 10 · log10(c / max(mean of P at offsets ±3…±6 inside the spectrum, floor))      tonal prominence, dB
F      = max(10 · log10(c / P_prev[k]), 0)                                                positive flux, dB
M      = 0 if the strongest previous-frame bin within ±movement_search_radius (the first maximum wins) has less
         than movement_min_relative_power · c, else norm(|k − k_prev|, movement_start_bins, movement_full_bins)
H      = 0 if c ≤ floor, else the maximum over the distinct bins round(k/2), round(k/3), round(k/4), 2k, 3k, 4k
         inside (0, 128] of clamp(max of P within ±harmonic_tolerance_bins / (harmonic_relative_power · c), 0, 1)
S      = clamp(norm(T, tonal_start_db, tonal_full_db)
               · max(norm(F, flux_start_db, flux_full_db), M, 0.35 · norm(T, tonal_start_db, tonal_full_db))
               · (1 − H), 0, 1)
target = max(min_gain, 1 − strength · S)                                                  when S > 0
```

`norm(v, a, b)` is 0 for `v ≤ a`, 1 for `v ≥ b`, and `(v − a)/(b − a)` between; `round(k/n)` rounds halves away from
zero. The term `0.35 · norm(T)` is persistent tonal evidence: a stationary tone with no flux or movement still scores
0.35 of its prominence. Each scored peak lowers the target gains of the bins within `±R` (`R = spread_radius`):
`target[k + d] = min(target[k + d], 1 − (1 − target) · (R − |d| + 1) / (R + 1))`. Finally every bin's gain follows its
target, falling with `attack` and rising with `release`, and is clamped to `[min_gain, 1]`:

```text
G = attack · G + (1 − attack) · target       if target < G
G = release · G + (1 − release) · target     otherwise
```

With the defaults only bins 64–126 (2000–3937.5 Hz) are analyzed, so voiced speech harmonics below 2 kHz are never
attenuated, and the gain never falls below 0.5 (−6.02 dB). The background excludes the two bins on each side of the
peak because the Hann main lobe spreads a pure tone across them.

| Parameter | Default | Valid range |
| --- | --- | --- |
| `tonal_transient.local_radius` | 2 bins | `[1, 32]` |
| `tonal_transient.tonal_start_db`, `tonal_full_db` | 5 dB, 14 dB | finite, `0 ≤ start < full` |
| `tonal_transient.flux_start_db`, `flux_full_db` | 3 dB, 18 dB | finite, `0 ≤ start < full` |
| `tonal_transient.movement_search_radius` | 6 bins | `[1, 64]` |
| `tonal_transient.movement_start_bins`, `movement_full_bins` | 1, 4 bins | `1 ≤ start < full ≤ 64` |
| `tonal_transient.min_frequency_bin` | 64 (2 kHz) | `[1, 128]` |
| `tonal_transient.movement_min_relative_power` | 0.1 | `(0, 1]` |
| `tonal_transient.harmonic_tolerance_bins` | 1 bin | `[0, 16]` |
| `tonal_transient.harmonic_relative_power` (`ρ`) | 0.15 | `(0, 1]` |
| `tonal_transient.strength` | 0.5 | `[0, 1]` |
| `tonal_transient.min_gain` | 0.5 | `[0, 1]` |
| `tonal_transient.attack` | 0.3 | `[0, 1)` |
| `tonal_transient.release` | 0.85 | `[0, 1)` |
| `tonal_transient.spread_radius` | 2 bins | `[0, 64]` |
| `tonal_transient.floor` | 1e−12 | finite, `> 0` |

The background guard (2 bins), background search (6 bins), and persistent tonal weight (0.35) are constants, as in
the reference.

## Configuration Validation

`CallEnhancer::new` validates every field of the high-pass filter, the chosen noise estimator, the decision-directed
estimator, Log-MMSE, and (when enabled) the tonal transient detector, in the tables above. Only the selected
alternatives exist in the configuration and are validated. It returns a `ConfigError` naming the first invalid field
and its constraint:

- `ConfigError::OutOfRange` for a real-valued field outside its range;
- `ConfigError::CountOutOfRange` for a frame or bin count outside its range;
- `ConfigError::StartNotBelowFull` when a tonal start threshold is not strictly below its full threshold;
- `ConfigError::MinGainAboveMaxGain` when the Log-MMSE minimum gain exceeds the maximum gain;
- `ConfigError::CalibrationTooLong` when the calibration duration is too long to count in samples.

NaN and infinities are always rejected. Invalid values are never replaced by defaults.
