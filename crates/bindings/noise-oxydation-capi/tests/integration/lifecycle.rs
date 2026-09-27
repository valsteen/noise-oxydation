//! Packet lifecycle through the C functions: priming, emission, drain, reset, phase, free and null arguments.

use std::{mem::MaybeUninit, ptr, time::Duration};

use noise_oxydation_capi::{
    NOX_OUTCOME_EMITTED, NOX_OUTCOME_PRIMING, NOX_PHASE_CALIBRATING, NOX_PHASE_DRAINED, NOX_PHASE_ENHANCING,
    NOX_STATUS_DRAINED, NOX_STATUS_NULL_ARGUMENT, NOX_STATUS_OK, NoxError, nox_call_drain, nox_call_free, nox_call_new,
    nox_call_phase, nox_call_process, nox_call_reset, nox_config_default, nox_error_message, nox_status_message,
};

use crate::support::{Handle, Packet, default_config, synthetic_packets};

#[test]
fn a_call_primes_emits_drains_and_resets() {
    let packets = synthetic_packets(10, 1);
    let mut handle = Handle::new(&default_config());
    assert_eq!(handle.phase(), (NOX_STATUS_OK, NOX_PHASE_CALIBRATING));

    let mut output: Packet = [0x55; 160];
    let mut outcomes = Vec::new();
    for packet in &packets {
        let (status, outcome) = handle.process(packet, &mut output);
        assert_eq!(status, NOX_STATUS_OK);
        outcomes.push(outcome);
    }
    assert_eq!(outcomes[..2], [NOX_OUTCOME_PRIMING; 2]);
    assert!(outcomes[2..].iter().all(|&outcome| outcome == NOX_OUTCOME_EMITTED));

    let mut tail = [[0; 160]; 2];
    assert_eq!(handle.drain(&mut tail), (NOX_STATUS_OK, 2));
    assert_eq!(handle.phase(), (NOX_STATUS_OK, NOX_PHASE_DRAINED));

    // After the drain only reset is accepted; the out-parameters are left untouched.
    let (status, outcome) = handle.process(&packets[0], &mut output);
    assert_eq!((status, outcome), (NOX_STATUS_DRAINED, 0));
    assert_eq!(handle.drain(&mut tail), (NOX_STATUS_DRAINED, usize::MAX));

    assert_eq!(handle.reset(), NOX_STATUS_OK);
    assert_eq!(handle.phase(), (NOX_STATUS_OK, NOX_PHASE_CALIBRATING));
    assert_eq!(handle.process(&packets[0], &mut output), (NOX_STATUS_OK, NOX_OUTCOME_PRIMING));
    assert_eq!(handle.drain(&mut tail), (NOX_STATUS_OK, 1));
}

#[test]
fn a_call_without_calibration_starts_enhancing_and_an_empty_drain_returns_nothing() {
    let mut config = default_config();
    config.calibration_duration_ns = 0;
    let mut handle = Handle::new(&config);
    assert_eq!(handle.phase(), (NOX_STATUS_OK, NOX_PHASE_ENHANCING));
    let mut tail = [[0; 160]; 2];
    assert_eq!(handle.drain(&mut tail), (NOX_STATUS_OK, 0));
}

#[test]
fn calibration_ends_at_the_configured_duration() {
    let mut config = default_config();
    config.calibration_duration_ns = u64::try_from(Duration::from_millis(100).as_nanos()).expect("fits");
    let mut handle = Handle::new(&config);
    let mut output = [0; 160];
    // 100 ms is 800 samples: frames 0..=4 end within it (frame 4 ends at 768), and frame 5, ending at 896, is the
    // first enhanced frame. It completes with the sixth packet (960 samples).
    for (index, packet) in synthetic_packets(6, 2).iter().enumerate() {
        assert_eq!(handle.phase(), (NOX_STATUS_OK, NOX_PHASE_CALIBRATING), "before packet {index}");
        assert_eq!(handle.process(packet, &mut output).0, NOX_STATUS_OK);
    }
    assert_eq!(handle.phase(), (NOX_STATUS_OK, NOX_PHASE_ENHANCING));
}

