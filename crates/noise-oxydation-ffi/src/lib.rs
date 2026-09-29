//! Static C ABI for the Rust packet processor.
//!
//! The opaque handle owns one core processor. Callers own packet and output
//! buffers and must use one handle in packet order from one thread at a time.

use std::slice;

use noise_oxydation_core::{PACKET_BYTES, Processor, ProcessorError};

/// Stable status values declared in `include/noise_oxydation.h`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NoiseOxydationStatus {
    Ok = 0,
    InvalidArgument = 1,
    InvalidPacketLength = 2,
    OutputTooSmall = 3,
    StreamDrained = 4,
    StreamTooLong = 5,
    InternalError = 6,
}

/// Opaque C handle. Its contents remain private to Rust.
pub struct NoiseOxydationProcessor(Processor);

/// Creates one independent processor.
///
/// # Safety
/// The returned handle must be released exactly once with
/// [`noise_oxydation_processor_free`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn noise_oxydation_processor_new() -> *mut NoiseOxydationProcessor {
    Box::into_raw(Box::new(NoiseOxydationProcessor(Processor::new())))
}

/// Releases a handle. Passing null is a no-op.
///
/// # Safety
/// A non-null pointer must have been returned by
/// [`noise_oxydation_processor_new`] and not previously freed. No concurrent
/// operation may use it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn noise_oxydation_processor_free(processor: *mut NoiseOxydationProcessor) {
    if !processor.is_null() {
        // SAFETY: the caller contract gives this function unique ownership of
        // a live handle allocated by `noise_oxydation_processor_new`.
        drop(unsafe { Box::from_raw(processor) });
    }
}

/// Pushes one exact 160-byte μ-law packet and writes ordered output.
///
/// `written` is required. On an output-capacity rejection it receives the
/// exact capacity needed, and the processor has not consumed the packet.
///
/// # Safety
/// `processor` must be a live handle not used concurrently. `packet` must
/// address `packet_length` readable bytes. When `output_capacity` is nonzero,
/// `output` must address that many writable bytes. `written` must address one
/// writable `size_t`. The packet is copied before output is written, so the
/// input and output regions may overlap. `written` and the handle must not
/// overlap any buffer, and the buffers must not overlap the handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn noise_oxydation_processor_push(
    processor: *mut NoiseOxydationProcessor,
    packet: *const u8,
    packet_length: usize,
    output: *mut u8,
    output_capacity: usize,
    written: *mut usize,
) -> NoiseOxydationStatus {
    if processor.is_null() || written.is_null() || (packet_length > 0 && packet.is_null()) {
        return NoiseOxydationStatus::InvalidArgument;
    }
    // SAFETY: `written` is validated non-null and the caller guarantees it is writable.
    unsafe { *written = 0 };
    if packet_length != PACKET_BYTES {
        return NoiseOxydationStatus::InvalidPacketLength;
    }
    if output_capacity > 0 && output.is_null() {
        return NoiseOxydationStatus::InvalidArgument;
    }

    let mut packet_copy = [0; PACKET_BYTES];
    // SAFETY: packet length is now exactly the fixed readable packet size.
    packet_copy.copy_from_slice(unsafe { slice::from_raw_parts(packet, PACKET_BYTES) });
    let output_slice = if output_capacity == 0 {
        &mut []
    } else {
        // SAFETY: the caller guarantees this region is writable for capacity bytes.
        unsafe { slice::from_raw_parts_mut(output, output_capacity) }
    };

    // SAFETY: the caller guarantees the live, exclusively borrowed handle.
    let processor = unsafe { &mut *processor };
    match processor.0.push_packet(&packet_copy, output_slice) {
        Ok(size) => {
            // SAFETY: `written` is a validated writable output pointer.
            unsafe { *written = size };
            NoiseOxydationStatus::Ok
        }
        Err(ProcessorError::OutputTooSmall { required }) => {
            // SAFETY: `written` is a validated writable output pointer.
            unsafe { *written = required };
            NoiseOxydationStatus::OutputTooSmall
        }
        Err(ProcessorError::StreamDrained) => NoiseOxydationStatus::StreamDrained,
        Err(ProcessorError::StreamTooLong) => NoiseOxydationStatus::StreamTooLong,
        Err(ProcessorError::CalibrationDurationTooLong) => NoiseOxydationStatus::InternalError,
    }
}

