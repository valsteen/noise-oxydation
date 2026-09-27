use super::{INTRO_SECONDS, MINIMUM_LEAD_IN_SAMPLES, SAMPLE_RATE, Sources, mix, scenarios, trim_lead_in};
use crate::{
    error::EvalError,
    metrics::{FRAME, energy},
};

/// Deterministic white noise in `[-amplitude, amplitude)`.
fn noise(length: usize, seed: u64, amplitude: f64) -> Vec<f64> {
    let mut state = seed | 1;
    (0..length)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let bits = u32::try_from(state >> 40).expect("24 bits");
            amplitude * (f64::from(bits) / f64::from(1_u32 << 23) - 1.0)
        })
        .collect()
}

/// Speech stand-in: `bursts` blocks of 8 tone frames followed by 4 silent frames, so the active samples are known.
fn bursts(count: usize, amplitude: f64) -> (Vec<f64>, Vec<std::ops::Range<usize>>) {
    let mut signal = Vec::new();
    let mut active = Vec::new();
    for _ in 0..count {
        let start = signal.len();
        signal.extend((0..8 * FRAME).map(|n| amplitude * (f64::from(u32::try_from(n).expect("small")) * 0.37).sin()));
        active.push(start..signal.len());
        signal.extend(std::iter::repeat_n(0.0, 4 * FRAME));
    }
    (signal, active)
}

#[test]
fn the_noise_is_scaled_to_the_target_snr_over_the_active_speech_samples() {
    let intro = 5 * FRAME;
    let (speech, active) = bursts(6, 0.2);
    let excerpt = noise(intro + speech.len(), 7, 0.05);
    for target in [0.0, 5.0, 12.5] {
        let scenario = mix("test", "synthetic", &speech, &excerpt, intro, target);
        let (mut speech_energy, mut noise_energy) = (0.0, 0.0);
        for range in &active {
            let range = intro + range.start..intro + range.end;
            let clean = &scenario.clean[range.clone()];
            let noise: Vec<f64> = scenario.noisy[range].iter().zip(clean).map(|(noisy, clean)| noisy - clean).collect();
            speech_energy += energy(clean);
            noise_energy += energy(&noise);
        }
        let measured = 10.0 * (speech_energy / noise_energy).log10();
        assert!((measured - target).abs() < 0.1, "target {target} dB, measured {measured:.4} dB");
        assert!(scenario.headroom_gain_db.abs() < 1e-12, "no attenuation was needed");
    }
}

#[test]
fn the_intro_is_noise_only_and_the_call_is_padded_to_whole_packets() {
    let intro = 5 * FRAME;
    let (speech, _) = bursts(2, 0.2);
    let speech = &speech[..speech.len() - 37];
    let excerpt = noise(intro + speech.len(), 9, 0.05);
    let scenario = mix("test", "synthetic", speech, &excerpt, intro, 5.0);
    assert_eq!(scenario.clean.len() % FRAME, 0);
    assert_eq!(scenario.clean.len(), scenario.noisy.len());
    assert!(scenario.clean[..intro].iter().all(|&sample| sample == 0.0));
    assert_eq!(&scenario.clean[intro..intro + speech.len()], speech);
    let gain = 10.0_f64.powf(scenario.noise_gain_db / 20.0);
    for (mixed, noise) in scenario.noisy[..intro].iter().zip(&excerpt) {
        assert!((mixed - gain * noise).abs() < 1e-12);
    }
    assert!(scenario.noisy[intro + speech.len()..].iter().all(|&sample| sample == 0.0), "padding is silent");
}

#[test]
fn a_loud_mix_is_attenuated_below_the_peak_limit_without_changing_its_snr() {
    let intro = 2 * FRAME;
    let (speech, _) = bursts(3, 0.95);
    let excerpt = noise(intro + speech.len(), 11, 0.5);
    let scenario = mix("test", "synthetic", &speech, &excerpt, intro, 0.0);
    let peak = scenario.noisy.iter().fold(0.0_f64, |peak, sample| peak.max(sample.abs()));
    assert!(scenario.headroom_gain_db < 0.0);
    assert!((20.0 * peak.log10() - (-1.0)).abs() < 1e-9, "peak {peak}");
    let scale = 10.0_f64.powf(scenario.headroom_gain_db / 20.0);
    for (clean, original) in scenario.clean[intro..].iter().zip(&speech) {
        assert!((clean - scale * original).abs() < 1e-12, "speech and noise share the attenuation");
    }
}

#[test]
fn trimming_keeps_the_documented_margin_before_speech() {
    // Seven silent frames precede the speech; the margin is measured to the first active frame after trimming.
    let mut speech = vec![0.0; 7 * FRAME];
    speech.extend(bursts(1, 0.3).0);
    assert_eq!(trim_lead_in("test", &speech, 4 * FRAME).expect("480 samples kept").len(), speech.len() - 4 * FRAME);
    const { assert!(3 * FRAME >= MINIMUM_LEAD_IN_SAMPLES && 2 * FRAME < MINIMUM_LEAD_IN_SAMPLES) };
    assert!(matches!(
        trim_lead_in("test", &speech, 5 * FRAME),
        Err(EvalError::LeadInTooShort { remaining: 320, required: MINIMUM_LEAD_IN_SAMPLES, .. })
    ));
}

#[test]
fn speech_during_calibration_is_the_after_scenario_without_its_intro() {
    let lead_in = vec![0.0; super::OSR_0031_TRIM_SAMPLES + 480];
    let (speech, _) = bursts(20, 0.2);
    let sources = Sources {
        speech_0030: speech.clone(),
        speech_0031: lead_in.into_iter().chain(speech).collect(),
        office_noise: noise(40 * SAMPLE_RATE, 3, 0.05),
        cafeteria_noise: noise(40 * SAMPLE_RATE, 4, 0.05),
    };
    let built = scenarios(&sources).expect("sources are long enough");
    let names: Vec<_> = built.iter().map(|scenario| scenario.name).collect();
    assert_eq!(names, ["office-5db", "cafeteria-5db", "speech-after-calibration", "speech-during-calibration"]);
    let (after, during) = (&built[2], &built[3]);
    let intro = INTRO_SECONDS * SAMPLE_RATE;
    assert_eq!(during.clean, after.clean[intro..]);
    assert_eq!(during.noisy, after.noisy[intro..]);
    assert_ne!(built[0].noisy, built[1].noisy, "office and cafeteria use different noise");

    let short = Sources { office_noise: noise(12 * SAMPLE_RATE, 3, 0.05), ..sources };
    assert!(matches!(scenarios(&short), Err(EvalError::SourceTooShort { .. })));
}
