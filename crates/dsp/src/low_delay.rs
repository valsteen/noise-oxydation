//! Experimental asymmetric synthesis: 256-sample analysis, 80-sample hop,
//! and a 160-sample synthesis tail. The ordinary enhancer is independent.
use super::{
    BINS, Complex, DspError, FLOOR, FRAME, NoiseEstimator, NoiseState, PI, TonalState,
    exp_integral, fft,
};

pub const LOW_HOP: usize = 80;
const PREPAD: usize = FRAME - LOW_HOP;
const SYNTH: usize = LOW_HOP * 2;
const TAIL: usize = FRAME - SYNTH;
const HISTORY: usize = 80; // 800 ms at the 10 ms experimental hop.
const MIN_LEARNING: usize = FRAME.div_ceil(LOW_HOP) * LOW_HOP;

pub struct LowDelayEnhancer {
    learning_samples: u64,
    frames: u64,
    learned_frames: u32,
    fill: usize,
    raw: [f32; FRAME],
    filtered: [f32; FRAME],
    hp_x: f32,
    hp_y: f32,
    analysis: [f32; FRAME],
    synthesis: [f32; FRAME],
    spectrum: [Complex; FRAME],
    overlap: [f32; SYNTH],
    noise: NoiseState<HISTORY>,
    tonal: TonalState,
    previous_gain: [f32; BINS],
    previous_posterior: [f32; BINS],
}

impl LowDelayEnhancer {
    /// Build per-call state. The quiet intro must contain a complete analysis
    /// frame ending on an 80-sample hop boundary (320 samples minimum).
    ///
    /// # Errors
    /// Returns [`DspError::LearningIntervalTooShort`] below 320 samples.
    pub fn with_estimator(
        learning_samples: u64,
        estimator: NoiseEstimator,
    ) -> Result<Self, DspError> {
        if learning_samples < MIN_LEARNING as u64 {
            return Err(DspError::LearningIntervalTooShort {
                samples: learning_samples,
                minimum: MIN_LEARNING as u64,
            });
        }
        let mut analysis = [0.0; FRAME];
        let mut synthesis = [0.0; FRAME];
        for i in 0..FRAME {
            // The last 80 analysis weights equal the square root of the
            // falling half of a 160-point periodic Hann window.
            analysis[i] = if i <= PREPAD {
                (PI * i as f32 / (2 * PREPAD) as f32).sin()
            } else {
                (PI * (FRAME - i) as f32 / SYNTH as f32).sin()
            };
            if i >= TAIL {
                let k = i - TAIL;
                let hann = 0.5 - 0.5 * (2.0 * PI * k as f32 / SYNTH as f32).cos();
                synthesis[i] = if i < PREPAD {
                    hann / analysis[i]
                } else {
                    analysis[i]
                };
            }
        }
        Ok(Self {
            learning_samples,
            frames: 0,
            learned_frames: 0,
            fill: PREPAD,
            raw: [0.0; FRAME],
            filtered: [0.0; FRAME],
            hp_x: 0.0,
            hp_y: 0.0,
            analysis,
            synthesis,
            spectrum: [Complex::default(); FRAME],
            overlap: [0.0; SYNTH],
            noise: NoiseState::new(estimator),
            tonal: TonalState::new(),
            previous_gain: [1.0; BINS],
            previous_posterior: [1.0; BINS],
        })
    }

    /// Push a sample; the zero-padded first frame represents time before call start.
    pub fn push(&mut self, sample: f32) -> Option<[f32; LOW_HOP]> {
        self.raw[self.fill] = sample;
        let filtered = sample - self.hp_x + 0.995 * self.hp_y;
        self.hp_x = sample;
        self.hp_y = filtered;
        self.filtered[self.fill] = filtered;
        self.fill += 1;
        if self.fill < FRAME {
            return None;
        }
        let output = self.process_frame();
        self.raw.copy_within(LOW_HOP..FRAME, 0);
        self.filtered.copy_within(LOW_HOP..FRAME, 0);
        self.fill = PREPAD;
        self.frames += 1;
        if self.frames == 1 { None } else { Some(output) }
    }

    pub fn reset(&mut self) {
        self.frames = 0;
        self.learned_frames = 0;
        self.fill = PREPAD;
        self.raw.fill(0.0);
        self.filtered.fill(0.0);
        self.hp_x = 0.0;
        self.hp_y = 0.0;
        self.spectrum.fill(Complex::default());
        self.overlap.fill(0.0);
        self.noise.reset();
        self.tonal.reset();
        self.previous_gain.fill(1.0);
        self.previous_posterior.fill(1.0);
    }

