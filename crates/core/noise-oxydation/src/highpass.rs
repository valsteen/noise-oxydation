//! First-order DC-blocking high-pass filter `y[n] = x[n] − x[n−1] + r·y[n−1]` with `r = exp(−2π·fc/8000)`.
//!
//! `fc` is the pole parameter, not the −3 dB frequency (reference documentation problem D7): with the 80 Hz default
//! the response is −3 dB near 75.3 Hz and −2.74 dB at 80 Hz. State carries across packets of one call.

use std::f64::consts::PI;

use crate::{convert::narrow, geometry::SAMPLE_RATE_HZ};

#[derive(Debug, Clone)]
pub(crate) struct HighPass {
    feedback: f32,
    previous_input: f32,
    previous_output: f32,
}

impl HighPass {
    /// Creates a filter for a validated cutoff in `(0, 4000)` Hz.
    pub(crate) fn new(cutoff_hz: f32) -> Self {
        let feedback = narrow((-2.0 * PI * f64::from(cutoff_hz) / f64::from(SAMPLE_RATE_HZ)).exp());
        Self { feedback, previous_input: 0.0, previous_output: 0.0 }
    }

    /// Filters `samples` in place, continuing from the previous call's state.
    pub(crate) fn process(&mut self, samples: &mut [f32]) {
        for sample in samples {
            let output = *sample - self.previous_input + self.feedback * self.previous_output;
            self.previous_input = *sample;
            self.previous_output = output;
            *sample = output;
        }
    }

    pub(crate) fn reset(&mut self) {
        self.previous_input = 0.0;
        self.previous_output = 0.0;
    }
}

#[cfg(test)]
#[path = "../tests/unit/highpass.rs"]
mod tests;
