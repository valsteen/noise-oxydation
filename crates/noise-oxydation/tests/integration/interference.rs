//! Tonal transient suppression attenuates a foreground tone by more than 0.5 dB and at most its 0.5 minimum gain
//! (6.02 dB) relative to the same call without interference suppression, for every noise estimator.

use std::{f64::consts::PI, time::Duration};

use noise_oxydation::{CallConfig, InterferenceConfig, TonalTransientConfig};

use crate::support::{Rng, decode, describe, encode_packets, enhancer, estimators, run_call};

const SAMPLE_RATE: f64 = 8000.0;
const CALIBRATION_SAMPLES: usize = 8000;
const TOTAL_SAMPLES: usize = 5 * 8000;

/// Quiet white noise throughout, plus from the end of the 1 s calibration a tone whose frequency sweeps linearly
/// between 2.2 and 3.8 kHz and back once per second.
fn noise_with_sweeping_tone() -> Vec<f64> {
    let mut rng = Rng::new(77);
    let mut phase = 0.0;
    (0..TOTAL_SAMPLES)
        .map(|index| {
            let noise = 0.004 * (f64::from(rng.next_u32()) / f64::from(u32::MAX) * 2.0 - 1.0);
            if index < CALIBRATION_SAMPLES {
                return noise;
            }
            let seconds = f64::from(u32::try_from(index - CALIBRATION_SAMPLES).expect("short signal")) / SAMPLE_RATE;
            let triangle = 1.0 - (2.0 * seconds.fract() - 1.0).abs();
            phase += 2.0 * PI * (2200.0 + 1600.0 * triangle) / SAMPLE_RATE;
            noise + 0.25 * phase.sin()
        })
        .collect()
}

/// Energy between `low_hz` and `high_hz` of the decoded output over `range`, from the DFT of consecutive 256-sample
/// blocks.
fn band_energy(output: &[u8], range: std::ops::Range<usize>, low_hz: f64, high_hz: f64) -> f64 {
    const BLOCK: usize = 256;
    let samples: Vec<f64> = output[range].iter().map(|&byte| decode(byte)).collect();
    let bin_hz = SAMPLE_RATE / 256.0;
    let mut energy = 0.0;
    for block in samples.as_chunks::<BLOCK>().0 {
        for bin in (0_u32..=128).filter(|&bin| (low_hz..=high_hz).contains(&(f64::from(bin) * bin_hz))) {
            let (mut re, mut im) = (0.0, 0.0);
            for (n, &value) in (0_u32..).zip(block) {
                let angle = -2.0 * PI * f64::from(bin) * f64::from(n) / 256.0;
                re += value * angle.cos();
                im += value * angle.sin();
            }
            energy += re * re + im * im;
        }
    }
    energy
}

#[test]
fn a_sweeping_tone_is_attenuated_within_the_tonal_gain_bounds() {
    let input = encode_packets(&noise_with_sweeping_tone());
    for noise_estimator in estimators() {
        let base =
            CallConfig { calibration_duration: Duration::from_secs(1), noise_estimator, ..CallConfig::default() };
        let enabled = CallConfig {
            interference: InterferenceConfig::TonalTransient(TonalTransientConfig::default()),
            ..base.clone()
        };
        let disabled = CallConfig { interference: InterferenceConfig::Disabled, ..base };
        let with_suppression = run_call(&mut enhancer(&enabled), &input).output;
        let without_suppression = run_call(&mut enhancer(&disabled), &input).output;

        // From 0.25 s after the tone starts to 0.25 s before the end.
        let window = CALIBRATION_SAMPLES + 2000..TOTAL_SAMPLES - 2000;
        let attenuation_db = 10.0
            * (band_energy(&without_suppression, window.clone(), 2000.0, 4000.0)
                / band_energy(&with_suppression, window, 2000.0, 4000.0))
            .log10();
        let context = describe(&enabled);
        assert!(attenuation_db > 0.5, "{context}: {attenuation_db:.2} dB");
        assert!(attenuation_db <= 6.03, "{context}: {attenuation_db:.2} dB");
    }
}
