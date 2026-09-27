use std::time::Duration;

use super::{
    CallConfig, DecisionDirectedConfig, HighPassConfig, InterferenceConfig, LogMmseConfig, McraConfig, MinimumConfig,
    NoiseEstimatorConfig, SppMmseConfig, TonalTransientConfig, ValidConfig,
};
use crate::error::{ConfigError, ConfigField, Constraint};

type Setter = fn(&mut CallConfig, f32);
type CountSetter = fn(&mut CallConfig, u16);
/// Sets a start/full threshold pair of the tonal transient configuration.
type PairSetter = fn(&mut TonalTransientConfig, f32, f32);

fn set_spp(config: &mut CallConfig, update: impl FnOnce(&mut SppMmseConfig)) {
    let mut spp = SppMmseConfig::default();
    update(&mut spp);
    config.noise_estimator = NoiseEstimatorConfig::SppMmse(spp);
}

fn set_mcra(config: &mut CallConfig, update: impl FnOnce(&mut McraConfig)) {
    let mut mcra = McraConfig::default();
    update(&mut mcra);
    config.noise_estimator = NoiseEstimatorConfig::Mcra(mcra);
}

fn set_minimum(config: &mut CallConfig, update: impl FnOnce(&mut MinimumConfig)) {
    let mut minimum = MinimumConfig::default();
    update(&mut minimum);
    config.noise_estimator = NoiseEstimatorConfig::Minimum(minimum);
}

fn set_tonal(config: &mut CallConfig, update: impl FnOnce(&mut TonalTransientConfig)) {
    let mut tonal = TonalTransientConfig::default();
    update(&mut tonal);
    config.interference = InterferenceConfig::TonalTransient(tonal);
}

