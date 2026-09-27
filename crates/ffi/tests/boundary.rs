use noise_oxydation_ffi::{
    NoCall, no_create, no_create_with_mode, no_destroy, no_finish, no_process, no_reset,
};
use noise_oxydation_pipeline::{Config, NoiseEstimator, PacketBatch, Pipeline, ProcessingMode};
use std::ptr;

fn append(batch: &PacketBatch, bytes: &mut Vec<u8>) {
    for i in 0..batch.len() {
        let (packet, valid) = batch.packet(i).unwrap();
        bytes.extend_from_slice(&packet[..valid]);
    }
}

#[test]
fn invalid_arguments_do_not_advance_and_output_matches_pipeline() {
    for (selector, estimator) in [
        (0, NoiseEstimator::SppMmse),
        (1, NoiseEstimator::Mcra),
        (2, NoiseEstimator::Minimum),
    ] {
        let mut handle: *mut NoCall = ptr::null_mut();
        // SAFETY: All buffers and the handle are valid and used serially.
        unsafe {
            assert_eq!(
                no_create(1_000_000_000, selector, &raw mut handle).status,
                0
            );
            assert!(!handle.is_null());
            let mut output = [0; 320];
            let packet = [0x80; 160];
            assert_eq!(
                no_process(handle, ptr::null(), 160, output.as_mut_ptr(), 320).status,
                1
            );
            assert_eq!(
                no_process(handle, packet.as_ptr(), 159, output.as_mut_ptr(), 320).status,
                2
            );
            assert_eq!(
                no_process(handle, packet.as_ptr(), 160, output.as_mut_ptr(), 319).status,
                3
            );
            assert_eq!(no_finish(handle, output.as_mut_ptr(), 319).status, 3);
            assert_eq!(
                no_process(
                    ptr::null_mut(),
                    packet.as_ptr(),
                    160,
                    output.as_mut_ptr(),
                    320
                )
                .status,
                1
            );
            let mut direct = Pipeline::new(Config {
                learning_duration: std::time::Duration::from_secs(1),
                noise_estimator: estimator,
            })
            .unwrap();
            let mut expected = Vec::new();
            let mut actual = Vec::new();
            for i in 0..110 {
                let input = [0x80_u8.wrapping_add(i); 160];
                append(&direct.process_packet(&input).unwrap(), &mut expected);
                let result = no_process(handle, input.as_ptr(), 160, output.as_mut_ptr(), 320);
                assert_eq!(result.status, 0);
                for index in 0..result.count as usize {
                    let valid = if index + 1 == result.count as usize {
                        result.final_valid as usize
                    } else {
                        160
                    };
                    actual.extend_from_slice(&output[index * 160..index * 160 + valid]);
                }
            }
            append(&direct.finish().unwrap(), &mut expected);
            let result = no_finish(handle, output.as_mut_ptr(), 320);
            assert_eq!(result.status, 0);
            for index in 0..result.count as usize {
                let valid = if index + 1 == result.count as usize {
                    result.final_valid as usize
                } else {
                    160
                };
                actual.extend_from_slice(&output[index * 160..index * 160 + valid]);
            }
            assert_eq!(actual, expected);
            assert_eq!(actual.len(), 110 * 160);
            assert_eq!(no_finish(handle, output.as_mut_ptr(), 320).status, 7);
            assert_eq!(no_reset(handle).status, 0);
            assert_eq!(
                no_process(handle, [0xff; 160].as_ptr(), 160, output.as_mut_ptr(), 320).status,
                0
            );
            assert_eq!(no_destroy(handle).status, 0);
        }
    }
}

#[test]
fn constructor_preserves_error_cause() {
    let mut handle: *mut NoCall = ptr::null_mut();
    // SAFETY: The output pointer is valid.
    unsafe {
        assert_eq!(no_create(0, 0, &raw mut handle).status, 5);
        assert!(handle.is_null());
        let short = no_create(20_000_000, 0, &raw mut handle);
        assert_eq!((short.status, short.samples, short.minimum), (6, 160, 256));
        assert!(handle.is_null());
        assert_eq!(no_create(5_000_000_000, 3, &raw mut handle).status, 4);
        assert!(handle.is_null());
        assert_eq!(no_create(5_000_000_000, 0, ptr::null_mut()).status, 1);
        assert_eq!(
            no_create_with_mode(5_000_000_000, 0, 99, &raw mut handle).status,
            10
        );
        assert!(handle.is_null());
    }
}

#[test]
fn experimental_ffi_matches_direct_pipeline() {
    let mut handle: *mut NoCall = ptr::null_mut();
    // SAFETY: The handle and fixed buffers are valid and used serially.
    unsafe {
        assert_eq!(
            no_create_with_mode(1_000_000_000, 1, 1, &raw mut handle).status,
            0
        );
        let mut direct = Pipeline::new_with_mode(
            Config {
                learning_duration: std::time::Duration::from_secs(1),
                noise_estimator: NoiseEstimator::Mcra,
            },
            ProcessingMode::ExperimentalLowDelay,
        )
        .unwrap();
        let mut ffi_output = [0; 320];
        let mut expected = Vec::new();
        let mut actual = Vec::new();
        for index in 0..110 {
            let input = [u8::try_from(index).unwrap(); 160];
            append(&direct.process_packet(&input).unwrap(), &mut expected);
            let result = no_process(handle, input.as_ptr(), 160, ffi_output.as_mut_ptr(), 320);
            assert_eq!(result.status, 0);
            for packet in 0..result.count as usize {
                actual.extend_from_slice(&ffi_output[packet * 160..packet * 160 + 160]);
            }
        }
        append(&direct.finish().unwrap(), &mut expected);
        let result = no_finish(handle, ffi_output.as_mut_ptr(), 320);
        assert_eq!(result.status, 0);
        for packet in 0..result.count as usize {
            let valid = if packet + 1 == result.count as usize {
                result.final_valid as usize
            } else {
                160
            };
            actual.extend_from_slice(&ffi_output[packet * 160..packet * 160 + valid]);
        }
        assert_eq!(actual, expected);
        assert_eq!(no_destroy(handle).status, 0);
    }
}
