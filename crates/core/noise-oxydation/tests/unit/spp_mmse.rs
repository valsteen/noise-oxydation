use super::SppMmse;
use crate::{config::SppMmseConfig, geometry::BINS};

fn new_estimator() -> SppMmse {
    SppMmse::new(SppMmseConfig::default())
}

fn uniform(value: f32) -> [f32; BINS] {
    [value; BINS]
}

fn assert_close(actual: f32, expected: f64, tolerance: f64, context: &str) {
    let error = (f64::from(actual) - expected).abs();
    assert!(error <= tolerance * expected.abs().max(1.0), "{context}: {actual} vs {expected}");
}

/// Calibrates to a noise estimate of exactly 1 in every bin.
fn calibrated_to_one() -> SppMmse {
    let mut estimator = new_estimator();
    estimator.estimate(&uniform(1.0), true);
    estimator
}

#[test]
fn calibration_estimate_is_the_running_mean_power() {
    let mut estimator = new_estimator();
    let frames: [[f32; BINS]; 3] = [uniform(2.0), uniform(4.0), uniform(9.0)];
    let mut sum = 0.0;
    for (count, frame) in (1_u8..).zip(&frames) {
        sum += f64::from(frame[0]);
        let noise = estimator.estimate(frame, true);
        assert_close(noise[0], sum / f64::from(count), 1e-7, "running mean");
    }
    let mut varied = [0.0; BINS];
    for (bin, value) in (0_u8..).zip(&mut varied) {
        *value = f32::from(bin) * 0.5;
    }
    let mut estimator = new_estimator();
    estimator.estimate(&varied, true);
    let noise = estimator.estimate(&uniform(1.0), true);
    for (bin, &value) in noise.iter().enumerate().skip(1) {
        assert_close(value, f64::midpoint(f64::from(varied[bin]), 1.0), 1e-7, "per-bin mean");
    }
    // Bin 0 observed 0, which is sanitized to the floor before averaging.
    assert_close(noise[0], f64::midpoint(1e-12, 1.0), 1e-7, "floored bin");
}

#[test]
fn without_calibration_the_first_frame_initializes_the_estimate() {
    let mut estimator = new_estimator();
    let mut power = uniform(3.0);
    power[5] = 0.0;
    power[6] = f32::NAN;
    let noise = estimator.estimate(&power, false);
    assert_close(noise[0], 3.0, 1e-7, "initialized");
    assert_close(noise[5], 1e-12, 1e-6, "zero floored");
    assert_close(noise[6], 1e-12, 1e-6, "NaN floored");
}

/// With `q = 0.5`, the likelihood ratio is 1 (so `p = 0.5`) at `γ* = ((1 + ξ)/ξ)·ln(1 + ξ)`. The update is then
/// `N = 0.8·1 + 0.2·(0.5·γ* + 0.5·1)`.
#[test]
fn speech_presence_is_one_half_where_the_likelihood_ratio_is_one() {
    let xi = 10_f64.powf(1.5);
    let gamma = (1.0 + xi) / xi * (1.0 + xi).ln();
    let mut estimator = calibrated_to_one();
    let noise = estimator.estimate(&uniform(super::narrow(gamma)), false);
    assert_close(noise[0], 0.8 + 0.2 * (0.5 * gamma + 0.5), 1e-6, "p = 0.5");
}

/// At `γ → 0` the probability is `1 / (1 + (1 + ξ))`; the observed floor power barely contributes.
#[test]
fn silence_has_the_speech_absence_floor_probability() {
    let xi = 10_f64.powf(1.5);
    let probability = 1.0 / (2.0 + xi);
    let mut estimator = calibrated_to_one();
    let noise = estimator.estimate(&uniform(0.0), false);
    assert_close(noise[0], 0.8 + 0.2 * probability, 1e-6, "silent frame");
}

/// Loud frames have `p = 1`, which freezes the estimate, until the smoothed probability `1 − 0.9ⁿ` exceeds 0.99
/// (after 44 frames); from then on `p` is capped at 0.99 and the estimate rises.
#[test]
fn stagnation_cap_lets_the_estimate_follow_persistent_loud_input() {
    let mut estimator = calibrated_to_one();
    let loud = uniform(1e6);
    let mut first_change = None;
    for frame in 1_u32..=60 {
        let noise = estimator.estimate(&loud, false)[0];
        if first_change.is_none() && (noise - 1.0).abs() > 1e-6 {
            first_change = Some((frame, noise));
        }
    }
    let (frame, noise) = first_change.expect("the cap releases the estimate");
    assert!((43..=45).contains(&frame), "released at frame {frame}");
    assert_close(noise, 0.8 + 0.2 * (0.01 * 1e6 + 0.99), 1e-5, "capped update");
}

#[test]
fn reset_restarts_calibration() {
    let mut estimator = calibrated_to_one();
    estimator.estimate(&uniform(50.0), false);
    estimator.reset();
    let noise = estimator.estimate(&uniform(7.0), true);
    assert_close(noise[0], 7.0, 1e-7, "fresh calibration mean");
}