/// Every validated real-valued field with its constraint, a setter, and values inside its range that must be accepted
/// (with the other fields at their defaults).
const FIELDS: [(ConfigField, Constraint, Setter, &[f32]); 33] = [
    (ConfigField::HighPassCutoffHz, Constraint::BelowNyquist, |c, v| c.high_pass.cutoff_hz = v, &[0.01, 3999.0]),
    (
        ConfigField::SppNoiseSmoothing,
        Constraint::OpenUnit,
        |c, v| set_spp(c, |s| s.noise_smoothing = v),
        &[1e-6, 0.999_99],
    ),
    (ConfigField::SppSmoothing, Constraint::OpenUnit, |c, v| set_spp(c, |s| s.spp_smoothing = v), &[1e-6, 0.999_99]),
    (ConfigField::SppSpeechPrior, Constraint::OpenUnit, |c, v| set_spp(c, |s| s.speech_prior = v), &[1e-6, 0.999_99]),
    (ConfigField::SppFixedPriorSnr, Constraint::Positive, |c, v| set_spp(c, |s| s.fixed_prior_snr = v), &[1e-30, 1e30]),
    (
        ConfigField::SppStagnationThreshold,
        Constraint::UnitExcludingZero,
        |c, v| set_spp(c, |s| s.stagnation_threshold = v),
        &[1e-6, 1.0],
    ),
    (
        ConfigField::SppMaxSpeechProbability,
        Constraint::UnitExcludingZero,
        |c, v| set_spp(c, |s| s.max_speech_probability = v),
        &[1e-6, 1.0],
    ),
    (ConfigField::SppFloor, Constraint::Positive, |c, v| set_spp(c, |s| s.floor = v), &[1e-30, 1e30]),
    (
        ConfigField::DecisionDirectedAlpha,
        Constraint::UnitExcludingOne,
        |c, v| c.decision_directed.alpha = v,
        &[0.0, 0.999_99],
    ),
    (ConfigField::DecisionDirectedFloor, Constraint::Positive, |c, v| c.decision_directed.floor = v, &[1e-30, 1e30]),
    (ConfigField::LogMmseMinGain, Constraint::ClosedUnit, |c, v| c.log_mmse.min_gain = v, &[0.0, 1.0]),
    (
        ConfigField::LogMmseMaxGain,
        Constraint::UnitExcludingZero,
        |c, v| {
            c.log_mmse.min_gain = 0.0;
            c.log_mmse.max_gain = v;
        },
        &[1e-6, 1.0],
    ),
    (ConfigField::LogMmseFloor, Constraint::Positive, |c, v| c.log_mmse.floor = v, &[1e-30, 1e30]),
    (
        ConfigField::LogMmseNoiseOverestimation,
        Constraint::Positive,
        |c, v| c.log_mmse.noise_overestimation = v,
        &[1e-30, 1e30],
    ),
    (
        ConfigField::McraSmoothing,
        Constraint::UnitExcludingOne,
        |c, v| set_mcra(c, |m| m.smoothing = v),
        &[0.0, 0.999_99],
    ),
    (
        ConfigField::McraSpeechSmoothing,
        Constraint::UnitExcludingOne,
        |c, v| set_mcra(c, |m| m.speech_smoothing = v),
        &[0.0, 0.999_99],
    ),
    (
        ConfigField::McraNoiseSmoothing,
        Constraint::UnitExcludingOne,
        |c, v| set_mcra(c, |m| m.noise_smoothing = v),
        &[0.0, 0.999_99],
    ),
    (
        ConfigField::McraRatioThreshold,
        Constraint::GreaterThanOne,
        |c, v| set_mcra(c, |m| m.ratio_threshold = v),
        &[1.000_001, 1e30],
    ),
    (ConfigField::McraFloor, Constraint::Positive, |c, v| set_mcra(c, |m| m.floor = v), &[1e-30, 1e30]),
    (
        ConfigField::MinimumSmoothing,
        Constraint::UnitExcludingOne,
        |c, v| set_minimum(c, |m| m.smoothing = v),
        &[0.0, 0.999_99],
    ),
    (ConfigField::MinimumFloor, Constraint::Positive, |c, v| set_minimum(c, |m| m.floor = v), &[1e-30, 1e30]),
    (ConfigField::TonalStartDb, Constraint::NonNegative, |c, v| set_tonal(c, |t| t.tonal_start_db = v), &[0.0, 13.9]),
    (ConfigField::TonalFullDb, Constraint::NonNegative, |c, v| set_tonal(c, |t| t.tonal_full_db = v), &[5.1, 1e30]),
    (
        ConfigField::TonalFluxStartDb,
        Constraint::NonNegative,
        |c, v| set_tonal(c, |t| t.flux_start_db = v),
        &[0.0, 17.9],
    ),
    (ConfigField::TonalFluxFullDb, Constraint::NonNegative, |c, v| set_tonal(c, |t| t.flux_full_db = v), &[3.1, 1e30]),
    (
        ConfigField::TonalMovementMinRelativePower,
        Constraint::UnitExcludingZero,
        |c, v| {
            set_tonal(c, |t| t.movement_min_relative_power = v);
        },
        &[1e-6, 1.0],
    ),
    (
        ConfigField::TonalHarmonicRelativePower,
        Constraint::UnitExcludingZero,
        |c, v| {
            set_tonal(c, |t| t.harmonic_relative_power = v);
        },
        &[1e-6, 1.0],
    ),
    (ConfigField::TonalStrength, Constraint::ClosedUnit, |c, v| set_tonal(c, |t| t.strength = v), &[0.0, 1.0]),
    (ConfigField::TonalMinGain, Constraint::ClosedUnit, |c, v| set_tonal(c, |t| t.min_gain = v), &[0.0, 1.0]),
    (ConfigField::TonalAttack, Constraint::UnitExcludingOne, |c, v| set_tonal(c, |t| t.attack = v), &[0.0, 0.999_99]),
    (ConfigField::TonalRelease, Constraint::UnitExcludingOne, |c, v| set_tonal(c, |t| t.release = v), &[0.0, 0.999_99]),
    (ConfigField::TonalFloor, Constraint::Positive, |c, v| set_tonal(c, |t| t.floor = v), &[1e-30, 1e30]),
    (
        ConfigField::TonalFullDb,
        Constraint::NonNegative,
        |c, v| {
            set_tonal(c, |t| {
                t.tonal_start_db = 0.0;
                t.tonal_full_db = v;
            });
        },
        &[1e-6],
    ),
];

