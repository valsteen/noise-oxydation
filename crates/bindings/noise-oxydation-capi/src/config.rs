//! Conversion between the C configuration [`NoxConfig`] and the Rust [`CallConfig`].

use std::time::Duration;

use noise_oxydation::{
    CallConfig, DecisionDirectedConfig, HighPassConfig, InterferenceConfig, LogMmseConfig, McraConfig, MinimumConfig,
    NoiseEstimatorConfig, SppMmseConfig, TonalTransientConfig,
};

use crate::abi::{
    NOX_INTERFERENCE_DISABLED, NOX_INTERFERENCE_TONAL_TRANSIENT, NOX_NOISE_ESTIMATOR_MCRA, NOX_NOISE_ESTIMATOR_MINIMUM,
    NOX_NOISE_ESTIMATOR_SPP_MMSE, NOX_SELECTOR_INTERFERENCE, NOX_SELECTOR_NOISE_ESTIMATOR, NoxConfig,
    NoxDecisionDirectedConfig, NoxHighPassConfig, NoxLogMmseConfig, NoxMcraConfig, NoxMinimumConfig, NoxSppMmseConfig,
    NoxTonalTransientConfig,
};

/// A selector of [`NoxConfig`] holds a value that names no variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UnknownSelector {
    /// `NOX_SELECTOR_*`.
    pub(crate) selector: u32,
    /// The unknown value.
    pub(crate) value: u32,
}

/// The C configuration of `CallConfig::default()`. The parameters of the estimators and the interference setting that
/// the default does not select come from their own `Default` implementations.
pub(crate) fn default_config() -> NoxConfig {
    from_call_config(&CallConfig::default())
}

/// The C form of `config`. Parameters of unselected variants are their Rust defaults.
pub(crate) fn from_call_config(config: &CallConfig) -> NoxConfig {
    let (noise_estimator, spp_mmse, mcra, minimum) = match config.noise_estimator {
        NoiseEstimatorConfig::SppMmse(spp_mmse) => {
            (NOX_NOISE_ESTIMATOR_SPP_MMSE, spp_mmse, McraConfig::default(), MinimumConfig::default())
        }
        NoiseEstimatorConfig::Mcra(mcra) => {
            (NOX_NOISE_ESTIMATOR_MCRA, SppMmseConfig::default(), mcra, MinimumConfig::default())
        }
        NoiseEstimatorConfig::Minimum(minimum) => {
            (NOX_NOISE_ESTIMATOR_MINIMUM, SppMmseConfig::default(), McraConfig::default(), minimum)
        }
    };
    let (interference, tonal) = match config.interference {
        InterferenceConfig::TonalTransient(tonal) => (NOX_INTERFERENCE_TONAL_TRANSIENT, tonal),
        InterferenceConfig::Disabled => (NOX_INTERFERENCE_DISABLED, TonalTransientConfig::default()),
    };
    NoxConfig {
        calibration_duration_ns: saturating_nanos(config.calibration_duration),
        high_pass: NoxHighPassConfig { cutoff_hz: config.high_pass.cutoff_hz },
        noise_estimator,
        spp_mmse: NoxSppMmseConfig {
            noise_smoothing: spp_mmse.noise_smoothing,
            spp_smoothing: spp_mmse.spp_smoothing,
            speech_prior: spp_mmse.speech_prior,
            fixed_prior_snr: spp_mmse.fixed_prior_snr,
            stagnation_threshold: spp_mmse.stagnation_threshold,
            max_speech_probability: spp_mmse.max_speech_probability,
            floor: spp_mmse.floor,
        },
        mcra: NoxMcraConfig {
            smoothing: mcra.smoothing,
            speech_smoothing: mcra.speech_smoothing,
            noise_smoothing: mcra.noise_smoothing,
            ratio_threshold: mcra.ratio_threshold,
            window_frames: mcra.window_frames,
            floor: mcra.floor,
        },
        minimum: NoxMinimumConfig {
            smoothing: minimum.smoothing,
            window_frames: minimum.window_frames,
            floor: minimum.floor,
        },
        decision_directed: NoxDecisionDirectedConfig {
            alpha: config.decision_directed.alpha,
            floor: config.decision_directed.floor,
        },
        log_mmse: NoxLogMmseConfig {
            min_gain: config.log_mmse.min_gain,
            max_gain: config.log_mmse.max_gain,
            floor: config.log_mmse.floor,
            noise_overestimation: config.log_mmse.noise_overestimation,
        },
        interference,
        tonal_transient: NoxTonalTransientConfig {
            local_radius: tonal.local_radius,
            tonal_start_db: tonal.tonal_start_db,
            tonal_full_db: tonal.tonal_full_db,
            flux_start_db: tonal.flux_start_db,
            flux_full_db: tonal.flux_full_db,
            movement_search_radius: tonal.movement_search_radius,
            movement_start_bins: tonal.movement_start_bins,
            movement_full_bins: tonal.movement_full_bins,
            min_frequency_bin: tonal.min_frequency_bin,
            movement_min_relative_power: tonal.movement_min_relative_power,
            harmonic_tolerance_bins: tonal.harmonic_tolerance_bins,
            harmonic_relative_power: tonal.harmonic_relative_power,
            strength: tonal.strength,
            min_gain: tonal.min_gain,
            attack: tonal.attack,
            release: tonal.release,
            spread_radius: tonal.spread_radius,
            floor: tonal.floor,
        },
    }
}

