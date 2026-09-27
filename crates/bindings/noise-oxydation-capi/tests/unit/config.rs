//! The C configuration carries every `CallConfig` field both ways.

use std::time::Duration;

use noise_oxydation::{
    CallConfig, DecisionDirectedConfig, HighPassConfig, InterferenceConfig, LogMmseConfig, McraConfig, MinimumConfig,
    NoiseEstimatorConfig, SppMmseConfig, TonalTransientConfig,
};

use super::*;

#[test]
fn the_default_round_trips_to_call_config_default() {
    assert_eq!(to_call_config(&default_config()), Ok(CallConfig::default()));
}

#[test]
fn the_default_carries_the_rust_defaults_of_unselected_variants() {
    let config = default_config();
    let mut mcra = config;
    mcra.noise_estimator = NOX_NOISE_ESTIMATOR_MCRA;
    let mut minimum = config;
    minimum.noise_estimator = NOX_NOISE_ESTIMATOR_MINIMUM;
    let mut disabled = config;
    disabled.interference = NOX_INTERFERENCE_DISABLED;

    let rust =
        |estimator, interference| CallConfig { noise_estimator: estimator, interference, ..CallConfig::default() };
    let tonal = InterferenceConfig::TonalTransient(TonalTransientConfig::default());
    assert_eq!(to_call_config(&mcra), Ok(rust(NoiseEstimatorConfig::Mcra(McraConfig::default()), tonal)));
    assert_eq!(to_call_config(&minimum), Ok(rust(NoiseEstimatorConfig::Minimum(MinimumConfig::default()), tonal)));
    assert_eq!(
        to_call_config(&disabled),
        Ok(rust(NoiseEstimatorConfig::SppMmse(SppMmseConfig::default()), InterferenceConfig::Disabled))
    );
}

/// A configuration whose every value differs from its default and from every other value of the same type, so that a
/// field copied to the wrong place cannot round-trip.
fn distinct_values(noise_estimator: NoiseEstimatorConfig, interference: InterferenceConfig) -> CallConfig {
    CallConfig {
        calibration_duration: Duration::from_nanos(1_234_567_891),
        high_pass: HighPassConfig { cutoff_hz: 101.0 },
        noise_estimator,
        decision_directed: DecisionDirectedConfig { alpha: 0.51, floor: 1e-11 },
        log_mmse: LogMmseConfig { min_gain: 0.07, max_gain: 0.93, floor: 2e-11, noise_overestimation: 1.3 },
        interference,
    }
}

#[test]
fn every_field_of_every_variant_round_trips() {
    let spp_mmse = NoiseEstimatorConfig::SppMmse(SppMmseConfig {
        noise_smoothing: 0.11,
        spp_smoothing: 0.12,
        speech_prior: 0.13,
        fixed_prior_snr: 14.0,
        stagnation_threshold: 0.15,
        max_speech_probability: 0.16,
        floor: 1.7e-12,
    });
    let mcra = NoiseEstimatorConfig::Mcra(McraConfig {
        smoothing: 0.21,
        speech_smoothing: 0.22,
        noise_smoothing: 0.23,
        ratio_threshold: 2.4,
        window_frames: 25,
        floor: 2.6e-12,
    });
    let minimum = NoiseEstimatorConfig::Minimum(MinimumConfig { smoothing: 0.31, window_frames: 32, floor: 3.3e-12 });
    let tonal = InterferenceConfig::TonalTransient(TonalTransientConfig {
        local_radius: 3,
        tonal_start_db: 4.0,
        tonal_full_db: 15.0,
        flux_start_db: 2.0,
        flux_full_db: 19.0,
        movement_search_radius: 7,
        movement_start_bins: 2,
        movement_full_bins: 5,
        min_frequency_bin: 60,
        movement_min_relative_power: 0.2,
        harmonic_tolerance_bins: 4,
        harmonic_relative_power: 0.25,
        strength: 0.6,
        min_gain: 0.4,
        attack: 0.35,
        release: 0.8,
        spread_radius: 6,
        floor: 4.1e-12,
    });
    for estimator in [spp_mmse, mcra, minimum] {
        for interference in [tonal, InterferenceConfig::Disabled] {
            let config = distinct_values(estimator, interference);
            assert_eq!(to_call_config(&from_call_config(&config)), Ok(config));
        }
    }
}

#[test]
fn unknown_selectors_are_reported_with_their_value() {
    let mut config = default_config();
    config.noise_estimator = 0;
    assert_eq!(to_call_config(&config), Err(UnknownSelector { selector: NOX_SELECTOR_NOISE_ESTIMATOR, value: 0 }));
    config.noise_estimator = NOX_NOISE_ESTIMATOR_MINIMUM;
    config.interference = 7;
    assert_eq!(to_call_config(&config), Err(UnknownSelector { selector: NOX_SELECTOR_INTERFERENCE, value: 7 }));
}
