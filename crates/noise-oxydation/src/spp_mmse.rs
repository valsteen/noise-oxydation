//! Speech-presence-probability MMSE noise power estimator with quiet-intro calibration.
//!
//! During calibration frames the estimate is the running mean of the observed power per bin (accumulated in `f64`).
//! After calibration each bin is updated from the previous estimate `N` and the observed power `P`:
//!
//! ```text
//! γ = P / N
//! p = 1 / (1 + ((1 − q) / q)·(1 + ξH1)·exp(−γ·ξH1 / (1 + ξH1)))      (f64; non-finite → 0; clamped to [0, 1])
//! p̄ = αP·p̄ + (1 − αP)·p
//! p = pmax                    when p̄ > stagnation threshold and p > pmax
//! N_mmse = (1 − p)·P + p·N
//! N = αN·N + (1 − αN)·N_mmse
//! ```
//!
//! `P` and `N` are sanitized to the floor (values below it, NaN and infinities become the floor). When calibration
//! produced no frame, the first adaptive frame initializes `N = P`.

use crate::{config::SppMmseConfig, convert::narrow, geometry::BINS};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Accumulating the calibration mean (possibly zero frames so far).
    Calibrating,
    /// Adaptive tracking from the current estimate.
    Tracking,
}

#[derive(Debug, Clone)]
pub(crate) struct SppMmse {
    config: SppMmseConfig,
    noise: [f32; BINS],
    smoothed_probability: [f32; BINS],
    calibration_sum: [f64; BINS],
    calibration_frames: f64,
    mode: Mode,
}

impl SppMmse {
    pub(crate) fn new(config: SppMmseConfig) -> Self {
        Self {
            config,
            noise: [0.0; BINS],
            smoothed_probability: [0.0; BINS],
            calibration_sum: [0.0; BINS],
            calibration_frames: 0.0,
            mode: Mode::Calibrating,
        }
    }

    /// Updates the noise estimate with one frame's power spectrum and returns it.
    ///
    /// `calibration` selects the calibration mean for this frame. The first frame with `calibration == false` ends
    /// calibration for the rest of the call.
    pub(crate) fn estimate(&mut self, power: &[f32; BINS], calibration: bool) -> &[f32; BINS] {
        match (self.mode, calibration) {
            (Mode::Calibrating, true) => self.accumulate(power),
            (Mode::Calibrating, false) if self.calibration_frames < 1.0 => self.initialize(power),
            (Mode::Calibrating, false) => {
                self.mode = Mode::Tracking;
                self.track(power);
            }
            (Mode::Tracking, _) => self.track(power),
        }
        &self.noise
    }

    pub(crate) fn reset(&mut self) {
        self.noise.fill(0.0);
        self.smoothed_probability.fill(0.0);
        self.calibration_sum.fill(0.0);
        self.calibration_frames = 0.0;
        self.mode = Mode::Calibrating;
    }

    fn accumulate(&mut self, power: &[f32; BINS]) {
        let floor = self.config.floor;
        self.calibration_frames += 1.0;
        for ((sum, noise), &observed) in self.calibration_sum.iter_mut().zip(&mut self.noise).zip(power) {
            *sum += f64::from(sanitize(observed, floor));
            *noise = sanitize(narrow(*sum / self.calibration_frames), floor);
        }
    }

    fn initialize(&mut self, power: &[f32; BINS]) {
        let floor = self.config.floor;
        for (noise, &observed) in self.noise.iter_mut().zip(power) {
            *noise = sanitize(observed, floor);
        }
        self.mode = Mode::Tracking;
    }

    fn track(&mut self, power: &[f32; BINS]) {
        let SppMmseConfig {
            noise_smoothing, spp_smoothing, stagnation_threshold, max_speech_probability, floor, ..
        } = self.config;
        for ((noise, smoothed), &observed) in self.noise.iter_mut().zip(&mut self.smoothed_probability).zip(power) {
            let observed = sanitize(observed, floor);
            let previous = sanitize(*noise, floor);
            let mut probability = speech_presence_probability(&self.config, observed / previous);
            *smoothed = spp_smoothing * *smoothed + (1.0 - spp_smoothing) * probability;
            if *smoothed > stagnation_threshold && probability > max_speech_probability {
                probability = max_speech_probability;
            }
            let conditional = (1.0 - probability) * observed + probability * previous;
            *noise = sanitize(noise_smoothing * previous + (1.0 - noise_smoothing) * conditional, floor);
        }
    }
}

/// Posterior speech presence probability for a posteriori SNR `gamma`, with a fixed a priori SNR under speech.
fn speech_presence_probability(config: &SppMmseConfig, gamma: f32) -> f32 {
    let prior = f64::from(config.speech_prior);
    let snr = f64::from(config.fixed_prior_snr);
    let likelihood_ratio = ((1.0 - prior) / prior) * (1.0 + snr) * (-f64::from(gamma) * snr / (1.0 + snr)).exp();
    let probability = 1.0 / (1.0 + likelihood_ratio);
    if probability.is_finite() { narrow(probability.clamp(0.0, 1.0)) } else { 0.0 }
}

/// Replaces values below `floor`, NaN and infinities with `floor`.
fn sanitize(value: f32, floor: f32) -> f32 {
    if value.is_finite() && value >= floor { value } else { floor }
}

#[cfg(test)]
#[path = "../tests/unit/spp_mmse.rs"]
mod tests;