/// Every validated count field with its allowed range, a setter, and values that must be accepted.
const COUNT_FIELDS: [(ConfigField, u16, u16, CountSetter, &[u16]); 10] = [
    (ConfigField::McraWindowFrames, 1, 1000, |c, v| set_mcra(c, |m| m.window_frames = v), &[1, 1000]),
    (ConfigField::MinimumWindowFrames, 1, 1000, |c, v| set_minimum(c, |m| m.window_frames = v), &[1, 1000]),
    (ConfigField::TonalLocalRadius, 1, 32, |c, v| set_tonal(c, |t| t.local_radius = v), &[1, 32]),
    (ConfigField::TonalMovementSearchRadius, 1, 64, |c, v| set_tonal(c, |t| t.movement_search_radius = v), &[1, 64]),
    (ConfigField::TonalMovementStartBins, 1, 64, |c, v| set_tonal(c, |t| t.movement_start_bins = v), &[1, 3]),
    (ConfigField::TonalMovementFullBins, 1, 64, |c, v| set_tonal(c, |t| t.movement_full_bins = v), &[2, 64]),
    (ConfigField::TonalMinFrequencyBin, 1, 128, |c, v| set_tonal(c, |t| t.min_frequency_bin = v), &[1, 128]),
    (ConfigField::TonalHarmonicToleranceBins, 0, 16, |c, v| set_tonal(c, |t| t.harmonic_tolerance_bins = v), &[0, 16]),
    (ConfigField::TonalSpreadRadius, 0, 64, |c, v| set_tonal(c, |t| t.spread_radius = v), &[0, 64]),
    (
        ConfigField::TonalMovementStartBins,
        1,
        64,
        |c, v| {
            set_tonal(c, |t| {
                t.movement_start_bins = v;
                t.movement_full_bins = 64;
            });
        },
        &[63],
    ),
];

fn invalid_values(constraint: Constraint) -> &'static [f32] {
    match constraint {
        Constraint::OpenUnit => &[0.0, 1.0, -0.1, 1.5, f32::NAN, f32::INFINITY],
        Constraint::UnitExcludingOne => &[1.0, -0.01, f32::NAN, f32::NEG_INFINITY],
        Constraint::UnitExcludingZero => &[0.0, 1.01, -0.5, f32::NAN],
        Constraint::ClosedUnit => &[-0.01, 1.01, f32::NAN],
        Constraint::Positive => &[0.0, -1.0, f32::INFINITY, f32::NAN],
        Constraint::NonNegative => &[-0.01, f32::INFINITY, f32::NAN],
        Constraint::GreaterThanOne => &[1.0, 0.5, -2.0, f32::INFINITY, f32::NAN],
        Constraint::BelowNyquist => &[0.0, 4000.0, -5.0, f32::NAN, f32::INFINITY],
    }
}

#[test]
fn every_invalid_field_is_rejected_with_its_own_error() {
    for (field, constraint, set, _) in FIELDS {
        for &value in invalid_values(constraint) {
            let mut config = CallConfig::default();
            set(&mut config, value);
            match ValidConfig::new(&config) {
                Err(ConfigError::OutOfRange { field: rejected, value: reported, constraint: violated }) => {
                    assert_eq!((rejected, violated), (field, constraint), "{field} = {value}");
                    assert_eq!(reported.to_bits(), value.to_bits(), "{field} = {value}");
                }
                other => panic!("{field} = {value} gave {other:?}"),
            }
        }
    }
}

