//! The exported functions. Each one checks its pointers for null, converts them to references under the documented
//! caller contract, and runs the Rust operation inside a panic guard.

use std::{ptr, slice};

use noise_oxydation::{CallEnhancer, CallPhase, DELAY_PACKETS, Packet, PacketOutcome};

use crate::{
    abi::{
        NOX_OUTCOME_EMITTED, NOX_OUTCOME_PRIMING, NOX_PHASE_CALIBRATING, NOX_PHASE_DRAINED, NOX_PHASE_ENHANCING,
        NOX_STATUS_NULL_ARGUMENT, NOX_STATUS_OK, NOX_STATUS_PANIC, NoxConfig, NoxError, NoxStr,
    },
    config::{default_config, to_call_config},
    error::{
        CountingBuffer, config_error_detail, constraint_from_id, field_from_id, status_message, stream_status,
        unknown_selector_detail, write_message,
    },
    guard::{GuardedEnhancer, guard},
};

/// Opaque handle of one call's enhancer, created by [`nox_call_new`] and released by [`nox_call_free`].
#[derive(Debug)]
pub struct NoxCall {
    inner: GuardedEnhancer,
}

/// Fills `config` with the default configuration: `CallConfig::default()`, with the Rust defaults of the MCRA and
/// minimum estimator parameters that it does not select.
///
/// Returns `NOX_STATUS_OK`, or `NOX_STATUS_NULL_ARGUMENT` when `config` is null.
///
/// # Safety
///
/// `config` is null or points to writable, aligned storage for one `nox_config`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nox_config_default(config: *mut NoxConfig) -> u32 {
    if config.is_null() {
        return NOX_STATUS_NULL_ARGUMENT;
    }
    let value = guard(None, || Some(default_config()));
    let Some(value) = value else { return NOX_STATUS_PANIC };
    // SAFETY: `config` is non-null, and the caller guarantees it is writable and aligned for one `nox_config`.
    unsafe { config.write(value) };
    NOX_STATUS_OK
}

/// Validates `config` and creates a call handle.
///
/// On success writes the handle to `*call` and returns `NOX_STATUS_OK`. Otherwise writes null to `*call` (when `call`
/// is not null) and returns `NOX_STATUS_NULL_ARGUMENT`, `NOX_STATUS_INVALID_CONFIG` or `NOX_STATUS_PANIC`. When `error`
/// is not null it always receives the detail of the outcome; for `NOX_STATUS_INVALID_CONFIG` the detail carries the
/// kind and every datum of the rejection.
///
/// This is the only function that allocates: the handle and the enhancer's construction.
///
/// # Safety
///
/// `config` is null or points to a readable, aligned `nox_config`; `call` is null or points to writable, aligned
/// storage for one pointer; `error` is null or points to writable, aligned storage for one `nox_error`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nox_call_new(config: *const NoxConfig, call: *mut *mut NoxCall, error: *mut NoxError) -> u32 {
    if !call.is_null() {
        // SAFETY: `call` is non-null, and the caller guarantees it is writable and aligned for one pointer.
        unsafe { call.write(ptr::null_mut()) };
    }
    let outcome = if config.is_null() || call.is_null() {
        Err(NoxError::status(NOX_STATUS_NULL_ARGUMENT))
    } else {
        // SAFETY: `config` is non-null, and the caller guarantees it points to a readable, aligned `nox_config`.
        // `NoxConfig` is `Copy` plain data, so any bit pattern of its integer and float fields is a valid value.
        let config = unsafe { config.read() };
        guard(Err(NoxError::status(NOX_STATUS_PANIC)), || create(&config))
    };
    let (status, detail) = match outcome {
        Ok(handle) => {
            // SAFETY: this branch runs only when `call` is non-null (checked above); the caller guarantees it is
            // writable and aligned for one pointer.
            unsafe { call.write(Box::into_raw(handle)) };
            (NOX_STATUS_OK, NoxError::status(NOX_STATUS_OK))
        }
        Err(detail) => (detail.status, detail),
    };
    if !error.is_null() {
        // SAFETY: `error` is non-null, and the caller guarantees it is writable and aligned for one `nox_error`.
        unsafe { error.write(detail) };
    }
    status
}

fn create(config: &NoxConfig) -> Result<Box<NoxCall>, NoxError> {
    let config = to_call_config(config).map_err(unknown_selector_detail)?;
    let enhancer = CallEnhancer::new(&config).map_err(|error| config_error_detail(&error))?;
    Ok(Box::new(NoxCall { inner: GuardedEnhancer::new(enhancer) }))
}

