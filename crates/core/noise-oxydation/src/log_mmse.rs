//! Log-spectral-amplitude MMSE suppression driven by decision-directed SNR estimates.
//!
//! Per bin, with overestimated noise `N_eff = β·N` (negative or non-finite `N` → 0, reference documentation problem D8):
//!
//! ```text
//! v = max(γ·ξ / (1 + ξ), floor)
//! G = ξ / (1 + ξ) · exp(E1(v) / 2)      (f64; non-finite → 0; then clamped to [min_gain, max_gain])
//! Y = G·X,   S_hat = G²·P
//! ```
//!
//! `E1` is the exponential integral, evaluated with its power series for `x ≤ 1` and a continued fraction above.

use crate::{
    config::{DecisionDirectedConfig, LogMmseConfig},
    convert::narrow,
    decision_directed::DecisionDirected,
    fft::Complex,
    geometry::BINS,
};

#[derive(Debug, Clone)]
pub(crate) struct LogMmse {
    config: LogMmseConfig,
    snr: DecisionDirected,
    effective_noise: [f32; BINS],
    gamma: [f32; BINS],
    xi: [f32; BINS],
    clean_power: [f32; BINS],
}

impl LogMmse {
    pub(crate) fn new(config: LogMmseConfig, snr: DecisionDirectedConfig) -> Self {
        Self {
            config,
            snr: DecisionDirected::new(snr),
            effective_noise: [0.0; BINS],
            gamma: [0.0; BINS],
            xi: [0.0; BINS],
            clean_power: [0.0; BINS],
        }
    }

    /// Applies the suppression gain to `spectrum` in place, given its power and the noise estimate.
    pub(crate) fn apply(&mut self, spectrum: &mut [Complex; BINS], power: &[f32; BINS], noise: &[f32; BINS]) {
        let LogMmseConfig { min_gain, max_gain, floor, noise_overestimation } = self.config;
        for (effective, &noise) in self.effective_noise.iter_mut().zip(noise) {
            let noise = if noise.is_finite() && noise >= 0.0 { noise } else { 0.0 };
            *effective = noise * noise_overestimation;
        }
        self.snr.estimate(power, &self.effective_noise, &mut self.gamma, &mut self.xi);

        let estimates = self.gamma.iter().zip(&self.xi).zip(power);
        for ((bin, clean), ((&gamma, &xi), &observed)) in spectrum.iter_mut().zip(&mut self.clean_power).zip(estimates)
        {
            let gain = gain(gamma, xi, floor).clamp(min_gain, max_gain);
            *bin = bin.scale(gain);
            *clean = gain * gain * observed;
        }
        self.snr.update(&self.clean_power, &self.effective_noise);
    }

    pub(crate) fn reset(&mut self) {
        self.snr.reset();
    }
}

/// Unclamped Log-MMSE gain for a posteriori SNR `gamma` and a priori SNR `xi`.
pub(crate) fn gain(gamma: f32, xi: f32, floor: f32) -> f32 {
    let gamma = gamma.max(0.0);
    let xi = xi.max(0.0);
    let v = (gamma * xi / (1.0 + xi)).max(floor);
    let wiener = f64::from(xi / (1.0 + xi));
    let gain = wiener * (0.5 * exponential_integral(f64::from(v))).exp();
    if gain.is_finite() { narrow(gain) } else { 0.0 }
}

/// Exponential integral `E1(x) = ∫ₓ^∞ e^(−t)/t dt` for `x > 0`; `+∞` for `x ≤ 0`.
pub(crate) fn exponential_integral(x: f64) -> f64 {
    if x <= 0.0 {
        f64::INFINITY
    } else if x <= 1.0 {
        exponential_integral_series(x)
    } else {
        exponential_integral_continued_fraction(x)
    }
}

const E1_EPSILON: f64 = 1e-14;
const E1_MAX_TERMS: u32 = 100;

/// `E1(x) = −γ − ln x − Σ_{k≥1} (−x)^k / (k·k!)`, used for `0 < x ≤ 1`.
fn exponential_integral_series(x: f64) -> f64 {
    const EULER_GAMMA: f64 = 0.577_215_664_901_532_9;
    let mut sum = -x.ln() - EULER_GAMMA;
    let mut power_over_factorial = 1.0;
    let mut index = 0.0;
    for _ in 0..E1_MAX_TERMS {
        index += 1.0;
        power_over_factorial *= -x / index;
        let term = -power_over_factorial / index;
        sum += term;
        if term.abs() < sum.abs() * E1_EPSILON {
            break;
        }
    }
    sum
}

/// `E1(x) = e^(−x) / (x + 1 − 1² / (x + 3 − 2² / (x + 5 − …)))` evaluated with the modified Lentz method, used for
/// `x > 1`.
fn exponential_integral_continued_fraction(x: f64) -> f64 {
    const TINY: f64 = 1e-300;
    let mut denominator_term = x + 1.0;
    let mut numerator_ratio = 1.0 / TINY;
    let mut denominator_ratio = 1.0 / denominator_term;
    let mut fraction = denominator_ratio;
    let mut index = 0.0;
    for _ in 0..E1_MAX_TERMS {
        index += 1.0;
        let partial_numerator = -(index * index);
        denominator_term += 2.0;
        denominator_ratio = partial_numerator * denominator_ratio + denominator_term;
        if denominator_ratio.abs() < TINY {
            denominator_ratio = TINY;
        }
        numerator_ratio = denominator_term + partial_numerator / numerator_ratio;
        if numerator_ratio.abs() < TINY {
            numerator_ratio = TINY;
        }
        denominator_ratio = 1.0 / denominator_ratio;
        let delta = numerator_ratio * denominator_ratio;
        fraction *= delta;
        if (delta - 1.0).abs() < E1_EPSILON {
            break;
        }
    }
    fraction * (-x).exp()
}

#[cfg(test)]
#[path = "../tests/unit/log_mmse.rs"]
mod tests;