/// The Rust configuration that `config` selects. Values are copied unchanged; `CallEnhancer::new` validates them.
///
/// # Errors
///
/// Returns [`UnknownSelector`] when `noise_estimator` or `interference` names no variant.
pub(crate) fn to_call_config(config: &NoxConfig) -> Result<CallConfig, UnknownSelector> {
    let noise_estimator = match config.noise_estimator {
        NOX_NOISE_ESTIMATOR_SPP_MMSE => {
            let spp_mmse = &config.spp_mmse;
            NoiseEstimatorConfig::SppMmse(SppMmseConfig {
                noise_smoothing: spp_mmse.noise_smoothing,
                spp_smoothing: spp_mmse.spp_smoothing,
                speech_prior: spp_mmse.speech_prior,
                fixed_prior_snr: spp_mmse.fixed_prior_snr,
                stagnation_threshold: spp_mmse.stagnation_threshold,
                max_speech_probability: spp_mmse.max_speech_probability,
                floor: spp_mmse.floor,
            })
        }
        NOX_NOISE_ESTIMATOR_MCRA => {
            let mcra = &config.mcra;
            NoiseEstimatorConfig::Mcra(McraConfig {
                smoothing: mcra.smoothing,
                speech_smoothing: mcra.speech_smoothing,
                noise_smoothing: mcra.noise_smoothing,
                ratio_threshold: mcra.ratio_threshold,
                window_frames: mcra.window_frames,
                floor: mcra.floor,
            })
        }
        NOX_NOISE_ESTIMATOR_MINIMUM => {
            let minimum = &config.minimum;
            NoiseEstimatorConfig::Minimum(MinimumConfig {
                smoothing: minimum.smoothing,
                window_frames: minimum.window_frames,
                floor: minimum.floor,
            })
        }
        value => return Err(UnknownSelector { selector: NOX_SELECTOR_NOISE_ESTIMATOR, value }),
    };
    let interference = match config.interference {
        NOX_INTERFERENCE_TONAL_TRANSIENT => {
            let tonal = &config.tonal_transient;
            InterferenceConfig::TonalTransient(TonalTransientConfig {
                local_radius: tonal.local_radius,
                tonal_start_db: tonal.tonal_start_db,
                tonal_full_db: tonal.tonal_full_db,
                flux_start_db: tonal.flux_start_db,
                flux_full_db: tonal.flux_full_db,
                movement_search_radius: tonal.movement_search_radius,
                movement_start_bins: tonal.movement_start_bins,
                movement_full_bins: tonal.movement_full_bins,
                min_frequency_bin: tonal.min_frequency_bin,
                movement_min_relative_power: tonal.movement_min_relative_power,
                harmonic_tolerance_bins: tonal.harmonic_tolerance_bins,
                harmonic_relative_power: tonal.harmonic_relative_power,
                strength: tonal.strength,
                min_gain: tonal.min_gain,
                attack: tonal.attack,
                release: tonal.release,
                spread_radius: tonal.spread_radius,
                floor: tonal.floor,
            })
        }
        NOX_INTERFERENCE_DISABLED => InterferenceConfig::Disabled,
        value => return Err(UnknownSelector { selector: NOX_SELECTOR_INTERFERENCE, value }),
    };
    Ok(CallConfig {
        calibration_duration: Duration::from_nanos(config.calibration_duration_ns),
        high_pass: HighPassConfig { cutoff_hz: config.high_pass.cutoff_hz },
        noise_estimator,
        decision_directed: DecisionDirectedConfig {
            alpha: config.decision_directed.alpha,
            floor: config.decision_directed.floor,
        },
        log_mmse: LogMmseConfig {
            min_gain: config.log_mmse.min_gain,
            max_gain: config.log_mmse.max_gain,
            floor: config.log_mmse.floor,
            noise_overestimation: config.log_mmse.noise_overestimation,
        },
        interference,
    })
}

/// `duration` in nanoseconds, saturated at `u64::MAX` (about 584 years). Only reachable from Rust configurations
/// longer than any `u64` nanosecond count; the C configuration cannot express them.
fn saturating_nanos(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "../tests/unit/config.rs"]
mod tests;
