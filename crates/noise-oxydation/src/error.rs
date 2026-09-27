//! Typed errors for configuration and call lifecycle.

use std::{error::Error, fmt, time::Duration};

/// A configuration value that [`crate::CallEnhancer::new`] rejected.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum ConfigError {
    /// A field is outside its allowed range (NaN is always outside).
    OutOfRange {
        /// The offending field.
        field: ConfigField,
        /// The rejected value.
        value: f32,
        /// The range the field must satisfy.
        constraint: Constraint,
    },
    /// `log_mmse.min_gain` is greater than `log_mmse.max_gain`.
    MinGainAboveMaxGain {
        /// The configured minimum gain.
        min_gain: f32,
        /// The configured maximum gain.
        max_gain: f32,
    },
    /// The calibration duration's sample count at 8 kHz does not fit in `u64`.
    CalibrationTooLong {
        /// The rejected duration.
        duration: Duration,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfRange { field, value, constraint } => {
                write!(formatter, "{field} must be {constraint}, got {value}")
            }
            Self::MinGainAboveMaxGain { min_gain, max_gain } => {
                write!(formatter, "log_mmse.min_gain ({min_gain}) must not exceed log_mmse.max_gain ({max_gain})")
            }
            Self::CalibrationTooLong { duration } => {
                write!(formatter, "calibration_duration {duration:?} has too many samples at 8 kHz")
            }
        }
    }
}

impl Error for ConfigError {}

/// A validated numeric configuration field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ConfigField {
    /// `high_pass.cutoff_hz`.
    HighPassCutoffHz,
    /// `spp_mmse.noise_smoothing`.
    SppNoiseSmoothing,
    /// `spp_mmse.spp_smoothing`.
    SppSmoothing,
    /// `spp_mmse.speech_prior`.
    SppSpeechPrior,
    /// `spp_mmse.fixed_prior_snr`.
    SppFixedPriorSnr,
    /// `spp_mmse.stagnation_threshold`.
    SppStagnationThreshold,
    /// `spp_mmse.max_speech_probability`.
    SppMaxSpeechProbability,
    /// `spp_mmse.floor`.
    SppFloor,
    /// `decision_directed.alpha`.
    DecisionDirectedAlpha,
    /// `decision_directed.floor`.
    DecisionDirectedFloor,
    /// `log_mmse.min_gain`.
    LogMmseMinGain,
    /// `log_mmse.max_gain`.
    LogMmseMaxGain,
    /// `log_mmse.floor`.
    LogMmseFloor,
    /// `log_mmse.noise_overestimation`.
    LogMmseNoiseOverestimation,
}

impl ConfigField {
    /// The field's path in [`crate::CallConfig`], for example `spp_mmse.speech_prior`.
    #[must_use]
    pub const fn path(self) -> &'static str {
        match self {
            Self::HighPassCutoffHz => "high_pass.cutoff_hz",
            Self::SppNoiseSmoothing => "spp_mmse.noise_smoothing",
            Self::SppSmoothing => "spp_mmse.spp_smoothing",
            Self::SppSpeechPrior => "spp_mmse.speech_prior",
            Self::SppFixedPriorSnr => "spp_mmse.fixed_prior_snr",
            Self::SppStagnationThreshold => "spp_mmse.stagnation_threshold",
            Self::SppMaxSpeechProbability => "spp_mmse.max_speech_probability",
            Self::SppFloor => "spp_mmse.floor",
            Self::DecisionDirectedAlpha => "decision_directed.alpha",
            Self::DecisionDirectedFloor => "decision_directed.floor",
            Self::LogMmseMinGain => "log_mmse.min_gain",
            Self::LogMmseMaxGain => "log_mmse.max_gain",
            Self::LogMmseFloor => "log_mmse.floor",
            Self::LogMmseNoiseOverestimation => "log_mmse.noise_overestimation",
        }
    }
}

impl fmt::Display for ConfigField {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.path())
    }
}

/// The range a numeric configuration field must lie in. Every range excludes NaN and infinities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Constraint {
    /// `(0, 1)`.
    OpenUnit,
    /// `[0, 1)`.
    UnitExcludingOne,
    /// `(0, 1]`.
    UnitExcludingZero,
    /// `[0, 1]`.
    ClosedUnit,
    /// `(0, ∞)`, finite.
    Positive,
    /// `(0, 4000)` Hz: positive and below the Nyquist frequency.
    BelowNyquist,
}

impl Constraint {
    pub(crate) fn admits(self, value: f32) -> bool {
        match self {
            Self::OpenUnit => value > 0.0 && value < 1.0,
            Self::UnitExcludingOne => (0.0..1.0).contains(&value),
            Self::UnitExcludingZero => value > 0.0 && value <= 1.0,
            Self::ClosedUnit => (0.0..=1.0).contains(&value),
            Self::Positive => value > 0.0 && value.is_finite(),
            Self::BelowNyquist => value > 0.0 && value < 4000.0,
        }
    }
}

impl fmt::Display for Constraint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::OpenUnit => "in (0, 1)",
            Self::UnitExcludingOne => "in [0, 1)",
            Self::UnitExcludingZero => "in (0, 1]",
            Self::ClosedUnit => "in [0, 1]",
            Self::Positive => "finite and greater than 0",
            Self::BelowNyquist => "in (0, 4000) Hz",
        })
    }
}

/// A call lifecycle violation reported by [`crate::CallEnhancer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StreamError {
    /// The call has been drained; call [`crate::CallEnhancer::reset`] before streaming another call.
    Drained,
}

impl fmt::Display for StreamError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Drained => formatter.write_str("the call has been drained; reset the enhancer to start a new call"),
        }
    }
}

impl Error for StreamError {}
