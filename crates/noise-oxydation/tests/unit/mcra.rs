use super::Mcra;
use crate::{config::McraConfig, geometry::BINS, test_support::assert_same_bits};

fn uniform(value: f32) -> [f32; BINS] {
    [value; BINS]
}

fn assert_close(actual: f32, expected: f64, context: &str) {
    let error = (f64::from(actual) - expected).abs();
    assert!(error <= 1e-6 * expected.abs().max(1.0), "{context}: {actual} vs {expected}");
}

/// Calibrates on frames of power 2 and 4, so the calibration mean is 3 in every bin.
fn calibrated_to_three(config: McraConfig) -> Mcra {
    let mut estimator = Mcra::new(config);
    assert_close(estimator.estimate(&uniform(2.0), true)[0], 2.0, "first calibration frame");
    assert_close(estimator.estimate(&uniform(4.0), true)[0], 3.0, "running calibration mean");
    estimator
}

/// Calibrates to a noise estimate of exactly 1 in every bin.
fn calibrated_to_one(config: McraConfig) -> Mcra {
    let mut estimator = Mcra::new(config);
    estimator.estimate(&uniform(1.0), true);
    estimator
}

#[test]
fn tracking_starts_from_the_floored_calibration_mean() {
    let mut estimator = calibrated_to_three(McraConfig::default());
    // A frame equal to the mean leaves every initialized quantity unchanged: S = 0.8·3 + 0.2·3, R = 1 ≤ δ, p = 0,
    // N = 0.95·3 + 0.05·3.
    estimator.estimate(&uniform(3.0), false);
    for values in [&estimator.noise, &estimator.smoothed, &estimator.current_minimum, &estimator.previous_minimum] {
        assert!(values.iter().all(|&value| value.to_bits() == 3.0_f32.to_bits()), "{values:?}");
    }
    assert!(estimator.speech_probability.iter().all(|&value| value == 0.0));

    // Without a frame equal to the mean, the first update starts from the mean: S = 0.8·3 + 0.2·8 = 4 and
    // N = 0.95·3 + 0.05·8 = 3.25.
    let mut estimator = calibrated_to_three(McraConfig::default());
    let noise = estimator.estimate(&uniform(8.0), false)[0];
    assert_close(estimator.smoothed[0], 4.0, "smoothed power");
    assert_close(noise, 3.25, "noise");
}

#[test]
fn without_calibration_the_first_frame_initializes_the_estimate() {
    let mut estimator = Mcra::new(McraConfig::default());
    let mut power = uniform(6.0);
    power[3] = 0.0;
    power[4] = -2.0;
    power[5] = f32::NAN;
    let noise = estimator.estimate(&power, false);
    assert_close(noise[0], 6.0, "initialized from the first frame");
    for (bin, &value) in noise.iter().enumerate().take(6).skip(3) {
        assert_close(value, 1e-12, &format!("bin {bin} floored"));
    }
    // The estimate is never the reference's permanent zero: later frames keep tracking from it.
    let noise = estimator.estimate(&uniform(10.0), false)[0];
    assert_close(noise, 0.95 * 6.0 + 0.05 * 10.0, "tracking continues");
}

/// With the estimate calibrated to 1, `S = 0.8 + 0.2·P` and the minimum stays 1, so `R > δ = 5` exactly when
/// `P > 21`. The indicator then sets `p = 0.8` and the noise update uses `a = 0.95 + 0.05·0.8 = 0.99`.
#[test]
fn the_ratio_indicator_switches_at_the_threshold() {
    let mut estimator = calibrated_to_one(McraConfig::default());
    let noise = estimator.estimate(&uniform(20.9), false)[0];
    assert_close(estimator.speech_probability[0], 0.0, "below the threshold");
    assert_close(noise, 0.95 + 0.05 * 20.9, "noise below the threshold");

    let mut estimator = calibrated_to_one(McraConfig::default());
    let noise = estimator.estimate(&uniform(21.1), false)[0];
    assert_close(estimator.speech_probability[0], 0.8, "above the threshold");
    assert_close(noise, 0.99 + 0.01 * 21.1, "noise above the threshold");
}

