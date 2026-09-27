use super::HannWindow;

#[test]
fn window_is_symmetric_with_zero_endpoints() {
    let window = HannWindow::new();
    let coefficients = window.coefficients();
    assert!(coefficients[0].abs() < 1e-7);
    assert!(coefficients[255].abs() < 1e-7);
    for (left, right) in coefficients.iter().zip(coefficients.iter().rev()) {
        assert!((left - right).abs() < 1e-6, "{left} vs {right}");
    }
}

/// `Σ_{n=0}^{255} (0.5 − 0.5·cos(2πn/255))`: the cosines over `n = 0..=254` cover one full period and sum to 0, and
/// the extra term `n = 255` contributes `cos(2π) = 1`, so the sum is `128 − 0.5`.
#[test]
fn coefficients_sum_to_the_symmetric_hann_total() {
    let window = HannWindow::new();
    let sum: f64 = window.coefficients().iter().map(|&coefficient| f64::from(coefficient)).sum();
    assert!((sum - 127.5).abs() < 1e-4, "sum {sum}");
}

#[test]
fn window_peaks_between_the_two_middle_samples() {
    let window = HannWindow::new();
    let coefficients = window.coefficients();
    let peak = coefficients.iter().copied().fold(0.0_f32, f32::max);
    assert!((coefficients[127] - peak).abs() < 1e-7 && (coefficients[128] - peak).abs() < 1e-7);
    assert!(peak < 1.0 && peak > 0.9999);
}
