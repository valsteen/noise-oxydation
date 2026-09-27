//! The `#[repr(C)]` types and the constants of the ABI. `go/noise_oxydation.h` declares the same items; the layout
//! assertions below and the header's `_Static_assert`s pin both sides to the same sizes and offsets, and the unit
//! tests compare every header constant and assertion with this module.

use std::mem::{offset_of, size_of};

use noise_oxydation::{DELAY_PACKETS, PACKET_SAMPLES};

/// Bytes of one 20 ms G.711 μ-law packet.
pub const NOX_PACKET_BYTES: usize = PACKET_SAMPLES;
/// Packets withheld by the fixed algorithmic delay; the capacity of the drain buffer in packets.
pub const NOX_DELAY_PACKETS: usize = DELAY_PACKETS;

/// The operation succeeded.
pub const NOX_STATUS_OK: u32 = 0;
/// A required pointer argument was null.
pub const NOX_STATUS_NULL_ARGUMENT: u32 = 1;
/// The configuration was rejected; the error detail says why.
pub const NOX_STATUS_INVALID_CONFIG: u32 = 2;
/// The call has been drained; only reset and free are accepted.
pub const NOX_STATUS_DRAINED: u32 = 3;
/// The enhancer panicked. The panic did not unwind across the ABI; the handle is poisoned.
pub const NOX_STATUS_PANIC: u32 = 4;

/// Noise estimator selector: SPP-MMSE, parameters in [`NoxConfig::spp_mmse`].
pub const NOX_NOISE_ESTIMATOR_SPP_MMSE: u32 = 1;
/// Noise estimator selector: MCRA, parameters in [`NoxConfig::mcra`].
pub const NOX_NOISE_ESTIMATOR_MCRA: u32 = 2;
/// Noise estimator selector: minimum estimator, parameters in [`NoxConfig::minimum`].
pub const NOX_NOISE_ESTIMATOR_MINIMUM: u32 = 3;

/// Interference selector: tonal transient suppression, parameters in [`NoxConfig::tonal_transient`].
pub const NOX_INTERFERENCE_TONAL_TRANSIENT: u32 = 1;
/// Interference selector: no interference suppression.
pub const NOX_INTERFERENCE_DISABLED: u32 = 2;

/// `nox_call_process` wrote nothing: one of the first two packets of a call.
pub const NOX_OUTCOME_PRIMING: u32 = 1;
/// `nox_call_process` wrote one enhanced output packet.
pub const NOX_OUTCOME_EMITTED: u32 = 2;

/// Call phase: frames pass through while the noise estimator learns the quiet intro.
pub const NOX_PHASE_CALIBRATING: u32 = 1;
/// Call phase: frames are enhanced.
pub const NOX_PHASE_ENHANCING: u32 = 2;
/// Call phase: the call has been drained.
pub const NOX_PHASE_DRAINED: u32 = 3;

/// No configuration error (the status is not [`NOX_STATUS_INVALID_CONFIG`]).
pub const NOX_CONFIG_ERROR_NONE: u32 = 0;
/// `ConfigError::OutOfRange`: `field`, `value`, `constraint` and the constraint's bounds.
pub const NOX_CONFIG_ERROR_OUT_OF_RANGE: u32 = 1;
/// `ConfigError::MinGainAboveMaxGain`: `value` is `min_gain`, `second_value` is `max_gain`.
pub const NOX_CONFIG_ERROR_MIN_GAIN_ABOVE_MAX_GAIN: u32 = 2;
/// `ConfigError::CountOutOfRange`: `field`, `count_value`, `count_min`, `count_max`.
pub const NOX_CONFIG_ERROR_COUNT_OUT_OF_RANGE: u32 = 3;
/// `ConfigError::StartNotBelowFull`: `field` and `value` are the start, `second_field` and `second_value` the full
/// threshold.
pub const NOX_CONFIG_ERROR_START_NOT_BELOW_FULL: u32 = 4;
/// `ConfigError::CalibrationTooLong`: `duration_secs` and `duration_subsec_nanos`.
pub const NOX_CONFIG_ERROR_CALIBRATION_TOO_LONG: u32 = 5;
/// A selector of [`NoxConfig`] holds an unknown value: `selector` names it, `selector_value` holds the value.
pub const NOX_CONFIG_ERROR_UNKNOWN_SELECTOR: u32 = 6;
/// A `ConfigError` variant this version of the ABI does not describe.
pub const NOX_CONFIG_ERROR_OTHER: u32 = 7;

