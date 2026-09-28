# Pinned reference notes

The comparison source is `github.com/sghaida/noise-cancelation` at commit `cfc7520a0625da90e4ad4699541a6ffe98e7c637`. The pinned selectors used here are `README.md`, `dsp/noise/mcra.go`, `dsp/noise/sppmmse.go`, `dsp/snr/decision_directed.go`, `dsp/suppressor/logmmse.go`, and `dsp/interference/tonal_transient.go`; the earlier STFT observations below remain part of the recorded comparison.

## Evidence classification

| Claim | Classification | Notes |
| --- | --- | --- |
| The README's Pipeline API is illustrative, not an in-tree production package. | Documented | Do not treat the example API as a supported implementation surface. |
| For 960,000 samples, the pinned STFT emits 7,499 complete frames. | Static source finding and arithmetic | With a 256-sample frame and 128-sample hop, the count is `floor((960000 - 256) / 128) + 1 = 7499`. |
| STFT flush adds a padded frame, and ISTFT flush adds 128 samples. | Static source finding | The raw Go path therefore yields 960,128 samples. This differs from the 960,000 accepted input samples. |
| The README's zero-allocation wrapper claim conflicts with its benchmark rows. | Documented conflict | STFT reports 2,296 bytes and 3 allocations; ISTFT reports 512 bytes and 1 allocation. |
| At the 40,000-sample intro boundary, 311 complete 256-sample windows with a 128-sample hop lie wholly inside. | Static arithmetic | A separate reference helper rounds to 313. This Rust crate uses the complete-window rule. |
| Speech in the quiet intro can enter the noise estimate and later be suppressed. | Source-based inference | This is not a measured speech result. |
| Rust behavior is numerically equivalent to Go. | Not established | No audio parity measurement is claimed. |
| The public Rust processor improved one OpenSLR utterance after adding deterministic synthetic noise. | Measured example run | The noisy input and enhanced output are compared with the same clean samples; results and listening status are recorded in [`REPLAY.md`](REPLAY.md). This is not general speech-quality or natural-noise evidence. |
| A processor push fits within the 20 ms packet cadence on the documented host. | Measured example run | The release replay observed a 91,792 ns maximum over 1,083 pushes, about 0.46% of the cadence on one Mac Studio M1 Ultra. This host result is not a scheduling guarantee. |
| Portable SIMD or minitrace provides a useful measured end-to-end benefit. | Not established | No SIMD candidate or minitrace comparison was run; neither was added. The replay profile does not measure end-to-end scheduling. |

## Estimator and suppression provenance

| Current Rust behavior | Classification | Pinned reference and disposition |
| --- | --- | --- |
| Default minimum-noise estimation smooths power with 0.8, tracks the minimum over 50 frames, and applies a 1e-12 floor. | README-derived | The README specifies this estimator; the pinned Go source has no corresponding minimum-noise estimator implementation. |
| MCRA uses smoothing 0.8, speech-probability smoothing 0.2, noise smoothing 0.95, ratio threshold 5, a 50-frame minimum window, and a 1e-12 floor. | Source-confirmed parameters and equations | `dsp/noise/mcra.go`; Rust uses the accepted complete-window quiet intro rather than the Go helper's 313-frame rounding. |
| SPP-MMSE uses noise smoothing 0.8, probability smoothing 0.9, speech prior 0.5, fixed prior SNR about 31.6228, stagnation threshold 0.99, maximum speech probability 0.99, and a 1e-12 floor. | Source-confirmed parameters and equations | `dsp/noise/sppmmse.go`; the Go implementation initializes from the first active frame when no baseline exists. |
| Decision-directed SNR uses alpha 0.98 and a 1e-12 floor; Log-MMSE uses minimum gain 0.05, maximum gain 1, noise overestimation 1.25, and a 1e-12 floor. | Source-confirmed parameters and equations | `dsp/snr/decision_directed.go` and `dsp/suppressor/logmmse.go`. |
| Tonal-transient detection uses original frame power and applies its smoothed gain after Log-MMSE. Its defaults include local radius 2, tonal prominence 5–14 dB, flux 3–18 dB, gain strength 0.5, minimum gain 0.5, and attack/release 0.3/0.85. | Source-confirmed heuristic | `dsp/interference/tonal_transient.go`; detection is a spectral heuristic, not semantic speech separation. |
| The 40,000-sample Rust default trains on 311 wholly-contained frames and applies suppression to the first frame ending after calibration. | Rust contract and arithmetic | This preserves the accepted Rust complete-window boundary; it does not copy the Go MCRA helper's 313-frame rounding. |

The Rust pipeline sends frame power to the selected estimator and tonal detector independently. Selected noise PSD and observed power feed decision-directed SNR and Log-MMSE; tonal gain multiplies the Log-MMSE spectrum before inverse transform and overlap-add. With no complete quiet-intro frame, Rust initializes MCRA from the first active frame so each selection has a finite starting estimate; the pinned MCRA estimator instead stays uninitialized without a baseline. These comparisons establish provenance only and do not establish numerical parity or speech quality.

The Rust processor deliberately preserves one valid output sample per accepted input sample after drain. Its 256-sample startup buffering and second-packet first output are frame arithmetic, not measured end-to-end latency. Its allocator test observes initialized push and drain in this crate; it is not a Go benchmark comparison.

The offline replay's source attribution, deterministic mix, exact alignment, metric definitions and observations are in [`REPLAY.md`](REPLAY.md). Audio files are external temporary outputs and are not checked in. Playback of both recorded waveforms was completed and confirmed as fine; no subjective speech-quality assessment was made, and calculated metrics do not substitute for one.
