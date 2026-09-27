use super::{FRAME, FrameClass, Reference, WindowMetrics, classify, recovery_window};

/// A signal of `frames` 160-sample frames whose frame `i` is a 500 Hz tone with amplitude `amplitudes[i]`.
fn tone_frames(amplitudes: &[f64]) -> Vec<f64> {
    amplitudes
        .iter()
        .flat_map(|&amplitude| {
            (0..FRAME).map(move |n| {
                amplitude
                    * (2.0 * std::f64::consts::PI * 500.0 * f64::from(u32::try_from(n).expect("small")) / 8000.0).sin()
            })
        })
        .collect()
}

fn db(value: f64) -> f64 {
    10.0_f64.powf(value / 20.0)
}

#[test]
fn frames_are_classified_relative_to_the_loudest_clean_frame() {
    let clean = tone_frames(&[1.0, db(-20.0), db(-30.0) * 1.001, db(-40.0), db(-45.0) * 0.999, db(-50.0), 0.0]);
    assert_eq!(
        classify(&clean),
        [
            FrameClass::Active,
            FrameClass::Active,
            FrameClass::Active,
            FrameClass::Other,
            FrameClass::Pause,
            FrameClass::Pause,
            FrameClass::Pause
        ]
    );
    assert_eq!(classify(&[0.0; 3 * FRAME]), [FrameClass::Pause; 3], "a silent reference has no active frame");
}

#[test]
fn an_identical_signal_has_zero_distance_and_the_upper_segmental_snr() {
    let clean = tone_frames(&[1.0, 0.5, 0.0, 0.8]);
    let reference = Reference::new(&clean);
    assert_eq!(reference.log_spectral_distance(&clean, 0..4), Some(0.0));
    assert_eq!(reference.segmental_snr(&clean, 0..4), Some(35.0));
    assert_eq!(reference.speech_level_change(&clean, 0..4), Some(0.0));
}

#[test]
fn a_known_gain_gives_the_known_level_change_snr_and_distance() {
    let clean = tone_frames(&[1.0, 0.5, 0.0, 0.8]);
    let halved: Vec<f64> = clean.iter().map(|sample| 0.5 * sample).collect();
    let reference = Reference::new(&clean);
    let expected = 20.0 * 0.5_f64.log10();
    let level = reference.speech_level_change(&halved, 0..4).expect("active frames");
    assert!((level - expected).abs() < 1e-9, "{level}");
    // The error is half the clean frame, so every frame's SNR is 10·log10(1 / 0.25).
    let snr = reference.segmental_snr(&halved, 0..4).expect("active frames");
    assert!((snr + expected).abs() < 1e-9, "{snr}");
    // Both spectra and their floors scale by the same factor, so every bin differs by exactly the gain.
    let distance = reference.log_spectral_distance(&halved, 0..4).expect("active frames");
    assert!((distance + expected).abs() < 1e-9, "{distance}");
}

#[test]
fn the_segmental_snr_is_clamped_to_its_range() {
    let clean = tone_frames(&[1.0, 1.0]);
    let reference = Reference::new(&clean);
    let inverted: Vec<f64> = clean.iter().map(|sample| -sample).collect();
    let expected = -10.0 * 4.0_f64.log10();
    assert!((reference.segmental_snr(&inverted, 0..2).expect("active") - expected).abs() < 1e-9);
    let buried: Vec<f64> = clean.iter().map(|sample| 10.0 * sample).collect();
    assert_eq!(reference.segmental_snr(&buried, 0..2), Some(-10.0));
}

#[test]
fn noise_attenuation_compares_pause_frames_only() {
    let clean = tone_frames(&[1.0, 0.0, 0.0, 1.0]);
    let noisy = tone_frames(&[1.0, 0.3, 0.2, 1.0]);
    let mut enhanced = noisy.clone();
    enhanced[FRAME..3 * FRAME].iter_mut().for_each(|sample| *sample *= 0.1);
    enhanced[..FRAME].iter_mut().for_each(|sample| *sample *= 0.5);
    let reference = Reference::new(&clean);
    let attenuation = reference.noise_attenuation(&noisy, &enhanced, 0..4).expect("pause frames");
    assert!((attenuation - 20.0).abs() < 1e-9, "{attenuation}");
    assert_eq!(reference.noise_attenuation(&noisy, &enhanced, 0..1), None, "no pause frame in range");
    assert_eq!(reference.speech_level_change(&enhanced, 1..3), None, "no active frame in range");
    assert_eq!(reference.count(FrameClass::Pause, 0..10), 2, "ranges are clipped to the signal");
}

fn window(level: Option<f64>) -> WindowMetrics {
    WindowMetrics { speech_level_change: level, noise_attenuation: None }
}

#[test]
fn recovery_is_the_first_window_after_which_every_speech_window_agrees_within_one_decibel() {
    let pair = |during: Option<f64>, after: Option<f64>| (window(during), window(after));
    let windows = [
        pair(Some(-9.0), Some(-1.0)),
        pair(Some(-0.5), Some(-1.0)),
        pair(Some(-4.0), Some(-1.0)),
        pair(None, None),
        pair(Some(-1.8), Some(-1.0)),
        pair(Some(-2.1), Some(-1.0)),
    ];
    assert_eq!(recovery_window(&windows[..5]), Some(3), "window 3 has no speech; window 4 agrees");
    assert_eq!(recovery_window(&windows), None, "the last window still differs by more than 1 dB");
    assert_eq!(recovery_window(&windows[1..2]), Some(0));
}
