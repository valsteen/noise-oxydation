//! Audited lossy numeric conversions of the evaluation crate.
//!
//! This is the crate's only module allowed to hold lint expectations. Every expectation is listed in
//! `docs/lint-exceptions.md`; add nothing here without updating that list.

/// Quantizes a normalized sample to signed 16-bit PCM: scale by 32768 (the inverse of the `/ 32768` normalization
/// used when reading 16-bit audio), round to nearest, and clamp to the `i16` range. NaN maps to 0.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the rounded value is clamped to the i16 range first, so the cast only drops the zero fraction"
)]
pub(crate) fn quantize_pcm16(sample: f64) -> i16 {
    (sample * 32768.0).round().clamp(-32768.0, 32767.0) as i16
}
