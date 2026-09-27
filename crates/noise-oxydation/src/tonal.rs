//! Tonal transient detector: per-bin attenuation of narrow foreground tones that appear suddenly, move, or persist,
//! while protecting harmonically supported (voiced) peaks.
//!
//! It runs on the original power spectrum of every enhanced frame (never on calibration frames). The first frame of a
//! call only stores its power and yields unit gains. For each later frame, every local peak `k` in
//! `max(local_radius, min_frequency_bin) ..= 128 − local_radius` whose sanitized power `c` is not exceeded by any bin
//! within `local_radius` is scored:
//!
//! ```text
//! T = 10·log10(c / max(mean power at offsets 3..=6 on both sides, floor))       tonal prominence, dB
//! F = max(10·log10(c / previous[k]), 0)                                          positive spectral flux, dB
//! M = normalize(|k − k_prev|, movement_start_bins, movement_full_bins)           0 when the previous peak is weak
//! H = max over k/2, k/3, k/4, 2k, 3k, 4k of clamp(local max / (ρ·c), 0, 1)       harmonic support
//! S = clamp(normalize(T)·max(normalize(F), M, 0.35·normalize(T))·(1 − H), 0, 1)
//! target = max(min_gain, 1 − strength·S), spread to ±spread_radius with linearly decreasing weight
//! ```
//!
//! Each bin's gain then moves toward its target with `attack` (falling) or `release` (rising) smoothing and is
//! clamped to `[min_gain, 1]`.

use crate::{config::TonalTransientConfig, convert::narrow, geometry::BINS, power::sanitize};

/// Bins on each side of a peak excluded from its background because the Hann main lobe spreads a tone across them.
const BACKGROUND_GUARD_BINS: usize = 2;
/// Outermost offset on each side of a peak included in its background.
const BACKGROUND_SEARCH_BINS: usize = 6;
/// Weight of persistent tonal evidence in the temporal score.
const PERSISTENT_TONAL_WEIGHT: f32 = 0.35;
/// Divisors and multipliers of a peak's bin where harmonic support is searched.
const HARMONIC_RELATIONS: [usize; 3] = [2, 3, 4];

#[derive(Debug, Clone)]
pub(crate) struct TonalTransient {
    config: TonalTransientConfig,
    previous_power: [f32; BINS],
    target_gain: [f32; BINS],
    gain: [f32; BINS],
    initialized: bool,
}

impl TonalTransient {
    pub(crate) fn new(config: TonalTransientConfig) -> Self {
        Self { config, previous_power: [0.0; BINS], target_gain: [1.0; BINS], gain: [1.0; BINS], initialized: false }
    }

    /// Analyzes one frame's original power spectrum and returns the smoothed per-bin gains in `[min_gain, 1]`.
    pub(crate) fn process(&mut self, power: &[f32; BINS]) -> &[f32; BINS] {
        if self.initialized {
            self.target_gain.fill(1.0);
            let radius = usize::from(self.config.local_radius);
            let start = radius.max(usize::from(self.config.min_frequency_bin));
            for bin in start..BINS.saturating_sub(radius) {
                self.process_bin(power, bin);
            }
            self.update_gains();
        } else {
            self.target_gain.fill(1.0);
            self.gain.fill(1.0);
            self.initialized = true;
        }
        let floor = self.config.floor;
        for (previous, &observed) in self.previous_power.iter_mut().zip(power) {
            *previous = sanitize(observed, floor);
        }
        &self.gain
    }

    pub(crate) fn reset(&mut self) {
        self.previous_power.fill(0.0);
        self.target_gain.fill(1.0);
        self.gain.fill(1.0);
        self.initialized = false;
    }

    fn process_bin(&mut self, power: &[f32; BINS], bin: usize) {
        let floor = self.config.floor;
        let center = sanitize(power[bin], floor);
        let radius = usize::from(self.config.local_radius);
        let neighbors = power[bin - radius..=bin + radius].iter();
        if neighbors.map(|&value| sanitize(value, floor)).any(|neighbor| neighbor > center) {
            return;
        }
        let score = self.score(power, bin, center);
        if score <= 0.0 {
            return;
        }
        let target = (1.0 - self.config.strength * score).max(self.config.min_gain);
        self.spread(bin, target);
    }

    /// Interference score `S` of the local peak at `bin` with sanitized power `center`.
    fn score(&self, power: &[f32; BINS], bin: usize, center: f32) -> f32 {
        let config = &self.config;
        let tonal =
            normalize(prominence(power, bin, center, config.floor), config.tonal_start_db, config.tonal_full_db);
        if tonal <= 0.0 {
            return 0.0;
        }
        let flux_db = narrow((10.0 * (f64::from(center) / f64::from(self.previous_power[bin])).log10()).max(0.0));
        let flux = normalize(flux_db, config.flux_start_db, config.flux_full_db);
        let movement = self.movement(bin, center);
        let harmonic = harmonic_support(power, bin, center, config);
        let temporal = flux.max(movement).max(PERSISTENT_TONAL_WEIGHT * tonal);
        (tonal * temporal * (1.0 - harmonic)).clamp(0.0, 1.0)
    }

