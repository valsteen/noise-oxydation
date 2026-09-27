//! In-place radix-2 complex FFT of the fixed size 256.
//!
//! Twiddle factors `exp(−2πik/256)` are computed once in `f64` and stored as `f32` (deliberate Rust choice R5) instead
//! of the reference's repeated `complex64` multiplication (reference concern U5).

use std::{
    f64::consts::PI,
    ops::{Add, Mul, Sub},
};

use crate::{
    convert::narrow,
    geometry::{FFT_SIZE, FFT_SIZE_U16},
};

/// Single-precision complex number.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Complex {
    pub(crate) re: f32,
    pub(crate) im: f32,
}

impl Complex {
    pub(crate) const ZERO: Self = Self { re: 0.0, im: 0.0 };

    pub(crate) const fn new(re: f32, im: f32) -> Self {
        Self { re, im }
    }

    pub(crate) const fn conj(self) -> Self {
        Self { re: self.re, im: -self.im }
    }

    /// Squared magnitude `re² + im²`.
    pub(crate) fn norm_sqr(self) -> f32 {
        self.re * self.re + self.im * self.im
    }

    pub(crate) fn scale(self, factor: f32) -> Self {
        Self { re: self.re * factor, im: self.im * factor }
    }
}

impl Add for Complex {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self { re: self.re + rhs.re, im: self.im + rhs.im }
    }
}

impl Sub for Complex {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        Self { re: self.re - rhs.re, im: self.im - rhs.im }
    }
}

impl Mul for Complex {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        Self { re: self.re * rhs.re - self.im * rhs.im, im: self.re * rhs.im + self.im * rhs.re }
    }
}

/// FFT plan for the fixed 256-point size. Each call owns its own plan; nothing is shared between calls.
#[derive(Debug, Clone)]
pub(crate) struct Fft {
    twiddles: [Complex; FFT_SIZE / 2],
}

impl Fft {
    pub(crate) fn new() -> Self {
        let mut twiddles = [Complex::ZERO; FFT_SIZE / 2];
        for (k, twiddle) in (0_u16..).zip(twiddles.iter_mut()) {
            let angle = -2.0 * PI * f64::from(k) / f64::from(FFT_SIZE_U16);
            *twiddle = Complex::new(narrow(angle.cos()), narrow(angle.sin()));
        }
        Self { twiddles }
    }

    /// Forward transform `X[k] = Σ x[n]·exp(−2πikn/256)`, in place.
    pub(crate) fn forward(&self, data: &mut [Complex; FFT_SIZE]) {
        bit_reverse(data);
        let mut length = 2;
        while length <= FFT_SIZE {
            let half = length / 2;
            let stride = FFT_SIZE / length;
            for block in data.chunks_exact_mut(length) {
                let (lower, upper) = block.split_at_mut(half);
                for (k, (even, odd)) in lower.iter_mut().zip(upper.iter_mut()).enumerate() {
                    let rotated = self.twiddles[k * stride] * *odd;
                    let sum = *even + rotated;
                    *odd = *even - rotated;
                    *even = sum;
                }
            }
            length *= 2;
        }
    }

    /// Inverse transform `x[n] = (1/256)·Σ X[k]·exp(2πikn/256)`, in place.
    pub(crate) fn inverse(&self, data: &mut [Complex; FFT_SIZE]) {
        for value in data.iter_mut() {
            *value = value.conj();
        }
        self.forward(data);
        let scale = 1.0 / f32::from(FFT_SIZE_U16);
        for value in data.iter_mut() {
            *value = value.conj().scale(scale);
        }
    }
}

// `bit_reverse` treats an index as exactly one byte.
const _: () = assert!(FFT_SIZE == 256, "bit_reverse assumes a 256-point FFT");

/// Reorders `data` by bit-reversed index. With 256 points an index is exactly one byte.
fn bit_reverse(data: &mut [Complex; FFT_SIZE]) {
    for index in 0..=u8::MAX {
        let reversed = index.reverse_bits();
        if index < reversed {
            data.swap(usize::from(index), usize::from(reversed));
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/fft.rs"]
mod tests;
