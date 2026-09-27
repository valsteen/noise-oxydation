//! Fixed-size streaming enhancement at 8 kHz: high-pass, STFT, selectable noise
//! estimation, decision-directed SNR, Log-MMSE, tonal gain, and normalized ISTFT.
#![allow(clippy::cast_precision_loss)] // All loop indices and frame counts here are bounded by the fixed FFT geometry.

mod low_delay;
mod noise;
mod tonal;

pub use low_delay::{LOW_HOP, LowDelayEnhancer};

pub use noise::NoiseEstimator;
use noise::NoiseState;
use std::f32::consts::PI;
use std::fmt;
use tonal::TonalState;

pub const FRAME: usize = 256;
pub const HOP: usize = 128;
const BINS: usize = FRAME / 2 + 1;
const FLOOR: f32 = 1.0e-12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DspError {
    LearningIntervalTooShort { samples: u64, minimum: u64 },
}

impl fmt::Display for DspError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LearningIntervalTooShort { samples, minimum } => {
                write!(f, "learning interval {samples} samples is below {minimum}")
            }
        }
    }
}

impl std::error::Error for DspError {}

#[derive(Clone, Copy, Default)]
struct Complex {
    re: f32,
    im: f32,
}

impl Complex {
    fn power(self) -> f32 {
        self.re.mul_add(self.re, self.im * self.im)
    }
}

