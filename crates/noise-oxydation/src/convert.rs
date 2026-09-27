//! Audited lossy numeric conversions.
//!
//! This is the only module allowed to hold lint expectations. Every expectation is listed in
//! `docs/lint-exceptions.md`; add nothing here without updating that list.

/// Narrows an `f64` intermediate result to `f32` storage, rounding to the nearest representable value.
///
/// Values beyond the `f32` range become infinities, which every caller either rules out or clamps afterwards.
#[expect(
    clippy::cast_possible_truncation,
    reason = "f64 -> f32 has no lossless std conversion; rounding to nearest is the intended narrowing"
)]
pub(crate) fn narrow(value: f64) -> f32 {
    value as f32
}

/// Quantizes a normalized sample to signed 16-bit PCM.
///
/// The sample is clamped to `[-1, 1]`, scaled by 32767 and truncated toward zero, which mirrors the reference encoder
/// (the decoder divides by 32768; that asymmetry is reference-documented). NaN maps to 0.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the clamped product lies in [-32767, 32767]; truncation toward zero is the documented quantization"
)]
pub(crate) fn quantize_pcm16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * 32767.0) as i16
}
