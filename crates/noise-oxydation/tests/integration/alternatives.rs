//! Every noise estimator and interference combination runs through the same packet path with the same timing,
//! reset and output guarantees.

use std::time::Duration;

use noise_oxydation::{CallPhase, DELAY_PACKETS, PACKET_SAMPLES, PacketOutcome};

use crate::support::{
    all_configurations, decode, decoded_high_passed, describe, enhancer, noise_packets, output_energy, run_call,
    signal_energy,
};

#[test]
fn every_combination_keeps_the_packet_timing_contract() {
    let input = noise_packets(130, 21, 70);
    for config in all_configurations(Duration::from_millis(200)) {
        for packets in [1, 2, 3, 7, 12, 13, 64, 130] {
            let mut call = enhancer(&config);
            let run = run_call(&mut call, &input[..packets]);
            let priming = packets.min(DELAY_PACKETS);
            let context = format!("{} with {packets} packets", describe(&config));
            assert!(run.outcomes[..priming].iter().all(|&outcome| outcome == PacketOutcome::Priming), "{context}");
            assert!(run.outcomes[priming..].iter().all(|&outcome| outcome == PacketOutcome::Emitted), "{context}");
            assert_eq!(run.drained, priming, "{context}");
            assert_eq!(run.output.len(), packets * PACKET_SAMPLES, "{context}");
            assert_eq!(call.phase(), CallPhase::Drained, "{context}");
        }
    }
}

#[test]
fn every_combination_resets_to_a_fresh_instance() {
    let previous_call = noise_packets(200, 31, 90);
    let next_call = noise_packets(180, 32, 60);
    for config in all_configurations(Duration::from_millis(300)) {
        let fresh = run_call(&mut enhancer(&config), &next_call).output;

        let mut drained = enhancer(&config);
        run_call(&mut drained, &previous_call);
        drained.reset();
        assert_eq!(run_call(&mut drained, &next_call).output, fresh, "{} after drain", describe(&config));

        let mut interrupted = enhancer(&config);
        let mut output = [0; PACKET_SAMPLES];
        for packet in &previous_call[..97] {
            interrupted.process_packet(packet, &mut output).expect("streaming");
        }
        interrupted.reset();
        assert_eq!(run_call(&mut interrupted, &next_call).output, fresh, "{} mid-call", describe(&config));
    }
}

/// Noise whose level jumps for a second exercises tracking in both directions. No enhanced packet has more than twice
/// the pass-through energy of its neighborhood (overlap-add smears level changes across packet edges, but gains never
/// exceed 1), and no packet collapses to digital silence: gains are at least `0.05 · 0.5`, so a non-finite gain (which quantizes to 0) would show up as a
/// silent packet.
#[test]
fn every_combination_produces_bounded_non_silent_output() {
    let quiet = noise_packets(200, 41, 40);
    let loud = noise_packets(50, 42, 110);
    let input: Vec<_> = quiet[..100].iter().chain(&loud).chain(&quiet[100..]).copied().collect();
    let pass_through = decoded_high_passed(&input);
    for config in all_configurations(Duration::from_millis(500)) {
        let run = run_call(&mut enhancer(&config), &input);
        let context = describe(&config);
        // Calibration covers samples below 4000; from packet 26 on every contributing frame is enhanced.
        for packet in 26..input.len() - 1 {
            let range = packet * PACKET_SAMPLES..(packet + 1) * PACKET_SAMPLES;
            let samples = &run.output[range.clone()];
            assert!(samples.iter().any(|&byte| decode(byte) != 0.0), "{context}: packet {packet} is silent");
            let around =
                range.start.saturating_sub(PACKET_SAMPLES)..(range.end + PACKET_SAMPLES).min(input.len() * 160);
            let ratio = output_energy(&run.output, range) / signal_energy(&pass_through, around);
            assert!(ratio < 2.0, "{context}: packet {packet} energy ratio {ratio}");
        }
        let total = output_energy(&run.output, 4000..pass_through.len())
            / signal_energy(&pass_through, 4000..pass_through.len());
        assert!(total < 1.0, "{context}: overall energy ratio {total}");
    }
}

/// After a 1 s calibration on stationary noise, every estimator attenuates the same noise, with and without
/// interference suppression.
#[test]
fn every_estimator_attenuates_stationary_noise_after_calibration() {
    let input = noise_packets(400, 51, 64);
    let pass_through = decoded_high_passed(&input);
    for config in all_configurations(Duration::from_secs(1)) {
        let run = run_call(&mut enhancer(&config), &input);
        // Skip one second after calibration so that every estimator has settled.
        let window = 16_000..60_000;
        let ratio = output_energy(&run.output, window.clone()) / signal_energy(&pass_through, window);
        let attenuation_db = -10.0 * ratio.log10();
        assert!(attenuation_db > 3.0, "{}: {attenuation_db:.2} dB", describe(&config));
    }
}
