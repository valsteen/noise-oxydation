//! Weighted overlap-add synthesis matching [`crate::analysis::Analyzer`].
//!
//! Each frame rebuilds the conjugate-symmetric spectrum, applies the inverse FFT and the synthesis window, and adds the
//! windowed samples and the squared window weights into overlap buffers. The first 128 accumulated samples are then
//! final and are emitted as `sum / weight` where `weight > 1e-8`, else 0. At the end of a call the remaining 128
//! overlap samples are emitted the same way.

use crate::{
    fft::{Complex, Fft},
    geometry::{BINS, FFT_SIZE, HOP_SIZE},
    window::HannWindow,
};

/// Output samples whose accumulated squared-window weight is at or below this threshold are emitted as 0.
const MIN_WEIGHT: f32 = 1e-8;

#[derive(Debug, Clone)]
pub(crate) struct Synthesizer {
    overlap: [f32; FFT_SIZE],
    weight: [f32; FFT_SIZE],
    scratch: [Complex; FFT_SIZE],
    pending: bool,
}

impl Synthesizer {
    pub(crate) fn new() -> Self {
        Self { overlap: [0.0; FFT_SIZE], weight: [0.0; FFT_SIZE], scratch: [Complex::ZERO; FFT_SIZE], pending: false }
    }

    /// Overlap-adds one frame and writes the 128 samples it finalizes.
    pub(crate) fn synthesize(
        &mut self,
        window: &HannWindow,
        fft: &Fft,
        spectrum: &[Complex; BINS],
        output: &mut [f32; HOP_SIZE],
    ) {
        // Conjugate-symmetric completion: X[256 − k] = conj(X[k]) for k in 1..128.
        let (one_sided, mirrored) = self.scratch.split_at_mut(BINS);
        one_sided.copy_from_slice(spectrum);
        for (mirror, bin) in mirrored.iter_mut().rev().zip(&spectrum[1..BINS - 1]) {
            *mirror = bin.conj();
        }
        fft.inverse(&mut self.scratch);

        let windowed = self.scratch.iter().zip(window.coefficients());
        for ((sum, weight), (value, coefficient)) in self.overlap.iter_mut().zip(&mut self.weight).zip(windowed) {
            *sum += value.re * coefficient;
            *weight += coefficient * coefficient;
        }
        self.pending = true;

        normalize(&self.overlap[..HOP_SIZE], &self.weight[..HOP_SIZE], output);
        self.overlap.copy_within(HOP_SIZE.., 0);
        self.weight.copy_within(HOP_SIZE.., 0);
        self.overlap[FFT_SIZE - HOP_SIZE..].fill(0.0);
        self.weight[FFT_SIZE - HOP_SIZE..].fill(0.0);
    }

    /// Writes the remaining overlap samples after the last frame and clears the state.
    ///
    /// Returns `false` without writing when no frame has been synthesized since the last reset.
    pub(crate) fn finish(&mut self, output: &mut [f32; FFT_SIZE - HOP_SIZE]) -> bool {
        if !self.pending {
            return false;
        }
        normalize(&self.overlap[..FFT_SIZE - HOP_SIZE], &self.weight[..FFT_SIZE - HOP_SIZE], output);
        self.reset();
        true
    }

    pub(crate) fn reset(&mut self) {
        self.overlap.fill(0.0);
        self.weight.fill(0.0);
        self.pending = false;
    }
}

fn normalize(sums: &[f32], weights: &[f32], output: &mut [f32]) {
    for ((sample, sum), weight) in output.iter_mut().zip(sums).zip(weights) {
        *sample = if *weight > MIN_WEIGHT { sum / weight } else { 0.0 };
    }
}

#[cfg(test)]
#[path = "../tests/unit/synthesis.rs"]
mod tests;
