use std::time::Duration;

use super::{CallConfig, DecisionDirectedConfig, HighPassConfig, LogMmseConfig, SppMmseConfig, ValidConfig};
use crate::error::{ConfigError, ConfigField, Constraint};

type Setter = fn(&mut CallConfig, f32);

/// Every validated numeric field with its constraint and a setter.
const FIELDS: [(ConfigField, Constraint, Setter); 14] = [
    (ConfigField::HighPassCutoffHz, Constraint::BelowNyquist, |config, value| config.high_pass.cutoff_hz = value),
    (ConfigField::SppNoiseSmoothing, Constraint::OpenUnit, |config, value| config.spp_mmse.noise_smoothing = value),
    (ConfigField::SppSmoothing, Constraint::OpenUnit, |config, value| config.spp_mmse.spp_smoothing = value),
    (ConfigField::SppSpeechPrior, Constraint::OpenUnit, |config, value| config.spp_mmse.speech_prior = value),
    (ConfigField::SppFixedPriorSnr, Constraint::Positive, |config, value| config.spp_mmse.fixed_prior_snr = value),
    (ConfigField::SppStagnationThreshold, Constraint::UnitExcludingZero, |config, value| {
        config.spp_mmse.stagnation_threshold = value;
    }),
    (ConfigField::SppMaxSpeechProbability, Constraint::UnitExcludingZero, |config, value| {
        config.spp_mmse.max_speech_probability = value;
    }),
    (ConfigField::SppFloor, Constraint::Positive, |config, value| config.spp_mmse.floor = value),
    (ConfigField::DecisionDirectedAlpha, Constraint::UnitExcludingOne, |config, value| {
        config.decision_directed.alpha = value;
    }),
    (ConfigField::DecisionDirectedFloor, Constraint::Positive, |config, value| config.decision_directed.floor = value),
    (ConfigField::LogMmseMinGain, Constraint::ClosedUnit, |config, value| config.log_mmse.min_gain = value),
    (ConfigField::LogMmseMaxGain, Constraint::UnitExcludingZero, |config, value| config.log_mmse.max_gain = value),
    (ConfigField::LogMmseFloor, Constraint::Positive, |config, value| config.log_mmse.floor = value),
    (ConfigField::LogMmseNoiseOverestimation, Constraint::Positive, |config, value| {
        config.log_mmse.noise_overestimation = value;
    }),
];

fn invalid_values(constraint: Constraint) -> &'static [f32] {
    match constraint {
        Constraint::OpenUnit => &[0.0, 1.0, -0.1, 1.5, f32::NAN, f32::INFINITY],
        Constraint::UnitExcludingOne => &[1.0, -0.01, f32::NAN, f32::NEG_INFINITY],
        Constraint::UnitExcludingZero => &[0.0, 1.01, -0.5, f32::NAN],
        Constraint::ClosedUnit => &[-0.01, 1.01, f32::NAN],
        Constraint::Positive => &[0.0, -1.0, f32::INFINITY, f32::NAN],
        Constraint::BelowNyquist => &[0.0, 4000.0, -5.0, f32::NAN, f32::INFINITY],
    }
}

fn boundary_values(constraint: Constraint) -> &'static [f32] {
    match constraint {
        Constraint::OpenUnit => &[1e-6, 0.999_99],
        Constraint::UnitExcludingOne => &[0.0, 0.999_99],
        Constraint::UnitExcludingZero => &[1e-6, 1.0],
        Constraint::ClosedUnit => &[0.0, 1.0],
        Constraint::Positive => &[1e-30, 1e30],
        Constraint::BelowNyquist => &[0.01, 3999.0],
    }
}

#[test]
fn every_invalid_field_is_rejected_with_its_own_error() {
    for (field, constraint, set) in FIELDS {
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
    for (field, constraint, set) in FIELDS {
        for &value in boundary_values(constraint) {
            // A zero minimum gain keeps every tested maximum gain consistent with it.
            let mut config = CallConfig::default();
            config.log_mmse.min_gain = 0.0;
            set(&mut config, value);
            assert!(ValidConfig::new(&config).is_ok(), "{field} = {value}");
        }
    }
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
    let reference = CallConfig {
        calibration_duration: Duration::from_secs(5),
        high_pass: HighPassConfig { cutoff_hz: 80.0 },
        spp_mmse: SppMmseConfig {
            noise_smoothing: 0.8,
            spp_smoothing: 0.9,
            speech_prior: 0.5,
            fixed_prior_snr: 31.622_776,
            stagnation_threshold: 0.99,
            max_speech_probability: 0.99,
            floor: 1e-12,
        },
        decision_directed: DecisionDirectedConfig { alpha: 0.98, floor: 1e-12 },
        log_mmse: LogMmseConfig { min_gain: 0.05, max_gain: 1.0, floor: 1e-12, noise_overestimation: 1.25 },
    };
    assert_eq!(CallConfig::default(), reference);
    // The fixed prior SNR is 15 dB.
    let decibels = 10.0 * f64::from(CallConfig::default().spp_mmse.fixed_prior_snr).log10();
    assert!((decibels - 15.0).abs() < 1e-6);
}

#[test]
fn error_messages_name_the_field_and_the_constraint() {
    let mut config = CallConfig::default();
    config.spp_mmse.speech_prior = 1.0;
    let message = ValidConfig::new(&config).expect_err("speech prior 1 is invalid").to_string();
    assert_eq!(message, "spp_mmse.speech_prior must be in (0, 1), got 1");
}