#[test]
fn boundary_values_inside_each_range_are_accepted() {
    for (field, _, set, accepted) in FIELDS {
        for &value in accepted {
            let mut config = CallConfig::default();
            set(&mut config, value);
            assert!(ValidConfig::new(&config).is_ok(), "{field} = {value}");
        }
    }
}

#[test]
fn every_count_outside_its_range_is_rejected_with_its_own_error() {
    for (field, min, max, set, _) in COUNT_FIELDS {
        let below = min.checked_sub(1);
        for value in below.into_iter().chain([max + 1, u16::MAX]) {
            let mut config = CallConfig::default();
            set(&mut config, value);
            assert_eq!(
                ValidConfig::new(&config).map(|_| ()),
                Err(ConfigError::CountOutOfRange { field, value, min, max }),
                "{field} = {value}"
            );
        }
    }
}

#[test]
fn counts_inside_each_range_are_accepted() {
    for (field, _, _, set, accepted) in COUNT_FIELDS {
        for &value in accepted {
            let mut config = CallConfig::default();
            set(&mut config, value);
            assert!(ValidConfig::new(&config).is_ok(), "{field} = {value}");
        }
    }
}

/// The reference silently restores both defaults when a start threshold is not below its full threshold; here the
/// pair is rejected.
#[test]
fn start_thresholds_must_lie_below_full_thresholds() {
    let cases: [(PairSetter, ConfigField, ConfigField); 3] = [
        (
            |t, start, full| (t.tonal_start_db, t.tonal_full_db) = (start, full),
            ConfigField::TonalStartDb,
            ConfigField::TonalFullDb,
        ),
        (
            |t, start, full| (t.flux_start_db, t.flux_full_db) = (start, full),
            ConfigField::TonalFluxStartDb,
            ConfigField::TonalFluxFullDb,
        ),
        (
            |t, start, full| {
                (t.movement_start_bins, t.movement_full_bins) = (bins(start), bins(full));
            },
            ConfigField::TonalMovementStartBins,
            ConfigField::TonalMovementFullBins,
        ),
    ];
    for (set, start, full) in cases {
        for (start_value, full_value) in [(4.0, 4.0), (9.0, 5.0)] {
            let mut config = CallConfig::default();
            set_tonal(&mut config, |tonal| set(tonal, start_value, full_value));
            assert_eq!(
                ValidConfig::new(&config).map(|_| ()),
                Err(ConfigError::StartNotBelowFull { start, start_value, full, full_value }),
                "{start} = {start_value}, {full} = {full_value}"
            );
        }
    }
}

fn bins(value: f32) -> u16 {
    [4, 5, 9].into_iter().find(|&count| f32::from(count).to_bits() == value.to_bits()).expect("test bin counts")
}

#[test]
fn only_the_selected_alternatives_are_validated() {
    let invalid_tonal = TonalTransientConfig { strength: 2.0, ..TonalTransientConfig::default() };
    let mut config =
        CallConfig { interference: InterferenceConfig::TonalTransient(invalid_tonal), ..CallConfig::default() };
    assert!(ValidConfig::new(&config).is_err());
    config.interference = InterferenceConfig::Disabled;
    assert!(ValidConfig::new(&config).is_ok());
}

#[test]
fn min_gain_above_max_gain_is_rejected_rather_than_replaced() {
    let mut config = CallConfig::default();
    config.log_mmse.min_gain = 0.5;
    config.log_mmse.max_gain = 0.4;
    assert!(matches!(
        ValidConfig::new(&config),
        Err(ConfigError::MinGainAboveMaxGain { min_gain, max_gain })
            if min_gain.to_bits() == 0.5_f32.to_bits() && max_gain.to_bits() == 0.4_f32.to_bits()
    ));
    config.log_mmse.min_gain = 0.4;
    assert!(ValidConfig::new(&config).is_ok());
}