    /// Movement score from the distance to the strongest previous-frame bin within the search radius (the first
    /// maximum wins); 0 when that bin is weaker than `movement_min_relative_power · center`.
    fn movement(&self, bin: usize, center: f32) -> f32 {
        let config = &self.config;
        let radius = usize::from(config.movement_search_radius);
        let first = bin.saturating_sub(radius);
        let last = (bin + radius).min(BINS - 1);
        let mut peak_bin = bin;
        let mut peak_power = 0.0;
        for (candidate, &value) in (first..=last).zip(&self.previous_power[first..=last]) {
            if value > peak_power {
                peak_power = value;
                peak_bin = candidate;
            }
        }
        if peak_power < center * config.movement_min_relative_power {
            return 0.0;
        }
        normalize(
            bin_count(bin.abs_diff(peak_bin)),
            f32::from(config.movement_start_bins),
            f32::from(config.movement_full_bins),
        )
    }

    /// Lowers the target gains around `center_bin` toward `target`: bins at distance `d` receive
    /// `1 − (1 − target)·(R − d + 1)/(R + 1)`.
    fn spread(&mut self, center_bin: usize, target: f32) {
        let radius = usize::from(self.config.spread_radius);
        let weight_scale = f32::from(self.config.spread_radius) + 1.0;
        let first = center_bin.saturating_sub(radius);
        let last = (center_bin + radius).min(BINS - 1);
        for (bin, gain) in (first..=last).zip(&mut self.target_gain[first..=last]) {
            let weight = (weight_scale - bin_count(bin.abs_diff(center_bin))) / weight_scale;
            *gain = gain.min(1.0 - (1.0 - target) * weight);
        }
    }

    fn update_gains(&mut self) {
        let TonalTransientConfig { attack, release, min_gain, .. } = self.config;
        for (gain, &target) in self.gain.iter_mut().zip(&self.target_gain) {
            let smoothing = if target < *gain { attack } else { release };
            *gain = (smoothing * *gain + (1.0 - smoothing) * target).clamp(min_gain, 1.0);
        }
    }
}

/// Tonal prominence in dB: `center` against the mean sanitized power at offsets `3..=6` on both sides (in range).
fn prominence(power: &[f32; BINS], bin: usize, center: f32, floor: f32) -> f32 {
    let mut sum = 0.0;
    let mut count = 0_u8;
    for offset in BACKGROUND_GUARD_BINS + 1..=BACKGROUND_SEARCH_BINS {
        for neighbor in
            [bin.checked_sub(offset), Some(bin + offset).filter(|&index| index < BINS)].into_iter().flatten()
        {
            sum += f64::from(sanitize(power[neighbor], floor));
            count += 1;
        }
    }
    if count == 0 {
        return 0.0;
    }
    let background = (sum / f64::from(count)).max(f64::from(floor));
    narrow(10.0 * (f64::from(center) / background).log10())
}

/// Harmonic support `H` of the peak at `bin`: the largest `clamp(local maximum / (ρ·center), 0, 1)` over the distinct
/// related bins `round(bin/n)` and `n·bin` (`n` = 2, 3, 4) inside `(0, 128]`, where halves round away from zero.
fn harmonic_support(power: &[f32; BINS], bin: usize, center: f32, config: &TonalTransientConfig) -> f32 {
    if center <= config.floor {
        return 0.0;
    }
    let mut targets = [0; 2 * HARMONIC_RELATIONS.len()];
    for (slot, n) in HARMONIC_RELATIONS.iter().enumerate() {
        targets[slot] = (2 * bin + n) / (2 * n);
        targets[HARMONIC_RELATIONS.len() + slot] = n * bin;
    }
    let tolerance = usize::from(config.harmonic_tolerance_bins);
    let mut support: f32 = 0.0;
    for (index, &target) in targets.iter().enumerate() {
        if target == 0 || target >= BINS || targets[..index].contains(&target) {
            continue;
        }
        let first = target.saturating_sub(tolerance);
        let last = (target + tolerance).min(BINS - 1);
        let related =
            power[first..=last].iter().fold(config.floor, |maximum, &value| maximum.max(sanitize(value, config.floor)));
        support = support.max((related / (config.harmonic_relative_power * center)).clamp(0.0, 1.0));
    }
    support
}

/// Maps `value` linearly from `[start, full]` to `[0, 1]`, clamping outside.
fn normalize(value: f32, start: f32, full: f32) -> f32 {
    if value <= start {
        0.0
    } else if value >= full {
        1.0
    } else {
        (value - start) / (full - start)
    }
}

/// A bin distance as `f32`. Distances within the 129-bin spectrum always fit `u16`, which converts losslessly.
fn bin_count(distance: usize) -> f32 {
    u16::try_from(distance).map_or(f32::INFINITY, f32::from)
}

#[cfg(test)]
#[path = "../tests/unit/tonal.rs"]
mod tests;
