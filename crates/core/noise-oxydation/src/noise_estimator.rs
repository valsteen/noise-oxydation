//! The call's noise power estimator, selected by [`NoiseEstimatorConfig`] and dispatched statically.
//!
//! Every estimator follows the same calibration contract: calibration frames accumulate the floored `f64` mean power
//! and expose it as the estimate; the first non-calibration frame starts adaptive tracking from that mean, or from its
//! own floored power when the call has no calibration frame.

use crate::{config::NoiseEstimatorConfig, geometry::BINS, mcra::Mcra, minimum::MinimumEstimator, spp_mmse::SppMmse};

/// MCRA keeps five spectra plus the calibration sums, about 1.5 KB more than the other estimators, so its state lives
/// in one fixed-size block allocated at construction instead of widening every call's enum to its size.
#[derive(Debug, Clone)]
pub(crate) enum NoiseEstimator {
    SppMmse(SppMmse),
    Mcra(Box<Mcra>),
    Minimum(MinimumEstimator),
}

impl NoiseEstimator {
    /// Creates the configured estimator, sizing all of its storage.
    pub(crate) fn new(config: NoiseEstimatorConfig) -> Self {
        match config {
            NoiseEstimatorConfig::SppMmse(config) => Self::SppMmse(SppMmse::new(config)),
            NoiseEstimatorConfig::Mcra(config) => Self::Mcra(Box::new(Mcra::new(config))),
            NoiseEstimatorConfig::Minimum(config) => Self::Minimum(MinimumEstimator::new(config)),
        }
    }

    /// Updates the noise estimate with one frame's power spectrum and returns it. `calibration` marks a calibration
    /// frame; the first frame with `calibration == false` ends calibration for the rest of the call.
    pub(crate) fn estimate(&mut self, power: &[f32; BINS], calibration: bool) -> &[f32; BINS] {
        match self {
            Self::SppMmse(estimator) => estimator.estimate(power, calibration),
            Self::Mcra(estimator) => estimator.estimate(power, calibration),
            Self::Minimum(estimator) => estimator.estimate(power, calibration),
        }
    }

    /// Restores the state of a new call without allocating.
    pub(crate) fn reset(&mut self) {
        match self {
            Self::SppMmse(estimator) => estimator.reset(),
            Self::Mcra(estimator) => estimator.reset(),
            Self::Minimum(estimator) => estimator.reset(),
        }
    }
}