    fn process_frame(&mut self) -> [f32; LOW_HOP] {
        let end = (self.frames + 1) * LOW_HOP as u64;
        let output_start = end.saturating_sub(SYNTH as u64);
        let bypass = output_start < self.learning_samples;
        let learning = end >= FRAME as u64 && end <= self.learning_samples;
        for i in 0..FRAME {
            self.spectrum[i] = Complex {
                re: self.filtered[i] * self.analysis[i],
                im: 0.0,
            };
        }
        fft(&mut self.spectrum, false);
        let mut powers = [FLOOR; BINS];
        for (bin, power) in powers.iter_mut().enumerate() {
            *power = self.spectrum[bin].power().max(FLOOR);
        }
        if learning {
            self.learned_frames = self.learned_frames.saturating_add(1);
            let count = self.learned_frames as f32;
            for (bin, &power) in powers.iter().enumerate() {
                self.noise.learn(bin, power, count);
            }
        }
        if bypass {
            self.tonal.observe(&powers);
        } else {
            self.noise.begin_frame();
            let sparse_excess = self.noise.selected == NoiseEstimator::Mcra
                && powers
                    .iter()
                    .enumerate()
                    .filter(|(bin, power)| **power > 5.0 * self.noise.noise[*bin])
                    .count()
                    < BINS / 2;
            for (bin, &power) in powers.iter().enumerate() {
                let posterior = (power / self.noise.noise[bin].max(FLOOR)).min(1.0e6);
                let prior = (0.98 * self.previous_gain[bin].powi(2) * self.previous_posterior[bin]
                    + 0.02 * (posterior - 1.0).max(0.0))
                .max(0.001);
                let v = (posterior * prior / (1.0 + prior)).max(1.0e-6);
                self.noise
                    .update(bin, power, prior, posterior, sparse_excess);
                let gain = (prior / (1.0 + prior) * (0.5 * exp_integral(v)).exp()).clamp(0.05, 1.0);
                self.previous_gain[bin] = gain;
                self.previous_posterior[bin] = posterior;
                self.spectrum[bin].re *= gain;
                self.spectrum[bin].im *= gain;
                if bin != 0 && bin != FRAME / 2 {
                    self.spectrum[FRAME - bin].re *= gain;
                    self.spectrum[FRAME - bin].im *= gain;
                }
            }
            self.noise.end_frame();
            let tonal_gain = self.tonal.gains(&powers);
            for (bin, &gain) in tonal_gain.iter().enumerate() {
                self.spectrum[bin].re *= gain;
                self.spectrum[bin].im *= gain;
                if bin != 0 && bin != FRAME / 2 {
                    self.spectrum[FRAME - bin].re *= gain;
                    self.spectrum[FRAME - bin].im *= gain;
                }
            }
        }
        fft(&mut self.spectrum, true);
        for i in 0..SYNTH {
            self.overlap[i] += self.spectrum[TAIL + i].re * self.synthesis[TAIL + i];
        }
        let mut output = [0.0; LOW_HOP];
        for (i, sample) in output.iter_mut().enumerate() {
            *sample = if bypass {
                self.raw[TAIL + i]
            } else {
                self.overlap[i]
            };
        }
        self.overlap.copy_within(LOW_HOP..SYNTH, 0);
        self.overlap[LOW_HOP..].fill(0.0);
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_pair_reconstructs_without_spectral_gains() {
        let enhancer = LowDelayEnhancer::with_estimator(8_000, NoiseEstimator::SppMmse).unwrap();
        for i in 0..LOW_HOP {
            let a = enhancer.analysis[TAIL + i] * enhancer.synthesis[TAIL + i];
            let b = enhancer.analysis[TAIL + LOW_HOP + i] * enhancer.synthesis[TAIL + LOW_HOP + i];
            assert!((a + b - 1.0).abs() < 1.0e-5, "bin {i}: {}", a + b);
        }
    }

    #[test]
    fn shortest_accepted_intro_learns_a_real_frame() {
        assert!(LowDelayEnhancer::with_estimator(319, NoiseEstimator::SppMmse).is_err());
        let mut enhancer = LowDelayEnhancer::with_estimator(320, NoiseEstimator::SppMmse).unwrap();
        for index in 0..320 {
            let sample = 0.15 * (2.0 * PI * 250.0 * index as f32 / 8_000.0).sin();
            let _ = enhancer.push(sample);
        }
        assert_eq!(enhancer.learned_frames, 1);
        assert!(enhancer.noise.noise.iter().any(|&power| power > FLOOR));
    }

    #[test]
    fn unmodified_stft_reconstructs_impulse_sine_and_noise_from_start() {
        let enhancer = LowDelayEnhancer::with_estimator(8_000, NoiseEstimator::SppMmse).unwrap();
        let mut seed = 0x1234_5678_u32;
        let input: [f32; 2_400] = std::array::from_fn(|i| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = f32::from((seed >> 16) as i16) / 32_768.0 * 0.1;
            let impulse = if i == 0 || i == 160 || i == 1_599 {
                0.4
            } else {
                0.0
            };
            noise + impulse + 0.2 * (2.0 * PI * 350.0 * i as f32 / 8_000.0).sin()
        });
        let mut frame = [0.0; FRAME];
        let mut overlap = [0.0; SYNTH];
        let mut output = Vec::with_capacity(input.len());
        for end in (LOW_HOP..=input.len() + LOW_HOP).step_by(LOW_HOP) {
            frame.copy_within(LOW_HOP..FRAME, 0);
            for (offset, sample) in frame[PREPAD..].iter_mut().enumerate() {
                *sample = input.get(end - LOW_HOP + offset).copied().unwrap_or(0.0);
            }
            let mut spectrum = std::array::from_fn(|i| Complex {
                re: frame[i] * enhancer.analysis[i],
                im: 0.0,
            });
            fft(&mut spectrum, false);
            fft(&mut spectrum, true);
            for i in 0..SYNTH {
                overlap[i] += spectrum[TAIL + i].re * enhancer.synthesis[TAIL + i];
            }
            if end > LOW_HOP {
                output.extend_from_slice(&overlap[..LOW_HOP]);
            }
            overlap.copy_within(LOW_HOP..SYNTH, 0);
            overlap[LOW_HOP..].fill(0.0);
        }
        assert_eq!(output.len(), input.len());
        for (index, (&actual, &expected)) in output.iter().zip(input.iter()).enumerate() {
            assert!(
                (actual - expected).abs() < 1.0e-4,
                "sample {index}: {actual} != {expected}"
            );
        }
    }
}
