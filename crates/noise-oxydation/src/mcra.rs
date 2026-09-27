//! Minimum-controlled recursive averaging (MCRA) noise power estimator with quiet-intro calibration.
//!
//! During calibration frames the estimate is the running mean of the observed power per bin (accumulated in `f64`).
//! At the first non-calibration frame the noise `N`, the smoothed power `S` and both minima start from that mean (or,
//! without any calibration frame, from that frame's floored power) and the speech probability `p` from 0. Every
//! non-calibration frame then updates each bin with the floored observed power `P`:
//!
//! ```text
//! S = αs·S + (1 − αs)·P
//! M_current = min(M_current, S)
//! R = S / max(min(M_previous, M_current), floor)
//! I = 1 if R > δ else 0
//! p = αp·p + (1 − αp)·I
//! a = αd + (1 − αd)·p
//! N = max(a·N + (1 − a)·P, floor)
//! ```
//!
//! After every `W` updated frames the current minimum becomes the previous minimum and the current minimum restarts
//! at `+∞`.

use crate::{
    config::McraConfig,
    geometry::BINS,
    power::{CalibrationMean, sanitize},
};

#[derive(Debug, Clone)]
pub(crate) struct Mcra {
    config: McraConfig,
    noise: [f32; BINS],
    smoothed: [f32; BINS],
    current_minimum: [f32; BINS],
    previous_minimum: [f32; BINS],
    speech_probability: [f32; BINS],
    calibration: CalibrationMean,
    frames_in_window: u16,
    tracking: bool,
}

impl Mcra {
    pub(crate) fn new(config: McraConfig) -> Self {
        Self {
            config,
            noise: [0.0; BINS],
            smoothed: [0.0; BINS],
            current_minimum: [0.0; BINS],
            previous_minimum: [0.0; BINS],
            speech_probability: [0.0; BINS],
            calibration: CalibrationMean::new(),
            frames_in_window: 0,
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
        self.current_minimum.fill(0.0);
        self.previous_minimum.fill(0.0);
        self.speech_probability.fill(0.0);
        self.calibration.reset();
        self.frames_in_window = 0;
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
        self.current_minimum = self.noise;
        self.previous_minimum = self.noise;
        self.speech_probability.fill(0.0);
        self.frames_in_window = 0;
        self.tracking = true;
    }

    fn track(&mut self, power: &[f32; BINS]) {
        let McraConfig { smoothing, speech_smoothing, noise_smoothing, ratio_threshold, window_frames, floor } =
            self.config;
        let minima = self.current_minimum.iter_mut().zip(&self.previous_minimum);
        let states = self.noise.iter_mut().zip(&mut self.smoothed).zip(&mut self.speech_probability);
        for (((noise, smoothed), probability), ((current_minimum, &previous_minimum), &observed)) in
            states.zip(minima.zip(power))
        {
            let observed = sanitize(observed, floor);
            *smoothed = smoothing * *smoothed + (1.0 - smoothing) * observed;
            *current_minimum = current_minimum.min(*smoothed);
            let ratio = *smoothed / previous_minimum.min(*current_minimum).max(floor);
            let indicator = if ratio > ratio_threshold { 1.0 } else { 0.0 };
            *probability = speech_smoothing * *probability + (1.0 - speech_smoothing) * indicator;
            let adaptive = noise_smoothing + (1.0 - noise_smoothing) * *probability;
            *noise = (adaptive * *noise + (1.0 - adaptive) * observed).max(floor);
        }
        self.frames_in_window += 1;
        if self.frames_in_window >= window_frames {
            self.previous_minimum = self.current_minimum;
            self.current_minimum.fill(f32::INFINITY);
            self.frames_in_window = 0;
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/mcra.rs"]
mod tests;
