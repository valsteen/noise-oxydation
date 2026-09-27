//! Error details and texts: the mapping of `ConfigError`, `ConfigField` and `Constraint` onto the ABI identifiers, and
//! the allocation-free message writer.

use std::{
    fmt::{self, Write},
    time::Duration,
};

use noise_oxydation::{ConfigError, ConfigField, Constraint, StreamError};

use crate::{
    abi::{
        NOX_CONFIG_ERROR_CALIBRATION_TOO_LONG, NOX_CONFIG_ERROR_COUNT_OUT_OF_RANGE,
        NOX_CONFIG_ERROR_MIN_GAIN_ABOVE_MAX_GAIN, NOX_CONFIG_ERROR_OTHER, NOX_CONFIG_ERROR_OUT_OF_RANGE,
        NOX_CONFIG_ERROR_START_NOT_BELOW_FULL, NOX_CONFIG_ERROR_UNKNOWN_SELECTOR, NOX_CONSTRAINT_OTHER,
        NOX_FIELD_OTHER, NOX_SELECTOR_INTERFERENCE, NOX_SELECTOR_NOISE_ESTIMATOR, NOX_STATUS_DRAINED,
        NOX_STATUS_INVALID_CONFIG, NOX_STATUS_NULL_ARGUMENT, NOX_STATUS_OK, NOX_STATUS_PANIC, NoxError,
    },
    config::UnknownSelector,
};

/// Every `ConfigField`; the ABI identifier of `FIELDS[i]` is `i + 1`.
pub(crate) const FIELDS: [ConfigField; 41] = [
    ConfigField::HighPassCutoffHz,
    ConfigField::SppNoiseSmoothing,
    ConfigField::SppSmoothing,
    ConfigField::SppSpeechPrior,
    ConfigField::SppFixedPriorSnr,
    ConfigField::SppStagnationThreshold,
    ConfigField::SppMaxSpeechProbability,
    ConfigField::SppFloor,
    ConfigField::DecisionDirectedAlpha,
    ConfigField::DecisionDirectedFloor,
    ConfigField::LogMmseMinGain,
    ConfigField::LogMmseMaxGain,
    ConfigField::LogMmseFloor,
    ConfigField::LogMmseNoiseOverestimation,
    ConfigField::McraSmoothing,
    ConfigField::McraSpeechSmoothing,
    ConfigField::McraNoiseSmoothing,
    ConfigField::McraRatioThreshold,
    ConfigField::McraWindowFrames,
    ConfigField::McraFloor,
    ConfigField::MinimumSmoothing,
    ConfigField::MinimumWindowFrames,
    ConfigField::MinimumFloor,
    ConfigField::TonalLocalRadius,
    ConfigField::TonalStartDb,
    ConfigField::TonalFullDb,
    ConfigField::TonalFluxStartDb,
    ConfigField::TonalFluxFullDb,
    ConfigField::TonalMovementSearchRadius,
    ConfigField::TonalMovementStartBins,
    ConfigField::TonalMovementFullBins,
    ConfigField::TonalMinFrequencyBin,
    ConfigField::TonalMovementMinRelativePower,
    ConfigField::TonalHarmonicToleranceBins,
    ConfigField::TonalHarmonicRelativePower,
    ConfigField::TonalStrength,
    ConfigField::TonalMinGain,
    ConfigField::TonalAttack,
    ConfigField::TonalRelease,
    ConfigField::TonalSpreadRadius,
    ConfigField::TonalFloor,
];

/// One `Constraint` with its ABI description and bounds.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ConstraintInfo {
    pub(crate) constraint: Constraint,
    /// Equal to the `Display` text of `constraint`, which the unit tests check.
    pub(crate) description: &'static str,
    pub(crate) lower: f32,
    pub(crate) upper: f32,
    pub(crate) lower_inclusive: bool,
    pub(crate) upper_inclusive: bool,
}

