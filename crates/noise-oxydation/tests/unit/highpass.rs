use std::f64::consts::PI;

use super::HighPass;

const SETTLE_SAMPLES: u32 = 8000;
const MEASURE_SAMPLES: u32 = 8000;

/// Steady-state gain in dB for a sinusoid at `frequency_hz`, measured as the output/input RMS ratio after the
/// filter's transient has decayed.
fn measured_gain_db(frequency_hz: f64) -> f64 {
    let mut filter = HighPass::new(80.0);
    let mut input_energy = 0.0;
    let mut output_energy = 0.0;
    for n in 0..SETTLE_SAMPLES + MEASURE_SAMPLES {
        let x = (2.0 * PI * frequency_hz * f64::from(n) / 8000.0 + 0.3).cos() * 0.5;
        let mut sample = [super::narrow(x)];
        filter.process(&mut sample);
        if n >= SETTLE_SAMPLES {
            input_energy += x * x;
            output_energy += f64::from(sample[0]) * f64::from(sample[0]);
        }
    }
    10.0 * (output_energy / input_energy).log10()
}

/// `|H(e^{jω})| = |1 − e^{−jω}| / |1 − r·e^{−jω}|` with `r = exp(−2π·80/8000)`.
fn transfer_function_gain_db(frequency_hz: f64) -> f64 {
    let r = (-2.0 * PI * 80.0 / 8000.0).exp();
    let omega = 2.0 * PI * frequency_hz / 8000.0;
    let numerator = (2.0 - 2.0 * omega.cos()).sqrt();
    let denominator = (1.0 - 2.0 * r * omega.cos() + r * r).sqrt();
    20.0 * (numerator / denominator).log10()
}

#[test]
fn magnitude_response_matches_the_transfer_function() {
    for frequency_hz in [40.0, 75.3, 80.0, 200.0, 1000.0, 3000.0] {
        let measured = measured_gain_db(frequency_hz);
        let expected = transfer_function_gain_db(frequency_hz);
        assert!((measured - expected).abs() < 0.02, "{frequency_hz} Hz: measured {measured} dB, expected {expected}");
    }
}

#[test]
fn documented_response_points_hold() {
    assert!((transfer_function_gain_db(75.3) + 3.0).abs() < 0.02);
    assert!((measured_gain_db(75.3) + 3.0).abs() < 0.03);
    assert!((measured_gain_db(80.0) + 2.74).abs() < 0.03);
}

#[test]
fn nyquist_is_slightly_amplified() {
    let mut filter = HighPass::new(80.0);
    let mut samples: Vec<f32> = (0..16_000).map(|n| if n % 2 == 0 { 0.25 } else { -0.25 }).collect();
    filter.process(&mut samples);
    let settled = &samples[8000..];
    let peak = settled.iter().fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    let gain_db = 20.0 * (f64::from(peak) / 0.25).log10();
    assert!((gain_db - 0.27).abs() < 0.01, "Nyquist gain {gain_db} dB");
}

#[test]
fn constant_input_decays_to_zero() {
    let mut filter = HighPass::new(80.0);
    let mut samples = vec![0.5_f32; 2000];
    filter.process(&mut samples);
    assert!((samples[0] - 0.5).abs() < 1e-6, "the step passes on the first sample");
    assert!(samples[1999].abs() < 1e-6, "DC is rejected, got {}", samples[1999]);
}

#[test]
fn state_carries_across_calls_and_reset_clears_it() {
    let input: Vec<f32> = (0..320_u16).map(|n| (f32::from(n) * 0.37).sin() * 0.4).collect();

    let mut whole = input.clone();
    HighPass::new(80.0).process(&mut whole);

    let mut split = input.clone();
    let mut filter = HighPass::new(80.0);
    let (first, second) = split.split_at_mut(160);
    filter.process(first);
    filter.process(second);
    assert_eq!(split, whole);

    filter.reset();
    let mut again = input;
    filter.process(&mut again);
    assert_eq!(again, whole);
}