/// Drains the final ordered samples and closes the stream.
///
/// # Safety
/// `processor` must be a live handle not used concurrently. When
/// `output_capacity` is nonzero, `output` must address that many writable
/// bytes. `written` must address one writable `size_t`; it must not overlap
/// the handle or output buffer, and the output buffer must not overlap the
/// handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn noise_oxydation_processor_drain(
    processor: *mut NoiseOxydationProcessor,
    output: *mut u8,
    output_capacity: usize,
    written: *mut usize,
) -> NoiseOxydationStatus {
    if processor.is_null() || written.is_null() || (output_capacity > 0 && output.is_null()) {
        return NoiseOxydationStatus::InvalidArgument;
    }
    // SAFETY: `written` is validated non-null and the caller guarantees it is writable.
    unsafe { *written = 0 };
    let output_slice = if output_capacity == 0 {
        &mut []
    } else {
        // SAFETY: the caller guarantees this region is writable for capacity bytes.
        unsafe { slice::from_raw_parts_mut(output, output_capacity) }
    };
    // SAFETY: the caller guarantees the live, exclusively borrowed handle.
    let processor = unsafe { &mut *processor };
    match processor.0.drain(output_slice) {
        Ok(size) => {
            // SAFETY: `written` is a validated writable output pointer.
            unsafe { *written = size };
            NoiseOxydationStatus::Ok
        }
        Err(ProcessorError::OutputTooSmall { required }) => {
            // SAFETY: `written` is a validated writable output pointer.
            unsafe { *written = required };
            NoiseOxydationStatus::OutputTooSmall
        }
        Err(
            ProcessorError::StreamDrained | ProcessorError::StreamTooLong | ProcessorError::CalibrationDurationTooLong,
        ) => NoiseOxydationStatus::InternalError,
    }
}

/// Resets one live processor and discards its pending tail.
///
/// # Safety
/// `processor` must be a live handle not used concurrently.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn noise_oxydation_processor_reset(
    processor: *mut NoiseOxydationProcessor,
) -> NoiseOxydationStatus {
    if processor.is_null() {
        return NoiseOxydationStatus::InvalidArgument;
    }
    // SAFETY: the caller guarantees the live, exclusively borrowed handle.
    unsafe { &mut *processor }.0.reset();
    NoiseOxydationStatus::Ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffi_capacity_rejection_preserves_packet_and_drain_state() {
        // SAFETY: each handle is used serially and freed once.
        let processor = unsafe { noise_oxydation_processor_new() };
        assert!(!processor.is_null());
        let packet = [0xff; PACKET_BYTES];
        let mut output = [0; 256];
        let mut written = 0;
        // SAFETY: pointers and lengths describe valid local buffers.
        assert_eq!(
            unsafe {
                noise_oxydation_processor_push(
                    processor,
                    packet.as_ptr(),
                    packet.len(),
                    output.as_mut_ptr(),
                    output.len(),
                    &raw mut written,
                )
            },
            NoiseOxydationStatus::Ok
        );
        let mut second = [0; 127];
        // SAFETY: pointers and lengths describe valid local buffers.
        assert_eq!(
            unsafe {
                noise_oxydation_processor_push(
                    processor,
                    packet.as_ptr(),
                    packet.len(),
                    second.as_mut_ptr(),
                    second.len(),
                    &raw mut written,
                )
            },
            NoiseOxydationStatus::OutputTooSmall
        );
        assert_eq!(written, 128);
        let mut second = [0; 128];
        // SAFETY: pointers and lengths describe valid local buffers.
        assert_eq!(
            unsafe {
                noise_oxydation_processor_push(
                    processor,
                    packet.as_ptr(),
                    packet.len(),
                    second.as_mut_ptr(),
                    second.len(),
                    &raw mut written,
                )
            },
            NoiseOxydationStatus::Ok
        );
        assert_eq!(written, 128);
        let mut short_tail = [0; 191];
        // SAFETY: pointers and lengths describe valid local buffers.
        assert_eq!(
            unsafe {
                noise_oxydation_processor_drain(processor, short_tail.as_mut_ptr(), short_tail.len(), &raw mut written)
            },
            NoiseOxydationStatus::OutputTooSmall
        );
        assert_eq!(written, 192);
        // SAFETY: the same handle remains live and is used serially.
        unsafe { noise_oxydation_processor_free(processor) };
    }

    #[test]
    fn wrong_packet_length_is_rejected_without_processing() {
        // SAFETY: each handle is used serially and freed once.
        let processor = unsafe { noise_oxydation_processor_new() };
        let packet = [0xff; PACKET_BYTES - 1];
        let mut output = [0; 256];
        let mut written = usize::MAX;
        // SAFETY: pointers and lengths describe valid local buffers.
        assert_eq!(
            unsafe {
                noise_oxydation_processor_push(
                    processor,
                    packet.as_ptr(),
                    packet.len(),
                    output.as_mut_ptr(),
                    output.len(),
                    &raw mut written,
                )
            },
            NoiseOxydationStatus::InvalidPacketLength
        );
        assert_eq!(written, 0);
        // SAFETY: the same handle remains live and is freed once.
        unsafe { noise_oxydation_processor_free(processor) };
    }

    #[test]
    fn freeing_null_is_a_noop() {
        // SAFETY: null is explicitly accepted.
        unsafe { noise_oxydation_processor_free(std::ptr::null_mut()) };
    }
}
