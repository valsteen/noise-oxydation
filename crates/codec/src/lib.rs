//! G.711 μ-law conversion for 8 kHz mono telephony samples.

/// Decode one G.711 μ-law codeword to signed 16-bit PCM.
#[must_use]
#[allow(clippy::cast_possible_truncation)] // G.711 codewords decode within ±32124.
pub fn decode(code: u8) -> i16 {
    let value = !code;
    let magnitude = (i32::from(value & 0x0f) << 3) + 132;
    let magnitude = magnitude << ((value & 0x70) >> 4);
    let sample = if value & 0x80 == 0 {
        magnitude - 132
    } else {
        132 - magnitude
    };
    sample as i16
}

/// Encode one signed 16-bit PCM sample as G.711 μ-law.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Code is exactly eight bits.
pub fn encode(sample: i16) -> u8 {
    let (magnitude, mask) = if sample < 0 {
        (-i32::from(sample), 0x7f)
    } else {
        (i32::from(sample), 0xff)
    };
    let biased = magnitude.min(32_635) + 132;
    let segment = (0..8)
        .find(|segment| biased <= (0xff << segment))
        .unwrap_or(7);
    let code = (segment << 4) | ((biased >> (segment + 3)) & 0x0f);
    (code as u8) ^ mask
}

#[cfg(test)]
mod tests {
    use super::{decode, encode};

    #[test]
    fn independent_codeword_vectors() {
        assert_eq!(decode(0xff), 0);
        assert_eq!(decode(0x7f), 0);
        assert_eq!(decode(0x00), -32_124);
        assert_eq!(decode(0x80), 32_124);
        assert_eq!(decode(0xfe), 8);
        assert_eq!(encode(0), 0xff);
        assert_eq!(encode(32_124), 0x80);
        assert_eq!(encode(-32_124), 0x00);
    }

    #[test]
    fn all_codewords_preserve_decoded_audio() {
        for code in u8::MIN..=u8::MAX {
            assert_eq!(decode(encode(decode(code))), decode(code));
        }
    }
}
