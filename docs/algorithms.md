# Algorithms

This document describes the signal processing that `noise-oxydation` implements today, stage by stage, with the
equations, defaults, and units the code uses. Packet timing, calibration timing, and ownership live in
[ARCHITECTURE.md](../ARCHITECTURE.md); differences from the Go reference live in [reference-log.md](reference-log.md).

**Status.** The default path is implemented: μ-law decode, high-pass filter, STFT, SPP-MMSE noise estimation with
quiet-intro calibration, decision-directed SNR, Log-MMSE suppression, ISTFT, and μ-law encode. The MCRA and minimum
noise estimators and tonal transient suppression are not implemented yet; they are the remaining work of the port.

## Signal Flow

For every 160-byte packet of a call, in order:

1. Decode each μ-law byte to a normalized sample.
2. High-pass filter the samples (state carries across packets).
3. Append them to the analysis buffer. Each time 256 samples are buffered, analyze one frame and advance by 128.
4. For each frame `t`:
   1. compute the power spectrum;
   2. update the noise estimate (calibration mean or adaptive SPP-MMSE);
   3. if the frame is a calibration frame, keep the spectrum unchanged; otherwise apply the Log-MMSE gain;
   4. overlap-add the inverse transform and finalize 128 output samples.
5. Encode the finalized samples to μ-law and queue them; emit one packet per input packet after the two-packet
   delay.

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

A frame is a calibration frame when `128t + 256 ≤ floor(calibration_duration · 8000)`. During calibration frames the
estimate is the running mean of the floored observed power per bin, accumulated in `f64`, and the spectrum passes
through unchanged. At the first non-calibration frame the adaptive update starts from that mean. If the duration is
shorter than one frame, there is no calibration frame and the first frame initializes `N = P` (and is enhanced).

| Parameter | Default | Valid range |
| --- | --- | --- |
| `calibration_duration` | 5 s | any `Duration` whose sample count fits `u64`; zero disables calibration |

The estimator assumes the intro contains only noise. Speech during calibration is passed through un-enhanced and is
averaged into the noise estimate, so the estimate is too high when enhancement starts and speech bands are
over-suppressed until the adaptive update tracks back down. How long that takes has not been measured yet; it is part
of the planned audio evidence.

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

## Configuration Validation

`CallEnhancer::new` validates every field in the tables above and returns a `ConfigError` naming the first invalid
field and its constraint (`ConfigError::OutOfRange`), a minimum gain above the maximum gain
(`ConfigError::MinGainAboveMaxGain`), or a calibration duration too long to count in samples
(`ConfigError::CalibrationTooLong`). NaN and infinities are always rejected. Invalid values are never replaced by
defaults.
