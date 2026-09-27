use super::{decode_pcm16, decode_sample, encode_pcm16, encode_sample};

#[test]
fn every_code_survives_the_normalized_round_trip_except_negative_zero() {
    for byte in 0..=u8::MAX {
        let expected = if byte == 0x7F { 0xFF } else { byte };
        assert_eq!(encode_sample(decode_sample(byte)), expected, "code {byte:#04x}");
    }
}

#[test]
fn zero_codes_decode_to_zero_and_zero_encodes_to_positive_zero() {
    assert_eq!(decode_pcm16(0xFF), 0);
    assert_eq!(decode_pcm16(0x7F), 0);
    assert_eq!(encode_pcm16(0), 0xFF);
}

#[test]
fn full_scale_codes_decode_to_the_g711_extremes() {
    assert_eq!(decode_pcm16(0x80), 32124);
    assert_eq!(decode_pcm16(0x00), -32124);
}

#[test]
fn decoding_is_symmetric_in_sign() {
    for magnitude_code in 0..0x80_u8 {
        let positive = 0xFF - magnitude_code;
        let negative = 0x7F - magnitude_code;
        assert_eq!(decode_pcm16(negative), -decode_pcm16(positive), "magnitude code {magnitude_code}");
    }
}

/// G.711 μ-law uses eight segments of sixteen levels; the level spacing doubles from one segment to the next,
/// starting at 8 (in 16-bit PCM units) and ending at 1024.
#[test]
fn level_spacing_doubles_every_sixteen_levels() {
    let levels: Vec<i32> = (0..0x80_u8).map(|magnitude_code| i32::from(decode_pcm16(0xFF - magnitude_code))).collect();
    let mut expected_step = 8;
    for (segment, pairs) in levels.windows(2).collect::<Vec<_>>().chunks(16).enumerate() {
        for pair in pairs.iter().take(15) {
            assert_eq!(pair[1] - pair[0], expected_step, "segment {segment}");
        }
        expected_step *= 2;
    }
}

/// The segment changes where the biased magnitude `|x| + 132` crosses a power of two, and magnitudes above the clip
/// level 32635 encode to the full-scale code.
#[test]
fn encoder_segments_start_at_the_biased_powers_of_two() {
    for (segment, boundary) in [124, 380, 892, 1916, 3964, 8060, 16252].into_iter().enumerate() {
        let below = !encode_pcm16(boundary - 1) >> 4;
        let at = !encode_pcm16(boundary) >> 4;
        let expected = u8::try_from(segment).expect("seven segments");
        assert_eq!((below, at), (expected, expected + 1), "boundary {boundary}");
    }
    assert_eq!(encode_pcm16(i16::MAX), 0x80);
    assert_eq!(encode_pcm16(i16::MIN), 0x00);
    assert_eq!(encode_pcm16(32635), encode_pcm16(32767));
}

#[test]
fn normalized_encoding_clamps_out_of_range_samples() {
    assert_eq!(encode_sample(3.0), encode_sample(1.0));
    assert_eq!(encode_sample(-3.0), encode_sample(-1.0));
    assert_eq!(encode_sample(1.0), 0x80);
    assert_eq!(encode_sample(-1.0), 0x00);
}
