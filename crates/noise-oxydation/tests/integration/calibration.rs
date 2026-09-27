//! Quiet-intro calibration boundary and pass-through.

use std::time::Duration;

use noise_oxydation::{CallPhase, PACKET_SAMPLES};

use crate::support::{
    decoded_high_passed, enhancer_with_calibration, noise_packets, output_energy, run_call, signal_energy,
};

/// With the 5 s default, frames 0–310 end at or before sample 40 000 and are calibration frames. Frame 311 covers
/// samples 39 808–40 063 and needs packet 251 (1-based). Output samples 39 808–39 935 cross-fade between the last
/// pass-through frame and the first enhanced frame; from sample 39 936 on, every contributing frame is enhanced.
#[test]
fn enhancement_starts_at_the_first_frame_ending_after_five_seconds() {
    let input = noise_packets(320, 3, 64);
    let mut enhancer = enhancer_with_calibration(Duration::from_secs(5));
    let mut output = [0; PACKET_SAMPLES];
    for (count, packet) in (1..).zip(&input[..251]) {
        enhancer.process_packet(packet, &mut output).expect("streaming");
        let expected = if count <= 250 { CallPhase::Calibrating } else { CallPhase::Enhancing };
        assert_eq!(enhancer.phase(), expected, "after {count} packets");
    }

    let mut fresh = enhancer_with_calibration(Duration::from_secs(5));
    let run = run_call(&mut fresh, &input);
    let pass_through = decoded_high_passed(&input);

    let before = 20_000..39_000;
    let ratio_before = output_energy(&run.output, before.clone()) / signal_energy(&pass_through, before);
    assert!((ratio_before - 1.0).abs() < 0.05, "calibration output energy ratio {ratio_before}");

    let after = 39_936..47_936;
    let ratio_after = output_energy(&run.output, after.clone()) / signal_energy(&pass_through, after);
    assert!(ratio_after < 0.25, "enhanced output energy ratio {ratio_after}");
}

#[test]
fn zero_calibration_enhances_from_the_first_frame() {
    let input = noise_packets(100, 4, 64);
    let mut enhancer = enhancer_with_calibration(Duration::ZERO);
    assert_eq!(enhancer.phase(), CallPhase::Enhancing);
    let run = run_call(&mut enhancer, &input);
    let pass_through = decoded_high_passed(&input);
    let window = 256..16_000;
    let ratio = output_energy(&run.output, window.clone()) / signal_energy(&pass_through, window);
    assert!(ratio < 0.25, "enhanced output energy ratio {ratio}");
}

/// A duration shorter than one frame yields no calibration frame, so the call starts enhancing immediately.
#[test]
fn a_calibration_shorter_than_one_frame_is_skipped() {
    let enhancer = enhancer_with_calibration(Duration::from_micros(31_875));
    assert_eq!(enhancer.phase(), CallPhase::Enhancing);
    let enhancer = enhancer_with_calibration(Duration::from_millis(32));
    assert_eq!(enhancer.phase(), CallPhase::Calibrating);
}
