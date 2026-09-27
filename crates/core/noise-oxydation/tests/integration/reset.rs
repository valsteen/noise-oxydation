//! Reset makes an instance indistinguishable from a freshly constructed one.

use std::time::Duration;

use noise_oxydation::PACKET_SAMPLES;

use crate::support::{enhancer_with_calibration, noise_packets, run_call};

#[test]
fn reset_after_drain_or_mid_call_matches_a_fresh_instance() {
    let calibration = Duration::from_millis(500);
    let previous_call = noise_packets(200, 5, 90);
    let next_call = noise_packets(250, 6, 70);
    let fresh = run_call(&mut enhancer_with_calibration(calibration), &next_call);

    let mut drained = enhancer_with_calibration(calibration);
    run_call(&mut drained, &previous_call);
    drained.reset();
    let initial_phase = drained.phase();
    assert_eq!(run_call(&mut drained, &next_call).output, fresh.output);

    let mut interrupted = enhancer_with_calibration(calibration);
    let mut output = [0; PACKET_SAMPLES];
    for packet in &previous_call[..137] {
        interrupted.process_packet(packet, &mut output).expect("streaming");
    }
    interrupted.reset();
    assert_eq!(interrupted.phase(), initial_phase);
    assert_eq!(run_call(&mut interrupted, &next_call).output, fresh.output);
}
