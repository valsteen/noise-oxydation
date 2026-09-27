use super::TonalTransient;
use crate::{config::TonalTransientConfig, geometry::BINS, test_support::assert_same_bits};

const PEAK_BIN: usize = 96;

fn flat(level: f32) -> [f32; BINS] {
    [level; BINS]
}

fn with_peaks(level: f32, peaks: &[(usize, f32)]) -> [f32; BINS] {
    let mut power = flat(level);
    for &(bin, value) in peaks {
        power[bin] = value;
    }
    power
}

/// Default parameters except `attack = 0`, so each frame's gain equals its target gain whenever the target falls.
fn immediate_attack() -> TonalTransientConfig {
    TonalTransientConfig { attack: 0.0, ..TonalTransientConfig::default() }
}

/// Runs `frames` through a new detector and returns the gains after the last one.
fn gains_after(config: TonalTransientConfig, frames: &[[f32; BINS]]) -> [f32; BINS] {
    let mut detector = TonalTransient::new(config);
    let mut gains = [f32::NAN; BINS];
    for frame in frames {
        gains = *detector.process(frame);
    }
    gains
}

fn assert_unit(gains: &[f32; BINS], context: &str) {
    for (bin, &gain) in gains.iter().enumerate() {
        assert!(gain.to_bits() == 1.0_f32.to_bits(), "{context}: bin {bin} has gain {gain}");
    }
}

fn assert_close(actual: f32, expected: f64, context: &str) {
    assert!((f64::from(actual) - expected).abs() < 1e-5, "{context}: {actual} vs {expected}");
}

/// Expected gains of a single scored peak at [`PEAK_BIN`] with target `target`, spread over ±2 bins with weights
/// 1, 2/3 and 1/3, and unit gains elsewhere.
fn assert_spread_target(gains: &[f32; BINS], target: f64) {
    for (bin, &gain) in gains.iter().enumerate() {
        let weight = match bin.abs_diff(PEAK_BIN) {
            0 => 1.0,
            1 => 2.0 / 3.0,
            2 => 1.0 / 3.0,
            _ => 0.0,
        };
        assert_close(gain, 1.0 - (1.0 - target) * weight, &format!("bin {bin}"));
    }
}

/// A 40 dB peak over a flat level of 1 at bin 96 has full tonal evidence (T = 40 dB ≥ 14 dB). Harmonic support comes
/// only from the flat level at bins 48, 32 and 24 (2k, 3k and 4k exceed 128): `H = 1 / (0.15·10⁴)`.
fn strong_peak_harmonic_support() -> f64 {
    1.0 / (0.15 * 1e4)
}

#[test]
fn the_first_frame_only_initializes_and_yields_unit_gains() {
    let gains = gains_after(TonalTransientConfig::default(), &[with_peaks(1.0, &[(PEAK_BIN, 1e4)])]);
    assert_unit(&gains, "first frame");
}

#[test]
fn a_flat_spectrum_yields_unit_gains() {
    let gains = gains_after(TonalTransientConfig::default(), &[flat(1.0), flat(1.0), flat(3.0)]);
    assert_unit(&gains, "flat spectra");
}

#[test]
fn peaks_below_the_minimum_frequency_bin_are_ignored() {
    let peaks = [(40, 1e4), (63, 1e4)];
    let gains = gains_after(TonalTransientConfig::default(), &[flat(1.0), with_peaks(1.0, &peaks)]);
    assert_unit(&gains, "peaks at 1.25 and 1.97 kHz");
    // The same peak at the minimum bin itself is analyzed.
    let gains = gains_after(TonalTransientConfig::default(), &[flat(1.0), with_peaks(1.0, &[(64, 1e4)])]);
    assert!(gains[64] < 1.0, "peak at 2 kHz");
}

/// A new 40 dB peak: `T` and the 40 dB flux give full evidence, the previous frame has no bin within ±6 of at least
/// 10 % of the peak (so no movement evidence), and `S = 1·max(1, 0, 0.35)·(1 − H)`.
#[test]
fn a_strong_new_peak_is_attenuated_with_spread_weights() {
    let gains = gains_after(immediate_attack(), &[flat(1.0), with_peaks(1.0, &[(PEAK_BIN, 1e4)])]);
    let score = 1.0 - strong_peak_harmonic_support();
    let target = (1.0 - 0.5 * score).max(0.5);
    assert_spread_target(&gains, target);
}

