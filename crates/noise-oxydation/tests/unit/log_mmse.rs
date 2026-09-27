use super::{LogMmse, exponential_integral, gain};
use crate::{
    config::{DecisionDirectedConfig, LogMmseConfig},
    fft::Complex,
    geometry::BINS,
};

/// Published values of the exponential integral (Abramowitz and Stegun, table 5.1).
#[test]
fn exponential_integral_matches_published_values() {
    for (x, expected) in [(0.1, 1.822_923_958_4), (1.0, 0.219_383_934_4), (5.0, 0.001_148_295_6)] {
        let actual = exponential_integral(x);
        assert!((actual - expected).abs() < 1e-9 * expected.max(1.0), "E1({x}) = {actual}, expected {expected}");
    }
    // Both evaluation branches agree where they meet.
    let below = exponential_integral(1.0);
    let above = exponential_integral(1.0 + 1e-12);
    assert!((below - above).abs() < 1e-10);
    assert!(exponential_integral(0.0).is_infinite());
}

#[test]
fn high_snr_gain_approaches_the_wiener_gain() {
    // v = γ·ξ/(1 + ξ) = 100, where E1(100) ≈ 3.7e-46, so G = ξ/(1 + ξ).
    let gain = gain(101.0, 100.0, 1e-12);
    assert!((gain - 100.0 / 101.0).abs() < 1e-6, "{gain}");
}

#[test]
fn zero_a_priori_snr_gives_zero_gain_before_clamping() {
    assert!(gain(5.0, 0.0, 1e-12).abs() < f32::EPSILON);
}

/// For `v ≪ 1`, `E1(v) ≈ −γ_E − ln v`, so `G ≈ ξ/(1 + ξ) · exp(−γ_E/2) / √v`: the gain exceeds the Wiener gain.
#[test]
fn low_snr_gain_exceeds_the_wiener_gain() {
    let (gamma, xi) = (0.01_f32, 0.01_f32);
    let v = f64::from(gamma * xi / (1.0 + xi));
    let wiener = f64::from(xi / (1.0 + xi));
    let approximation = wiener * (-0.577_215_664_9_f64 / 2.0).exp() / v.sqrt();
    let actual = f64::from(gain(gamma, xi, 1e-12));
    assert!(actual > wiener);
    assert!((actual - approximation).abs() < 1e-3 * approximation, "{actual} vs {approximation}");
}

fn suppressor(min_gain: f32, max_gain: f32) -> LogMmse {
    let config = LogMmseConfig { min_gain, max_gain, ..LogMmseConfig::default() };
    LogMmse::new(config, DecisionDirectedConfig::default())
}

#[test]
fn gains_are_clamped_and_applied_to_every_bin() {
    // Noise estimate 0.8 is overestimated to 1.0, so γ equals the observed power.
    let noise = [0.8; BINS];
    let mut spectrum = [Complex::new(3.0, -4.0); BINS];
    let power = [25.0; BINS];

    // First frame: ξ = γ − 1 = 24 and v = 24, so G = 24/25 = 0.96 (E1(24) ≈ 1.5e-12); a 0.5 cap applies.
    let mut capped = suppressor(0.05, 0.5);
    capped.apply(&mut spectrum, &power, &noise);
    assert!(spectrum.iter().all(|bin| (bin.re - 1.5).abs() < 1e-6 && (bin.im + 2.0).abs() < 1e-6));

    // Power below the effective noise gives ξ = 0 and G = 0, raised to the minimum gain.
    let mut spectrum = [Complex::new(0.3, 0.4); BINS];
    let mut floored = suppressor(0.05, 1.0);
    floored.apply(&mut spectrum, &[0.25; BINS], &noise);
    assert!(spectrum.iter().all(|bin| (bin.re - 0.015).abs() < 1e-7 && (bin.im - 0.02).abs() < 1e-7));
}

#[test]
fn clean_power_feeds_the_next_frame_and_reset_clears_it() {
    let noise = [0.8; BINS];
    let power = [25.0; BINS];
    let first_frame_gain = |suppressor: &mut LogMmse| {
        let mut spectrum = [Complex::new(5.0, 0.0); BINS];
        suppressor.apply(&mut spectrum, &power, &noise);
        spectrum[3].re / 5.0
    };
    let mut suppressor = suppressor(0.0, 1.0);
    let first = first_frame_gain(&mut suppressor);
    assert!((first - 0.96).abs() < 1e-6);
    // Second frame: ξ = 0.98·(G²·25/1) + 0.02·24 = 0.98·23.04 + 0.48 = 23.0592.
    let second = first_frame_gain(&mut suppressor);
    let xi = 0.98 * 23.04 + 0.02 * 24.0;
    assert!((f64::from(second) - xi / (1.0 + xi)).abs() < 1e-5, "{second}");
    suppressor.reset();
    assert!((first_frame_gain(&mut suppressor) - first).abs() < f32::EPSILON);
}
