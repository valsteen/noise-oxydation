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
    /// Noise power estimator and its parameters. Default SPP-MMSE.
    pub noise_estimator: NoiseEstimatorConfig,
    /// Decision-directed a priori SNR estimator.
    pub decision_directed: DecisionDirectedConfig,
    /// Log-MMSE suppressor.
    pub log_mmse: LogMmseConfig,
    /// Foreground interference suppression applied after Log-MMSE. Default tonal transient suppression.
    pub interference: InterferenceConfig,
}

impl Default for CallConfig {
    fn default() -> Self {
        Self {
            calibration_duration: Duration::from_secs(5),
            high_pass: HighPassConfig::default(),
            noise_estimator: NoiseEstimatorConfig::default(),
            decision_directed: DecisionDirectedConfig::default(),
            log_mmse: LogMmseConfig::default(),
            interference: InterferenceConfig::default(),
        }
    }
}

/// Choice of noise power estimator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NoiseEstimatorConfig {
    /// Speech-presence-probability MMSE estimation (the default, as in the reference end-to-end composition).
    SppMmse(SppMmseConfig),
    /// Minimum-controlled recursive averaging.
    Mcra(McraConfig),
    /// Exact minimum of the smoothed power over a sliding window of frames.
    Minimum(MinimumConfig),
}

impl Default for NoiseEstimatorConfig {
    fn default() -> Self {
        Self::SppMmse(SppMmseConfig::default())
    }
}

impl NoiseEstimatorConfig {
    /// Short name of the chosen estimator for construction records.
    #[cfg(feature = "log")]
    pub(crate) const fn name(&self) -> &'static str {
        match self {
            Self::SppMmse(_) => "SPP-MMSE",
            Self::Mcra(_) => "MCRA",
            Self::Minimum(_) => "minimum",
        }
    }
}

/// Choice of foreground interference suppression.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InterferenceConfig {
    /// Tonal transient suppression after Log-MMSE (the default, as in every reference composition).
    TonalTransient(TonalTransientConfig),
    /// No interference suppression: the Log-MMSE output is synthesized directly.
    Disabled,
}

impl Default for InterferenceConfig {
    fn default() -> Self {
        Self::TonalTransient(TonalTransientConfig::default())
    }
}

impl InterferenceConfig {
    /// Short name of the chosen interference setting for construction records.
    #[cfg(feature = "log")]
    pub(crate) const fn name(&self) -> &'static str {
        match self {
            Self::TonalTransient(_) => "tonal transient suppression",
            Self::Disabled => "disabled",
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

/// MCRA noise estimator parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct McraConfig {
    /// Power smoothing `αs` in `[0, 1)`. Default 0.8.
    pub smoothing: f32,
    /// Speech presence probability smoothing `αp` in `[0, 1)`. Default 0.2.
    pub speech_smoothing: f32,
    /// Base noise smoothing `αd` in `[0, 1)`; the effective smoothing is `αd + (1 − αd)·p`. Default 0.95.
    pub noise_smoothing: f32,
    /// Ratio `δ` of smoothed power to its minimum above which a bin counts as speech, finite and greater than 1.
    /// Default 5.
    pub ratio_threshold: f32,
    /// Frames `W` per minimum-search window, in `[1, 1000]`. Default 50.
    pub window_frames: u16,
    /// Lower bound of power values, finite and positive. Default 1e-12.
    pub floor: f32,
}

impl Default for McraConfig {
    fn default() -> Self {
        Self {
            smoothing: 0.8,
            speech_smoothing: 0.2,
            noise_smoothing: 0.95,
            ratio_threshold: 5.0,
            window_frames: 50,
            floor: 1e-12,
        }
    }
}

/// Minimum noise estimator parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MinimumConfig {
    /// Power smoothing `αs` in `[0, 1)`. Default 0.8.
    pub smoothing: f32,
    /// Frames `W` in the sliding minimum window, in `[1, 1000]`. The window's history is allocated at construction.
    /// Default 50.
    pub window_frames: u16,
    /// Lower bound of power values and of the estimate, finite and positive. Default 1e-12.
    pub floor: f32,
}