#[test]
fn input_may_overlap_output() {
    let packets = synthetic_packets(30, 3);
    let mut separate = Handle::new(&default_config());
    let in_place = Handle::new(&default_config());
    let mut output = [0; 160];
    for packet in &packets {
        let (status, outcome) = separate.process(packet, &mut output);
        assert_eq!(status, NOX_STATUS_OK);

        let mut buffer = *packet;
        let mut in_place_outcome = 0;
        // SAFETY: the handle is live and used by this thread only; `buffer` is one live 160-byte packet used as both
        // input and output, which the function permits.
        let in_place_status = unsafe {
            nox_call_process(in_place.raw(), buffer.as_ptr(), buffer.as_mut_ptr(), &raw mut in_place_outcome)
        };
        assert_eq!((in_place_status, in_place_outcome), (status, outcome));
        if outcome == NOX_OUTCOME_EMITTED {
            assert_eq!(buffer, output);
        }
    }
}

#[test]
fn null_arguments_are_rejected() {
    let config = default_config();
    let mut handle = Handle::new(&config);
    let input = [0xFF; 160];
    let mut output = [0; 160];
    let mut outcome = 0;
    let mut written = 0;
    let mut phase = 0;
    let mut tail = [[0_u8; 160]; 2];
    let call = handle.raw();
    let mut error = MaybeUninit::<NoxError>::uninit();

    // SAFETY: every non-null pointer below refers to live, aligned storage of the right type, and the handle is used
    // by this thread only. Each call passes at least one null pointer, which must be reported, not dereferenced.
    unsafe {
        assert_eq!(nox_config_default(ptr::null_mut()), NOX_STATUS_NULL_ARGUMENT);
        assert_eq!(nox_call_new(ptr::null(), ptr::null_mut(), ptr::null_mut()), NOX_STATUS_NULL_ARGUMENT);
        let mut created = call;
        assert_eq!(nox_call_new(ptr::null(), &raw mut created, error.as_mut_ptr()), NOX_STATUS_NULL_ARGUMENT);
        assert!(created.is_null(), "a failed creation writes a null handle");
        assert_eq!(error.assume_init_ref().status, NOX_STATUS_NULL_ARGUMENT);
        assert_eq!(nox_call_new(&raw const config, ptr::null_mut(), error.as_mut_ptr()), NOX_STATUS_NULL_ARGUMENT);

        assert_eq!(
            nox_call_process(ptr::null_mut(), input.as_ptr(), output.as_mut_ptr(), &raw mut outcome),
            NOX_STATUS_NULL_ARGUMENT
        );
        assert_eq!(
            nox_call_process(call, ptr::null(), output.as_mut_ptr(), &raw mut outcome),
            NOX_STATUS_NULL_ARGUMENT
        );
        assert_eq!(nox_call_process(call, input.as_ptr(), ptr::null_mut(), &raw mut outcome), NOX_STATUS_NULL_ARGUMENT);
        assert_eq!(
            nox_call_process(call, input.as_ptr(), output.as_mut_ptr(), ptr::null_mut()),
            NOX_STATUS_NULL_ARGUMENT
        );
        assert_eq!(
            nox_call_drain(ptr::null_mut(), tail.as_mut_ptr().cast(), &raw mut written),
            NOX_STATUS_NULL_ARGUMENT
        );
        assert_eq!(nox_call_drain(call, ptr::null_mut(), &raw mut written), NOX_STATUS_NULL_ARGUMENT);
        assert_eq!(nox_call_drain(call, tail.as_mut_ptr().cast(), ptr::null_mut()), NOX_STATUS_NULL_ARGUMENT);
        assert_eq!(nox_call_reset(ptr::null_mut()), NOX_STATUS_NULL_ARGUMENT);
        assert_eq!(nox_call_phase(ptr::null(), &raw mut phase), NOX_STATUS_NULL_ARGUMENT);
        assert_eq!(nox_call_phase(call, ptr::null_mut()), NOX_STATUS_NULL_ARGUMENT);
        nox_call_free(ptr::null_mut());
        assert_eq!(nox_error_message(ptr::null(), output.as_mut_ptr(), output.len()), 0);
    }

    // The rejected calls left the handle untouched: it still primes.
    assert_eq!(handle.process(&input, &mut output), (NOX_STATUS_OK, NOX_OUTCOME_PRIMING));
    assert_eq!(handle.phase(), (NOX_STATUS_OK, NOX_PHASE_CALIBRATING));
}

#[test]
fn status_messages_are_static_utf8() {
    for status in 0..=5 {
        let text = nox_status_message(status);
        assert!(!text.ptr.is_null());
        // SAFETY: `nox_status_message` returns `len` bytes of static UTF-8 at `ptr`.
        let bytes = unsafe { std::slice::from_raw_parts(text.ptr, text.len) };
        assert!(!std::str::from_utf8(bytes).expect("UTF-8").is_empty());
    }
}
