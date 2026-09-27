//! C ABI of the `noise-oxydation` per-call telephony enhancer.
//!
//! The crate is built as a static library and linked into Go binaries by the `noiseox` package in `go/`, whose
//! `noise_oxydation.h` is the single C declaration of this ABI. It mirrors the Rust per-call API:
//!
//! - [`nox_config_default`] fills a [`NoxConfig`] from the Rust `Default` implementations, so defaults have one source.
//! - [`nox_call_new`] validates a configuration and returns an opaque [`NoxCall`] handle, or a status plus a
//!   [`NoxError`] detail that carries the kind and every datum of the Rust `ConfigError`.
//! - [`nox_call_process`], [`nox_call_drain`], [`nox_call_reset`] and [`nox_call_phase`] run one call's packet
//!   lifecycle on caller-owned buffers; [`nox_call_free`] releases the handle.
//! - [`nox_status_message`], [`nox_config_field_name`], [`nox_constraint_description`] and [`nox_error_message`] return
//!   the Rust texts for statuses, fields, constraints and complete errors without allocating.
//!
//! Only [`nox_call_new`] allocates (the handle and the enhancer's construction). Every exported function catches
//! unwinding and reports [`NOX_STATUS_PANIC`] instead; a handle whose operation panicked is poisoned and answers
//! every later operation with the same status until it is freed.
//!
//! This is the one crate of the workspace that contains `unsafe` code outside the allocation-counting harnesses: the
//! raw-pointer handling of the exported functions in `ffi`. Every `unsafe` block states why it is sound.
#![deny(unsafe_op_in_unsafe_fn, clippy::undocumented_unsafe_blocks)]

mod abi;
mod config;
mod error;
mod ffi;
mod guard;

pub use crate::{
    abi::{
        NOX_CONFIG_ERROR_CALIBRATION_TOO_LONG, NOX_CONFIG_ERROR_COUNT_OUT_OF_RANGE,
        NOX_CONFIG_ERROR_MIN_GAIN_ABOVE_MAX_GAIN, NOX_CONFIG_ERROR_NONE, NOX_CONFIG_ERROR_OTHER,
        NOX_CONFIG_ERROR_OUT_OF_RANGE, NOX_CONFIG_ERROR_START_NOT_BELOW_FULL, NOX_CONFIG_ERROR_UNKNOWN_SELECTOR,
        NOX_CONSTRAINT_NONE, NOX_CONSTRAINT_OTHER, NOX_DELAY_PACKETS, NOX_FIELD_NONE, NOX_FIELD_OTHER,
        NOX_INTERFERENCE_DISABLED, NOX_INTERFERENCE_TONAL_TRANSIENT, NOX_NOISE_ESTIMATOR_MCRA,
        NOX_NOISE_ESTIMATOR_MINIMUM, NOX_NOISE_ESTIMATOR_SPP_MMSE, NOX_OUTCOME_EMITTED, NOX_OUTCOME_PRIMING,
        NOX_PACKET_BYTES, NOX_PHASE_CALIBRATING, NOX_PHASE_DRAINED, NOX_PHASE_ENHANCING, NOX_SELECTOR_INTERFERENCE,
        NOX_SELECTOR_NOISE_ESTIMATOR, NOX_SELECTOR_NONE, NOX_STATUS_DRAINED, NOX_STATUS_INVALID_CONFIG,
        NOX_STATUS_NULL_ARGUMENT, NOX_STATUS_OK, NOX_STATUS_PANIC, NoxConfig, NoxDecisionDirectedConfig, NoxError,
        NoxHighPassConfig, NoxLogMmseConfig, NoxMcraConfig, NoxMinimumConfig, NoxSppMmseConfig, NoxStr,
        NoxTonalTransientConfig,
    },
    ffi::{
        NoxCall, nox_call_drain, nox_call_free, nox_call_new, nox_call_phase, nox_call_process, nox_call_reset,
        nox_config_default, nox_config_field_name, nox_constraint_description, nox_error_message, nox_status_message,
    },
};
