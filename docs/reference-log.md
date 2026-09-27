# Reference Evidence Log

This log records every observed difference, error, ambiguity, or concern relating to the Go behavior reference
[sghaida/noise-cancelation at `cfc7520`](https://github.com/sghaida/noise-cancelation/tree/cfc7520a0625da90e4ad4699541a6ffe98e7c637),
and every deliberate Rust difference. Each entry links pinned evidence.

Classes:

- **Confirmed documentation problem**: the reference documentation disagrees with its own code or with mathematics
  that this project verified. The verification is stated.
- **Implementation concern**: possible reference bug or quality issue. Each entry states whether running the reference
  confirmed or dismissed it, with the evidence, or, while it is unverified, what would confirm or dismiss it.
- **Deliberate Rust choice**: a place where this implementation intentionally behaves differently or narrows scope.
- **Measured parity**: a numeric comparison with the reference that was actually run. No parity claim exists without
  an entry here.

`R` below abbreviates `https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637`.

## Reference Timing And Flush Behavior (Observed From Source)

These facts define the analysis grid that the Rust implementation mirrors:

- The analyzer buffers input and emits a 256-sample frame each time 256 samples are buffered, then advances by 128
  ([R/dsp/stft/stft.go#L76-L108](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/stft/stft.go#L76-L108)).
- The synthesizer windows each inverse FFT, overlap-adds it, divides by the accumulated squared-window weight where
  that weight exceeds `1e-8`, and emits 128 samples per frame
  ([R/dsp/stft/istft.go#L58-L117](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/stft/istft.go#L58-L117)).
  Output sample *n* therefore becomes final when the frame starting at `128·⌊n/128⌋` has been processed.
- Flush emits one analysis frame from the remaining buffered samples, zero-padded to 256
  ([R/dsp/stft/stft.go#L145-L162](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/stft/stft.go#L145-L162)),
  then the synthesizer tail emits the remaining 128 overlap samples
  ([R/dsp/stft/istft.go#L120-L146](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/stft/istft.go#L120-L149)).
  For `L` input samples this yields between `L + 1` and `L + 128` output samples; every reference pipeline trims the
  excess ([R/smoke/tonal_transient_pipeline_test.go#L371-L384](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/smoke/tonal_transient_pipeline_test.go#L371-L384)).
- A frame is a baseline (calibration) frame when `frameCount·128 + 256 ≤ baselineSamples`; the first non-baseline frame
  calls `FinishBaseline` before processing
  ([R/benchmark/end_to_end_benchmark_test.go#L341-L375](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/benchmark/end_to_end_benchmark_test.go#L341-L375)).
  With 5 s at 8 kHz that is frames 0–310. Baseline frames bypass Log-MMSE and tonal suppression, so their audio passes
  through un-enhanced.
- The composed per-packet order is μ-law decode → high-pass (in place) → analyzer → per frame: power, noise estimate,
  Log-MMSE, tonal gain → synthesizer → μ-law encode
  ([R/benchmark/end_to_end_benchmark_test.go#L286-L375](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/benchmark/end_to_end_benchmark_test.go#L286-L375)).

## Confirmed Documentation Problems

### D1. The Hann window is symmetric, not periodic

The code comment says "periodic Hann window" but the formula divides by `N − 1`, which is the symmetric window
([R/dsp/stft/window.go#L5-L31](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/stft/window.go#L5-L31)).
The README equation also uses `N − 1`. Verified by reading the formula. Rust uses the symmetric window to match the
documented equation and the code.

### D2. The tonal detector ignores everything below 2 kHz, and the README does not say so

`MinFrequencyBin: 64` limits detection to bins 64 and above, which is 2000 Hz at 31.25 Hz per bin
([R/dsp/interference/tonal_transient.go#L77](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/interference/tonal_transient.go#L77),
[#L327-L336](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/interference/tonal_transient.go#L327-L336)).
The README default table lists guard and search bins, which are hard-coded constants, but omits this parameter
([R/README.md#L848-L856](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/README.md#L848-L856)).
Rust exposes it as `tonal_transient.min_frequency_bin` (default 64) and documents the 2 kHz limit in
[algorithms.md](algorithms.md#tonal-transient-suppression).

### D3. The documented minimum noise estimator has no implementation

The README documents a standalone minimum noise estimator (smoothing 0.8, 50-frame window, floor 1e−12)
([R/README.md#L422-L465](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/README.md#L422-L465)),
but no such estimator exists at the pinned revision; minimum tracking exists only inside MCRA. Rust implements it from
the documented equations; the window scheme is a deliberate choice (R11).

### D4. The documented pipeline package does not exist

The README "How To Use" section presents `pipeline/pipeline.go` as code to create
([R/README.md#L1451-L1732](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/README.md#L1451-L1732)),
but no pipeline package exists at the pinned revision. The executable compositions are the end-to-end benchmark and the
smoke tests. They differ from the README sketch only in ordering independent steps and in when the tonal detector is
reset; the Rust port follows the benchmark composition.

### D5. MCRA's adaptive noise smoothing factor is not defined in the README

The README writes `N = α_n[t,k] N + (1 − α_n[t,k]) P` without defining `α_n[t,k]`
([R/README.md#L521-L529](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/README.md#L521-L529)).
The code uses `α_d + (1 − α_d)·p[t,k]` with `α_d = 0.95`
([R/dsp/noise/mcra.go#L518-L536](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/noise/mcra.go#L520-L535)).
Rust follows the code.

### D6. The MCRA baseline-frame example uses 10 s instead of 5 s

The comment computes `10 * 8000 / 128 = 313 frames` for a 5 s duration
([R/dsp/noise/mcra.go#L563-L585](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/noise/mcra.go#L563-L585));
`5 · 8000 / 128 = 312.5`, which the code rounds up to 313. That internal limit is never reached in the composed
pipeline, which ends calibration after 311 frames.

### D7. The "80 Hz cutoff" is the pole parameter, not the −3 dB frequency

The filter is `y[n] = x[n] − x[n−1] + r·y[n−1]` with `r = exp(−2π·80/8000) ≈ 0.9391`
([R/dsp/highpass/highpass.go](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/highpass/highpass.go),
[R/README.md#L233-L267](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/README.md#L233-L267)).
Evaluating `|H(e^{jω})|` gives −3 dB at about 75.3 Hz, −2.74 dB at 80 Hz, and +0.27 dB at Nyquist. Verified
numerically during preparation; the Rust unit tests reassert it.

### D8. The Log-MMSE SNRs use the overestimated noise, which the equations do not show

The README defines γ and ξ from `N[t,k]` in the decision-directed section
([R/README.md#L662-L701](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/README.md#L662-L701)),
but Log-MMSE passes `β·N` (β = 1.25) to the SNR estimator for both γ and the stored clean SNR
([R/dsp/suppressor/logmmse.go#L119-L166](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/suppressor/logmmse.go#L119-L166)).
Rust follows the code.

### D9. The tonal score comment omits persistent tonal evidence

The type comment gives `S = T·max(F, M)·(1 − H)`
([R/dsp/interference/tonal_transient.go#L123](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/interference/tonal_transient.go#L123)),
while the code and README use `max(F, M, 0.35·T)`
([#L306-L307](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/interference/tonal_transient.go#L306-L307)).
Rust follows the code and README, so a stationary tone keeps 0.35 of its tonal score.

### D10. The SPP stagnation clamp is not in the README equations

The README lists a stagnation threshold and maximum speech probability (both 0.99) as defaults
([R/README.md#L623-L634](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/README.md#L623-L634)),
but the rule that uses them, `p = min(p, 0.99)` when smoothed `p̄ > 0.99`, is only in the code
([R/dsp/noise/sppmmse.go#L201-L207](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/noise/sppmmse.go#L201-L207)).

### D11. Flush output extends past the input, and the README does not say so

The README says flush "returns the final PCM samples"
([R/README.md#L1681-L1711](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/README.md#L1681-L1711));
as shown above it returns up to 128 samples of analysis padding beyond the input end, which every caller trims.

## Implementation Concerns

U1 and U3 are confirmed and U2 was not observed, all by running the reference through `tools/go-parity` (see
[Measured Parity](#measured-parity) for the harness, machine and toolchains). U4–U6 remain unverified.

### U1. The first output sample of every stream is zero

**Status: confirmed.** The symmetric Hann window is zero at `n = 0` and no earlier frame overlaps sample 0, so its
squared-window weight is 0 and the synthesizer emits 0
([R/dsp/stft/istft.go#L98-L106](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/stft/istft.go#L98-L106)).
Samples 1–127 are reconstructed from a single frame. The effect is inaudible in the quiet intro, but it is a lost input
sample.

Evidence: `go run . u1 -write-input <file>` in `tools/go-parity` feeds 50 packets holding the full-scale impulse `0x80`
(+32124) at sample 0 and μ-law silence elsewhere through the reference composition. Output sample 0 decodes to 0;
samples 1–3 decode to −1980, −1820 and −1756, the high-pass filter's response to the impulse. The Rust `enhance_mulaw`
example produces the same 8000 bytes (`noise-oxydation-eval compare`: 100 % identical).

Rust keeps this behavior. It follows from the analysis grid the port mirrors exactly (R2), changing it would break the
measured parity, and the lost sample falls in the quiet intro.

Observed in the Rust implementation, which keeps the reference grid: sample 0 is always emitted as 0, and sample 1 is
divided by its single-frame squared-window weight `w[1]² ≈ 2.3e−8`, so the `f32` FFT rounding in that frame is
amplified by about `1/w[1] ≈ 6600`. Even with unmodified spectra, sample 1 therefore carries an error far larger than
single-precision rounding, while samples 2 onward reconstruct within one μ-law level. The packet lifecycle
integration test exempts samples 0 and 1 for this reason
([crates/core/noise-oxydation/tests/integration/lifecycle.rs](../crates/core/noise-oxydation/tests/integration/lifecycle.rs)).

### U2. The last valid samples may be amplified after enhancement

**Status: not observed on enhanced real speech in either implementation.** After flush, samples beyond the last full
overlap come from one frame divided by `w[n]²`, which approaches `2.3e−8` near the frame end. Unmodified spectra
reconstruct exactly, but gain-modified frames are no longer zero at their edges, so the division could amplify them.

With whole 160-sample packets the input length is `160p`, so the drain frame holds 128, 160, 192, or 224 valid samples
and the last valid sample sits at most at window index 223 (`w ≈ 0.15`). The near-zero weights at the very end of the
frame only affect analysis padding, which both the reference compositions (by trimming) and Rust (by never emitting
it) discard. The bound on amplification of the emitted samples is therefore `1/w ≈ 6.7` (16.5 dB).

Evidence: the `office-5db` noisy input of the [replay](evaluation.md) was cut after 400, 520, …, 2560 and 2640 packets
(20 cuts spread over the call). Each cut was enhanced as a complete call by the Rust `enhance_mulaw` example
(SPP-MMSE with tonal suppression) and by `go run . enhance` (the same composition in the reference). For each
implementation, `noise-oxydation-eval compare` then compared the last 127 samples of the cut call with the same samples
of the uninterrupted call. The drained tail's peak never exceeded the uninterrupted peak: it was unchanged in 14 cuts
and 0.38–1.94 dB lower in 6, and the two implementations agreed to 0.01 dB in every cut ([P3](#p3-truncated-calls-u2)).
On the four full replay scenarios the tail peak also stays 6.5–16.0 dB below the peak of the preceding second, with
the same values to 0.01 dB in both implementations.

Rust keeps its end-of-call behavior. The measurement covers one noise type, SPP-MMSE and whole-packet lengths; the
reference with arbitrary input lengths was not measured.

### U3. MCRA never initializes without baseline frames

**Status: confirmed.** If `FinishBaseline` runs before any baseline frame (calibration shorter than one frame), MCRA
stays uninitialized and returns a zero noise estimate on every later frame
([R/dsp/noise/mcra.go#L275-L312](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/noise/mcra.go#L275-L312)).
SPP-MMSE instead initializes from the first frame.

Evidence: `go run . u3` in `tools/go-parity` calls `StartBaseline` and then `FinishBaseline` on a new reference
estimator before any frame, and feeds 500 frames whose bin powers are between 1e−3 and 7e−3. The MCRA estimate is 0 in
every bin after 1, 10 and 500 frames. The SPP-MMSE estimator under the same sequence returns a largest bin of 0.007
from the first frame on.

Rust keeps its deliberate difference (R10): MCRA initializes from the first frame when no calibration frame exists.

### U4. Resets allocate

The noise estimators, the decision-directed SNR estimator, and the tonal detector set their slices to `nil` in `Reset`
(and SPP-MMSE also in `StartBaseline`), then reallocate them on the next `Process`
([R/dsp/noise/sppmmse.go#L233-L266](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/noise/sppmmse.go#L233-L266)).
Per-call reuse therefore allocates after initialization.

### U5. The FFT twiddle recurrence accumulates float32 rounding

Twiddles are generated by repeated complex64 multiplication
([R/dsp/stft/fft.go#L30-L47](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/stft/fft.go#L30-L47)),
so later twiddles in the 128-point stage carry accumulated error. Confirmation: compare against a double-precision DFT.

### U6. Invalid configuration is silently replaced by defaults

For example, `MinGain > MaxGain` quietly restores the default minimum gain
([R/dsp/suppressor/logmmse.go#L92-L112](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/suppressor/logmmse.go#L92-L112)),
so a misconfiguration runs with values the caller did not choose.

## Deliberate Rust Choices

### R1. Fixed telephony geometry

8 kHz, FFT 256, hop 128, and 160-byte packets are compile-time constants. The reference accepts other sizes. Reason:
the product scope is G.711 telephony, and fixed sizes make storage bounds and timing provable.

### R2. Packet-in, packet-out timing with exact sample accounting

The reference returns 0, 128, or 256 samples per call and pads up to 128 extra samples on flush. Rust emits one 160-byte
packet per input packet after a fixed two-packet delay, and `drain` returns exactly the withheld packets, so output
length always equals input length. The analysis grid, frame classification, and per-frame processing are unchanged.
See [ARCHITECTURE.md](../ARCHITECTURE.md#packet-timing-contract).

### R3. Invalid configuration is rejected

Construction returns a typed error naming the invalid field instead of substituting a default (contrast U6). This
covers the MCRA, minimum-estimator, and tonal transient parameters too: for example, a tonal start threshold at or
above its full threshold is rejected with `ConfigError::StartNotBelowFull`, where the reference silently restores both
defaults ([R/dsp/interference/tonal_transient.go#L177-L196](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/interference/tonal_transient.go#L177-L196)).

### R4. One calibration clock

The call owns the calibration boundary (frame-end rule). Estimators do not keep their own baseline duration (the
reference MCRA also has an internal 313-frame limit, D6).

### R5. Precomputed double-precision twiddles

The FFT uses twiddles computed in f64 and stored as f32 rather than the reference recurrence (U5). Output therefore
differs numerically from the reference; parity is claimed only as measured below.

### R6. No allocation after construction

All storage is sized at construction; `reset` reuses it (contrast U4).

### R7. Calibration sample count by integer arithmetic

The calibration boundary is `floor(duration · 8000)` computed exactly from the `Duration`'s nanoseconds. The reference
benchmark converts `Seconds() · 8000` through `float64` before truncating
([R/benchmark/end_to_end_benchmark_test.go#L282](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/benchmark/end_to_end_benchmark_test.go#L282)),
which can land one sample lower for durations that are not exactly representable. The boundary frame can differ only
when that single sample crosses a frame end; the 5 s default gives 40 000 samples in both.

### R8. Noise estimator chosen per call, SPP-MMSE and tonal suppression by default

The reference keeps each stage behind a replaceable component
([R/README.md#L2220-L2233](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/README.md#L2220-L2233)), and its end-to-end composition uses SPP-MMSE and
applies the tonal transient gain after Log-MMSE
([R/benchmark/end_to_end_benchmark_test.go#L341-L375](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/benchmark/end_to_end_benchmark_test.go#L341-L375)).
Rust selects the estimator with the `NoiseEstimatorConfig` enum (SPP-MMSE by default, MCRA, or the minimum estimator)
and interference suppression with the `InterferenceConfig` enum (tonal transient suppression by default, or
disabled), dispatched statically inside the call. The MCRA and tonal detector defaults equal the reference
`DefaultMCRAConfig` and `DefaultTonalTransientConfig`
([R/dsp/noise/mcra.go#L59-L73](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/noise/mcra.go#L59-L73),
[R/dsp/interference/tonal_transient.go#L67-L88](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/interference/tonal_transient.go#L67-L88)); the
minimum estimator's defaults are the README's.

### R9. Calibration means in `f64` for every estimator

The reference SPP-MMSE accumulates its baseline in `float64`
([R/dsp/noise/sppmmse.go#L287](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/noise/sppmmse.go#L287)), but MCRA accumulates in `float32`
([R/dsp/noise/mcra.go#L366-L378](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/noise/mcra.go#L366-L378)), whose sum can lose low-order
bits over the 311 frames of a 5 s intro. All Rust estimators share one `f64` accumulator and narrow the floored mean
to `f32`. Every estimator also floors observed power the same way (below the floor, NaN, and infinities become the
floor); the reference MCRA leaves `+∞` unchanged, which cannot occur with bounded μ-law input.

### R10. MCRA initializes from the first frame without calibration

When no calibration frame exists, the reference MCRA never initializes and returns a zero estimate forever (U3).
Rust initializes MCRA like its other estimators: from the first frame's floored power, followed by that frame's normal
update, so a zero calibration duration still produces a working noise estimate. With calibration, Rust matches the
reference: tracking starts from the calibration mean and the first enhanced frame is updated normally.

### R11. The minimum estimator uses an exact sliding window

The README defines the estimate as the minimum of the smoothed power "inside the configured window" without defining
how the window advances (D3), and MCRA approximates a window minimum with two alternating blocks
([R/dsp/noise/mcra.go#L553-L563](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/noise/mcra.go#L553-L563)). Rust keeps the smoothed spectra of the
most recent `W` frames in a history allocated at construction and takes the exact minimum over them, including the
current frame. At the first non-calibration frame every history slot holds the calibration mean (or the first frame's
power), so the window is full from the start. MCRA keeps the reference's two-block scheme.

### R12. MCRA diagnostics are not ported

The reference MCRA offers `SpeechProbability`, `NoiseProbability`, and the `MinProbabilityFrequency` parameter they use
([R/dsp/noise/mcra.go#L52](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/noise/mcra.go#L52),
[#L482-L517](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/noise/mcra.go#L482-L517)). Only the MCRA smoke test calls them, to write diagnostic
output ([R/smoke/mcra_smoke_test.go#L165-L166](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/smoke/mcra_smoke_test.go#L165-L166)); no processing
composition uses them and they do not affect the noise estimate. The packet API exposes no per-frame diagnostics, so
Rust omits them and the parameter.

### R13. Tonal detector flux ratio in `f64`

The reference computes the flux ratio `current / previous` in `float32` before taking the logarithm in `float64`
([R/dsp/interference/tonal_transient.go#L613-L624](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/interference/tonal_transient.go#L613-L624)).
Rust forms the ratio in `f64`. The two differ only by `f32` rounding; no parity is claimed.

### R14. NaN output samples quantize to 0

`convert::quantize_pcm16` clamps each output sample to `[−1, 1]`, scales it by 32767, and converts it with Rust's
saturating float-to-integer cast, which maps NaN to 0. The reference converts with `int16(sample * 32767.0)`
([R/codec/mulaw/mulaw.go#L227-L240](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/codec/mulaw/mulaw.go#L227-L240)); Go leaves the result for NaN
implementation-specific, so it can differ by platform. NaN cannot reach the Rust encoder from valid μ-law input: every
stage replaces non-finite intermediate values with 0 or its floor.

## Measured Parity

Every numeric comparison with the reference that was actually run. Parity is claimed only for what these entries
measured: the listed inputs, configurations, machine and toolchains.

**Harness.** [`tools/go-parity`](../tools/go-parity/main.go) is a Go module that requires the reference module
`github.com/sghaida/noise-cancelation v0.0.0-20260920200827-cfc7520a0625` (pinned by the committed `go.sum`) and
composes its public packages in the order of the reference's end-to-end benchmark: μ-law decode, high-pass, STFT,
SPP-MMSE or MCRA with the baseline started at construction and finished at the first frame ending after 5 s, Log-MMSE,
tonal transient gain, ISTFT, analyzer flush, synthesizer flush, trim to the input length, μ-law encode. It reads and
writes headerless μ-law files and copies no reference code.

**Machine and toolchains.** Apple M1 Ultra, macOS 15.5, `aarch64`; Rust 1.98.1; Go 1.27.1.

**Comparison.** `noise-oxydation-eval compare <go output> <rust output>` decodes both files and reports the fraction
of identical bytes, the largest absolute difference in 16-bit PCM units, and the energy of the difference relative to
the Go output. It splits the call at output sample 39808, the first sample influenced by an enhanced frame
(frame 311 starts at 128 · 311), into a calibration and an enhanced region.

### P1. Replay scenarios, SPP-MMSE and MCRA with tonal suppression

Inputs: the four replay scenarios' `noisy.ul` files ([evaluation.md](evaluation.md#scenarios)), 41.6–52.9 s each,
enhanced with the default 5 s calibration by both implementations. Commands are in
[evaluation.md](evaluation.md#reproduce).

| Scenario | Estimator | Identical bytes | Max \|diff\| | Difference energy, overall | Calibration region | Enhanced region |
| --- | --- | ---: | ---: | ---: | --- | ---: |
| `office-5db` | SPP-MMSE | 99.9993 % | 32 | −86.6 dB | identical | −86.6 dB |
| `office-5db` | MCRA | 99.9986 % | 64 | −80.4 dB | identical | −80.4 dB |
| `cafeteria-5db` | SPP-MMSE | 99.9995 % | 32 | −86.7 dB | identical | −86.5 dB |
| `cafeteria-5db` | MCRA | 99.9991 % | 16 | −88.7 dB | identical | −88.5 dB |
| `speech-after-calibration` | SPP-MMSE | 99.9984 % | 128 | −72.0 dB | identical | −72.0 dB |
| `speech-after-calibration` | MCRA | 99.9995 % | 256 | −69.0 dB | identical | −69.0 dB |
| `speech-during-calibration` | SPP-MMSE | 99.9982 % | 128 | −74.6 dB | 99.9975 %, max 8, −92.3 dB | −73.7 dB |
| `speech-during-calibration` | MCRA | 99.9982 % | 64 | −76.9 dB | 99.9975 %, max 8, −92.3 dB | −75.9 dB |

The outputs are not bit-identical, but fewer than 2 bytes in 100 000 differ, and the largest decoded difference is 256
units (0.8 % of full scale). Where the calibration region carries only noise it is byte-identical. With speech during
calibration it differs in one byte in 40 000, which fits `f32` rounding differences in the pass-through reaching a
μ-law decision threshold; the deliberate FFT twiddle difference (R5) is one known source of such rounding differences,
and the measurements do not attribute the differences further. The minimum estimator (D3) and runs with tonal
suppression disabled have no reference counterpart in the harness and were not compared.

### P2. Impulse at sample 0 (U1)

Input: 50 packets with `0x80` at sample 0 and `0xFF` elsewhere, SPP-MMSE with tonal suppression, 5 s calibration.
The 8000 output bytes are identical (100 %).

### P3. Truncated calls (U2)

Inputs: the `office-5db` noisy input cut after 400, 520, …, 2560 and 2640 packets (20 cuts), SPP-MMSE with tonal
suppression. Between the Rust and Go output of each cut, 99.998–100 % of the bytes are identical, and the drained tail
peaks, relative to the uninterrupted calls, agree to 0.01 dB in every cut.
