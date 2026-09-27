//! Runtime configuration of one call. Defaults equal the reference-documented values; construction validates every
//! field and rejects invalid values with a typed [`ConfigError`] instead of substituting defaults.

use std::time::Duration;

use crate::{
    error::{ConfigError, ConfigField, Constraint},
    geometry::SAMPLE_RATE_HZ,
};

/// Configuration of one call's processing path.
#[derive(Debug, Clone, PartialEq)]
pub struct CallConfig {
    /// Length of the quiet intro used to learn the noise spectrum. Frames that end within it pass through
    /// un-enhanced. Default 5 s; zero disables calibration.
    pub calibration_duration: Duration,
    /// DC-blocking high-pass filter.
    pub high_pass: HighPassConfig,
    /// SPP-MMSE noise estimator.
    pub spp_mmse: SppMmseConfig,
    /// Decision-directed a priori SNR estimator.
    pub decision_directed: DecisionDirectedConfig,
    /// Log-MMSE suppressor.
    pub log_mmse: LogMmseConfig,
}

impl Default for CallConfig {
    fn default() -> Self {
        Self {
            calibration_duration: Duration::from_secs(5),
            high_pass: HighPassConfig::default(),
            spp_mmse: SppMmseConfig::default(),
            decision_directed: DecisionDirectedConfig::default(),
            log_mmse: LogMmseConfig::default(),
        }
    }
}

/// High-pass filter parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HighPassConfig {
    /// Pole parameter `fc` of `r = exp(−2π·fc/8000)`, in hertz, in `(0, 4000)`. Default 80 Hz. This is not the −3 dB
    /// frequency, which is slightly lower (about 75.3 Hz for the default).
    pub cutoff_hz: f32,
}

impl Default for HighPassConfig {
    fn default() -> Self {
        Self { cutoff_hz: 80.0 }
    }
}

/// SPP-MMSE noise estimator parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SppMmseConfig {
    /// Noise smoothing `αN` in `(0, 1)`. Default 0.8.
    pub noise_smoothing: f32,
    /// Speech presence probability smoothing `αP` in `(0, 1)`. Default 0.9.
    pub spp_smoothing: f32,
    /// A priori speech presence probability `q` in `(0, 1)`. Default 0.5.
    pub speech_prior: f32,
    /// Fixed a priori SNR under speech presence `ξH1` (linear), finite and positive. Default 31.622 776 6 (15 dB).
    pub fixed_prior_snr: f32,
    /// Smoothed probability above which the probability is capped, in `(0, 1]`. Default 0.99.
    pub stagnation_threshold: f32,
    /// Cap applied to the probability when the smoothed probability stagnates, in `(0, 1]`. Default 0.99.
    pub max_speech_probability: f32,
    /// Lower bound of power values, finite and positive. Default 1e-12.
    pub floor: f32,
}

impl Default for SppMmseConfig {
    fn default() -> Self {
        Self {
            noise_smoothing: 0.8,
            spp_smoothing: 0.9,
            speech_prior: 0.5,
            fixed_prior_snr: 31.622_776,
            stagnation_threshold: 0.99,
            max_speech_probability: 0.99,
            floor: 1e-12,
        }
    }
}

/// Decision-directed SNR estimator parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecisionDirectedConfig {
    /// Weight `α` of the previous clean-speech SNR, in `[0, 1)`. Default 0.98.
    pub alpha: f32,
    /// Lower bound of the noise power, finite and positive. Default 1e-12.
    pub floor: f32,
}

impl Default for DecisionDirectedConfig {
    fn default() -> Self {
        Self { alpha: 0.98, floor: 1e-12 }
    }
}

/// Log-MMSE suppressor parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogMmseConfig {
    /// Minimum gain in `[0, 1]`, at most `max_gain`. Default 0.05.
    pub min_gain: f32,
    /// Maximum gain in `(0, 1]`. Default 1.
    pub max_gain: f32,
    /// Lower bound of `v` and of the effective noise power, finite and positive. Default 1e-12.
    pub floor: f32,
    /// Noise overestimation factor `β` applied to the noise estimate, finite and positive. Default 1.25.
    pub noise_overestimation: f32,
}

impl Default for LogMmseConfig {
    fn default() -> Self {
        Self { min_gain: 0.05, max_gain: 1.0, floor: 1e-12, noise_overestimation: 1.25 }
    }
}

/// A configuration whose every field has been validated.
#[derive(Debug, Clone)]
pub(crate) struct ValidConfig {
    pub(crate) config: CallConfig,
    /// `floor(calibration_duration · 8000 Hz)`.
    pub(crate) calibration_samples: u64,
}

impl ValidConfig {
    pub(crate) fn new(config: &CallConfig) -> Result<Self, ConfigError> {
        let calibration_samples = calibration_samples(config.calibration_duration)?;
        check(ConfigField::HighPassCutoffHz, config.high_pass.cutoff_hz, Constraint::BelowNyquist)?;
        validate_spp_mmse(config.spp_mmse)?;
        validate_decision_directed(config.decision_directed)?;
        validate_log_mmse(config.log_mmse)?;
        Ok(Self { config: config.clone(), calibration_samples })
    }
}

fn calibration_samples(duration: Duration) -> Result<u64, ConfigError> {
    let samples = duration.as_nanos() * u128::from(SAMPLE_RATE_HZ) / 1_000_000_000;
    u64::try_from(samples).map_err(|_| ConfigError::CalibrationTooLong { duration })
}

fn validate_spp_mmse(config: SppMmseConfig) -> Result<(), ConfigError> {
    check(ConfigField::SppNoiseSmoothing, config.noise_smoothing, Constraint::OpenUnit)?;
    check(ConfigField::SppSmoothing, config.spp_smoothing, Constraint::OpenUnit)?;
    check(ConfigField::SppSpeechPrior, config.speech_prior, Constraint::OpenUnit)?;
    check(ConfigField::SppFixedPriorSnr, config.fixed_prior_snr, Constraint::Positive)?;
    check(ConfigField::SppStagnationThreshold, config.stagnation_threshold, Constraint::UnitExcludingZero)?;
    check(ConfigField::SppMaxSpeechProbability, config.max_speech_probability, Constraint::UnitExcludingZero)?;
    check(ConfigField::SppFloor, config.floor, Constraint::Positive)
}

fn validate_decision_directed(config: DecisionDirectedConfig) -> Result<(), ConfigError> {
    check(ConfigField::DecisionDirectedAlpha, config.alpha, Constraint::UnitExcludingOne)?;
    check(ConfigField::DecisionDirectedFloor, config.floor, Constraint::Positive)
}

fn validate_log_mmse(config: LogMmseConfig) -> Result<(), ConfigError> {
    check(ConfigField::LogMmseMinGain, config.min_gain, Constraint::ClosedUnit)?;
    check(ConfigField::LogMmseMaxGain, config.max_gain, Constraint::UnitExcludingZero)?;
    check(ConfigField::LogMmseFloor, config.floor, Constraint::Positive)?;
    check(ConfigField::LogMmseNoiseOverestimation, config.noise_overestimation, Constraint::Positive)?;
    if config.min_gain > config.max_gain {
        return Err(ConfigError::MinGainAboveMaxGain { min_gain: config.min_gain, max_gain: config.max_gain });
    }
    Ok(())
}

fn check(field: ConfigField, value: f32, constraint: Constraint) -> Result<(), ConfigError> {
    if constraint.admits(value) { Ok(()) } else { Err(ConfigError::OutOfRange { field, value, constraint }) }
}

#[cfg(test)]
#[path = "../tests/unit/config.rs"]
mod tests;