/// Every `Constraint`; the ABI identifier of `CONSTRAINTS[i]` is `i + 1`. The unit tests check each description
/// against `Display` and each bound against what `CallEnhancer::new` admits.
pub(crate) const CONSTRAINTS: [ConstraintInfo; 8] = [
    ConstraintInfo {
        constraint: Constraint::OpenUnit,
        description: "in (0, 1)",
        lower: 0.0,
        upper: 1.0,
        lower_inclusive: false,
        upper_inclusive: false,
    },
    ConstraintInfo {
        constraint: Constraint::UnitExcludingOne,
        description: "in [0, 1)",
        lower: 0.0,
        upper: 1.0,
        lower_inclusive: true,
        upper_inclusive: false,
    },
    ConstraintInfo {
        constraint: Constraint::UnitExcludingZero,
        description: "in (0, 1]",
        lower: 0.0,
        upper: 1.0,
        lower_inclusive: false,
        upper_inclusive: true,
    },
    ConstraintInfo {
        constraint: Constraint::ClosedUnit,
        description: "in [0, 1]",
        lower: 0.0,
        upper: 1.0,
        lower_inclusive: true,
        upper_inclusive: true,
    },
    ConstraintInfo {
        constraint: Constraint::Positive,
        description: "finite and greater than 0",
        lower: 0.0,
        upper: f32::INFINITY,
        lower_inclusive: false,
        upper_inclusive: false,
    },
    ConstraintInfo {
        constraint: Constraint::NonNegative,
        description: "finite and at least 0",
        lower: 0.0,
        upper: f32::INFINITY,
        lower_inclusive: true,
        upper_inclusive: false,
    },
    ConstraintInfo {
        constraint: Constraint::GreaterThanOne,
        description: "finite and greater than 1",
        lower: 1.0,
        upper: f32::INFINITY,
        lower_inclusive: false,
        upper_inclusive: false,
    },
    ConstraintInfo {
        constraint: Constraint::BelowNyquist,
        description: "in (0, 4000) Hz",
        lower: 0.0,
        upper: 4000.0,
        lower_inclusive: false,
        upper_inclusive: false,
    },
];

/// Message of [`NOX_STATUS_DRAINED`], equal to the `Display` text of `StreamError::Drained`.
const DRAINED_MESSAGE: &str = "the call has been drained; reset the enhancer to start a new call";

/// The static message of a status.
pub(crate) fn status_message(status: u32) -> &'static str {
    match status {
        NOX_STATUS_OK => "success",
        NOX_STATUS_NULL_ARGUMENT => "a required pointer argument was null",
        NOX_STATUS_INVALID_CONFIG => "the call configuration is invalid",
        NOX_STATUS_DRAINED => DRAINED_MESSAGE,
        NOX_STATUS_PANIC => "the enhancer panicked; the call handle is unusable and must be freed",
        _ => "unknown status",
    }
}

/// The status of a Rust lifecycle error. `StreamError::Drained` is the only one; the enum is `#[non_exhaustive]`, and a
/// variant added later would also end the stream, so every value maps to [`NOX_STATUS_DRAINED`].
pub(crate) const fn stream_status(_error: StreamError) -> u32 {
    NOX_STATUS_DRAINED
}

/// ABI identifier of `field`.
pub(crate) fn field_id(field: ConfigField) -> u32 {
    FIELDS.iter().position(|&candidate| candidate == field).map_or(NOX_FIELD_OTHER, index_to_id)
}

/// The field with ABI identifier `id`.
pub(crate) fn field_from_id(id: u32) -> Option<ConfigField> {
    id_to_index(id).and_then(|index| FIELDS.get(index)).copied()
}

/// ABI identifier of `constraint`.
pub(crate) fn constraint_id(constraint: Constraint) -> u32 {
    CONSTRAINTS.iter().position(|info| info.constraint == constraint).map_or(NOX_CONSTRAINT_OTHER, index_to_id)
}

/// The constraint with ABI identifier `id`.
pub(crate) fn constraint_from_id(id: u32) -> Option<&'static ConstraintInfo> {
    id_to_index(id).and_then(|index| CONSTRAINTS.get(index))
}

fn index_to_id(index: usize) -> u32 {
    // Both tables are far shorter than `u32::MAX`; the fallback is unreachable.
    u32::try_from(index + 1).unwrap_or(u32::MAX)
}

fn id_to_index(id: u32) -> Option<usize> {
    usize::try_from(id).ok()?.checked_sub(1)
}

/// The detail of a rejected configuration, carrying every datum of `error`.
pub(crate) fn config_error_detail(error: &ConfigError) -> NoxError {
    let mut detail = NoxError::status(NOX_STATUS_INVALID_CONFIG);
    match *error {
        ConfigError::OutOfRange { field, value, constraint } => {
            detail.config_error = NOX_CONFIG_ERROR_OUT_OF_RANGE;
            detail.field = field_id(field);
            detail.value = value;
            detail.constraint = constraint_id(constraint);
            if let Some(info) = constraint_from_id(detail.constraint) {
                detail.constraint_lower = info.lower;
                detail.constraint_upper = info.upper;
                detail.constraint_lower_inclusive = u8::from(info.lower_inclusive);
                detail.constraint_upper_inclusive = u8::from(info.upper_inclusive);
            }
        }
        ConfigError::MinGainAboveMaxGain { min_gain, max_gain } => {
            detail.config_error = NOX_CONFIG_ERROR_MIN_GAIN_ABOVE_MAX_GAIN;
            detail.value = min_gain;
            detail.second_value = max_gain;
        }
        ConfigError::CountOutOfRange { field, value, min, max } => {
            detail.config_error = NOX_CONFIG_ERROR_COUNT_OUT_OF_RANGE;
            detail.field = field_id(field);
            detail.count_value = value;
            detail.count_min = min;
            detail.count_max = max;
        }
        ConfigError::StartNotBelowFull { start, start_value, full, full_value } => {
            detail.config_error = NOX_CONFIG_ERROR_START_NOT_BELOW_FULL;
            detail.field = field_id(start);
            detail.value = start_value;
            detail.second_field = field_id(full);
            detail.second_value = full_value;
        }
        ConfigError::CalibrationTooLong { duration } => {
            detail.config_error = NOX_CONFIG_ERROR_CALIBRATION_TOO_LONG;
            detail.duration_secs = duration.as_secs();
            detail.duration_subsec_nanos = duration.subsec_nanos();
        }
        _ => detail.config_error = NOX_CONFIG_ERROR_OTHER,
    }
    detail
}

