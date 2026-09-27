//! The committed C header declares exactly the constants and layout of this module.

use std::{
    collections::BTreeMap,
    fs,
    mem::{offset_of, size_of},
    path::Path,
};

use super::*;
use crate::error::{CONSTRAINTS, FIELDS};

fn header() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../go/noise_oxydation.h");
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

/// `NAME = value,` enumerator lines of the header.
fn header_constants(header: &str) -> BTreeMap<String, u64> {
    header
        .lines()
        .filter_map(|line| {
            let (name, value) = line.trim().strip_suffix(',')?.split_once(" = ")?;
            name.starts_with("NOX_").then(|| (name.to_owned(), value.parse().expect("enumerator value")))
        })
        .collect()
}

/// `(type, field)` → value of every `_Static_assert(sizeof(type) == value` and
/// `_Static_assert(offsetof(type, field) == value` of the header; `field` is empty for sizes.
fn header_layout(header: &str) -> BTreeMap<(String, String), usize> {
    header
        .lines()
        .filter_map(|line| {
            let assertion = line.strip_prefix("_Static_assert(")?;
            let (operand, rest) = assertion.split_once(") == ")?;
            let value = rest.split_once(',')?.0.parse().expect("layout value");
            if let Some(type_name) = operand.strip_prefix("sizeof(") {
                (type_name != "void *").then(|| ((type_name.to_owned(), String::new()), value))
            } else {
                let (type_name, field) = operand.strip_prefix("offsetof(")?.split_once(", ")?;
                Some(((type_name.to_owned(), field.to_owned()), value))
            }
        })
        .collect()
}

fn screaming_snake(camel: &str) -> String {
    let mut name = String::new();
    for (index, character) in camel.chars().enumerate() {
        if character.is_ascii_uppercase() && index > 0 {
            name.push('_');
        }
        name.push(character.to_ascii_uppercase());
    }
    name
}

