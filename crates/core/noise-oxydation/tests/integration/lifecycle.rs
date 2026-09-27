//! Packet timing contract: priming, one output per input, exact drain, ordered lossless output, lifecycle errors.

use std::time::Duration;

use noise_oxydation::{CallPhase, DELAY_PACKETS, PACKET_SAMPLES, PacketOutcome, StreamError};

use crate::support::{
    decoded_high_passed, enhancer_with_calibration, nearest_ordinal, noise_packets, ordinal, run_call,
};

const MAX_PACKETS: usize = 600;

/// With calibration covering the whole input and the drain frame, the output is the high-passed input, delayed and
/// re-encoded. Samples 0 and 1 are exempt: their squared-window weights are 0 and about 2.3e-8 (reference concern
/// U1); every other sample may differ by at most one μ-law level.
#[test]
fn every_packet_count_is_emitted_in_order_with_exact_length() {
    let input = noise_packets(MAX_PACKETS, 1, 80);
    let expected: Vec<i32> = decoded_high_passed(&input).into_iter().map(nearest_ordinal).collect();
    for packets in 1..=MAX_PACKETS {
        let mut enhancer = enhancer_with_calibration(Duration::from_secs(60));
        let run = run_call(&mut enhancer, &input[..packets]);

        let priming = packets.min(DELAY_PACKETS);
        assert!(run.outcomes[..priming].iter().all(|&outcome| outcome == PacketOutcome::Priming), "{packets}");
        assert!(run.outcomes[priming..].iter().all(|&outcome| outcome == PacketOutcome::Emitted), "{packets}");
        assert_eq!(run.drained, priming, "drain count for {packets} packets");
        assert_eq!(run.output.len(), packets * PACKET_SAMPLES, "total length for {packets} packets");
        assert_eq!(enhancer.phase(), CallPhase::Drained);

        for (index, (&byte, &level)) in run.output.iter().zip(&expected).enumerate().skip(2) {
            let difference = (ordinal(byte) - level).abs();
            assert!(difference <= 1, "{packets} packets, sample {index}: {difference} levels apart");
        }
    }
}

#[test]
fn draining_an_empty_call_returns_nothing() {
    let mut enhancer = enhancer_with_calibration(Duration::from_secs(5));
    let run = run_call(&mut enhancer, &[]);
    assert_eq!(run.drained, 0);
    assert!(run.output.is_empty());
    assert_eq!(enhancer.phase(), CallPhase::Drained);
}

#[test]
fn a_drained_call_rejects_packets_and_drains_until_reset() {
    let input = noise_packets(5, 2, 40);
    let mut enhancer = enhancer_with_calibration(Duration::from_secs(5));
    run_call(&mut enhancer, &input);

    let mut output = [0x55; PACKET_SAMPLES];
    assert_eq!(enhancer.process_packet(&input[0], &mut output), Err(StreamError::Drained));
    assert_eq!(output, [0x55; PACKET_SAMPLES], "a rejected packet writes nothing");
    let mut tail = [[0; PACKET_SAMPLES]; DELAY_PACKETS];
    assert_eq!(enhancer.drain(&mut tail), Err(StreamError::Drained));
    assert_eq!(enhancer.phase(), CallPhase::Drained);

    enhancer.reset();
    assert_eq!(enhancer.phase(), CallPhase::Calibrating);
    assert_eq!(enhancer.process_packet(&input[0], &mut output), Ok(PacketOutcome::Priming));
}