/// Processes one 160-byte input packet. After the first two packets of a call, writes one enhanced packet to `output`
/// and `NOX_OUTCOME_EMITTED` to `*outcome`; for the first two it writes `NOX_OUTCOME_PRIMING` and leaves `output`
/// untouched.
///
/// Returns `NOX_STATUS_OK`, `NOX_STATUS_DRAINED` after [`nox_call_drain`] until [`nox_call_reset`],
/// `NOX_STATUS_NULL_ARGUMENT` or `NOX_STATUS_PANIC`; `*outcome` is written only with `NOX_STATUS_OK`. Allocates
/// nothing.
///
/// # Safety
///
/// `call` is null or a live handle from [`nox_call_new`] that no other thread uses during the call. `input` is null or
/// points to 160 readable bytes, `output` is null or points to 160 writable bytes, and `outcome` is null or points to a
/// writable, aligned `uint32_t`. `input` may overlap `output`: the input is copied before the output is written.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nox_call_process(
    call: *mut NoxCall,
    input: *const u8,
    output: *mut u8,
    outcome: *mut u32,
) -> u32 {
    if call.is_null() || input.is_null() || output.is_null() || outcome.is_null() {
        return NOX_STATUS_NULL_ARGUMENT;
    }
    // SAFETY: `input` is non-null and points to 160 readable bytes; `Packet` is `[u8; 160]` with alignment 1. The
    // bytes are copied, so no reference to the input outlives this statement.
    let packet: Packet = unsafe { input.cast::<Packet>().read() };
    // SAFETY: `output` is non-null and points to 160 writable bytes with alignment 1. No other reference to them exists
    // in this function: the input was copied above.
    let output = unsafe { &mut *output.cast::<Packet>() };
    // SAFETY: `call` is non-null, is a live handle, and no other thread uses it, so this is the only reference to it.
    let call = unsafe { &mut *call };
    let mut result = NOX_OUTCOME_PRIMING;
    let status = call.inner.run(|enhancer| match enhancer.process_packet(&packet, output) {
        Ok(PacketOutcome::Priming) => NOX_STATUS_OK,
        Ok(PacketOutcome::Emitted) => {
            result = NOX_OUTCOME_EMITTED;
            NOX_STATUS_OK
        }
        Err(error) => stream_status(error),
    });
    if status == NOX_STATUS_OK {
        // SAFETY: `outcome` is non-null, and the caller guarantees it is writable and aligned for one `uint32_t`.
        unsafe { outcome.write(result) };
    }
    status
}

/// Ends the call: writes the withheld packets (at most two) to the front of the 320-byte `output` and their number to
/// `*written`. The call's total output then equals its input length.
///
/// Returns `NOX_STATUS_OK`, `NOX_STATUS_DRAINED` if the call was already drained, `NOX_STATUS_NULL_ARGUMENT` or
/// `NOX_STATUS_PANIC`; `*written` is written only with `NOX_STATUS_OK`. Allocates nothing.
///
/// # Safety
///
/// `call` is null or a live handle from [`nox_call_new`] that no other thread uses during the call. `output` is null or
/// points to 320 writable bytes, and `written` is null or points to a writable, aligned `size_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nox_call_drain(call: *mut NoxCall, output: *mut u8, written: *mut usize) -> u32 {
    if call.is_null() || output.is_null() || written.is_null() {
        return NOX_STATUS_NULL_ARGUMENT;
    }
    // SAFETY: `output` is non-null and points to 320 writable bytes, the size of `[[u8; 160]; 2]`, whose alignment
    // is 1. No other reference to them exists in this function.
    let output = unsafe { &mut *output.cast::<[Packet; DELAY_PACKETS]>() };
    // SAFETY: `call` is non-null, is a live handle, and no other thread uses it, so this is the only reference to it.
    let call = unsafe { &mut *call };
    let mut count = 0;
    let status = call.inner.run(|enhancer| match enhancer.drain(output) {
        Ok(packets) => {
            count = packets;
            NOX_STATUS_OK
        }
        Err(error) => stream_status(error),
    });
    if status == NOX_STATUS_OK {
        // SAFETY: `written` is non-null, and the caller guarantees it is writable and aligned for one `size_t`.
        unsafe { written.write(count) };
    }
    status
}

/// Starts a new call on the handle: discards buffered input and withheld output and restores the initial phase.
///
/// Returns `NOX_STATUS_OK`, `NOX_STATUS_NULL_ARGUMENT` or `NOX_STATUS_PANIC`. Allocates nothing.
///
/// # Safety
///
/// `call` is null or a live handle from [`nox_call_new`] that no other thread uses during the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nox_call_reset(call: *mut NoxCall) -> u32 {
    if call.is_null() {
        return NOX_STATUS_NULL_ARGUMENT;
    }
    // SAFETY: `call` is non-null, is a live handle, and no other thread uses it, so this is the only reference to it.
    let call = unsafe { &mut *call };
    call.inner.run(|enhancer| {
        enhancer.reset();
        NOX_STATUS_OK
    })
}

