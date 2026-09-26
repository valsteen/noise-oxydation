use super::{BINS, FLOOR};

pub(super) struct TonalState {
    previous_power: [f32; BINS],
    previous_gain: [f32; BINS],
}

impl TonalState {
    pub(super) fn new() -> Self {
        Self {
            previous_power: [FLOOR; BINS],
            previous_gain: [1.0; BINS],
        }
    }

    pub(super) fn reset(&mut self) {
        self.previous_power.fill(FLOOR);
        self.previous_gain.fill(1.0);
    }

    pub(super) fn observe(&mut self, power: &[f32; BINS]) {
        self.previous_power = *power;
    }

    pub(super) fn gains(&mut self, power: &[f32; BINS]) -> [f32; BINS] {
        let mut spread = [1.0f32; BINS];
        for bin in 0..BINS {
            let (tonal, flux, movement, harmonic) = self.features(power, bin);
            let evidence = flux.max(movement).max(0.35 * tonal);
            let score = (tonal * evidence * (1.0 - harmonic)).clamp(0.0, 1.0);
            let target = (1.0 - 0.5 * score).max(0.5);
            let alpha = if target < self.previous_gain[bin] {
                0.3
            } else {
                0.85
            };
            let gain = alpha * self.previous_gain[bin] + (1.0 - alpha) * target;
            self.previous_gain[bin] = gain;
            for distance in 0..=2 {
                let neighbor = 1.0 - (1.0 - gain) * (3 - distance) as f32 / 3.0;
                if let Some(left) = bin.checked_sub(distance) {
                    spread[left] = spread[left].min(neighbor);
                }
                if bin + distance < BINS {
                    spread[bin + distance] = spread[bin + distance].min(neighbor);
                }
            }
        }
        self.observe(power);
        spread
    }

    fn features(&self, power: &[f32; BINS], bin: usize) -> (f32, f32, f32, f32) {
        let mut background = 0.0;
        let mut sides = 0;
        for distance in 3..=6 {
            if let Some(left) = bin.checked_sub(distance) {
                background += power[left];
                sides += 1;
            }
            if bin + distance < BINS {
                background += power[bin + distance];
                sides += 1;
            }
        }
        let tonal_db = 10.0 * (power[bin] / (background / sides as f32 + FLOOR)).log10();
        let tonal = normalize(tonal_db, 5.0, 14.0);
        let flux_db = (10.0 * (power[bin] / (self.previous_power[bin] + FLOOR)).log10()).max(0.0);
        let flux = normalize(flux_db, 3.0, 18.0);

        let first = bin.saturating_sub(6);
        let last = (bin + 6).min(BINS - 1);
        let mut previous_bin = bin;
        let mut previous_peak = 0.0;
        for nearby in first..=last {
            if self.previous_power[nearby] > previous_peak {
                previous_peak = self.previous_power[nearby];
                previous_bin = nearby;
            }
        }
        let movement = if previous_peak >= 0.1 * power[bin] {
            normalize(bin.abs_diff(previous_bin) as f32, 1.0, 4.0)
        } else {
            0.0
        };

        let mut harmonic = 0.0f32;
        for divisor in 2..=4 {
            let sub = (bin + divisor / 2) / divisor;
            harmonic = harmonic.max(related_power(power, bin, sub));
            let multiple = bin * divisor;
            if multiple < BINS {
                harmonic = harmonic.max(related_power(power, bin, multiple));
            }
        }
        (tonal, flux, movement, harmonic)
    }
}

fn normalize(value: f32, start: f32, full: f32) -> f32 {
    ((value - start) / (full - start)).clamp(0.0, 1.0)
}

fn related_power(power: &[f32; BINS], bin: usize, related: usize) -> f32 {
    let mut peak = 0.0f32;
    for (neighbor, &value) in power
        .iter()
        .enumerate()
        .take((related + 1).min(BINS - 1) + 1)
        .skip(related.saturating_sub(1))
    {
        // The candidate's own Hann main lobe is not independent harmonic evidence.
        if neighbor > 0 && neighbor.abs_diff(bin) > 2 {
            peak = peak.max(value);
        }
    }
    (peak / (0.15 * power[bin] + FLOOR)).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::{BINS, TonalState};

    #[test]
    fn onset_movement_persistence_harmonics_and_edges() {
        let mut detector = TonalState::new();
        let mut previous = [1.0; BINS];
        previous[17] = 100.0;
        detector.observe(&previous);
        let mut current = [1.0; BINS];
        current[20] = 100.0;
        let (tonal, flux, movement, harmonic) = detector.features(&current, 20);
        assert!((tonal - 1.0).abs() < 1.0e-6);
        assert!(flux > 0.99);
        assert!((movement - 2.0 / 3.0).abs() < 1.0e-5);
        assert!(harmonic < 0.1);
        let first = detector.gains(&current);
        assert!((first[20] - 0.673_333_35).abs() < 0.001);
        assert!((first[19] - 0.782_222_2).abs() < 0.001);
        assert!((first[18] - 0.891_111_1).abs() < 0.001);
        assert!(first[20] < first[19] && first[19] < first[18]);
        assert!((first[23] - 1.0).abs() < 1.0e-6);
        let second = detector.gains(&current);
        assert!((second[20] - 0.697_833_36).abs() < 0.001); // Persistent evidence and 0.85 release.

        current[40] = 20.0;
        assert!((detector.features(&current, 20).3 - 1.0).abs() < 1.0e-6);
        assert!((detector.features(&current, 40).3 - 1.0).abs() < 1.0e-6);
        let protected = detector.gains(&current);
        assert!(protected[20] > second[20]); // Release, not another attack.

        for edge in [0, BINS - 1] {
            let mut isolated = [1.0; BINS];
            isolated[edge] = 100.0;
            assert!(detector.features(&isolated, edge).0 > 0.9);
            assert!(detector.gains(&isolated)[edge].is_finite());
        }

        let mut overlap = [1.0; BINS];
        overlap[20] = 100.0;
        overlap[22] = 100.0;
        let gains = TonalState::new().gains(&overlap);
        assert!((gains[21] - 0.782_222_2).abs() < 0.001);
    }
}
