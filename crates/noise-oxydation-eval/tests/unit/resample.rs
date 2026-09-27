use std::f64::consts::PI;

use super::decimate_16k_to_8k;

/// Amplitude of the component at `frequency` (Hz, 8 kHz rate) in `signal`, by a least-squares sine/cosine fit.
fn fitted_amplitude(signal: &[f64], frequency: f64) -> f64 {
    let (mut cc, mut cs, mut ss, mut yc, mut ys) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for (index, &value) in (0_u32..).zip(signal) {
        let phase = 2.0 * PI * frequency * f64::from(index) / 8000.0;
        let (c, s) = (phase.cos(), phase.sin());
        cc += c * c;
        cs += c * s;
        ss += s * s;
        yc += value * c;
        ys += value * s;
    }
    let determinant = cc * ss - cs * cs;
    let a = (yc * ss - ys * cs) / determinant;
    let b = (ys * cc - yc * cs) / determinant;
    a.hypot(b)
}

/// Decimates a 16 kHz tone and returns its output amplitude relative to the input amplitude in dB, measured at the
/// frequency where the tone lands at 8 kHz (aliased for inputs above 4 kHz), away from the signal edges.
fn response_db(frequency: u32) -> f64 {
    let amplitude = 0.5;
    let input: Vec<f64> = (0..32_000_u32)
        .map(|n| amplitude * (2.0 * PI * f64::from(frequency) * f64::from(n) / 16_000.0 + 0.3).sin())
        .collect();
    let output = decimate_16k_to_8k(&input);
    assert_eq!(output.len(), 16_000);
    let landed = if frequency <= 4000 { f64::from(frequency) } else { 8000.0 - f64::from(frequency) };
    20.0 * (fitted_amplitude(&output[200..15_800], landed) / amplitude).log10()
}

#[test]
fn the_passband_is_flat_within_a_tenth_of_a_decibel_below_3400_hz() {
    for frequency in (100..=3400).step_by(100) {
        let response = response_db(frequency);
        assert!(response.abs() <= 0.1, "{frequency} Hz: {response:.4} dB");
    }
}

#[test]
fn tones_above_4300_hz_are_attenuated_by_at_least_60_db() {
    for frequency in (4300..=7900).step_by(100) {
        let response = response_db(frequency);
        assert!(response <= -60.0, "{frequency} Hz aliases at {response:.1} dB");
    }
}

#[test]
fn a_constant_passes_with_unit_gain_away_from_the_edges() {
    let output = decimate_16k_to_8k(&[0.25; 1000]);
    assert_eq!(output.len(), 500);
    for &sample in &output[40..460] {
        assert!((sample - 0.25).abs() < 1e-12, "{sample}");
    }
}
