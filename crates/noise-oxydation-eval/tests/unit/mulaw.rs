use super::{decode, decode_signal, encode, encode_signal};

#[test]
fn decoding_matches_g711_reference_levels() {
    // ITU-T G.711 μ-law: 0xFF and 0x7F are the two zeros, 0x80 and 0x00 the extremes (±8031 in 14-bit units, ×4),
    // and 0xFE/0x7E the smallest non-zero levels (±2 in 14-bit units, ×4).
    assert_eq!(decode(0xFF), 0);
    assert_eq!(decode(0x7F), 0);
    assert_eq!(decode(0x80), 32124);
    assert_eq!(decode(0x00), -32124);
    assert_eq!(decode(0xFE), 8);
    assert_eq!(decode(0x7E), -8);
}

#[test]
fn every_code_survives_a_decode_encode_round_trip() {
    for byte in 0..=u8::MAX {
        let expected = if byte == 0x7F { 0xFF } else { byte };
        assert_eq!(encode(decode(byte)), expected, "{byte:#04x}");
    }
}

#[test]
fn encoding_clips_at_full_scale_and_quantizes_to_the_nearest_level() {
    assert_eq!(encode(i16::MAX), 0x80);
    assert_eq!(encode(i16::MIN), 0x00);
    // The first segment has levels 8m with decision thresholds halfway between them (ties round up in magnitude).
    for magnitude in 0_i16..=123 {
        let level = 8 * ((magnitude + 4) / 8);
        assert_eq!(decode(encode(magnitude)), level, "{magnitude}");
        assert_eq!(decode(encode(-magnitude)), -level, "-{magnitude}");
    }
}

#[test]
fn normalized_signals_round_trip_through_the_packet_format() {
    let bytes: Vec<u8> = (0..=u8::MAX).filter(|&byte| byte != 0x7F).collect();
    assert_eq!(encode_signal(&decode_signal(&bytes)), bytes);
}
