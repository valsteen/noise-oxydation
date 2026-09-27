//! Opt-in per-call stage timing (Cargo feature `stage-timing`, off by default).
//!
//! The call owns one [`StageClock`]. With the feature enabled it reads [`std::time::Instant`] at stage boundaries and
//! adds each stage's duration to a fixed array of accumulators inside the call: no allocation, locks, atomics or I/O.
//! Without the feature the clock and its marks are zero-sized and every method is empty, so the packet path compiles
//! to the same stage code and never reads the time.
//!
//! Aggregation across calls is the caller's job: [`StageTimings`] is a `Copy` snapshot that a monitoring or
//! evaluation tool combines after its call threads have finished.

#[cfg(feature = "stage-timing")]
use std::time::{Duration, Instant};

/// A processing stage of one call, in packet-path order.
///
/// Without the `stage-timing` feature this type is private to the crate and only keeps the call sites identical.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage {
    /// μ-law decoding and high-pass filtering of one input packet.
    DecodeHighPass,
    /// Windowing, forward FFT and power spectrum of one analysis frame.
    Analysis,
    /// The noise estimator's update for one frame, calibration frames included.
    NoiseEstimation,
    /// The tonal transient detector on one enhanced frame (only when interference suppression is enabled).
    TonalDetection,
    /// Decision-directed SNR and Log-MMSE gains for one enhanced frame, including applying the tonal gains.
    LogMmse,
    /// Inverse FFT and weighted overlap-add of one frame, or the synthesis tail at drain.
    Synthesis,
    /// μ-law encoding of the samples one frame (or the drain tail) finalizes into the output queue.
    EncodeQueue,
}

#[cfg(feature = "stage-timing")]
impl Stage {
    /// Every stage, in packet-path order.
    pub const ALL: [Self; 7] = [
        Self::DecodeHighPass,
        Self::Analysis,
        Self::NoiseEstimation,
        Self::TonalDetection,
        Self::LogMmse,
        Self::Synthesis,
        Self::EncodeQueue,
    ];

    /// Short human-readable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::DecodeHighPass => "decode + high-pass",
            Self::Analysis => "analysis",
            Self::NoiseEstimation => "noise estimation",
            Self::TonalDetection => "tonal detection",
            Self::LogMmse => "Log-MMSE",
            Self::Synthesis => "synthesis",
            Self::EncodeQueue => "encode + queue",
        }
    }

    const fn index(self) -> usize {
        self as usize
    }
}

/// Accumulated timing of one stage within one call.
#[cfg(feature = "stage-timing")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct StageTiming {
    /// How many times the stage ran.
    pub invocations: u64,
    /// Sum of the measured durations (saturating).
    pub total: Duration,
    /// Longest single measured duration.
    pub max: Duration,
}

/// Snapshot of every stage's accumulated timing for one call since construction or the last reset.
#[cfg(feature = "stage-timing")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct StageTimings {
    stages: [StageTiming; Stage::ALL.len()],
}

#[cfg(feature = "stage-timing")]
impl StageTimings {
    /// The accumulated timing of `stage`.
    #[must_use]
    pub const fn get(&self, stage: Stage) -> StageTiming {
        self.stages[stage.index()]
    }

    fn add(&mut self, stage: Stage, elapsed: Duration) {
        let timing = &mut self.stages[stage.index()];
        timing.invocations = timing.invocations.saturating_add(1);
        timing.total = timing.total.saturating_add(elapsed);
        timing.max = timing.max.max(elapsed);
    }
}

/// A point in time at a stage boundary (zero-sized without the `stage-timing` feature).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Mark {
    #[cfg(feature = "stage-timing")]
    at: Instant,
}

/// The per-call stage accumulators (zero-sized without the `stage-timing` feature).
#[derive(Debug, Clone, Default)]
pub(crate) struct StageClock {
    #[cfg(feature = "stage-timing")]
    timings: StageTimings,
}

impl StageClock {
    /// The current time, as the start of a stage.
    #[inline]
    pub(crate) fn mark() -> Mark {
        Mark {
            #[cfg(feature = "stage-timing")]
            at: Instant::now(),
        }
    }

    /// Records one invocation of `stage` that started at `start` and ends now, and returns now as the start of the
    /// next stage. Without the feature it records nothing and returns `start`.
    #[inline]
    pub(crate) fn record(&mut self, stage: Stage, start: Mark) -> Mark {
        #[cfg(feature = "stage-timing")]
        {
            let now = Instant::now();
            self.timings.add(stage, now.saturating_duration_since(start.at));
            Mark { at: now }
        }
        #[cfg(not(feature = "stage-timing"))]
        {
            let _ = (self, stage);
            start
        }
    }

    /// Clears every accumulator.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    #[cfg(feature = "stage-timing")]
    pub(crate) fn snapshot(&self) -> StageTimings {
        self.timings
    }
}

#[cfg(all(test, feature = "stage-timing"))]
#[path = "../tests/unit/stage_timing.rs"]
mod tests;
