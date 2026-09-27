use std::{thread, time::Duration};

use super::{Stage, StageClock, StageTimings};

#[test]
fn accumulation_counts_sums_and_keeps_the_maximum_per_stage() {
    let mut timings = StageTimings::default();
    for micros in [3, 11, 5] {
        timings.add(Stage::LogMmse, Duration::from_micros(micros));
    }
    timings.add(Stage::Analysis, Duration::from_nanos(7));

    let log_mmse = timings.get(Stage::LogMmse);
    assert_eq!(log_mmse.invocations, 3);
    assert_eq!(log_mmse.total, Duration::from_micros(19));
    assert_eq!(log_mmse.max, Duration::from_micros(11));
    let analysis = timings.get(Stage::Analysis);
    assert_eq!(
        (analysis.invocations, analysis.total, analysis.max),
        (1, Duration::from_nanos(7), Duration::from_nanos(7))
    );
    for stage in Stage::ALL.into_iter().filter(|stage| ![Stage::LogMmse, Stage::Analysis].contains(stage)) {
        assert_eq!(timings.get(stage), super::StageTiming::default(), "{}", stage.name());
    }
}

#[test]
fn totals_saturate_instead_of_overflowing() {
    let mut timings = StageTimings::default();
    timings.add(Stage::Synthesis, Duration::MAX);
    timings.add(Stage::Synthesis, Duration::from_secs(1));
    assert_eq!(timings.get(Stage::Synthesis).total, Duration::MAX);
    assert_eq!(timings.get(Stage::Synthesis).invocations, 2);
}

#[test]
fn the_clock_measures_elapsed_time_from_the_mark_and_chains_stages() {
    let mut clock = StageClock::default();
    let start = StageClock::mark();
    thread::sleep(Duration::from_millis(2));
    let next = clock.record(Stage::DecodeHighPass, start);
    clock.record(Stage::Analysis, next);

    let timings = clock.snapshot();
    let first = timings.get(Stage::DecodeHighPass);
    assert_eq!(first.invocations, 1);
    assert!(first.max >= Duration::from_millis(2) && first.total == first.max, "{first:?}");
    let second = timings.get(Stage::Analysis);
    assert_eq!(second.invocations, 1);
    assert!(second.max < first.max, "the second stage starts where the first ended: {second:?}");

    clock.reset();
    assert_eq!(clock.snapshot(), StageTimings::default());
}
