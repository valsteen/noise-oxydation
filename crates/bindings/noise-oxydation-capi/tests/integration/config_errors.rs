//! Configuration errors reach C callers with their kind and complete payload, and their Rust message.

use std::ptr;

use noise_oxydation::{
    CallConfig, CallEnhancer, ConfigError, ConfigField, Constraint, LogMmseConfig, McraConfig, NoiseEstimatorConfig,
    TonalTransientConfig,
};
use noise_oxydation_capi::{
    NOX_CONFIG_ERROR_COUNT_OUT_OF_RANGE, NOX_CONFIG_ERROR_MIN_GAIN_ABOVE_MAX_GAIN, NOX_CONFIG_ERROR_NONE,
    NOX_CONFIG_ERROR_OUT_OF_RANGE, NOX_CONFIG_ERROR_START_NOT_BELOW_FULL, NOX_CONFIG_ERROR_UNKNOWN_SELECTOR,
    NOX_INTERFERENCE_DISABLED, NOX_NOISE_ESTIMATOR_MCRA, NOX_SELECTOR_INTERFERENCE, NOX_SELECTOR_NOISE_ESTIMATOR,
    NOX_STATUS_INVALID_CONFIG, NOX_STATUS_OK, NoxConfig, NoxStr, nox_call_new, nox_config_field_name,
    nox_constraint_description,
};

use crate::support::{create, default_config, message};

fn text(value: NoxStr) -> &'static str {
    // SAFETY: the ABI returns `len` bytes of static UTF-8 at `ptr`.
    std::str::from_utf8(unsafe { std::slice::from_raw_parts(value.ptr, value.len) }).expect("UTF-8")
}

/// Creates a handle from `config`, requires the rejection, and returns its detail after checking that its message is
/// the `Display` text of `expected`.
fn rejected(config: &NoxConfig, expected: &ConfigError) -> noise_oxydation_capi::NoxError {
    let (status, call, error) = create(config);
    assert_eq!(status, NOX_STATUS_INVALID_CONFIG);
    assert!(call.is_null());
    assert_eq!(error.status, NOX_STATUS_INVALID_CONFIG);
    assert_eq!(message(&error), expected.to_string());
    error
}

/// The error the Rust library reports for `config`.
fn rust_error(config: &CallConfig) -> ConfigError {
    CallEnhancer::new(config).expect_err("the configuration is invalid")
}

#[test]
fn out_of_range_carries_field_value_constraint_and_bounds() {
    let mut config = default_config();
    config.spp_mmse.speech_prior = 1.5;
    let expected = rust_error(&CallConfig {
        noise_estimator: NoiseEstimatorConfig::SppMmse(noise_oxydation::SppMmseConfig {
            speech_prior: 1.5,
            ..Default::default()
        }),
        ..CallConfig::default()
    });
    assert_eq!(
        expected,
        ConfigError::OutOfRange { field: ConfigField::SppSpeechPrior, value: 1.5, constraint: Constraint::OpenUnit }
    );
    let error = rejected(&config, &expected);
    assert_eq!(error.config_error, NOX_CONFIG_ERROR_OUT_OF_RANGE);
    assert_eq!(text(nox_config_field_name(error.field)), "spp_mmse.speech_prior");
    assert_eq!(error.value.to_bits(), 1.5_f32.to_bits());
    assert_eq!(text(nox_constraint_description(error.constraint)), "in (0, 1)");
    assert_eq!((error.constraint_lower, error.constraint_upper), (0.0, 1.0));
    assert_eq!((error.constraint_lower_inclusive, error.constraint_upper_inclusive), (0, 0));
}

#[test]
fn a_nan_value_is_carried_as_nan() {
    let mut config = default_config();
    config.high_pass.cutoff_hz = f32::NAN;
    let (status, _, error) = create(&config);
    assert_eq!(status, NOX_STATUS_INVALID_CONFIG);
    assert_eq!(text(nox_config_field_name(error.field)), "high_pass.cutoff_hz");
    assert!(error.value.is_nan());
    assert_eq!(text(nox_constraint_description(error.constraint)), "in (0, 4000) Hz");
}

#[test]
fn min_gain_above_max_gain_carries_both_gains() {
    let mut config = default_config();
    config.log_mmse.min_gain = 0.9;
    config.log_mmse.max_gain = 0.2;
    let expected = rust_error(&CallConfig {
        log_mmse: LogMmseConfig { min_gain: 0.9, max_gain: 0.2, ..LogMmseConfig::default() },
        ..CallConfig::default()
    });
    let error = rejected(&config, &expected);
    assert_eq!(error.config_error, NOX_CONFIG_ERROR_MIN_GAIN_ABOVE_MAX_GAIN);
    assert_eq!((error.value, error.second_value), (0.9, 0.2));
}