/// No selector.
pub const NOX_SELECTOR_NONE: u32 = 0;
/// [`NoxConfig::noise_estimator`].
pub const NOX_SELECTOR_NOISE_ESTIMATOR: u32 = 1;
/// [`NoxConfig::interference`].
pub const NOX_SELECTOR_INTERFERENCE: u32 = 2;

/// No field. Field identifiers `1..=41` follow the order of `ConfigField`; `nox_config_field_name` names them.
pub const NOX_FIELD_NONE: u32 = 0;
/// A `ConfigField` this version of the ABI does not describe.
pub const NOX_FIELD_OTHER: u32 = 999;

/// No constraint. Constraint identifiers `1..=8` follow the order of `Constraint`; `nox_constraint_description`
/// describes them.
pub const NOX_CONSTRAINT_NONE: u32 = 0;
/// A `Constraint` this version of the ABI does not describe.
pub const NOX_CONSTRAINT_OTHER: u32 = 999;

/// A borrowed UTF-8 string with static lifetime: `len` bytes at `ptr`, not NUL-terminated. `ptr` is never null.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoxStr {
    /// First byte.
    pub ptr: *const u8,
    /// Length in bytes.
    pub len: usize,
}

impl NoxStr {
    pub(crate) const fn new(text: &'static str) -> Self {
        Self { ptr: text.as_ptr(), len: text.len() }
    }
}

/// `HighPassConfig`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoxHighPassConfig {
    /// `cutoff_hz`.
    pub cutoff_hz: f32,
}

/// `SppMmseConfig`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoxSppMmseConfig {
    /// `noise_smoothing`.
    pub noise_smoothing: f32,
    /// `spp_smoothing`.
    pub spp_smoothing: f32,
    /// `speech_prior`.
    pub speech_prior: f32,
    /// `fixed_prior_snr`.
    pub fixed_prior_snr: f32,
    /// `stagnation_threshold`.
    pub stagnation_threshold: f32,
    /// `max_speech_probability`.
    pub max_speech_probability: f32,
    /// `floor`.
    pub floor: f32,
}

/// `McraConfig`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoxMcraConfig {
    /// `smoothing`.
    pub smoothing: f32,
    /// `speech_smoothing`.
    pub speech_smoothing: f32,
    /// `noise_smoothing`.
    pub noise_smoothing: f32,
    /// `ratio_threshold`.
    pub ratio_threshold: f32,
    /// `window_frames`.
    pub window_frames: u16,
    /// `floor`.
    pub floor: f32,
}

/// `MinimumConfig`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoxMinimumConfig {
    /// `smoothing`.
    pub smoothing: f32,
    /// `window_frames`.
    pub window_frames: u16,
    /// `floor`.
    pub floor: f32,
}

/// `DecisionDirectedConfig`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoxDecisionDirectedConfig {
    /// `alpha`.
    pub alpha: f32,
    /// `floor`.
    pub floor: f32,
}

/// `LogMmseConfig`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoxLogMmseConfig {
    /// `min_gain`.
    pub min_gain: f32,
    /// `max_gain`.
    pub max_gain: f32,
    /// `floor`.
    pub floor: f32,
    /// `noise_overestimation`.
    pub noise_overestimation: f32,
}