/// The detail of a configuration whose selector names no variant.
pub(crate) fn unknown_selector_detail(unknown: UnknownSelector) -> NoxError {
    let mut detail = NoxError::status(NOX_STATUS_INVALID_CONFIG);
    detail.config_error = NOX_CONFIG_ERROR_UNKNOWN_SELECTOR;
    detail.selector = unknown.selector;
    detail.selector_value = unknown.value;
    detail
}

/// The Rust `ConfigError` that `detail` describes, when it describes one completely.
pub(crate) fn config_error_from_detail(detail: &NoxError) -> Option<ConfigError> {
    let field = || field_from_id(detail.field);
    match detail.config_error {
        NOX_CONFIG_ERROR_OUT_OF_RANGE => Some(ConfigError::OutOfRange {
            field: field()?,
            value: detail.value,
            constraint: constraint_from_id(detail.constraint)?.constraint,
        }),
        NOX_CONFIG_ERROR_MIN_GAIN_ABOVE_MAX_GAIN => {
            Some(ConfigError::MinGainAboveMaxGain { min_gain: detail.value, max_gain: detail.second_value })
        }
        NOX_CONFIG_ERROR_COUNT_OUT_OF_RANGE => Some(ConfigError::CountOutOfRange {
            field: field()?,
            value: detail.count_value,
            min: detail.count_min,
            max: detail.count_max,
        }),
        NOX_CONFIG_ERROR_START_NOT_BELOW_FULL => Some(ConfigError::StartNotBelowFull {
            start: field()?,
            start_value: detail.value,
            full: field_from_id(detail.second_field)?,
            full_value: detail.second_value,
        }),
        NOX_CONFIG_ERROR_CALIBRATION_TOO_LONG => Some(ConfigError::CalibrationTooLong {
            duration: Duration::from_secs(detail.duration_secs)
                .checked_add(Duration::from_nanos(u64::from(detail.duration_subsec_nanos)))?,
        }),
        _ => None,
    }
}

/// Writes the message of `detail`: the `Display` text of the Rust error for a configuration error, the status
/// message otherwise.
pub(crate) fn write_message(detail: &NoxError, output: &mut impl Write) -> fmt::Result {
    if detail.status != NOX_STATUS_INVALID_CONFIG {
        return output.write_str(status_message(detail.status));
    }
    if let Some(error) = config_error_from_detail(detail) {
        return write!(output, "{error}");
    }
    match detail.config_error {
        NOX_CONFIG_ERROR_UNKNOWN_SELECTOR => {
            let name = match detail.selector {
                NOX_SELECTOR_NOISE_ESTIMATOR => "noise_estimator",
                NOX_SELECTOR_INTERFERENCE => "interference",
                _ => "a selector",
            };
            write!(output, "{name} must name a variant, got {}", detail.selector_value)
        }
        NOX_CONFIG_ERROR_OTHER => {
            output.write_str("the call configuration is invalid (an error this ABI version does not describe)")
        }
        _ => output.write_str("the call configuration is invalid (the error detail is not recognized)"),
    }
}

/// A [`Write`] target over a byte buffer that stores what fits and counts every byte written.
pub(crate) struct CountingBuffer<'buffer> {
    buffer: &'buffer mut [u8],
    written: usize,
}

impl<'buffer> CountingBuffer<'buffer> {
    pub(crate) fn new(buffer: &'buffer mut [u8]) -> Self {
        Self { buffer, written: 0 }
    }

    /// Bytes written in total, including those that did not fit.
    pub(crate) fn written(&self) -> usize {
        self.written
    }
}

impl Write for CountingBuffer<'_> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let bytes = text.as_bytes();
        if let Some(room) = self.buffer.get_mut(self.written..) {
            let stored = room.len().min(bytes.len());
            room[..stored].copy_from_slice(&bytes[..stored]);
        }
        self.written += bytes.len();
        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/unit/error.rs"]
mod tests;
