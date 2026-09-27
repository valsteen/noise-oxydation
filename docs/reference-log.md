# Reference Evidence Log

This log records every observed difference, error, ambiguity, or concern relating to the Go behavior reference
[sghaida/noise-cancelation at `cfc7520`](https://github.com/sghaida/noise-cancelation/tree/cfc7520a0625da90e4ad4699541a6ffe98e7c637),
and every deliberate Rust difference. Each entry links pinned evidence.

Classes:

- **Confirmed documentation problem**: the reference documentation disagrees with its own code or with mathematics
  that this project verified. The verification is stated.
- **Unverified implementation concern**: possible reference bug or quality issue that has not been demonstrated by
  running the reference. The entry states what would confirm or dismiss it.
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

### D3. The documented minimum noise estimator has no implementation

The README documents a standalone minimum noise estimator (smoothing 0.8, 50-frame window, floor 1e−12)
([R/README.md#L422-L465](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/README.md#L422-L465)),
but no such estimator exists at the pinned revision; minimum tracking exists only inside MCRA. Rust implements it from
the documented equations.

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

### D6. The MCRA baseline-frame example uses 10 s instead of 5 s

The comment computes `10 * 8000 / 128 = 313 frames` for a 5 s duration
([R/dsp/noise/mcra.go#L563-L585](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/noise/mcra.go#L563-L585));
`5 · 8000 / 128 = 312.5`, which the code rounds up to 313. That internal limit is never reached in the composed pipeline,
which ends calibration after 311 frames.

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

### D10. The SPP stagnation clamp is not in the README equations

The README lists a stagnation threshold and maximum speech probability (both 0.99) as defaults
([R/README.md#L623-L634](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/README.md#L623-L634)),
but the rule that uses them, `p = min(p, 0.99)` when smoothed `p̄ > 0.99`, is only in the code
([R/dsp/noise/sppmmse.go#L201-L207](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/noise/sppmmse.go#L201-L207)).

### D11. Flush output extends past the input, and the README does not say so

The README says flush "returns the final PCM samples"
([R/README.md#L1681-L1711](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/README.md#L1681-L1711));
as shown above it returns up to 128 samples of analysis padding beyond the input end, which every caller trims.

## Unverified Implementation Concerns

### U1. The first output sample of every stream is zero

The symmetric Hann window is zero at `n = 0` and no earlier frame overlaps sample 0, so its squared-window weight is 0
and the synthesizer emits 0 ([R/dsp/stft/istft.go#L98-L106](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/stft/istft.go#L98-L106)).
Samples 1–127 are reconstructed from a single frame. The effect is inaudible in the quiet intro, but it is a lost input
sample. Confirmation: run the reference on an impulse at sample 0.

Observed in the Rust implementation, which keeps the reference grid: sample 0 is always emitted as 0, and sample 1 is
divided by its single-frame squared-window weight `w[1]² ≈ 2.3e−8`, so the `f32` FFT rounding in that frame is
amplified by about `1/w[1] ≈ 6600`. Even with unmodified spectra, sample 1 therefore carries an error far larger than
single-precision rounding, while samples 2 onward reconstruct within one μ-law level. The packet lifecycle
integration test exempts samples 0 and 1 for this reason
([crates/noise-oxydation/tests/integration/lifecycle.rs](../crates/noise-oxydation/tests/integration/lifecycle.rs)).

### U2. The last valid samples may be amplified after enhancement

After flush, samples beyond the last full overlap come from one frame divided by `w[n]²`, which approaches `2.3e−8` near
the frame end. Unmodified spectra reconstruct exactly, but gain-modified frames are no longer zero at their edges, so the
division can amplify them. Confirmation: measure the peak of the last 127 output samples on enhanced real speech against
the preceding samples.

Rust note: with whole 160-sample packets the input length is `160p`, so the drain frame holds 128, 160, 192, or 224
valid samples and the last valid sample sits at most at window index 223 (`w ≈ 0.15`). The near-zero weights at the
very end of the frame only affect analysis padding, which Rust never emits. Enhanced frames can still be amplified
near that end by up to `1/w`; the measurement above remains planned.

### U3. MCRA never initializes without baseline frames

If `FinishBaseline` runs before any baseline frame (calibration shorter than one frame), MCRA stays uninitialized and
returns a zero noise estimate on every later frame
([R/dsp/noise/mcra.go#L275-L312](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/noise/mcra.go#L275-L312)).
SPP-MMSE instead initializes from the first frame. Confirmation: run the reference MCRA with `FinishBaseline` before
`Process`.

### U4. Resets allocate

The estimators, SNR, and tonal detector drop their slices on `Reset` and reallocate on the next `Process`
in `StartBaseline` and `Reset` ([R/dsp/noise/sppmmse.go#L233-L266](https://github.com/sghaida/noise-cancelation/blob/cfc7520a0625da90e4ad4699541a6ffe98e7c637/dsp/noise/sppmmse.go#L233-L266)).
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

Construction returns a typed error naming the invalid field instead of substituting a default (contrast U6).

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

## Measured Parity

None yet. Numeric comparison against the reference is planned with the audio and performance evidence; until an entry
appears here, no parity is claimed.
