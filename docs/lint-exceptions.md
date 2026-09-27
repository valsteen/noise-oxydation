# Lint Exceptions

Clippy runs with the `pedantic` group and `-D warnings`. The workspace has no tool-level lint configuration beyond
enabling `pedantic`, and no `#[allow]` attribute. The C ABI crate `crates/bindings/noise-oxydation-capi` makes two
lints stricter rather than looser: its library and integration tests deny `unsafe_op_in_unsafe_fn` and
`clippy::undocumented_unsafe_blocks`, so every `unsafe` block states its safety argument. The only lint expectations
are the narrow `#[expect]` attributes below, each in the audited numeric-conversion module of its crate:
[`crates/core/noise-oxydation/src/convert.rs`](../crates/core/noise-oxydation/src/convert.rs) in the library and
[`crates/tools/noise-oxydation-eval/src/convert.rs`](../crates/tools/noise-oxydation-eval/src/convert.rs) in the
evaluation crate. The guide renderer `crates/tools/how-it-works` has none: its geometry is integer arithmetic. The C
ABI crate has none either: it copies values between identical C and Rust types. Every
other conversion uses a lossless `From`, a checked `try_from`, or integer arithmetic.

| Location | Lint | Reason |
| --- | --- | --- |
| `convert::narrow` (library) | `clippy::cast_possible_truncation` | `f64 → f32` has no lossless std conversion. Intermediate results computed in `f64` (window and twiddle coefficients, the filter pole, the calibration mean of every noise estimator, the speech presence probability, the Log-MMSE gain, the tonal prominence and flux in dB) are rounded to the nearest `f32` for storage. |
| `convert::quantize_pcm16` (library) | `clippy::cast_possible_truncation` | Output samples are clamped to `[−1, 1]`, scaled by 32767, and truncated toward zero to `i16` before μ-law encoding, as the reference encoder does. The clamped product always fits `i16`. |
| `convert::quantize_pcm16` (evaluation crate) | `clippy::cast_possible_truncation` | Evaluation signals (the clean reference and the noisy mix) are scaled by 32768, rounded to nearest and clamped to the `i16` range before WAV writing and μ-law encoding. The clamp makes the cast exact. |

Adding an expectation requires a concrete reason, a narrow scope inside one of these modules, and a row in this
table. If an exception seems necessary anywhere else, stop and explain the tradeoff instead (see
[AGENTS.md](../AGENTS.md)).
