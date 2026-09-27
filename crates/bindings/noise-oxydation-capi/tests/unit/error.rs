//! Error details carry every datum of the Rust errors, and the texts equal the Rust `Display` output.

use std::{collections::BTreeSet, time::Duration};

use noise_oxydation::{
    CallConfig, CallEnhancer, ConfigError, ConfigField, Constraint, DecisionDirectedConfig, HighPassConfig,
    LogMmseConfig, McraConfig, NoiseEstimatorConfig, SppMmseConfig, StreamError, TonalTransientConfig,
};

use super::*;
use crate::abi::{NOX_CONSTRAINT_NONE, NOX_FIELD_NONE};

fn message(detail: &NoxError) -> String {
    let mut text = String::new();
    write_message(detail, &mut text).expect("writing to a String succeeds");
    text
}

#[test]
fn field_identifiers_cover_every_field_once_and_round_trip() {
    let paths: BTreeSet<&str> = FIELDS.iter().map(|field| field.path()).collect();
    assert_eq!(paths.len(), FIELDS.len());
    for (id, field) in (1..).zip(FIELDS) {
        assert_eq!(field_id(field), id);
        assert_eq!(field_from_id(id), Some(field));
    }
    assert_eq!(field_from_id(NOX_FIELD_NONE), None);
    assert_eq!(field_from_id(42), None);
    assert_eq!(field_from_id(NOX_FIELD_OTHER), None);
}

#[test]
fn constraint_descriptions_equal_the_rust_display_text() {
    for (id, info) in (1..).zip(CONSTRAINTS) {
        assert_eq!(info.description, info.constraint.to_string());
        assert_eq!(constraint_id(info.constraint), id);
        assert_eq!(constraint_from_id(id).map(|found| found.constraint), Some(info.constraint));
    }
    assert!(constraint_from_id(NOX_CONSTRAINT_NONE).is_none());
    assert!(constraint_from_id(9).is_none());
}

/// Validates `config` and returns the constraint it violated, if any.
fn violated(config: &CallConfig) -> Option<Constraint> {
    match CallEnhancer::new(config) {
        Ok(_) => None,
        Err(ConfigError::OutOfRange { constraint, .. }) => Some(constraint),
        Err(other) => panic!("unexpected error {other}"),
    }
}

/// Builds a default configuration with one field set to the value.
type Setter = fn(f32) -> CallConfig;

#[test]
fn constraint_bounds_match_what_the_library_admits() {
    // One field per constraint; each setter returns a default configuration with that field set to the value.
    let setters: [(Constraint, Setter); 8] = [
        (Constraint::OpenUnit, |value| CallConfig {
            noise_estimator: NoiseEstimatorConfig::SppMmse(SppMmseConfig {
                noise_smoothing: value,
                ..SppMmseConfig::default()
            }),
            ..CallConfig::default()
        }),
        (Constraint::UnitExcludingOne, |value| CallConfig {
            decision_directed: DecisionDirectedConfig { alpha: value, ..DecisionDirectedConfig::default() },
            ..CallConfig::default()
        }),
        (Constraint::UnitExcludingZero, |value| CallConfig {
            log_mmse: LogMmseConfig { max_gain: value, min_gain: 0.0, ..LogMmseConfig::default() },
            ..CallConfig::default()
        }),
        (Constraint::ClosedUnit, |value| CallConfig {
            interference: noise_oxydation::InterferenceConfig::TonalTransient(TonalTransientConfig {
                strength: value,
                ..TonalTransientConfig::default()
            }),
            ..CallConfig::default()
        }),
        (Constraint::Positive, |value| CallConfig {
            log_mmse: LogMmseConfig { noise_overestimation: value, ..LogMmseConfig::default() },
            ..CallConfig::default()
        }),
        (Constraint::NonNegative, |value| CallConfig {
            interference: noise_oxydation::InterferenceConfig::TonalTransient(TonalTransientConfig {
                tonal_start_db: value,
                tonal_full_db: f32::MAX,
                ..TonalTransientConfig::default()
            }),
            ..CallConfig::default()
        }),
        (Constraint::GreaterThanOne, |value| CallConfig {
            noise_estimator: NoiseEstimatorConfig::Mcra(McraConfig { ratio_threshold: value, ..McraConfig::default() }),
            ..CallConfig::default()
        }),
        (Constraint::BelowNyquist, |value| CallConfig {
            high_pass: HighPassConfig { cutoff_hz: value },
            ..CallConfig::default()
        }),
    ];
    for (constraint, setter) in setters {
        let info = constraint_from_id(constraint_id(constraint)).expect("known constraint");
        let expect_at = |value: f32, admitted: bool| {
            assert_eq!(violated(&setter(value)), (!admitted).then_some(constraint), "{constraint:?} at {value}");
        };
        expect_at(info.lower, info.lower_inclusive);
        expect_at(info.lower.next_down(), false);
        expect_at(info.lower.next_up(), true);
        expect_at(info.upper, info.upper_inclusive);
        // Unbounded ranges still reject infinity (checked above); their largest finite values are not probed because
        // they trip unrelated checks such as the start/full order.
        if info.upper.is_finite() {
            expect_at(info.upper.next_down(), true);
            expect_at(info.upper.next_up(), false);
        }
        expect_at(f32::NAN, false);
    }
}

