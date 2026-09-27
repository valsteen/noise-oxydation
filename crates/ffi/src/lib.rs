//! C ABI for one Rust pipeline per call. Raw C callers serialize each handle.

use noise_oxydation_pipeline::{
    Config, NoiseEstimator, PACKET_SAMPLES, PacketBatch, Pipeline, PipelineError, ProcessingMode,
};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Duration;

const OK: u32 = 0;
const NULL: u32 = 1;
const INPUT_LENGTH: u32 = 2;
const OUTPUT_CAPACITY: u32 = 3;
const ESTIMATOR: u32 = 4;
const DURATION: u32 = 5;
const DSP_INIT: u32 = 6;
const FINISHED: u32 = 7;
const PANIC: u32 = 8;
const MODE: u32 = 10;
const OUTPUT_BYTES: usize = 2 * PACKET_SAMPLES;

/// Opaque to C. One owner must serialize access to a handle.
pub struct NoCall(Pipeline);

/// ABI result. `samples` and `minimum` carry the DSP interval cause.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NoResult {
    pub status: u32,
    pub count: u32,
    pub final_valid: u32,
    pub samples: u64,
    pub minimum: u64,
}

impl NoResult {
    const fn error(status: u32) -> Self {
        Self {
            status,
            count: 0,
            final_valid: 0,
            samples: 0,
            minimum: 0,
        }
    }

    fn from_pipeline(error: &PipelineError) -> Self {
        match error {
            PipelineError::InvalidLearningDuration => Self::error(DURATION),
            PipelineError::DspInitialization(
                noise_oxydation_pipeline::DspError::LearningIntervalTooShort { samples, minimum },
            ) => Self {
                status: DSP_INIT,
                samples: *samples,
                minimum: *minimum,
                ..Self::error(DSP_INIT)
            },
            PipelineError::AlreadyFinished => Self::error(FINISHED),
        }
    }
}

fn boundary(f: impl FnOnce() -> NoResult) -> NoResult {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| NoResult::error(PANIC))
}

#[allow(clippy::cast_possible_truncation)] // A batch has at most two 160-byte packets.
unsafe fn write_batch(batch: &PacketBatch, output: *mut u8) -> NoResult {
    let mut final_valid = 0;
    for index in 0..batch.len() {
        let (packet, valid) = batch.packet(index).expect("index is in batch");
        // SAFETY: The caller promised a valid 320-byte output buffer. This
        // pointer is used only during the call and never retained.
        unsafe { std::ptr::copy(packet.as_ptr(), output.add(index * PACKET_SAMPLES), valid) };
        final_valid = valid;
    }
    NoResult {
        status: OK,
        count: batch.len() as u32,
        final_valid: final_valid as u32,
        samples: 0,
        minimum: 0,
    }
}

/// Construct one call. `out` is set to null on construction failure.
///
/// # Safety
/// `out` must point to writable storage when non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn no_create(
    learning_duration_ns: u64,
    estimator: u32,
    out: *mut *mut NoCall,
) -> NoResult {
    // SAFETY: This forwards the same caller-owned out pointer and keeps the
    // legacy constructor on the conservative path.
    unsafe { no_create_with_mode(learning_duration_ns, estimator, 0, out) }
}

/// Construct one call with a selected processing mode (0 conservative,
/// 1 experimental lower delay). Existing `no_create` remains conservative.
///
/// # Safety
/// `out` must point to writable storage when non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn no_create_with_mode(
    learning_duration_ns: u64,
    estimator: u32,
    mode: u32,
    out: *mut *mut NoCall,
) -> NoResult {
    boundary(|| {
        if out.is_null() {
            return NoResult::error(NULL);
        }
        // SAFETY: Checked non-null; the caller promises writable storage.
        unsafe { *out = std::ptr::null_mut() };
        let noise_estimator = match estimator {
            0 => NoiseEstimator::SppMmse,
            1 => NoiseEstimator::Mcra,
            2 => NoiseEstimator::Minimum,
            _ => return NoResult::error(ESTIMATOR),
        };
        let mode = match mode {
            0 => ProcessingMode::Conservative,
            1 => ProcessingMode::ExperimentalLowDelay,
            _ => return NoResult::error(MODE),
        };
        let pipeline = match Pipeline::new_with_mode(
            Config {
                learning_duration: Duration::from_nanos(learning_duration_ns),
                noise_estimator,
            },
            mode,
        ) {
            Ok(pipeline) => pipeline,
            Err(error) => return NoResult::from_pipeline(&error),
        };
        // SAFETY: Checked non-null; the caller owns the returned handle.
        unsafe { *out = Box::into_raw(Box::new(NoCall(pipeline))) };
        NoResult::error(OK)
    })
}

/// Process exactly one 160-byte packet into a fixed-capacity output buffer.
///
/// # Safety
/// `call` must be a live handle used serially. Non-null buffers must be valid
/// for their lengths and must not overlap the handle. No pointer is retained.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn no_process(
    call: *mut NoCall,
    input: *const u8,
    input_len: usize,
    output: *mut u8,
    output_capacity: usize,
) -> NoResult {
    boundary(|| {
        if call.is_null() || input.is_null() || output.is_null() {
            return NoResult::error(NULL);
        }
        if input_len != PACKET_SAMPLES {
            return NoResult::error(INPUT_LENGTH);
        }
        if output_capacity < OUTPUT_BYTES {
            return NoResult::error(OUTPUT_CAPACITY);
        }
        // SAFETY: All argument checks precede mutation. The caller promises
        // valid storage and serial ownership of the live handle.
        let packet = unsafe { *(input.cast::<[u8; PACKET_SAMPLES]>()) };
        let pipeline = unsafe { &mut (*call).0 };
        match pipeline.process_packet(&packet) {
            Ok(batch) => unsafe { write_batch(&batch, output) },
            Err(error) => NoResult::from_pipeline(&error),
        }
    })
}

/// Drain final valid output once.
///
/// # Safety
/// `call` must be a live handle used serially. `output` must be valid for at
/// least `output_capacity` bytes and must not overlap the handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn no_finish(
    call: *mut NoCall,
    output: *mut u8,
    output_capacity: usize,
) -> NoResult {
    boundary(|| {
        if call.is_null() || output.is_null() {
            return NoResult::error(NULL);
        }
        if output_capacity < OUTPUT_BYTES {
            return NoResult::error(OUTPUT_CAPACITY);
        }
        // SAFETY: The caller promises valid output and serial ownership.
        match unsafe { &mut (*call).0 }.finish() {
            Ok(batch) => unsafe { write_batch(&batch, output) },
            Err(error) => NoResult::from_pipeline(&error),
        }
    })
}

/// Clear all audio state while preserving configuration.
///
/// # Safety
/// `call` must be a live handle used serially.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn no_reset(call: *mut NoCall) -> NoResult {
    boundary(|| {
        if call.is_null() {
            return NoResult::error(NULL);
        }
        // SAFETY: The caller promises a live, serially used handle.
        unsafe { (*call).0.reset() };
        NoResult::error(OK)
    })
}

/// Free a live handle. Do not use it again.
///
/// # Safety
/// `call` must be a live handle used serially, created by `no_create` and not
/// previously destroyed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn no_destroy(call: *mut NoCall) -> NoResult {
    boundary(|| {
        if call.is_null() {
            return NoResult::error(NULL);
        }
        // SAFETY: The caller promises unique ownership of a live handle.
        unsafe { drop(Box::from_raw(call)) };
        NoResult::error(OK)
    })
}
