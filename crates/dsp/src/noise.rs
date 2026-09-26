use super::{BINS, FLOOR};

const WINDOW: usize = 50;

/// Noise estimator used by the shared spectral enhancer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NoiseEstimator {
    #[default]
    SppMmse,
    Mcra,
    Minimum,
}

pub(super) struct NoiseState {
    pub(super) selected: NoiseEstimator,
    pub(super) noise: [f32; BINS],
    probability: [f32; BINS],
    smoothed: [f32; BINS],
    history: [[f32; BINS]; WINDOW],
    history_len: usize,
    history_next: usize,
}

impl NoiseState {
    #[allow(clippy::large_stack_arrays)] // Fixed 50-by-129 history belongs to the call, including after construction.
    pub(super) fn new(selected: NoiseEstimator) -> Self {
        Self {
            selected,
            noise: [FLOOR; BINS],
            probability: [0.0; BINS],
            smoothed: [FLOOR; BINS],
            history: [[0.0; BINS]; WINDOW],
            history_len: 0,
            history_next: 0,
        }
    }

    pub(super) fn reset(&mut self) {
        self.noise.fill(FLOOR);
        self.probability.fill(0.0);
        self.smoothed.fill(FLOOR);
        self.history.fill([0.0; BINS]);
        self.history_len = 0;
        self.history_next = 0;
    }

    pub(super) fn learn(&mut self, bin: usize, power: f32, count: f32) {
        self.noise[bin] += (power - self.noise[bin]) / count;
        self.smoothed[bin] = self.noise[bin];
    }

    pub(super) fn begin_frame(&mut self) {
        if self.selected != NoiseEstimator::SppMmse {
            if self.history_len == 0 {
                self.history[0] = self.smoothed;
                self.history_len = 1;
                self.history_next = 1;
            }
            self.history_len = (self.history_len + 1).min(WINDOW);
        }
    }

    pub(super) fn end_frame(&mut self) {
        if self.selected != NoiseEstimator::SppMmse {
            self.history_next = (self.history_next + 1) % WINDOW;
        }
    }

    pub(super) fn update(
        &mut self,
        bin: usize,
        power: f32,
        prior: f32,
        posterior: f32,
        sparse_excess: bool,
    ) {
        match self.selected {
            NoiseEstimator::SppMmse => {
                let v = (posterior * prior / (1.0 + prior)).max(1.0e-6);
                let instantaneous = 1.0 / (1.0 + (1.0 + prior) * (-v).exp());
                let probability = 0.8 * self.probability[bin] + 0.2 * instantaneous;
                self.probability[bin] = probability;
                self.noise[bin] = (0.98 * self.noise[bin]
                    + 0.02 * (probability * self.noise[bin] + (1.0 - probability) * power))
                    .max(FLOOR);
            }
            NoiseEstimator::Mcra | NoiseEstimator::Minimum => {
                self.smoothed[bin] = 0.8f32.mul_add(self.smoothed[bin], 0.2 * power);
                self.history[self.history_next][bin] = self.smoothed[bin];
                let minimum = self.history[..self.history_len]
                    .iter()
                    .map(|frame| frame[bin])
                    .fold(f32::INFINITY, f32::min)
                    .max(FLOOR);
                if self.selected == NoiseEstimator::Minimum {
                    self.noise[bin] = minimum;
                } else {
                    // A narrowband voiced bin can outlive the minimum window.
                    // Broadband rises still enter the ordinary MCRA update.
                    let indicator = f32::from(
                        self.smoothed[bin] / (minimum + FLOOR) > 5.0
                            || (sparse_excess && posterior > 5.0),
                    );
                    self.probability[bin] = 0.2f32.mul_add(self.probability[bin], 0.8 * indicator);
                    let alpha = 0.95 + 0.05 * self.probability[bin];
                    self.noise[bin] = alpha
                        .mul_add(self.noise[bin], (1.0 - alpha) * power)
                        .max(FLOOR);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{NoiseEstimator, NoiseState, WINDOW};

    #[test]
    fn minimum_window_evicts_oldest_frame() {
        let mut state = NoiseState::new(NoiseEstimator::Minimum);
        state.noise[10] = 1.0;
        state.smoothed[10] = 1.0;
        for frame in 0..=WINDOW {
            state.begin_frame();
            state.update(10, if frame == 0 { 0.0 } else { 1.0 }, 0.0, 0.0, false);
            state.end_frame();
        }
        // The first smoothed value was 0.8; it leaves after frame 50.
        assert!((state.noise[10] - 0.84).abs() < 1.0e-5);
    }

    #[test]
    fn mcra_probability_controls_noise_coefficient() {
        let mut onset = NoiseState::new(NoiseEstimator::Mcra);
        onset.noise[10] = 1.0;
        onset.smoothed[10] = 1.0;
        onset.begin_frame();
        onset.update(10, 100.0, 0.0, 0.0, false);
        onset.end_frame();
        assert!((onset.noise[10] - 1.99).abs() < 1.0e-4);

        let mut state = NoiseState::new(NoiseEstimator::Mcra);
        state.noise[10] = 1.0;
        state.smoothed[10] = 1.0;
        state.begin_frame();
        state.update(10, 1.0, 0.0, 0.0, false);
        state.end_frame();
        assert!((state.noise[10] - 1.0).abs() < 1.0e-6);
        state.begin_frame();
        state.update(10, 100.0, 0.0, 0.0, false);
        state.end_frame();
        // S=20.8, min=1, I=1, p=0.8, alpha=0.99.
        assert!((state.probability[10] - 0.8).abs() < 1.0e-6);
        assert!((state.noise[10] - 1.99).abs() < 1.0e-4);
    }

    #[test]
    fn mcra_does_not_learn_sustained_voice_after_minimum_turnover() {
        let mut state = NoiseState::new(NoiseEstimator::Mcra);
        state.noise[10] = 1.0;
        state.smoothed[10] = 1.0;
        for _ in 0..=WINDOW * 2 {
            state.begin_frame();
            let posterior = 100.0 / state.noise[10];
            state.update(10, 100.0, 0.0, posterior, true);
            state.end_frame();
        }
        assert!(state.probability[10] > 0.99);
        assert!(state.noise[10] < 3.0);
    }

    #[test]
    fn mcra_tracks_a_broadband_rise_after_minimum_turnover() {
        let mut state = NoiseState::new(NoiseEstimator::Mcra);
        state.noise[10] = 1.0;
        state.smoothed[10] = 1.0;
        for _ in 0..=WINDOW * 2 {
            state.begin_frame();
            let posterior = 100.0 / state.noise[10];
            state.update(10, 100.0, 0.0, posterior, false);
            state.end_frame();
        }
        assert!(state.noise[10] > 50.0);
    }
}