/// `TonalTransientConfig`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoxTonalTransientConfig {
    /// `local_radius`.
    pub local_radius: u16,
    /// `tonal_start_db`.
    pub tonal_start_db: f32,
    /// `tonal_full_db`.
    pub tonal_full_db: f32,
    /// `flux_start_db`.
    pub flux_start_db: f32,
    /// `flux_full_db`.
    pub flux_full_db: f32,
    /// `movement_search_radius`.
    pub movement_search_radius: u16,
    /// `movement_start_bins`.
    pub movement_start_bins: u16,
    /// `movement_full_bins`.
    pub movement_full_bins: u16,
    /// `min_frequency_bin`.
    pub min_frequency_bin: u16,
    /// `movement_min_relative_power`.
    pub movement_min_relative_power: f32,
    /// `harmonic_tolerance_bins`.
    pub harmonic_tolerance_bins: u16,
    /// `harmonic_relative_power`.
    pub harmonic_relative_power: f32,
    /// `strength`.
    pub strength: f32,
    /// `min_gain`.
    pub min_gain: f32,
    /// `attack`.
    pub attack: f32,
    /// `release`.
    pub release: f32,
    /// `spread_radius`.
    pub spread_radius: u16,
    /// `floor`.
    pub floor: f32,
}

/// `CallConfig`. Every estimator's and the tonal detector's parameters are present; the selectors choose which are
/// used, as the Rust enums `NoiseEstimatorConfig` and `InterferenceConfig` do.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoxConfig {
    /// `calibration_duration` in nanoseconds.
    pub calibration_duration_ns: u64,
    /// `high_pass`.
    pub high_pass: NoxHighPassConfig,
    /// `noise_estimator` selector: `NOX_NOISE_ESTIMATOR_*`.
    pub noise_estimator: u32,
    /// Parameters of `NoiseEstimatorConfig::SppMmse`.
    pub spp_mmse: NoxSppMmseConfig,
    /// Parameters of `NoiseEstimatorConfig::Mcra`.
    pub mcra: NoxMcraConfig,
    /// Parameters of `NoiseEstimatorConfig::Minimum`.
    pub minimum: NoxMinimumConfig,
    /// `decision_directed`.
    pub decision_directed: NoxDecisionDirectedConfig,
    /// `log_mmse`.
    pub log_mmse: NoxLogMmseConfig,
    /// `interference` selector: `NOX_INTERFERENCE_*`.
    pub interference: u32,
    /// Parameters of `InterferenceConfig::TonalTransient`.
    pub tonal_transient: NoxTonalTransientConfig,
}

/// Structured detail of a failed operation, written without allocation into caller-owned storage.
///
/// `status` repeats the returned status. For [`NOX_STATUS_INVALID_CONFIG`], `config_error` names the variant and the
/// fields listed at its `NOX_CONFIG_ERROR_*` constant hold its data; every other field is zero.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoxError {
    /// `NOX_STATUS_*`.
    pub status: u32,
    /// `NOX_CONFIG_ERROR_*`.
    pub config_error: u32,
    /// `NOX_FIELD_*` identifier of the offending field or of the start threshold.
    pub field: u32,
    /// `NOX_FIELD_*` identifier of the full threshold.
    pub second_field: u32,
    /// `NOX_CONSTRAINT_*` identifier of the violated range.
    pub constraint: u32,
    /// The rejected value, the minimum gain or the start value.
    pub value: f32,
    /// The maximum gain or the full value.
    pub second_value: f32,
    /// Lower bound of `constraint`.
    pub constraint_lower: f32,
    /// Upper bound of `constraint`; positive infinity when unbounded (finite values are still required).
    pub constraint_upper: f32,
    /// 1 when the lower bound is admitted.
    pub constraint_lower_inclusive: u8,
    /// 1 when the upper bound is admitted.
    pub constraint_upper_inclusive: u8,
    /// The rejected count.
    pub count_value: u16,
    /// The smallest allowed count.
    pub count_min: u16,
    /// The largest allowed count.
    pub count_max: u16,
    /// `NOX_SELECTOR_*` of the unknown selector.
    pub selector: u32,
    /// The unknown selector value.
    pub selector_value: u32,
    /// Sub-second part of the rejected calibration duration, in nanoseconds.
    pub duration_subsec_nanos: u32,
    /// Whole seconds of the rejected calibration duration.
    pub duration_secs: u64,
}

