//! Minimum noise power estimator: the exact minimum of the smoothed power over the most recent `W` frames.
//!
//! During calibration frames the estimate is the running mean of the observed power per bin (accumulated in `f64`).
//! At the first non-calibration frame the smoothed power `S` and every one of the `W` history slots start from that
//! mean (or, without any calibration frame, from that frame's floored power). Every non-calibration frame then updates
//! each bin with the floored observed power `P`:
//!
//! ```text
//! S = αs·S + (1 − αs)·P
//! N = max(min of S over the most recent W frames, including this one, floor)
//! ```
//!
//! The history is the only buffer sized at construction from configuration (`W` frames of 129 bins); it is reused for
//! the whole life of the enhancer.

use crate::{
    config::MinimumConfig,
    geometry::BINS,
    power::{CalibrationMean, sanitize},
};

#[derive(Debug, Clone)]
pub(crate) struct MinimumEstimator {
    config: MinimumConfig,
    noise: [f32; BINS],
    smoothed: [f32; BINS],
    /// Smoothed power of the most recent `W` frames, written cyclically at `next_slot`.
    history: Box<[[f32; BINS]]>,
    next_slot: usize,
    calibration: CalibrationMean,
    tracking: bool,
}

impl MinimumEstimator {
    pub(crate) fn new(config: MinimumConfig) -> Self {
        Self {
            config,
            noise: [0.0; BINS],
            smoothed: [0.0; BINS],
            history: vec![[0.0; BINS]; usize::from(config.window_frames)].into_boxed_slice(),
            next_slot: 0,
            calibration: CalibrationMean::new(),
            tracking: false,
        }
    }

    /// Updates the noise estimate with one frame's power spectrum and returns it.
    ///
    /// `calibration` selects the calibration mean for this frame. The first frame with `calibration == false` ends
    /// calibration for the rest of the call.
    pub(crate) fn estimate(&mut self, power: &[f32; BINS], calibration: bool) -> &[f32; BINS] {
        if !self.tracking {
            if calibration {
                self.calibration.accumulate(power, self.config.floor, &mut self.noise);
                return &self.noise;
            }
            self.start_tracking(power);
        }
        self.track(power);
        &self.noise
    }

    pub(crate) fn reset(&mut self) {
        self.noise.fill(0.0);
        self.smoothed.fill(0.0);
        for frame in &mut self.history {
            frame.fill(0.0);
        }
        self.next_slot = 0;
        self.calibration.reset();
        self.tracking = false;
    }

    /// Initializes tracking from the calibration mean already held in `noise`, or from the floored `power` of this
    /// first frame when no calibration frame exists.
    fn start_tracking(&mut self, power: &[f32; BINS]) {
        if self.calibration.is_empty() {
            for (noise, &observed) in self.noise.iter_mut().zip(power) {
                *noise = sanitize(observed, self.config.floor);
            }
        }
        self.smoothed = self.noise;
        self.history.fill(self.noise);
        self.next_slot = 0;
        self.tracking = true;
    }

    fn track(&mut self, power: &[f32; BINS]) {
        let MinimumConfig { smoothing, floor, .. } = self.config;
        for (smoothed, &observed) in self.smoothed.iter_mut().zip(power) {
            *smoothed = smoothing * *smoothed + (1.0 - smoothing) * sanitize(observed, floor);
        }
        self.history[self.next_slot] = self.smoothed;
        self.next_slot = (self.next_slot + 1) % self.history.len();

        self.noise = self.smoothed;
        for frame in &self.history {
            for (noise, &smoothed) in self.noise.iter_mut().zip(frame) {
                *noise = noise.min(smoothed);
            }
        }
        for noise in &mut self.noise {
            *noise = noise.max(floor);
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/minimum.rs"]
mod tests;