#[test]
fn count_out_of_range_carries_field_count_and_bounds() {
    let mut config = default_config();
    config.noise_estimator = NOX_NOISE_ESTIMATOR_MCRA;
    config.mcra.window_frames = 0;
    let expected = rust_error(&CallConfig {
        noise_estimator: NoiseEstimatorConfig::Mcra(McraConfig { window_frames: 0, ..McraConfig::default() }),
        ..CallConfig::default()
    });
    let error = rejected(&config, &expected);
    assert_eq!(error.config_error, NOX_CONFIG_ERROR_COUNT_OUT_OF_RANGE);
    assert_eq!(text(nox_config_field_name(error.field)), "mcra.window_frames");
    assert_eq!((error.count_value, error.count_min, error.count_max), (0, 1, 1000));
}

#[test]
fn start_not_below_full_carries_both_fields_and_values() {
    let mut config = default_config();
    config.tonal_transient.tonal_start_db = 20.0;
    let expected = rust_error(&CallConfig {
        interference: noise_oxydation::InterferenceConfig::TonalTransient(TonalTransientConfig {
            tonal_start_db: 20.0,
            ..TonalTransientConfig::default()
        }),
        ..CallConfig::default()
    });
    let error = rejected(&config, &expected);
    assert_eq!(error.config_error, NOX_CONFIG_ERROR_START_NOT_BELOW_FULL);
    assert_eq!(text(nox_config_field_name(error.field)), "tonal_transient.tonal_start_db");
    assert_eq!(text(nox_config_field_name(error.second_field)), "tonal_transient.tonal_full_db");
    assert_eq!((error.value, error.second_value), (20.0, 14.0));
}

#[test]
fn unselected_parameters_are_not_validated() {
    let mut config = default_config();
    config.mcra.window_frames = 0;
    config.interference = NOX_INTERFERENCE_DISABLED;
    config.tonal_transient.strength = 7.0;
    let (status, call, error) = create(&config);
    assert_eq!(status, NOX_STATUS_OK);
    assert_eq!(error.config_error, NOX_CONFIG_ERROR_NONE);
    // SAFETY: `call` is a live handle from `nox_call_new`, freed once.
    unsafe { noise_oxydation_capi::nox_call_free(call) };
}

#[test]
fn unknown_selectors_are_configuration_errors() {
    let mut config = default_config();
    config.noise_estimator = 0;
    let (status, call, error) = create(&config);
    assert_eq!((status, call.is_null()), (NOX_STATUS_INVALID_CONFIG, true));
    assert_eq!(error.config_error, NOX_CONFIG_ERROR_UNKNOWN_SELECTOR);
    assert_eq!((error.selector, error.selector_value), (NOX_SELECTOR_NOISE_ESTIMATOR, 0));
    assert_eq!(message(&error), "noise_estimator must name a variant, got 0");

    let mut config = default_config();
    config.interference = 3;
    let (status, _, error) = create(&config);
    assert_eq!(status, NOX_STATUS_INVALID_CONFIG);
    assert_eq!((error.selector, error.selector_value), (NOX_SELECTOR_INTERFERENCE, 3));
}

#[test]
fn the_error_detail_is_optional() {
    let mut config = default_config();
    config.decision_directed.alpha = 1.0;
    let mut call = ptr::dangling_mut();
    // SAFETY: `config` and `call` are live, aligned storage; the error detail may be null.
    let status = unsafe { nox_call_new(&raw const config, &raw mut call, ptr::null_mut()) };
    assert_eq!(status, NOX_STATUS_INVALID_CONFIG);
    assert!(call.is_null());
}

#[test]
fn every_field_identifier_has_its_rust_name() {
    let names: Vec<&str> = (1..=41).map(|id| text(nox_config_field_name(id))).collect();
    assert_eq!(names.first(), Some(&"high_pass.cutoff_hz"));
    assert_eq!(names.last(), Some(&"tonal_transient.floor"));
    assert!(names.iter().all(|name| !name.is_empty()));
    assert_eq!(text(nox_config_field_name(0)), "");
    assert_eq!(text(nox_config_field_name(42)), "");
    assert_eq!(text(nox_constraint_description(0)), "");
    assert_eq!(text(nox_constraint_description(9)), "");
}
