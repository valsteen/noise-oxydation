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
    /// An integer field (a frame or bin count) is outside `[min, max]`.
    CountOutOfRange {
        /// The offending field.
        field: ConfigField,
        /// The rejected value.
        value: u16,
        /// The smallest allowed value.
        min: u16,
        /// The largest allowed value.
        max: u16,
    },
    /// The start threshold of a start/full threshold pair is not strictly below its full threshold.
    StartNotBelowFull {
        /// The start threshold field.
        start: ConfigField,
        /// The configured start value.
        start_value: f32,
        /// The full threshold field.
        full: ConfigField,
        /// The configured full value.
        full_value: f32,
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
            Self::CountOutOfRange { field, value, min, max } => {
                write!(formatter, "{field} must be in [{min}, {max}], got {value}")
            }
            Self::StartNotBelowFull { start, start_value, full, full_value } => {
                write!(formatter, "{start} ({start_value}) must be less than {full} ({full_value})")
            }
            Self::CalibrationTooLong { duration } => {
                write!(formatter, "calibration_duration {duration:?} has too many samples at 8 kHz")
            }
        }
    }
}

impl Error for ConfigError {}

/// A validated configuration field.
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
    /// `mcra.smoothing`.
    McraSmoothing,
    /// `mcra.speech_smoothing`.
    McraSpeechSmoothing,
    /// `mcra.noise_smoothing`.
    McraNoiseSmoothing,
    /// `mcra.ratio_threshold`.
    McraRatioThreshold,
    /// `mcra.window_frames`.
    McraWindowFrames,
    /// `mcra.floor`.
    McraFloor,
    /// `minimum.smoothing`.
    MinimumSmoothing,
    /// `minimum.window_frames`.
    MinimumWindowFrames,
    /// `minimum.floor`.
    MinimumFloor,
    /// `tonal_transient.local_radius`.
    TonalLocalRadius,
    /// `tonal_transient.tonal_start_db`.
    TonalStartDb,
    /// `tonal_transient.tonal_full_db`.
    TonalFullDb,
    /// `tonal_transient.flux_start_db`.
    TonalFluxStartDb,
    /// `tonal_transient.flux_full_db`.
    TonalFluxFullDb,
    /// `tonal_transient.movement_search_radius`.
    TonalMovementSearchRadius,
    /// `tonal_transient.movement_start_bins`.
    TonalMovementStartBins,
    /// `tonal_transient.movement_full_bins`.
    TonalMovementFullBins,
    /// `tonal_transient.min_frequency_bin`.
    TonalMinFrequencyBin,
    /// `tonal_transient.movement_min_relative_power`.
    TonalMovementMinRelativePower,
    /// `tonal_transient.harmonic_tolerance_bins`.
    TonalHarmonicToleranceBins,
    /// `tonal_transient.harmonic_relative_power`.
    TonalHarmonicRelativePower,
    /// `tonal_transient.strength`.
    TonalStrength,
    /// `tonal_transient.min_gain`.
    TonalMinGain,
    /// `tonal_transient.attack`.
    TonalAttack,
    /// `tonal_transient.release`.
    TonalRelease,
    /// `tonal_transient.spread_radius`.
    TonalSpreadRadius,
    /// `tonal_transient.floor`.
    TonalFloor,
}

impl ConfigField {
    /// The field's name qualified by its stage configuration, for example `spp_mmse.speech_prior` (in
    /// [`crate::NoiseEstimatorConfig::SppMmse`]) or `tonal_transient.min_gain` (in
    /// [`crate::InterferenceConfig::TonalTransient`]).
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
            Self::McraSmoothing => "mcra.smoothing",
            Self::McraSpeechSmoothing => "mcra.speech_smoothing",
            Self::McraNoiseSmoothing => "mcra.noise_smoothing",
            Self::McraRatioThreshold => "mcra.ratio_threshold",
            Self::McraWindowFrames => "mcra.window_frames",
            Self::McraFloor => "mcra.floor",
            Self::MinimumSmoothing => "minimum.smoothing",
            Self::MinimumWindowFrames => "minimum.window_frames",
            Self::MinimumFloor => "minimum.floor",
            Self::TonalLocalRadius => "tonal_transient.local_radius",
            Self::TonalStartDb => "tonal_transient.tonal_start_db",
            Self::TonalFullDb => "tonal_transient.tonal_full_db",
            Self::TonalFluxStartDb => "tonal_transient.flux_start_db",
            Self::TonalFluxFullDb => "tonal_transient.flux_full_db",
            Self::TonalMovementSearchRadius => "tonal_transient.movement_search_radius",
            Self::TonalMovementStartBins => "tonal_transient.movement_start_bins",
            Self::TonalMovementFullBins => "tonal_transient.movement_full_bins",
            Self::TonalMinFrequencyBin => "tonal_transient.min_frequency_bin",
            Self::TonalMovementMinRelativePower => "tonal_transient.movement_min_relative_power",
            Self::TonalHarmonicToleranceBins => "tonal_transient.harmonic_tolerance_bins",
            Self::TonalHarmonicRelativePower => "tonal_transient.harmonic_relative_power",
            Self::TonalStrength => "tonal_transient.strength",
            Self::TonalMinGain => "tonal_transient.min_gain",
            Self::TonalAttack => "tonal_transient.attack",
            Self::TonalRelease => "tonal_transient.release",
            Self::TonalSpreadRadius => "tonal_transient.spread_radius",
            Self::TonalFloor => "tonal_transient.floor",
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
    /// `[0, ∞)`, finite.
    NonNegative,
    /// `(1, ∞)`, finite.
    GreaterThanOne,
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
            Self::NonNegative => value >= 0.0 && value.is_finite(),
            Self::GreaterThanOne => value > 1.0 && value.is_finite(),
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
            Self::NonNegative => "finite and at least 0",
            Self::GreaterThanOne => "finite and greater than 1",
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
