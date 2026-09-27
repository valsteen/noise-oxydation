# Lint Exceptions

Clippy runs with the `pedantic` group and `-D warnings`. The workspace has no tool-level lint configuration beyond
enabling `pedantic`, and no `#[allow]` attribute. The only lint expectations are the narrow `#[expect]` attributes
below, all in the audited numeric-conversion module
[`crates/noise-oxydation/src/convert.rs`](../crates/noise-oxydation/src/convert.rs). Every other conversion uses a
lossless `From`, a checked `try_from`, or integer arithmetic.

| Location | Lint | Reason |
| --- | --- | --- |
| `convert::narrow` | `clippy::cast_possible_truncation` | `f64 → f32` has no lossless std conversion. Intermediate results computed in `f64` (window and twiddle coefficients, the filter pole, the calibration mean of every noise estimator, the speech presence probability, the Log-MMSE gain, the tonal prominence and flux in dB) are rounded to the nearest `f32` for storage. |
| `convert::quantize_pcm16` | `clippy::cast_possible_truncation` | Output samples are clamped to `[−1, 1]`, scaled by 32767, and truncated toward zero to `i16` before μ-law encoding, as the reference encoder does. The clamped product always fits `i16`. |

Adding an expectation requires a concrete reason, a narrow scope inside this module, and a row in this table. If an
exception seems necessary anywhere else, stop and explain the tradeoff instead (see [AGENTS.md](../AGENTS.md)).