/// Persistent loud frames keep `I = 1`, so `p_n = 1 − 0.2ⁿ`; a quiet frame with `I = 0` then gives `p = 0.2·p`.
#[test]
fn speech_probability_follows_its_recursion() {
    let mut estimator = calibrated_to_one(McraConfig::default());
    for frame in 1..=4 {
        estimator.estimate(&uniform(1e4), false);
        assert_close(estimator.speech_probability[0], 1.0 - 0.2_f64.powi(frame), &format!("loud frame {frame}"));
    }
    let loud = 1.0 - 0.2_f64.powi(4);
    // S after the loud frames is far above 5 × the minimum only while loud frames continue; force I = 0 with a new
    // estimator state where the ratio stays below δ.
    estimator.smoothed.fill(1.0);
    estimator.estimate(&uniform(1.0), false);
    assert_close(estimator.speech_probability[0], 0.2 * loud, "quiet frame");
}

/// With `p = 0` the noise tracks the observed power with the base smoothing `αd`; with `p = 1` it freezes.
#[test]
fn adaptive_smoothing_tracks_without_speech_and_freezes_with_speech() {
    let mut estimator = calibrated_to_one(McraConfig::default());
    let mut expected = 1.0;
    for _ in 0..20 {
        // P = 4 keeps R = S / 1 ≤ 4 < δ, so p stays 0.
        let noise = estimator.estimate(&uniform(4.0), false)[0];
        expected = 0.95 * expected + 0.05 * 4.0;
        assert_close(noise, expected, "tracking with the base smoothing");
    }

    let config = McraConfig { speech_smoothing: 0.0, ..McraConfig::default() };
    let mut estimator = calibrated_to_one(config);
    for _ in 0..10 {
        // With αp = 0 a loud frame sets p = 1 immediately, so a = 1 and the estimate stays exactly 1.
        let noise = estimator.estimate(&uniform(1e6), false)[0];
        assert_eq!(noise.to_bits(), 1.0_f32.to_bits());
    }
}

/// After every `W` updated frames the current minimum becomes the previous minimum and restarts at `+∞`, so a rise
/// in the power floor stops counting as speech once two windows have passed.
#[test]
fn the_minimum_window_rotates_every_w_frames() {
    let config = McraConfig { window_frames: 3, ..McraConfig::default() };
    let mut estimator = calibrated_to_one(config);
    let raised = uniform(100.0);
    estimator.estimate(&raised, false);
    estimator.estimate(&raised, false);
    assert_eq!(estimator.frames_in_window, 2);
    assert_close(estimator.current_minimum[0], 1.0, "minimum before rotation");
    estimator.estimate(&raised, false);
    assert_eq!(estimator.frames_in_window, 0);
    assert_close(estimator.previous_minimum[0], 1.0, "rotated minimum");
    assert!(estimator.current_minimum.iter().all(|&value| value == f32::INFINITY));

    // Frames 4–6 fill the second window with the smoothed values of the raised floor; after its rotation both minima
    // are high, so R falls below δ and p decays.
    for _ in 0..3 {
        estimator.estimate(&raised, false);
    }
    let smoothed_after_four = {
        let mut smoothed = 1.0_f64;
        for _ in 0..4 {
            smoothed = 0.8 * smoothed + 0.2 * 100.0;
        }
        smoothed
    };
    assert_close(estimator.previous_minimum[0], smoothed_after_four, "second window minimum");
    let before = estimator.speech_probability[0];
    estimator.estimate(&raised, false);
    assert_close(estimator.speech_probability[0], 0.2 * f64::from(before), "indicator off after rotation");
}

#[test]
fn reset_restarts_calibration() {
    let mut estimator = calibrated_to_three(McraConfig::default());
    estimator.estimate(&uniform(50.0), false);
    estimator.reset();
    assert_close(estimator.estimate(&uniform(7.0), true)[0], 7.0, "fresh calibration mean");
    let mut fresh = Mcra::new(McraConfig::default());
    fresh.estimate(&uniform(7.0), true);
    let expected = *fresh.estimate(&uniform(9.0), false);
    assert_same_bits(estimator.estimate(&uniform(9.0), false), &expected, "after reset");
}
