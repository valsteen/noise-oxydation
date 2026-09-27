use std::f64::consts::PI;

use super::{Complex, Fft};
use crate::{geometry::FFT_SIZE, test_support::TestRng};

fn random_signal(seed: u64) -> [Complex; FFT_SIZE] {
    let mut rng = TestRng::new(seed);
    let mut data = [Complex::ZERO; FFT_SIZE];
    for value in &mut data {
        *value = Complex::new(rng.next_signed_f32(), rng.next_signed_f32());
    }
    data
}

/// Direct double-precision DFT `X[k] = Σ x[n]·exp(−2πikn/N)`.
fn direct_dft(input: &[Complex; FFT_SIZE]) -> Vec<(f64, f64)> {
    (0..256_u32)
        .map(|k| {
            (0..256_u32).zip(input).fold((0.0, 0.0), |(re, im), (n, value)| {
                let angle = -2.0 * PI * f64::from(k * n % 256) / 256.0;
                let (sin, cos) = angle.sin_cos();
                let (x_re, x_im) = (f64::from(value.re), f64::from(value.im));
                (re + x_re * cos - x_im * sin, im + x_re * sin + x_im * cos)
            })
        })
        .collect()
}

#[test]
fn forward_transform_matches_a_double_precision_dft() {
    for seed in [1, 2, 3] {
        let input = random_signal(seed);
        let expected = direct_dft(&input);
        let mut actual = input;
        Fft::new().forward(&mut actual);
        for (k, (value, (re, im))) in actual.iter().zip(expected).enumerate() {
            let error = (f64::from(value.re) - re).hypot(f64::from(value.im) - im);
            assert!(error < 2e-5, "bin {k}: error {error}");
        }
    }
}

#[test]
fn inverse_undoes_forward() {
    let input = random_signal(7);
    let fft = Fft::new();
    let mut data = input;
    fft.forward(&mut data);
    fft.inverse(&mut data);
    for (restored, original) in data.iter().zip(&input) {
        assert!((restored.re - original.re).abs() < 1e-6 && (restored.im - original.im).abs() < 1e-6);
    }
}

#[test]
fn a_cosine_on_bin_ten_lands_in_bins_ten_and_two_hundred_forty_six() {
    let mut data = [Complex::ZERO; FFT_SIZE];
    for (n, value) in (0..256_u16).zip(&mut data) {
        *value = Complex::new(super::narrow((2.0 * PI * 10.0 * f64::from(n) / 256.0).cos()), 0.0);
    }
    Fft::new().forward(&mut data);
    for (k, value) in data.iter().enumerate() {
        let expected = if k == 10 || k == 246 { 128.0 } else { 0.0 };
        assert!((value.norm_sqr().sqrt() - expected).abs() < 1e-3, "bin {k}: {value:?}");
    }
}