/// Writes the call's phase (`NOX_PHASE_*`) to `*phase`.
///
/// Returns `NOX_STATUS_OK`, `NOX_STATUS_NULL_ARGUMENT` or `NOX_STATUS_PANIC`; `*phase` is written only with
/// `NOX_STATUS_OK`.
///
/// # Safety
///
/// `call` is null or a live handle from [`nox_call_new`] that no other thread mutates during the call. `phase` is null
/// or points to a writable, aligned `uint32_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nox_call_phase(call: *const NoxCall, phase: *mut u32) -> u32 {
    if call.is_null() || phase.is_null() {
        return NOX_STATUS_NULL_ARGUMENT;
    }
    // SAFETY: `call` is non-null and a live handle that no other thread mutates, so a shared reference is sound.
    let call = unsafe { &*call };
    let mut value = NOX_PHASE_CALIBRATING;
    let status = call.inner.read(|enhancer| {
        value = match enhancer.phase() {
            CallPhase::Calibrating => NOX_PHASE_CALIBRATING,
            CallPhase::Enhancing => NOX_PHASE_ENHANCING,
            CallPhase::Drained => NOX_PHASE_DRAINED,
        };
        NOX_STATUS_OK
    });
    if status == NOX_STATUS_OK {
        // SAFETY: `phase` is non-null, and the caller guarantees it is writable and aligned for one `uint32_t`.
        unsafe { phase.write(value) };
    }
    status
}

/// Releases a handle. Null is ignored.
///
/// # Safety
///
/// `call` is null or a live handle from [`nox_call_new`] that is not used again, by this or any other thread, and not
/// freed twice.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nox_call_free(call: *mut NoxCall) {
    if call.is_null() {
        return;
    }
    // SAFETY: `call` came from `Box::into_raw` in `nox_call_new`, is live, and the caller never uses it again, so
    // ownership returns to this `Box` exactly once.
    let handle = unsafe { Box::from_raw(call) };
    guard((), || drop(handle));
}

/// The static message of a status (`NOX_STATUS_*`); `"unknown status"` for other values.
#[unsafe(no_mangle)]
pub extern "C" fn nox_status_message(status: u32) -> NoxStr {
    guard(NoxStr::new(""), || NoxStr::new(status_message(status)))
}

/// The qualified name of a configuration field (`NOX_FIELD_*`), for example `spp_mmse.speech_prior`, taken from the
/// Rust `ConfigField::path`; empty for identifiers the ABI does not define.
#[unsafe(no_mangle)]
pub extern "C" fn nox_config_field_name(field: u32) -> NoxStr {
    guard(NoxStr::new(""), || NoxStr::new(field_from_id(field).map_or("", |field| field.path())))
}

/// The description of a constraint (`NOX_CONSTRAINT_*`), for example `in (0, 1)`, equal to the Rust `Constraint`
/// display text; empty for identifiers the ABI does not define.
#[unsafe(no_mangle)]
pub extern "C" fn nox_constraint_description(constraint: u32) -> NoxStr {
    guard(NoxStr::new(""), || NoxStr::new(constraint_from_id(constraint).map_or("", |info| info.description)))
}

/// Writes the message of an error detail to `buffer`, at most `capacity` bytes of UTF-8 without a terminating NUL, and
/// returns the full message length. When the result exceeds `capacity`, call again with a buffer of that length.
///
/// A configuration error's message is the Rust `ConfigError` display text; any other status gives its status
/// message. Returns 0 when `error` is null or the formatting panicked. Allocates nothing.
///
/// # Safety
///
/// `error` is null or points to a readable, aligned `nox_error`. `buffer` is null (then `capacity` is ignored) or points
/// to `capacity` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nox_error_message(error: *const NoxError, buffer: *mut u8, capacity: usize) -> usize {
    if error.is_null() {
        return 0;
    }
    // SAFETY: `error` is non-null, and the caller guarantees it points to a readable, aligned `nox_error`. `NoxError`
    // is `Copy` plain data, so any bit pattern of its integer and float fields is a valid value.
    let detail = unsafe { error.read() };
    let storage: &mut [u8] = if buffer.is_null() {
        &mut []
    } else {
        // SAFETY: `buffer` is non-null and points to `capacity` writable bytes that nothing else references during
        // this call.
        unsafe { slice::from_raw_parts_mut(buffer, capacity) }
    };
    guard(0, || {
        let mut output = CountingBuffer::new(storage);
        // Writing to `CountingBuffer` never fails; the `Display` implementations of the Rust errors do not either.
        match write_message(&detail, &mut output) {
            Ok(()) => output.written(),
            Err(_) => 0,
        }
    })
}