#[test]
fn header_constants_equal_the_rust_constants() {
    let mut expected: BTreeMap<String, u64> = [
        ("NOX_PACKET_BYTES", u64::try_from(NOX_PACKET_BYTES).expect("fits")),
        ("NOX_DELAY_PACKETS", u64::try_from(NOX_DELAY_PACKETS).expect("fits")),
        ("NOX_STATUS_OK", NOX_STATUS_OK.into()),
        ("NOX_STATUS_NULL_ARGUMENT", NOX_STATUS_NULL_ARGUMENT.into()),
        ("NOX_STATUS_INVALID_CONFIG", NOX_STATUS_INVALID_CONFIG.into()),
        ("NOX_STATUS_DRAINED", NOX_STATUS_DRAINED.into()),
        ("NOX_STATUS_PANIC", NOX_STATUS_PANIC.into()),
        ("NOX_NOISE_ESTIMATOR_SPP_MMSE", NOX_NOISE_ESTIMATOR_SPP_MMSE.into()),
        ("NOX_NOISE_ESTIMATOR_MCRA", NOX_NOISE_ESTIMATOR_MCRA.into()),
        ("NOX_NOISE_ESTIMATOR_MINIMUM", NOX_NOISE_ESTIMATOR_MINIMUM.into()),
        ("NOX_INTERFERENCE_TONAL_TRANSIENT", NOX_INTERFERENCE_TONAL_TRANSIENT.into()),
        ("NOX_INTERFERENCE_DISABLED", NOX_INTERFERENCE_DISABLED.into()),
        ("NOX_OUTCOME_PRIMING", NOX_OUTCOME_PRIMING.into()),
        ("NOX_OUTCOME_EMITTED", NOX_OUTCOME_EMITTED.into()),
        ("NOX_PHASE_CALIBRATING", NOX_PHASE_CALIBRATING.into()),
        ("NOX_PHASE_ENHANCING", NOX_PHASE_ENHANCING.into()),
        ("NOX_PHASE_DRAINED", NOX_PHASE_DRAINED.into()),
        ("NOX_CONFIG_ERROR_NONE", NOX_CONFIG_ERROR_NONE.into()),
        ("NOX_CONFIG_ERROR_OUT_OF_RANGE", NOX_CONFIG_ERROR_OUT_OF_RANGE.into()),
        ("NOX_CONFIG_ERROR_MIN_GAIN_ABOVE_MAX_GAIN", NOX_CONFIG_ERROR_MIN_GAIN_ABOVE_MAX_GAIN.into()),
        ("NOX_CONFIG_ERROR_COUNT_OUT_OF_RANGE", NOX_CONFIG_ERROR_COUNT_OUT_OF_RANGE.into()),
        ("NOX_CONFIG_ERROR_START_NOT_BELOW_FULL", NOX_CONFIG_ERROR_START_NOT_BELOW_FULL.into()),
        ("NOX_CONFIG_ERROR_CALIBRATION_TOO_LONG", NOX_CONFIG_ERROR_CALIBRATION_TOO_LONG.into()),
        ("NOX_CONFIG_ERROR_UNKNOWN_SELECTOR", NOX_CONFIG_ERROR_UNKNOWN_SELECTOR.into()),
        ("NOX_CONFIG_ERROR_OTHER", NOX_CONFIG_ERROR_OTHER.into()),
        ("NOX_SELECTOR_NONE", NOX_SELECTOR_NONE.into()),
        ("NOX_SELECTOR_NOISE_ESTIMATOR", NOX_SELECTOR_NOISE_ESTIMATOR.into()),
        ("NOX_SELECTOR_INTERFERENCE", NOX_SELECTOR_INTERFERENCE.into()),
        ("NOX_FIELD_NONE", NOX_FIELD_NONE.into()),
        ("NOX_FIELD_OTHER", NOX_FIELD_OTHER.into()),
        ("NOX_CONSTRAINT_NONE", NOX_CONSTRAINT_NONE.into()),
        ("NOX_CONSTRAINT_OTHER", NOX_CONSTRAINT_OTHER.into()),
    ]
    .into_iter()
    .map(|(name, value)| (name.to_owned(), value))
    .collect();
    // Field identifiers are named after the Rust field path, constraint identifiers after the Rust variant.
    for (id, field) in (1_u64..).zip(FIELDS) {
        expected.insert(format!("NOX_FIELD_{}", field.path().replace('.', "_").to_ascii_uppercase()), id);
    }
    for (id, info) in (1_u64..).zip(CONSTRAINTS) {
        expected.insert(format!("NOX_CONSTRAINT_{}", screaming_snake(&format!("{:?}", info.constraint))), id);
    }
    assert_eq!(header_constants(&header()), expected);
}

type LayoutEntry = ((String, String), usize);

fn entry(type_name: &str, field: &str, value: usize) -> LayoutEntry {
    ((type_name.to_owned(), field.to_owned()), value)
}