#[test]
fn calibration_duration_converts_to_whole_samples_with_integer_arithmetic() {
    let samples = |duration| {
        let config = CallConfig { calibration_duration: duration, ..CallConfig::default() };
        ValidConfig::new(&config).map(|valid| valid.calibration_samples)
    };
    assert_eq!(samples(Duration::from_secs(5)), Ok(40_000));
    assert_eq!(samples(Duration::ZERO), Ok(0));
    assert_eq!(samples(Duration::from_micros(125)), Ok(1));
    assert_eq!(samples(Duration::from_nanos(124_999)), Ok(0));
    assert_eq!(samples(Duration::from_micros(999)), Ok(7));
    assert_eq!(samples(Duration::MAX), Err(ConfigError::CalibrationTooLong { duration: Duration::MAX }));
}

#[test]
fn defaults_equal_the_reference_documented_values() {
    let spp_mmse = SppMmseConfig {
        noise_smoothing: 0.8,
        spp_smoothing: 0.9,
        speech_prior: 0.5,
        fixed_prior_snr: 31.622_776,
        stagnation_threshold: 0.99,
        max_speech_probability: 0.99,
        floor: 1e-12,
    };
    let tonal_transient = TonalTransientConfig {
        local_radius: 2,
        tonal_start_db: 5.0,
        tonal_full_db: 14.0,
        flux_start_db: 3.0,
        flux_full_db: 18.0,
        movement_search_radius: 6,
        movement_start_bins: 1,
        movement_full_bins: 4,
        min_frequency_bin: 64,
        movement_min_relative_power: 0.1,
        harmonic_tolerance_bins: 1,
        harmonic_relative_power: 0.15,
        strength: 0.5,
        min_gain: 0.5,
        attack: 0.3,
        release: 0.85,
        spread_radius: 2,
        floor: 1e-12,
    };
    let reference = CallConfig {
        calibration_duration: Duration::from_secs(5),
        high_pass: HighPassConfig { cutoff_hz: 80.0 },
        noise_estimator: NoiseEstimatorConfig::SppMmse(spp_mmse),
        decision_directed: DecisionDirectedConfig { alpha: 0.98, floor: 1e-12 },
        log_mmse: LogMmseConfig { min_gain: 0.05, max_gain: 1.0, floor: 1e-12, noise_overestimation: 1.25 },
        interference: InterferenceConfig::TonalTransient(tonal_transient),
    };
    assert_eq!(CallConfig::default(), reference);
    // The fixed prior SNR is 15 dB.
    let decibels = 10.0 * f64::from(spp_mmse.fixed_prior_snr).log10();
    assert!((decibels - 15.0).abs() < 1e-6);

    let mcra = McraConfig {
        smoothing: 0.8,
        speech_smoothing: 0.2,
        noise_smoothing: 0.95,
        ratio_threshold: 5.0,
        window_frames: 50,
        floor: 1e-12,
    };
    assert_eq!(McraConfig::default(), mcra);
    assert_eq!(MinimumConfig::default(), MinimumConfig { smoothing: 0.8, window_frames: 50, floor: 1e-12 });
}

#[test]
fn error_messages_name_the_field_and_the_constraint() {
    let message = |update: fn(&mut CallConfig)| {
        let mut config = CallConfig::default();
        update(&mut config);
        ValidConfig::new(&config).expect_err("invalid configuration").to_string()
    };
    assert_eq!(
        message(|config| set_spp(config, |spp| spp.speech_prior = 1.0)),
        "spp_mmse.speech_prior must be in (0, 1), got 1"
    );
    assert_eq!(
        message(|config| set_mcra(config, |mcra| mcra.window_frames = 0)),
        "mcra.window_frames must be in [1, 1000], got 0"
    );
    assert_eq!(
        message(|config| set_tonal(config, |tonal| tonal.tonal_full_db = 5.0)),
        "tonal_transient.tonal_start_db (5) must be less than tonal_transient.tonal_full_db (5)"
    );
}