/// Prominence and flux between their start and full thresholds are normalized linearly: a peak of 10 over a level
/// of 1 has `T = 10 dB` (score 5/9); against a previous level of 0.5 its flux is `10·log10(20)` dB (score
/// `(F − 3)/15`), and the previous frame is too weak for movement evidence. Harmonic support is `1 / (0.15·10)`.
#[test]
fn partial_prominence_and_flux_scale_the_score() {
    let gains = gains_after(immediate_attack(), &[flat(0.5), with_peaks(1.0, &[(PEAK_BIN, 10.0)])]);
    let tonal = 5.0 / 9.0;
    let flux = (10.0 * 20_f64.log10() - 3.0) / 15.0;
    let harmonic = 1.0 / 1.5;
    let score = tonal * flux.max(0.35 * tonal) * (1.0 - harmonic);
    assert_spread_target(&gains, 1.0 - 0.5 * score);
}

#[test]
fn harmonic_support_protects_the_peak() {
    // Support at k/2 = 48 of at least 15 % of the peak power gives H = 1, so the score is 0.
    let supported = with_peaks(1.0, &[(PEAK_BIN, 1e4), (48, 2e3)]);
    assert_unit(&gains_after(immediate_attack(), &[flat(1.0), supported]), "subharmonic support");
    // Support at 2k = 128, within one bin of tolerance, protects a peak at 64.
    let supported = with_peaks(1.0, &[(64, 1e4), (127, 2e3)]);
    assert_unit(&gains_after(immediate_attack(), &[flat(1.0), supported]), "harmonic support");
    // Support outside the tolerance does not.
    let unsupported = with_peaks(1.0, &[(64, 1e4), (125, 2e3)]);
    assert!(gains_after(immediate_attack(), &[flat(1.0), unsupported])[64] < 1.0, "support two bins away");
}

/// A peak that moved from bin 92 to 96 keeps its power, so its flux is 0 dB, but the movement of 4 bins gives full
/// movement evidence; a 3-bin move gives (3 − 1)/(4 − 1).
#[test]
fn a_moving_peak_gets_movement_evidence() {
    let current = with_peaks(1.0, &[(PEAK_BIN, 1e4)]);
    let harmonic = strong_peak_harmonic_support();
    for (previous_bin, movement) in [(92, 1.0), (93, 2.0 / 3.0)] {
        // The previous frame also holds power at 96 so that the flux is 0; the first maximum in 90..=102 wins.
        let previous = with_peaks(1.0, &[(previous_bin, 1e4), (PEAK_BIN, 1e4)]);
        let gains = gains_after(immediate_attack(), &[previous, current]);
        let score = f64::max(movement, 0.35) * (1.0 - harmonic);
        assert_spread_target(&gains, 1.0 - 0.5 * score);
    }
}

/// A stationary tone has no flux and no movement; the persistent tonal evidence `0.35·T` keeps it attenuated.
#[test]
fn a_persistent_tone_stays_attenuated_by_the_persistent_weight() {
    let tone = with_peaks(1.0, &[(PEAK_BIN, 1e4)]);
    let gains = gains_after(immediate_attack(), &[tone, tone]);
    let score = 0.35 * (1.0 - strong_peak_harmonic_support());
    assert_spread_target(&gains, 1.0 - 0.5 * score);
}

/// Falling gains follow `G = 0.3·G + 0.7·target`, rising gains `G = 0.85·G + 0.15·target`; gains stay in
/// `[min_gain, 1]` even when the strength would allow a lower target.
#[test]
fn attack_and_release_smooth_the_gain() {
    let tone = with_peaks(1.0, &[(PEAK_BIN, 1e4)]);
    let target = 1.0 - 0.5 * (1.0 - strong_peak_harmonic_support());
    let mut detector = TonalTransient::new(TonalTransientConfig::default());
    detector.process(&flat(1.0));
    let attacked = 0.3 + 0.7 * target;
    assert_close(detector.process(&tone)[PEAK_BIN], attacked, "attack");
    let released = 0.85 * attacked + 0.15;
    assert_close(detector.process(&flat(1.0))[PEAK_BIN], released, "release");

    let config = TonalTransientConfig { strength: 1.0, min_gain: 0.6, ..TonalTransientConfig::default() };
    let mut detector = TonalTransient::new(config);
    let quiet = flat(1.0);
    for frame in 0..40 {
        let power = if frame % 2 == 0 { &quiet } else { &tone };
        let gains = detector.process(power);
        assert!(gains.iter().all(|&gain| (0.6..=1.0).contains(&gain)), "frame {frame}: {gains:?}");
    }
}

#[test]
fn reset_restarts_from_unit_gains() {
    let tone = with_peaks(1.0, &[(PEAK_BIN, 1e4)]);
    let mut detector = TonalTransient::new(TonalTransientConfig::default());
    detector.process(&flat(1.0));
    assert!(detector.process(&tone)[PEAK_BIN] < 1.0);
    detector.reset();
    assert_unit(detector.process(&tone), "first frame after reset");
    let mut fresh = TonalTransient::new(TonalTransientConfig::default());
    fresh.process(&tone);
    let expected = *fresh.process(&flat(2.0));
    assert_same_bits(detector.process(&flat(2.0)), &expected, "after reset");
}
