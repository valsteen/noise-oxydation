//! Short-time Fourier analysis on the reference grid: a 256-sample Hann-windowed frame every 128 samples.
//!
//! Samples accumulate in a fixed 256-sample buffer. When it is full, the frame is transformed and the buffer advances
//! by one hop, keeping the 128 overlapping samples. At the end of a call, [`Analyzer::analyze_remainder`] transforms
//! whatever is buffered, zero-padded to 256 samples, exactly once (reference flush behavior).

use crate::{
    fft::{Complex, Fft},
    geometry::{BINS, FFT_SIZE, HOP_SIZE},
    window::HannWindow,
};

#[derive(Debug, Clone)]
pub(crate) struct Analyzer {
    buffer: [f32; FFT_SIZE],
    buffered: usize,
    scratch: [Complex; FFT_SIZE],
}

impl Analyzer {
    pub(crate) fn new() -> Self {
        Self { buffer: [0.0; FFT_SIZE], buffered: 0, scratch: [Complex::ZERO; FFT_SIZE] }
    }

    /// Appends samples until the frame buffer is full and returns how many were consumed.
    pub(crate) fn push(&mut self, samples: &[f32]) -> usize {
        let free = &mut self.buffer[self.buffered..];
        let count = free.len().min(samples.len());
        free[..count].copy_from_slice(&samples[..count]);
        self.buffered += count;
        count
    }

    /// Whether a complete frame is buffered and must be analyzed before more samples fit.
    pub(crate) fn frame_ready(&self) -> bool {
        self.buffered == FFT_SIZE
    }

    /// Transforms the complete buffered frame into its one-sided spectrum and advances by one hop.
    pub(crate) fn analyze(&mut self, window: &HannWindow, fft: &Fft, spectrum: &mut [Complex; BINS]) {
        debug_assert!(self.frame_ready(), "analyze requires a complete frame");
        self.transform(window, fft, spectrum);
        self.buffer.copy_within(HOP_SIZE.., 0);
        self.buffered = FFT_SIZE - HOP_SIZE;
    }

    /// Transforms the buffered samples, zero-padded to a full frame, and empties the buffer.
    ///
    /// Returns `false` without producing a spectrum when nothing is buffered.
    pub(crate) fn analyze_remainder(&mut self, window: &HannWindow, fft: &Fft, spectrum: &mut [Complex; BINS]) -> bool {
        if self.buffered == 0 {
            return false;
        }
        self.buffer[self.buffered..].fill(0.0);
        self.transform(window, fft, spectrum);
        self.buffered = 0;
        true
    }

    pub(crate) fn reset(&mut self) {
        self.buffered = 0;
    }

    fn transform(&mut self, window: &HannWindow, fft: &Fft, spectrum: &mut [Complex; BINS]) {
        for ((bin, sample), weight) in self.scratch.iter_mut().zip(&self.buffer).zip(window.coefficients()) {
            *bin = Complex::new(sample * weight, 0.0);
        }
        fft.forward(&mut self.scratch);
        spectrum.copy_from_slice(&self.scratch[..BINS]);
    }
}

#[cfg(test)]
#[path = "../tests/unit/analysis.rs"]
mod tests;
