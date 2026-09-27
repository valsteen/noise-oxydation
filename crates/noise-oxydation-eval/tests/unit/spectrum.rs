use std::f64::consts::PI;

use super::{BINS, POINTS, SpectrumAnalyzer};

#[test]
fn a_bin_centered_tone_has_the_closed_form_hann_main_lobe() {
    // A unit cosine at bin 32 under a periodic Hann window has |X[32]| = N/4 and |X[31]| = |X[33]| = N/8.
    let segment: Vec<f64> =
        (0..POINTS).map(|n| (2.0 * PI * 32.0 * f64::from(u32::try_from(n).expect("small")) / 256.0).cos()).collect();
    let power = SpectrumAnalyzer::new().power(&segment);
    assert!((power[32] - 64.0_f64.powi(2)).abs() < 1e-6, "{}", power[32]);
    assert!((power[31] - 32.0_f64.powi(2)).abs() < 1e-6, "{}", power[31]);
    assert!((power[33] - 32.0_f64.powi(2)).abs() < 1e-6, "{}", power[33]);
    for (bin, &value) in power.iter().enumerate().filter(|(bin, _)| !(31..=33).contains(bin)) {
        assert!(value < 1e-12, "bin {bin}: {value}");
    }
}

#[test]
fn a_constant_lands_in_the_dc_bin_and_short_segments_are_zero_padded() {
    let power = SpectrumAnalyzer::new().power(&[1.0; POINTS]);
    // Σ w = N/2 for the periodic Hann window.
    assert!((power[0] - 128.0_f64.powi(2)).abs() < 1e-6);
    assert_eq!(power.len(), BINS);
    assert!(SpectrumAnalyzer::new().power(&[]).iter().all(|&value| value == 0.0));
}