/// The layout of the parameter structs nested in `nox_config`.
fn parameter_layout() -> Vec<LayoutEntry> {
    vec![
        entry("nox_high_pass_config", "", size_of::<NoxHighPassConfig>()),
        entry("nox_high_pass_config", "cutoff_hz", offset_of!(NoxHighPassConfig, cutoff_hz)),
        entry("nox_spp_mmse_config", "", size_of::<NoxSppMmseConfig>()),
        entry("nox_spp_mmse_config", "noise_smoothing", offset_of!(NoxSppMmseConfig, noise_smoothing)),
        entry("nox_spp_mmse_config", "spp_smoothing", offset_of!(NoxSppMmseConfig, spp_smoothing)),
        entry("nox_spp_mmse_config", "speech_prior", offset_of!(NoxSppMmseConfig, speech_prior)),
        entry("nox_spp_mmse_config", "fixed_prior_snr", offset_of!(NoxSppMmseConfig, fixed_prior_snr)),
        entry("nox_spp_mmse_config", "stagnation_threshold", offset_of!(NoxSppMmseConfig, stagnation_threshold)),
        entry("nox_spp_mmse_config", "max_speech_probability", offset_of!(NoxSppMmseConfig, max_speech_probability)),
        entry("nox_spp_mmse_config", "floor", offset_of!(NoxSppMmseConfig, floor)),
        entry("nox_mcra_config", "", size_of::<NoxMcraConfig>()),
        entry("nox_mcra_config", "smoothing", offset_of!(NoxMcraConfig, smoothing)),
        entry("nox_mcra_config", "speech_smoothing", offset_of!(NoxMcraConfig, speech_smoothing)),
        entry("nox_mcra_config", "noise_smoothing", offset_of!(NoxMcraConfig, noise_smoothing)),
        entry("nox_mcra_config", "ratio_threshold", offset_of!(NoxMcraConfig, ratio_threshold)),
        entry("nox_mcra_config", "window_frames", offset_of!(NoxMcraConfig, window_frames)),
        entry("nox_mcra_config", "floor", offset_of!(NoxMcraConfig, floor)),
        entry("nox_minimum_config", "", size_of::<NoxMinimumConfig>()),
        entry("nox_minimum_config", "smoothing", offset_of!(NoxMinimumConfig, smoothing)),
        entry("nox_minimum_config", "window_frames", offset_of!(NoxMinimumConfig, window_frames)),
        entry("nox_minimum_config", "floor", offset_of!(NoxMinimumConfig, floor)),
        entry("nox_decision_directed_config", "", size_of::<NoxDecisionDirectedConfig>()),
        entry("nox_decision_directed_config", "alpha", offset_of!(NoxDecisionDirectedConfig, alpha)),
        entry("nox_decision_directed_config", "floor", offset_of!(NoxDecisionDirectedConfig, floor)),
        entry("nox_log_mmse_config", "", size_of::<NoxLogMmseConfig>()),
        entry("nox_log_mmse_config", "min_gain", offset_of!(NoxLogMmseConfig, min_gain)),
        entry("nox_log_mmse_config", "max_gain", offset_of!(NoxLogMmseConfig, max_gain)),
        entry("nox_log_mmse_config", "floor", offset_of!(NoxLogMmseConfig, floor)),
        entry("nox_log_mmse_config", "noise_overestimation", offset_of!(NoxLogMmseConfig, noise_overestimation)),
        entry("nox_tonal_transient_config", "", size_of::<NoxTonalTransientConfig>()),
        entry("nox_tonal_transient_config", "local_radius", offset_of!(NoxTonalTransientConfig, local_radius)),
        entry("nox_tonal_transient_config", "tonal_start_db", offset_of!(NoxTonalTransientConfig, tonal_start_db)),
        entry("nox_tonal_transient_config", "tonal_full_db", offset_of!(NoxTonalTransientConfig, tonal_full_db)),
        entry("nox_tonal_transient_config", "flux_start_db", offset_of!(NoxTonalTransientConfig, flux_start_db)),
        entry("nox_tonal_transient_config", "flux_full_db", offset_of!(NoxTonalTransientConfig, flux_full_db)),
        entry(
            "nox_tonal_transient_config",
            "movement_search_radius",
            offset_of!(NoxTonalTransientConfig, movement_search_radius),
        ),
        entry(
            "nox_tonal_transient_config",
            "movement_start_bins",
            offset_of!(NoxTonalTransientConfig, movement_start_bins),
        ),
        entry(
            "nox_tonal_transient_config",
            "movement_full_bins",
            offset_of!(NoxTonalTransientConfig, movement_full_bins),
        ),
        entry(
            "nox_tonal_transient_config",
            "min_frequency_bin",
            offset_of!(NoxTonalTransientConfig, min_frequency_bin),
        ),
        entry(
            "nox_tonal_transient_config",
            "movement_min_relative_power",
            offset_of!(NoxTonalTransientConfig, movement_min_relative_power),
        ),
        entry(
            "nox_tonal_transient_config",
            "harmonic_tolerance_bins",
            offset_of!(NoxTonalTransientConfig, harmonic_tolerance_bins),
        ),
        entry(
            "nox_tonal_transient_config",
            "harmonic_relative_power",
            offset_of!(NoxTonalTransientConfig, harmonic_relative_power),
        ),
        entry("nox_tonal_transient_config", "strength", offset_of!(NoxTonalTransientConfig, strength)),
        entry("nox_tonal_transient_config", "min_gain", offset_of!(NoxTonalTransientConfig, min_gain)),
        entry("nox_tonal_transient_config", "attack", offset_of!(NoxTonalTransientConfig, attack)),
        entry("nox_tonal_transient_config", "release", offset_of!(NoxTonalTransientConfig, release)),
        entry("nox_tonal_transient_config", "spread_radius", offset_of!(NoxTonalTransientConfig, spread_radius)),
        entry("nox_tonal_transient_config", "floor", offset_of!(NoxTonalTransientConfig, floor)),
    ]
}

