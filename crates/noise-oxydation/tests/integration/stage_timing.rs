//! The `stage-timing` feature counts every stage invocation of a call, and `reset` clears the counts.

use std::time::Duration;

use noise_oxydation::{CallConfig, InterferenceConfig, PACKET_SAMPLES, Stage, StageTimings};

use crate::support::{all_configurations, enhancer, noise_packets, run_call};

/// Expected invocations per stage for `packets` whole packets, derived from the packet timing contract: frame `t`
/// covers samples `[128t, 128t + 256)`, `drain` analyzes one zero-padded frame and synthesizes the overlap tail, and
/// frames ending within the calibration duration are calibration frames.
fn expected_invocations(packets: u64, calibration_samples: u64, interference: bool) -> [(Stage, u64); 7] {
    let samples = packets * PACKET_SAMPLES as u64;
    let complete_frames = if samples >= 256 { (samples - 256) / 128 + 1 } else { 0 };
    let frames = complete_frames + 1;
    let calibration_frames = if calibration_samples >= 256 { (calibration_samples - 256) / 128 + 1 } else { 0 };
    let enhanced = frames - calibration_frames.min(frames);
    [
        (Stage::DecodeHighPass, packets),
        (Stage::Analysis, frames),
        (Stage::NoiseEstimation, frames),
        (Stage::TonalDetection, if interference { enhanced } else { 0 }),
        (Stage::LogMmse, enhanced),
        (Stage::Synthesis, frames + 1),
        (Stage::EncodeQueue, frames + 1),
    ]
}

#[test]
fn every_stage_invocation_is_counted_and_timed() {
    let input = noise_packets(400, 17, 80);
    for config in all_configurations(Duration::from_secs(1)) {
        let mut call = enhancer(&config);
        assert_eq!(call.stage_timings(), StageTimings::default(), "a new call starts with no timings");
        run_call(&mut call, &input);
        let timings = call.stage_timings();
        let interference = matches!(config.interference, InterferenceConfig::TonalTransient(_));
        for (stage, invocations) in expected_invocations(400, 8000, interference) {
            let timing = timings.get(stage);
            assert_eq!(timing.invocations, invocations, "{} with {config:?}", stage.name());
            assert!(timing.max <= timing.total, "{}: {timing:?}", stage.name());
            if invocations > 0 {
                assert!(timing.total > Duration::ZERO, "{} was timed", stage.name());
            }
        }
    }
}

#[test]
fn reset_clears_the_timings_of_the_previous_call() {
    let mut call = enhancer(&CallConfig::default());
    run_call(&mut call, &noise_packets(300, 3, 60));
    assert_ne!(call.stage_timings(), StageTimings::default());
    call.reset();
    assert_eq!(call.stage_timings(), StageTimings::default());

    run_call(&mut call, &noise_packets(10, 4, 60));
    for (stage, invocations) in expected_invocations(10, 40_000, true) {
        assert_eq!(call.stage_timings().get(stage).invocations, invocations, "{}", stage.name());
    }
}
