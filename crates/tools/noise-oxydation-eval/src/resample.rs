//! 16 kHz → 8 kHz decimation for the DEMAND noise recordings.
//!
//! A 121-tap Blackman-windowed sinc low-pass with a 3850 Hz cutoff, normalized to unit DC gain, is applied centered
//! (zero phase) and every second output sample is kept:
//!
//! ```text
//! h[j] = w[j] · 2·fc/fs · sinc(2·fc/fs · (j − 60)) / Σ h,   w[j] = 0.42 − 0.5·cos(2πj/120) + 0.08·cos(4πj/120)
//! y[m] = Σ_j h[j] · x[2m + 60 − j]     (x = 0 outside the recording)
//! ```
//!
//! By design the response is flat within 0.01 dB below 3.4 kHz and at least 70 dB down above 4.3 kHz, where aliasing
//! into the telephone band would begin to matter. The unit tests measure synthetic tones and assert the evaluation's
//! requirements: at most 0.1 dB of passband deviation and at least 60 dB of stopband attenuation.

use std::f64::consts::PI;

const TAPS: usize = 121;
const CENTER: usize = TAPS / 2;
const INPUT_RATE_HZ: f64 = 16_000.0;
const CUTOFF_HZ: f64 = 3850.0;

/// The low-pass filter coefficients.
fn coefficients() -> [f64; TAPS] {
    let mut taps = [0.0; TAPS];
    let normalized_cutoff = 2.0 * CUTOFF_HZ / INPUT_RATE_HZ;
    let span = f64::from(u32::try_from(TAPS - 1).expect("small"));
    for (index, tap) in (0_u32..).zip(taps.iter_mut()) {
        let offset = f64::from(index) - span / 2.0;
        let argument = PI * normalized_cutoff * offset;
        let sinc = if offset == 0.0 { 1.0 } else { argument.sin() / argument };
        let phase = 2.0 * PI * f64::from(index) / span;
        let window = 0.42 - 0.5 * phase.cos() + 0.08 * (2.0 * phase).cos();
        *tap = normalized_cutoff * sinc * window;
    }
    let dc_gain: f64 = taps.iter().sum();
    for tap in &mut taps {
        *tap /= dc_gain;
    }
    taps
}

/// Decimates a 16 kHz signal to 8 kHz. The output has `input.len() / 2` samples; output sample `m` is centered on
/// input sample `2m`.
pub(crate) fn decimate_16k_to_8k(input: &[f64]) -> Vec<f64> {
    let taps = coefficients();
    (0..input.len() / 2)
        .map(|output_index| {
            let center = 2 * output_index;
            taps.iter()
                .enumerate()
                .filter_map(|(tap_index, &tap)| {
                    (center + CENTER).checked_sub(tap_index).and_then(|position| input.get(position)).map(|&x| tap * x)
                })
                .sum()
        })
        .collect()
}

#[cfg(test)]
#[path = "../tests/unit/resample.rs"]
mod tests;
