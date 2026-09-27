//! G.711 μ-law codec for evaluation files (bias 0x84, clip 32635), written independently of the library's private
//! codec so that the tool reads and writes files without widening the library's public API.

use crate::convert::quantize_pcm16;

const BIAS: i32 = 0x84;
const CLIP: i32 = 32635;

/// Decodes one μ-law byte to linear 16-bit PCM.
pub(crate) fn decode(byte: u8) -> i16 {
    let code = !byte;
    let exponent = (code >> 4) & 0x07;
    let mantissa = i32::from(code & 0x0F);
    let magnitude = (((mantissa << 3) + BIAS) << exponent) - BIAS;
    let value = if code & 0x80 == 0 { magnitude } else { -magnitude };
    i16::try_from(value).expect("μ-law magnitudes are at most 32124")
}

/// Decodes one μ-law byte to a normalized sample (PCM / 32768).
pub(crate) fn decode_normalized(byte: u8) -> f64 {
    f64::from(decode(byte)) / 32768.0
}

/// Encodes one linear 16-bit PCM sample as a μ-law byte.
pub(crate) fn encode(sample: i16) -> u8 {
    let negative = sample < 0;
    let biased = i32::from(sample).abs().min(CLIP) + BIAS;
    // The biased magnitude lies in [132, 32767]: its highest set bit is bit 7..=14, giving segments 0..=7.
    let segment = 31 - biased.leading_zeros() - 7;
    let mantissa = (biased >> (segment + 3)) & 0x0F;
    let code = u8::try_from((segment << 4) | u32::try_from(mantissa).expect("four bits")).expect("seven bits");
    !(if negative { code | 0x80 } else { code })
}

/// Encodes a normalized signal as μ-law bytes (scale by 32768, round, clamp, encode).
pub(crate) fn encode_signal(signal: &[f64]) -> Vec<u8> {
    signal.iter().map(|&sample| encode(quantize_pcm16(sample))).collect()
}

/// Decodes μ-law bytes to a normalized signal.
pub(crate) fn decode_signal(bytes: &[u8]) -> Vec<f64> {
    bytes.iter().map(|&byte| decode_normalized(byte)).collect()
}

#[cfg(test)]
#[path = "../tests/unit/mulaw.rs"]
mod tests;