#[allow(clippy::many_single_char_names)] // FFT butterfly notation.
fn fft(values: &mut [Complex; FRAME], inverse: bool) {
    let mut j = 0;
    for i in 1..FRAME {
        let mut bit = FRAME / 2;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            values.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= FRAME {
        let angle = (if inverse { 2.0 } else { -2.0 }) * PI / len as f32;
        let (s, c) = angle.sin_cos();
        let half = len / 2;
        for start in (0..FRAME).step_by(len) {
            let mut twiddle = Complex { re: 1.0, im: 0.0 };
            for k in 0..half {
                let a = values[start + k];
                let b = values[start + k + half];
                let t = Complex {
                    re: twiddle.re.mul_add(b.re, -twiddle.im * b.im),
                    im: twiddle.re.mul_add(b.im, twiddle.im * b.re),
                };
                values[start + k] = Complex {
                    re: a.re + t.re,
                    im: a.im + t.im,
                };
                values[start + k + half] = Complex {
                    re: a.re - t.re,
                    im: a.im - t.im,
                };
                twiddle = Complex {
                    re: twiddle.re.mul_add(c, -twiddle.im * s),
                    im: twiddle.re.mul_add(s, twiddle.im * c),
                };
            }
        }
        len *= 2;
    }
    if inverse {
        for value in values {
            value.re /= FRAME as f32;
            value.im /= FRAME as f32;
        }
    }
}

// E1(x) for x > 0. Series below one, continued fraction above one.
#[allow(clippy::many_single_char_names)] // Conventional continued-fraction recurrence.
fn exp_integral(x: f32) -> f32 {
    if x <= 1.0 {
        let mut term = 1.0;
        let mut sum = 0.0;
        for k in 1..40 {
            term *= -x / k as f32;
            let add = term / k as f32;
            sum += add;
            if add.abs() < 1.0e-7 {
                break;
            }
        }
        -0.577_215_7 - x.ln() - sum
    } else {
        let mut b = x + 1.0;
        let mut c = 1.0e30;
        let mut d = 1.0 / b;
        let mut h = d;
        for k in 1..80 {
            let a = -(k * k) as f32;
            b += 2.0;
            d = 1.0 / (a.mul_add(d, b).max(1.0e-30));
            c = b + a / c;
            let delta = c * d;
            h *= delta;
            if (delta - 1.0).abs() < 1.0e-6 {
                break;
            }
        }
        h * (-x).exp()
    }
}

/// Single-owner, fixed-storage enhancer. Each full frame produces 128 samples.
pub struct Enhancer {
    learning_samples: u64,
    frames: u64,
    learned_frames: u32,
    fill: usize,
    raw: [f32; FRAME],
    filtered: [f32; FRAME],
    hp_x: f32,
    hp_y: f32,
    window: [f32; FRAME],
    spectrum: [Complex; FRAME],
    overlap: [f32; FRAME],
    normalization: [f32; FRAME],
    noise: NoiseState,
    tonal: TonalState,
    previous_gain: [f32; BINS],
    previous_posterior: [f32; BINS],
}

impl Enhancer {
    /// The interval must include at least one complete 256-sample frame.
    ///
    /// # Errors
    /// Returns [`DspError::LearningIntervalTooShort`] below one frame.
    pub fn new(learning_samples: u64) -> Result<Self, DspError> {
        Self::with_estimator(learning_samples, NoiseEstimator::default())
    }

    /// Construct with a selected noise estimator and default tonal suppression.
    ///
    /// # Errors
    /// Returns [`DspError::LearningIntervalTooShort`] below one frame.
    pub fn with_estimator(
        learning_samples: u64,
        estimator: NoiseEstimator,
    ) -> Result<Self, DspError> {
        if learning_samples < FRAME as u64 {
            return Err(DspError::LearningIntervalTooShort {
                samples: learning_samples,
                minimum: FRAME as u64,
            });
        }
        let mut window = [0.0; FRAME];
        for (i, weight) in window.iter_mut().enumerate() {
            *weight = 0.5 - 0.5 * (2.0 * PI * i as f32 / FRAME as f32).cos();
        }
        Ok(Self {
            learning_samples,
            frames: 0,
            learned_frames: 0,
            fill: 0,
            raw: [0.0; FRAME],
            filtered: [0.0; FRAME],
            hp_x: 0.0,
            hp_y: 0.0,
            window,
            spectrum: [Complex::default(); FRAME],
            overlap: [0.0; FRAME],
            normalization: [0.0; FRAME],
            noise: NoiseState::new(estimator),
            tonal: TonalState::new(),
            previous_gain: [1.0; BINS],
            previous_posterior: [1.0; BINS],
        })
    }

    /// Push one normalized sample. Padding at end of stream uses the same method.
    pub fn push(&mut self, sample: f32) -> Option<[f32; HOP]> {
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
        self.raw.copy_within(HOP..FRAME, 0);
        self.filtered.copy_within(HOP..FRAME, 0);
        self.fill = HOP;
        self.frames += 1;
        Some(output)
    }

    /// Clear the filter, analyzer, estimator, and overlap-add state for a new call.
    pub fn reset(&mut self) {
        self.frames = 0;
        self.learned_frames = 0;
        self.fill = 0;
        self.raw.fill(0.0);
        self.filtered.fill(0.0);
        self.hp_x = 0.0;
        self.hp_y = 0.0;
        self.spectrum.fill(Complex::default());
        self.overlap.fill(0.0);
        self.normalization.fill(0.0);
        self.noise.reset();
        self.tonal.reset();
        self.previous_gain.fill(1.0);
        self.previous_posterior.fill(1.0);
    }

    fn process_frame(&mut self) -> [f32; HOP] {
        let frame_start = self.frames * (HOP as u64);
        let bypass = frame_start < self.learning_samples;
        let learning = frame_start + FRAME as u64 <= self.learning_samples;
        for i in 0..FRAME {
            self.spectrum[i] = Complex {
                re: self.filtered[i] * self.window[i],
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
        for i in 0..FRAME {
            let weight = self.window[i];
            self.overlap[i] += self.spectrum[i].re * weight;
            self.normalization[i] += weight * weight;
        }
        let mut output = [0.0; HOP];
        for (i, sample) in output.iter_mut().enumerate() {
            *sample = if bypass {
                self.raw[i]
            } else if self.normalization[i] > 1.0e-8 {
                self.overlap[i] / self.normalization[i]
            } else {
                self.raw[i]
            };
        }
        self.overlap.copy_within(HOP..FRAME, 0);
        self.normalization.copy_within(HOP..FRAME, 0);
        self.overlap[HOP..].fill(0.0);
        self.normalization[HOP..].fill(0.0);
        output
    }
}

#[cfg(test)]
mod tests {
    use super::tonal::TonalState;
    use super::{Complex, Enhancer, FRAME, HOP, exp_integral, fft};

    #[test]
    fn fft_round_trip_and_exponential_integral() {
        let mut data = [Complex::default(); FRAME];
        for (i, bin) in data.iter_mut().enumerate() {
            bin.re = (i as f32 * 0.13).sin();
        }
        let original = data;
        fft(&mut data, false);
        fft(&mut data, true);
        for (actual, expected) in data.iter().zip(original) {
            assert!((actual.re - expected.re).abs() < 0.0001);
        }
        assert!((exp_integral(1.0) - 0.219_383_94).abs() < 0.0001);
        assert!((exp_integral(2.0) - 0.048_900_51).abs() < 0.0001);
    }

    #[test]
    fn calibration_uses_only_whole_windows() {
        let mut enhancer = Enhancer::new(FRAME as u64).unwrap();
        let mut emitted = 0;
        for _ in 0..FRAME {
            emitted += enhancer.push(0.1).map_or(0, |_| HOP);
        }
        assert_eq!(emitted, HOP);
        assert_eq!(enhancer.learned_frames, 1);
        for _ in 0..HOP {
            emitted += enhancer.push(0.1).map_or(0, |_| HOP);
        }
        assert_eq!(emitted, FRAME);
        assert_eq!(enhancer.learned_frames, 1);
    }

    #[test]
    fn tonal_gain_multiplies_the_log_mmse_spectrum() {
        let mut enhancer = Enhancer::new(FRAME as u64).unwrap();
        enhancer.frames = 2;
        for i in 0..FRAME {
            enhancer.filtered[i] =
                0.2 * (2.0 * std::f32::consts::PI * 20.0 * i as f32 / FRAME as f32).sin();
        }
        let mut original = [Complex::default(); FRAME];
        for (i, value) in original.iter_mut().enumerate() {
            value.re = enhancer.filtered[i] * enhancer.window[i];
        }
        fft(&mut original, false);
        let powers = std::array::from_fn(|bin| original[bin].power().max(super::FLOOR));
        enhancer.noise.noise = powers;
        let tonal_gain = TonalState::new().gains(&powers);
        assert!(tonal_gain[20] < 1.0);
        let _ = enhancer.process_frame();
        assert!(enhancer.previous_gain[20] < 1.0);
        fft(&mut enhancer.spectrum, false);
        for bin in [18, 19, 20, 21, 22] {
            let expected = original[bin].re * enhancer.previous_gain[bin] * tonal_gain[bin];
            assert!((enhancer.spectrum[bin].re - expected).abs() < 0.0002);
        }
    }
}
