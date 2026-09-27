//! 256-point Hann-windowed power spectra for the log-spectral distance, computed by a direct real DFT.

use std::f64::consts::PI;

/// Transform length.
pub(crate) const POINTS: usize = 256;
/// One-sided bins.
pub(crate) const BINS: usize = POINTS / 2 + 1;

/// Precomputed window and twiddle tables.
pub(crate) struct SpectrumAnalyzer {
    window: [f64; POINTS],
    cosine: [f64; POINTS],
    sine: [f64; POINTS],
}

impl SpectrumAnalyzer {
    pub(crate) fn new() -> Self {
        let mut analyzer = Self { window: [0.0; POINTS], cosine: [0.0; POINTS], sine: [0.0; POINTS] };
        let length = f64::from(u32::try_from(POINTS).expect("small"));
        for (index, ((window, cosine), sine)) in
            (0_u32..).zip(analyzer.window.iter_mut().zip(&mut analyzer.cosine).zip(&mut analyzer.sine))
        {
            let position = f64::from(index);
            // Periodic Hann window: a spectral-analysis window, not the enhancer's synthesis window.
            *window = 0.5 - 0.5 * (2.0 * PI * position / length).cos();
            *cosine = (2.0 * PI * position / length).cos();
            *sine = (2.0 * PI * position / length).sin();
        }
        analyzer
    }

    /// Power spectrum `|X[k]|²`, `k = 0..=128`, of the Hann-windowed `segment` (zero-padded to 256 samples).
    pub(crate) fn power(&self, segment: &[f64]) -> [f64; BINS] {
        let mut windowed = [0.0; POINTS];
        for ((value, &sample), &weight) in windowed.iter_mut().zip(segment).zip(&self.window) {
            *value = sample * weight;
        }
        let mut power = [0.0; BINS];
        for (bin, value) in power.iter_mut().enumerate() {
            let (mut real, mut imaginary) = (0.0, 0.0);
            for (index, &sample) in windowed.iter().enumerate() {
                let twiddle = (bin * index) % POINTS;
                real += sample * self.cosine[twiddle];
                imaginary -= sample * self.sine[twiddle];
            }
            *value = real * real + imaginary * imaginary;
        }
        power
    }
}

#[cfg(test)]
#[path = "../tests/unit/spectrum.rs"]
mod tests;