impl Default for MinimumConfig {
    fn default() -> Self {
        Self { smoothing: 0.8, window_frames: 50, floor: 1e-12 }
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

/// Tonal transient detector parameters. Bin indices count 31.25 Hz bins of the 129-bin spectrum.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TonalTransientConfig {
    /// Neighbors on each side a peak must not be exceeded by, in `[1, 32]`. Default 2.
    pub local_radius: u16,
    /// Tonal prominence where tonal evidence begins, in dB, finite and at least 0, below `tonal_full_db`. Default 5.
    pub tonal_start_db: f32,
    /// Tonal prominence of full tonal evidence, in dB, finite. Default 14.
    pub tonal_full_db: f32,
    /// Positive spectral flux where transient evidence begins, in dB, finite and at least 0, below `flux_full_db`.
    /// Default 3.
    pub flux_start_db: f32,
    /// Positive spectral flux of full transient evidence, in dB, finite. Default 18.
    pub flux_full_db: f32,
    /// Bins on each side searched for the previous frame's peak, in `[1, 64]`. Default 6.
    pub movement_search_radius: u16,
    /// Peak movement where movement evidence begins, in bins, in `[1, 64]`, below `movement_full_bins`. Default 1.
    pub movement_start_bins: u16,
    /// Peak movement of full movement evidence, in bins, at most 64. Default 4.
    pub movement_full_bins: u16,
    /// Lowest analyzed bin, in `[1, 128]`. Default 64 (2 kHz): voiced speech below it is never attenuated.
    pub min_frequency_bin: u16,
    /// Minimum power of the previous peak relative to the current one for movement evidence, in `(0, 1]`.
    /// Default 0.1.
    pub movement_min_relative_power: f32,
    /// Bins searched on each side of a harmonic relation, in `[0, 16]`. Default 1.
    pub harmonic_tolerance_bins: u16,
    /// Related power `ρ` (relative to the peak) that gives full harmonic protection, in `(0, 1]`. Default 0.15.
    pub harmonic_relative_power: f32,
    /// Maximum attenuation depth: `target = 1 − strength·S`, in `[0, 1]`. Default 0.5.
    pub strength: f32,
    /// Lowest gain, in `[0, 1]`. Default 0.5 (about −6.02 dB).
    pub min_gain: f32,
    /// Smoothing while the gain falls, in `[0, 1)`. Default 0.3.
    pub attack: f32,
    /// Smoothing while the gain rises, in `[0, 1)`. Default 0.85.
    pub release: f32,
    /// Bins on each side of a peak that receive partial attenuation, in `[0, 64]`. Default 2.
    pub spread_radius: u16,
    /// Lower bound of power values, finite and positive. Default 1e-12.
    pub floor: f32,
}

impl Default for TonalTransientConfig {
    fn default() -> Self {
        Self {
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
        }
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
        match config.noise_estimator {
            NoiseEstimatorConfig::SppMmse(spp_mmse) => validate_spp_mmse(spp_mmse)?,
            NoiseEstimatorConfig::Mcra(mcra) => validate_mcra(mcra)?,
            NoiseEstimatorConfig::Minimum(minimum) => validate_minimum(minimum)?,
        }
        validate_decision_directed(config.decision_directed)?;
        validate_log_mmse(config.log_mmse)?;
        if let InterferenceConfig::TonalTransient(tonal) = config.interference {
            validate_tonal_transient(tonal)?;
        }
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

/// Largest `window_frames` of the MCRA and minimum estimators.
const MAX_WINDOW_FRAMES: u16 = 1000;
/// Largest bin index of the 129-bin spectrum.
const MAX_BIN: u16 = 128;

fn validate_mcra(config: McraConfig) -> Result<(), ConfigError> {
    check(ConfigField::McraSmoothing, config.smoothing, Constraint::UnitExcludingOne)?;
    check(ConfigField::McraSpeechSmoothing, config.speech_smoothing, Constraint::UnitExcludingOne)?;
    check(ConfigField::McraNoiseSmoothing, config.noise_smoothing, Constraint::UnitExcludingOne)?;
    check(ConfigField::McraRatioThreshold, config.ratio_threshold, Constraint::GreaterThanOne)?;
    check_count(ConfigField::McraWindowFrames, config.window_frames, 1, MAX_WINDOW_FRAMES)?;
    check(ConfigField::McraFloor, config.floor, Constraint::Positive)
}

fn validate_minimum(config: MinimumConfig) -> Result<(), ConfigError> {
    check(ConfigField::MinimumSmoothing, config.smoothing, Constraint::UnitExcludingOne)?;
    check_count(ConfigField::MinimumWindowFrames, config.window_frames, 1, MAX_WINDOW_FRAMES)?;
    check(ConfigField::MinimumFloor, config.floor, Constraint::Positive)
}

fn validate_tonal_transient(config: TonalTransientConfig) -> Result<(), ConfigError> {
    check_count(ConfigField::TonalLocalRadius, config.local_radius, 1, 32)?;
    check(ConfigField::TonalStartDb, config.tonal_start_db, Constraint::NonNegative)?;
    check(ConfigField::TonalFullDb, config.tonal_full_db, Constraint::NonNegative)?;
    check_order((ConfigField::TonalStartDb, config.tonal_start_db), (ConfigField::TonalFullDb, config.tonal_full_db))?;
    check(ConfigField::TonalFluxStartDb, config.flux_start_db, Constraint::NonNegative)?;
    check(ConfigField::TonalFluxFullDb, config.flux_full_db, Constraint::NonNegative)?;
    check_order(
        (ConfigField::TonalFluxStartDb, config.flux_start_db),
        (ConfigField::TonalFluxFullDb, config.flux_full_db),
    )?;
    check_count(ConfigField::TonalMovementSearchRadius, config.movement_search_radius, 1, 64)?;
    check_count(ConfigField::TonalMovementStartBins, config.movement_start_bins, 1, 64)?;
    check_count(ConfigField::TonalMovementFullBins, config.movement_full_bins, 1, 64)?;
    check_order(
        (ConfigField::TonalMovementStartBins, f32::from(config.movement_start_bins)),
        (ConfigField::TonalMovementFullBins, f32::from(config.movement_full_bins)),
    )?;
    check_count(ConfigField::TonalMinFrequencyBin, config.min_frequency_bin, 1, MAX_BIN)?;
    check(
        ConfigField::TonalMovementMinRelativePower,
        config.movement_min_relative_power,
        Constraint::UnitExcludingZero,
    )?;
    check_count(ConfigField::TonalHarmonicToleranceBins, config.harmonic_tolerance_bins, 0, 16)?;
    check(ConfigField::TonalHarmonicRelativePower, config.harmonic_relative_power, Constraint::UnitExcludingZero)?;
    check(ConfigField::TonalStrength, config.strength, Constraint::ClosedUnit)?;
    check(ConfigField::TonalMinGain, config.min_gain, Constraint::ClosedUnit)?;
    check(ConfigField::TonalAttack, config.attack, Constraint::UnitExcludingOne)?;
    check(ConfigField::TonalRelease, config.release, Constraint::UnitExcludingOne)?;
    check_count(ConfigField::TonalSpreadRadius, config.spread_radius, 0, 64)?;
    check(ConfigField::TonalFloor, config.floor, Constraint::Positive)
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

fn check_count(field: ConfigField, value: u16, min: u16, max: u16) -> Result<(), ConfigError> {
    if (min..=max).contains(&value) { Ok(()) } else { Err(ConfigError::CountOutOfRange { field, value, min, max }) }
}

/// Requires the start threshold of a start/full pair to lie strictly below the full threshold.
fn check_order(
    (start, start_value): (ConfigField, f32),
    (full, full_value): (ConfigField, f32),
) -> Result<(), ConfigError> {
    if start_value < full_value {
        Ok(())
    } else {
        Err(ConfigError::StartNotBelowFull { start, start_value, full, full_value })
    }
}

#[cfg(test)]
#[path = "../tests/unit/config.rs"]
mod tests;
