//! Symmetric Hann window `w[n] = 0.5 − 0.5·cos(2πn / 255)` shared by analysis and synthesis.
//!
//! The denominator `N − 1` makes the window symmetric with zeros at both ends, as the reference formula does (its
//! comment says "periodic"; reference documentation problem D1).

use std::f64::consts::PI;

use crate::{
    convert::narrow,
    geometry::{FFT_SIZE, FFT_SIZE_U16},
};

#[derive(Debug, Clone)]
pub(crate) struct HannWindow {
    coefficients: [f32; FFT_SIZE],
}

impl HannWindow {
    pub(crate) fn new() -> Self {
        let mut coefficients = [0.0; FFT_SIZE];
        let denominator = f64::from(FFT_SIZE_U16 - 1);
        for (n, coefficient) in (0_u16..).zip(coefficients.iter_mut()) {
            *coefficient = narrow(0.5 - 0.5 * (2.0 * PI * f64::from(n) / denominator).cos());
        }
        Self { coefficients }
    }

    pub(crate) fn coefficients(&self) -> &[f32; FFT_SIZE] {
        &self.coefficients
    }
}

#[cfg(test)]
#[path = "../tests/unit/window.rs"]
mod tests;
