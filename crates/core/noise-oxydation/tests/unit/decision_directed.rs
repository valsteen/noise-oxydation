use super::DecisionDirected;
use crate::{config::DecisionDirectedConfig, geometry::BINS};

fn estimate(snr: &DecisionDirected, power: f32, noise: f32) -> (f32, f32) {
    let (mut gamma, mut xi) = ([0.0; BINS], [0.0; BINS]);
    snr.estimate(&[power; BINS], &[noise; BINS], &mut gamma, &mut xi);
    (gamma[7], xi[7])
}

fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() <= 1e-6 * expected.abs().max(1.0), "{actual} vs {expected}");
}

#[test]
fn first_frame_uses_the_instantaneous_snr() {
    let snr = DecisionDirected::new(DecisionDirectedConfig::default());
    let (gamma, xi) = estimate(&snr, 4.0, 1.0);
    assert_close(gamma, 4.0);
    assert_close(xi, 3.0);
    // Below the noise level the instantaneous SNR is clipped at 0.
    let (gamma, xi) = estimate(&snr, 0.5, 1.0);
    assert_close(gamma, 0.5);
    assert_close(xi, 0.0);
}

#[test]
fn later_frames_blend_the_previous_clean_snr_with_the_instantaneous_snr() {
    let mut snr = DecisionDirected::new(DecisionDirectedConfig::default());
    estimate(&snr, 4.0, 1.0);
    // Previous clean power 2 over noise 0.5 gives a stored clean SNR of 4.
    snr.update(&[2.0; BINS], &[0.5; BINS]);
    let (gamma, xi) = estimate(&snr, 3.0, 1.0);
    assert_close(gamma, 3.0);
    assert_close(xi, 0.98 * 4.0 + 0.02 * 2.0);
}

#[test]
fn noise_and_power_are_bounded_before_division() {
    let mut snr = DecisionDirected::new(DecisionDirectedConfig { alpha: 0.5, floor: 1e-3 });
    let (gamma, xi) = estimate(&snr, -1.0, 0.0);
    assert_close(gamma, 0.0);
    assert_close(xi, 0.0);
    let (gamma, _) = estimate(&snr, 1.0, 0.0);
    assert_close(gamma, 1000.0);
    snr.update(&[-5.0; BINS], &[0.0; BINS]);
    let (_, xi) = estimate(&snr, 3.0, 1.0);
    assert_close(xi, 0.5 * 0.0 + 0.5 * 2.0);
}

#[test]
fn reset_returns_to_first_frame_behavior() {
    let mut snr = DecisionDirected::new(DecisionDirectedConfig::default());
    snr.update(&[100.0; BINS], &[1.0; BINS]);
    snr.reset();
    let (_, xi) = estimate(&snr, 4.0, 1.0);
    assert_close(xi, 3.0);
}
