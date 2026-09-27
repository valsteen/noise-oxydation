//! Decision-directed a priori SNR estimation.
//!
//! For observed power `P` and (effective) noise power `N` per bin:
//!
//! ```text
//! γ = max(P, 0) / max(N, floor)
//! ξ_inst = max(γ − 1, 0)
//! ξ = ξ_inst                                  on the first frame after construction or reset
//! ξ = α·ξ_clean_prev + (1 − α)·ξ_inst         afterwards
//! ```
//!
//! `ξ_clean_prev = max(S_hat, 0) / max(N, floor)` is stored from the previous frame's estimated clean power `S_hat`.

use crate::{config::DecisionDirectedConfig, geometry::BINS};

#[derive(Debug, Clone)]
pub(crate) struct DecisionDirected {
    config: DecisionDirectedConfig,
    previous_clean_snr: [f32; BINS],
    initialized: bool,
}

impl DecisionDirected {
    pub(crate) fn new(config: DecisionDirectedConfig) -> Self {
        Self { config, previous_clean_snr: [0.0; BINS], initialized: false }
    }

    /// Writes the a posteriori SNR `gamma` and the a priori SNR `xi` for one frame.
    pub(crate) fn estimate(
        &self,
        power: &[f32; BINS],
        noise: &[f32; BINS],
        gamma: &mut [f32; BINS],
        xi: &mut [f32; BINS],
    ) {
        let DecisionDirectedConfig { alpha, floor } = self.config;
        let bins = power.iter().zip(noise).zip(&self.previous_clean_snr);
        for ((gamma, xi), ((&observed, &noise), &previous)) in gamma.iter_mut().zip(xi.iter_mut()).zip(bins) {
            *gamma = observed.max(0.0) / noise.max(floor);
            let instantaneous = (*gamma - 1.0).max(0.0);
            *xi = if self.initialized { alpha * previous + (1.0 - alpha) * instantaneous } else { instantaneous };
        }
    }

    /// Stores the clean-speech SNR of the frame just processed for the next frame's recursion.
    pub(crate) fn update(&mut self, clean_power: &[f32; BINS], noise: &[f32; BINS]) {
        let floor = self.config.floor;
        for ((snr, &clean), &noise) in self.previous_clean_snr.iter_mut().zip(clean_power).zip(noise) {
            *snr = clean.max(0.0) / noise.max(floor);
        }
        self.initialized = true;
    }

    pub(crate) fn reset(&mut self) {
        self.previous_clean_snr.fill(0.0);
        self.initialized = false;
    }
}

#[cfg(test)]
#[path = "../tests/unit/decision_directed.rs"]
mod tests;