fn every_config_error() -> [ConfigError; 5] {
    [
        ConfigError::OutOfRange { field: ConfigField::SppSpeechPrior, value: 1.5, constraint: Constraint::OpenUnit },
        ConfigError::MinGainAboveMaxGain { min_gain: 0.9, max_gain: 0.2 },
        ConfigError::CountOutOfRange { field: ConfigField::McraWindowFrames, value: 1001, min: 1, max: 1000 },
        ConfigError::StartNotBelowFull {
            start: ConfigField::TonalFluxStartDb,
            start_value: 20.0,
            full: ConfigField::TonalFluxFullDb,
            full_value: 18.0,
        },
        ConfigError::CalibrationTooLong { duration: Duration::MAX },
    ]
}

#[test]
fn every_config_error_variant_round_trips_through_its_detail() {
    let kinds = every_config_error().map(|error| {
        let detail = config_error_detail(&error);
        assert_eq!(detail.status, NOX_STATUS_INVALID_CONFIG);
        assert_eq!(config_error_from_detail(&detail), Some(error));
        detail.config_error
    });
    assert_eq!(
        kinds,
        [
            NOX_CONFIG_ERROR_OUT_OF_RANGE,
            NOX_CONFIG_ERROR_MIN_GAIN_ABOVE_MAX_GAIN,
            NOX_CONFIG_ERROR_COUNT_OUT_OF_RANGE,
            NOX_CONFIG_ERROR_START_NOT_BELOW_FULL,
            NOX_CONFIG_ERROR_CALIBRATION_TOO_LONG,
        ]
    );
}

#[test]
fn details_fill_exactly_the_slots_of_their_variant() {
    let [out_of_range, min_gain, count, order, too_long] =
        every_config_error().map(|error| config_error_detail(&error));

    let mut expected = NoxError::status(NOX_STATUS_INVALID_CONFIG);
    expected.config_error = NOX_CONFIG_ERROR_OUT_OF_RANGE;
    expected.field = 4;
    expected.value = 1.5;
    expected.constraint = 1;
    expected.constraint_lower = 0.0;
    expected.constraint_upper = 1.0;
    assert_eq!(out_of_range, expected);

    let mut expected = NoxError::status(NOX_STATUS_INVALID_CONFIG);
    expected.config_error = NOX_CONFIG_ERROR_MIN_GAIN_ABOVE_MAX_GAIN;
    expected.value = 0.9;
    expected.second_value = 0.2;
    assert_eq!(min_gain, expected);

    let mut expected = NoxError::status(NOX_STATUS_INVALID_CONFIG);
    expected.config_error = NOX_CONFIG_ERROR_COUNT_OUT_OF_RANGE;
    expected.field = 19;
    expected.count_value = 1001;
    expected.count_min = 1;
    expected.count_max = 1000;
    assert_eq!(count, expected);

    let mut expected = NoxError::status(NOX_STATUS_INVALID_CONFIG);
    expected.config_error = NOX_CONFIG_ERROR_START_NOT_BELOW_FULL;
    expected.field = 27;
    expected.value = 20.0;
    expected.second_field = 28;
    expected.second_value = 18.0;
    assert_eq!(order, expected);

    let mut expected = NoxError::status(NOX_STATUS_INVALID_CONFIG);
    expected.config_error = NOX_CONFIG_ERROR_CALIBRATION_TOO_LONG;
    expected.duration_secs = u64::MAX;
    expected.duration_subsec_nanos = 999_999_999;
    assert_eq!(too_long, expected);
}

#[test]
fn config_error_messages_equal_the_rust_display_text() {
    for error in every_config_error() {
        assert_eq!(message(&config_error_detail(&error)), error.to_string());
    }
}

#[test]
fn unknown_selector_and_unrecognized_details_have_messages() {
    let detail = unknown_selector_detail(UnknownSelector { selector: NOX_SELECTOR_NOISE_ESTIMATOR, value: 9 });
    assert_eq!(detail.config_error, NOX_CONFIG_ERROR_UNKNOWN_SELECTOR);
    assert_eq!(message(&detail), "noise_estimator must name a variant, got 9");

    let mut other = NoxError::status(NOX_STATUS_INVALID_CONFIG);
    other.config_error = NOX_CONFIG_ERROR_OTHER;
    assert!(message(&other).starts_with("the call configuration is invalid"));

    let mut unknown_field = config_error_detail(&every_config_error()[0]);
    unknown_field.field = 77;
    assert_eq!(config_error_from_detail(&unknown_field), None);
    assert!(message(&unknown_field).contains("not recognized"));

    let mut bad_duration = config_error_detail(&every_config_error()[4]);
    bad_duration.duration_subsec_nanos = 1_000_000_000;
    assert_eq!(config_error_from_detail(&bad_duration), None);
}

#[test]
fn status_messages_cover_every_status() {
    assert_eq!(message(&NoxError::status(NOX_STATUS_DRAINED)), StreamError::Drained.to_string());
    assert_eq!(stream_status(StreamError::Drained), NOX_STATUS_DRAINED);
    for status in [NOX_STATUS_OK, NOX_STATUS_NULL_ARGUMENT, NOX_STATUS_INVALID_CONFIG, NOX_STATUS_PANIC] {
        assert_ne!(status_message(status), status_message(99));
    }
    assert_eq!(status_message(99), "unknown status");
}

#[test]
fn counting_buffer_stores_what_fits_and_counts_everything() {
    let mut storage = [0_u8; 8];
    let mut buffer = CountingBuffer::new(&mut storage);
    buffer.write_str("abcdefghijkl").expect("never fails");
    assert_eq!(buffer.written(), 12);
    assert_eq!(&storage, b"abcdefgh");

    let mut empty = CountingBuffer::new(&mut []);
    empty.write_str("xyz").expect("never fails");
    assert_eq!(empty.written(), 3);
}