impl NoxError {
    /// A detail with every field zero except `status`.
    pub(crate) const fn status(status: u32) -> Self {
        Self {
            status,
            config_error: NOX_CONFIG_ERROR_NONE,
            field: NOX_FIELD_NONE,
            second_field: NOX_FIELD_NONE,
            constraint: NOX_CONSTRAINT_NONE,
            value: 0.0,
            second_value: 0.0,
            constraint_lower: 0.0,
            constraint_upper: 0.0,
            constraint_lower_inclusive: 0,
            constraint_upper_inclusive: 0,
            count_value: 0,
            count_min: 0,
            count_max: 0,
            selector: NOX_SELECTOR_NONE,
            selector_value: 0,
            duration_subsec_nanos: 0,
            duration_secs: 0,
        }
    }
}

// Layout pins, repeated by the `_Static_assert`s of `go/noise_oxydation.h` and compared with them by the unit tests.
const _: () = {
    assert!(size_of::<NoxStr>() == 2 * size_of::<usize>());
    assert!(offset_of!(NoxStr, ptr) == 0);
    assert!(offset_of!(NoxStr, len) == size_of::<usize>());

    assert!(size_of::<NoxHighPassConfig>() == 4);
    assert!(offset_of!(NoxHighPassConfig, cutoff_hz) == 0);

    assert!(size_of::<NoxSppMmseConfig>() == 28);
    assert!(offset_of!(NoxSppMmseConfig, noise_smoothing) == 0);
    assert!(offset_of!(NoxSppMmseConfig, spp_smoothing) == 4);
    assert!(offset_of!(NoxSppMmseConfig, speech_prior) == 8);
    assert!(offset_of!(NoxSppMmseConfig, fixed_prior_snr) == 12);
    assert!(offset_of!(NoxSppMmseConfig, stagnation_threshold) == 16);
    assert!(offset_of!(NoxSppMmseConfig, max_speech_probability) == 20);
    assert!(offset_of!(NoxSppMmseConfig, floor) == 24);

    assert!(size_of::<NoxMcraConfig>() == 24);
    assert!(offset_of!(NoxMcraConfig, smoothing) == 0);
    assert!(offset_of!(NoxMcraConfig, speech_smoothing) == 4);
    assert!(offset_of!(NoxMcraConfig, noise_smoothing) == 8);
    assert!(offset_of!(NoxMcraConfig, ratio_threshold) == 12);
    assert!(offset_of!(NoxMcraConfig, window_frames) == 16);
    assert!(offset_of!(NoxMcraConfig, floor) == 20);

    assert!(size_of::<NoxMinimumConfig>() == 12);
    assert!(offset_of!(NoxMinimumConfig, smoothing) == 0);
    assert!(offset_of!(NoxMinimumConfig, window_frames) == 4);
    assert!(offset_of!(NoxMinimumConfig, floor) == 8);

    assert!(size_of::<NoxDecisionDirectedConfig>() == 8);
    assert!(offset_of!(NoxDecisionDirectedConfig, alpha) == 0);
    assert!(offset_of!(NoxDecisionDirectedConfig, floor) == 4);

    assert!(size_of::<NoxLogMmseConfig>() == 16);
    assert!(offset_of!(NoxLogMmseConfig, min_gain) == 0);
    assert!(offset_of!(NoxLogMmseConfig, max_gain) == 4);
    assert!(offset_of!(NoxLogMmseConfig, floor) == 8);
    assert!(offset_of!(NoxLogMmseConfig, noise_overestimation) == 12);

    assert!(size_of::<NoxTonalTransientConfig>() == 64);
    assert!(offset_of!(NoxTonalTransientConfig, local_radius) == 0);
    assert!(offset_of!(NoxTonalTransientConfig, tonal_start_db) == 4);
    assert!(offset_of!(NoxTonalTransientConfig, tonal_full_db) == 8);
    assert!(offset_of!(NoxTonalTransientConfig, flux_start_db) == 12);
    assert!(offset_of!(NoxTonalTransientConfig, flux_full_db) == 16);
    assert!(offset_of!(NoxTonalTransientConfig, movement_search_radius) == 20);
    assert!(offset_of!(NoxTonalTransientConfig, movement_start_bins) == 22);
    assert!(offset_of!(NoxTonalTransientConfig, movement_full_bins) == 24);
    assert!(offset_of!(NoxTonalTransientConfig, min_frequency_bin) == 26);
    assert!(offset_of!(NoxTonalTransientConfig, movement_min_relative_power) == 28);
    assert!(offset_of!(NoxTonalTransientConfig, harmonic_tolerance_bins) == 32);
    assert!(offset_of!(NoxTonalTransientConfig, harmonic_relative_power) == 36);
    assert!(offset_of!(NoxTonalTransientConfig, strength) == 40);
    assert!(offset_of!(NoxTonalTransientConfig, min_gain) == 44);
    assert!(offset_of!(NoxTonalTransientConfig, attack) == 48);
    assert!(offset_of!(NoxTonalTransientConfig, release) == 52);
    assert!(offset_of!(NoxTonalTransientConfig, spread_radius) == 56);
    assert!(offset_of!(NoxTonalTransientConfig, floor) == 60);

    assert!(size_of::<NoxConfig>() == 176);
    assert!(offset_of!(NoxConfig, calibration_duration_ns) == 0);
    assert!(offset_of!(NoxConfig, high_pass) == 8);
    assert!(offset_of!(NoxConfig, noise_estimator) == 12);
    assert!(offset_of!(NoxConfig, spp_mmse) == 16);
    assert!(offset_of!(NoxConfig, mcra) == 44);
    assert!(offset_of!(NoxConfig, minimum) == 68);
    assert!(offset_of!(NoxConfig, decision_directed) == 80);
    assert!(offset_of!(NoxConfig, log_mmse) == 88);
    assert!(offset_of!(NoxConfig, interference) == 104);
    assert!(offset_of!(NoxConfig, tonal_transient) == 108);

    assert!(size_of::<NoxError>() == 64);
    assert!(offset_of!(NoxError, status) == 0);
    assert!(offset_of!(NoxError, config_error) == 4);
    assert!(offset_of!(NoxError, field) == 8);
    assert!(offset_of!(NoxError, second_field) == 12);
    assert!(offset_of!(NoxError, constraint) == 16);
    assert!(offset_of!(NoxError, value) == 20);
    assert!(offset_of!(NoxError, second_value) == 24);
    assert!(offset_of!(NoxError, constraint_lower) == 28);
    assert!(offset_of!(NoxError, constraint_upper) == 32);
    assert!(offset_of!(NoxError, constraint_lower_inclusive) == 36);
    assert!(offset_of!(NoxError, constraint_upper_inclusive) == 37);
    assert!(offset_of!(NoxError, count_value) == 38);
    assert!(offset_of!(NoxError, count_min) == 40);
    assert!(offset_of!(NoxError, count_max) == 42);
    assert!(offset_of!(NoxError, selector) == 44);
    assert!(offset_of!(NoxError, selector_value) == 48);
    assert!(offset_of!(NoxError, duration_subsec_nanos) == 52);
    assert!(offset_of!(NoxError, duration_secs) == 56);
};

#[cfg(test)]
#[path = "../tests/unit/abi.rs"]
mod tests;
