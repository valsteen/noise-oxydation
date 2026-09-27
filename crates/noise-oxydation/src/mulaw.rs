//! G.711 μ-law codec (bias 0x84, clip 32635).
//!
//! A μ-law byte is the one's complement of `sign | exponent | mantissa` (1, 3 and 4 bits). Decoding reconstructs the
//! middle of the quantization interval; encoding clips the magnitude, adds the bias, and selects the segment from the
//! position of the highest set bit.

use crate::convert::quantize_pcm16;

const BIAS: u16 = 0x84;
const DECODE_BIAS: i16 = 0x84;
const CLIP: u16 = 32635;

/// Decodes one μ-law byte to linear 16-bit PCM.
pub(crate) fn decode_pcm16(byte: u8) -> i16 {
    let code = !byte;
    let exponent = (code >> 4) & 0x07;
    let mantissa = i16::from(code & 0x0F);
    // At most ((15 << 3) + 132) << 7 = 32256, so the arithmetic stays inside i16.
    let magnitude = (((mantissa << 3) + DECODE_BIAS) << exponent) - DECODE_BIAS;
    if code & 0x80 == 0 { magnitude } else { -magnitude }
}

/// Encodes one linear 16-bit PCM sample as a μ-law byte.
pub(crate) fn encode_pcm16(sample: i16) -> u8 {
    let sign: u16 = if sample < 0 { 0x80 } else { 0 };
    // Clipped and biased magnitude lies in [132, 32767], so the highest set bit is bit 7..=14.
    let biased = sample.unsigned_abs().min(CLIP) + BIAS;
    let exponent = u16::BITS - 8 - biased.leading_zeros();
    let mantissa = (biased >> (exponent + 3)) & 0x0F;
    let code = sign | (segment_bits(exponent) << 4) | mantissa;
    let [low, _] = (!code).to_le_bytes();
    low
}

/// Decodes one μ-law byte to a normalized sample in `[-1, 1)`.
pub(crate) fn decode_sample(byte: u8) -> f32 {
    f32::from(decode_pcm16(byte)) / 32768.0
}

/// Encodes one normalized sample as a μ-law byte (clamp, scale by 32767, truncate toward zero, encode).
pub(crate) fn encode_sample(sample: f32) -> u8 {
    encode_pcm16(quantize_pcm16(sample))
}

/// The segment exponent is at most 7; keep its three bits as a `u16` without a lossy cast.
fn segment_bits(exponent: u32) -> u16 {
    let [low, ..] = exponent.to_le_bytes();
    u16::from(low & 0x07)
}

#[cfg(test)]
#[path = "../tests/unit/mulaw.rs"]
mod tests;