/// The layout of `nox_config`, `nox_error` and `nox_str`.
fn top_level_layout() -> Vec<LayoutEntry> {
    vec![
        entry("nox_str", "", size_of::<NoxStr>()),
        entry("nox_str", "ptr", offset_of!(NoxStr, ptr)),
        entry("nox_str", "len", offset_of!(NoxStr, len)),
        entry("nox_config", "", size_of::<NoxConfig>()),
        entry("nox_config", "calibration_duration_ns", offset_of!(NoxConfig, calibration_duration_ns)),
        entry("nox_config", "high_pass", offset_of!(NoxConfig, high_pass)),
        entry("nox_config", "noise_estimator", offset_of!(NoxConfig, noise_estimator)),
        entry("nox_config", "spp_mmse", offset_of!(NoxConfig, spp_mmse)),
        entry("nox_config", "mcra", offset_of!(NoxConfig, mcra)),
        entry("nox_config", "minimum", offset_of!(NoxConfig, minimum)),
        entry("nox_config", "decision_directed", offset_of!(NoxConfig, decision_directed)),
        entry("nox_config", "log_mmse", offset_of!(NoxConfig, log_mmse)),
        entry("nox_config", "interference", offset_of!(NoxConfig, interference)),
        entry("nox_config", "tonal_transient", offset_of!(NoxConfig, tonal_transient)),
        entry("nox_error", "", size_of::<NoxError>()),
        entry("nox_error", "status", offset_of!(NoxError, status)),
        entry("nox_error", "config_error", offset_of!(NoxError, config_error)),
        entry("nox_error", "field", offset_of!(NoxError, field)),
        entry("nox_error", "second_field", offset_of!(NoxError, second_field)),
        entry("nox_error", "constraint", offset_of!(NoxError, constraint)),
        entry("nox_error", "value", offset_of!(NoxError, value)),
        entry("nox_error", "second_value", offset_of!(NoxError, second_value)),
        entry("nox_error", "constraint_lower", offset_of!(NoxError, constraint_lower)),
        entry("nox_error", "constraint_upper", offset_of!(NoxError, constraint_upper)),
        entry("nox_error", "constraint_lower_inclusive", offset_of!(NoxError, constraint_lower_inclusive)),
        entry("nox_error", "constraint_upper_inclusive", offset_of!(NoxError, constraint_upper_inclusive)),
        entry("nox_error", "count_value", offset_of!(NoxError, count_value)),
        entry("nox_error", "count_min", offset_of!(NoxError, count_min)),
        entry("nox_error", "count_max", offset_of!(NoxError, count_max)),
        entry("nox_error", "selector", offset_of!(NoxError, selector)),
        entry("nox_error", "selector_value", offset_of!(NoxError, selector_value)),
        entry("nox_error", "duration_subsec_nanos", offset_of!(NoxError, duration_subsec_nanos)),
        entry("nox_error", "duration_secs", offset_of!(NoxError, duration_secs)),
    ]
}

#[test]
fn header_layout_assertions_equal_the_rust_layout() {
    let expected: BTreeMap<(String, String), usize> =
        parameter_layout().into_iter().chain(top_level_layout()).collect();
    assert_eq!(header_layout(&header()), expected);
}
