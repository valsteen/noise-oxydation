//! Evaluation scenarios: real speech mixed with real noise at a target speech-active SNR after a noise-only intro.
//!
//! For each scenario one contiguous noise excerpt, starting [`NOISE_OFFSET_SECONDS`] into the (8 kHz) noise recording,
//! covers the intro and the speech, so the noise learned during the intro is the noise under the speech:
//!
//! ```text
//! noisy = [noise-only intro] ++ [speech + noise],  clean = [zeros] ++ [speech],  both zero-padded to whole packets
//! ```
//!
//! The noise is scaled so that `10·log10(Σ speech² / Σ noise²)`, summed over the samples of the speech's active frames
//! (see [`crate::metrics`]), equals the target SNR. If the mix would then peak above −1 dBFS, speech and noise are
//! attenuated together (keeping the SNR) so that μ-law encoding never clips.

use noise_oxydation::PACKET_SAMPLES;

use crate::{
    error::EvalError,
    metrics::{FRAME, FrameClass, classify, db_to_power, energy},
};

/// Sample rate of every scenario signal.
pub(crate) const SAMPLE_RATE: usize = 8000;
/// The noise excerpt starts this far into the noise recording.
pub(crate) const NOISE_OFFSET_SECONDS: usize = 10;
/// Noise-only intro before the speech in every scenario built from a recording.
pub(crate) const INTRO_SECONDS: usize = 6;
/// Target speech-active SNR.
pub(crate) const SNR_DB: f64 = 5.0;
/// Highest permitted peak of the noisy mix.
pub(crate) const PEAK_LIMIT_DBFS: f64 = -1.0;
/// Samples trimmed from the start of `OSR_us_000_0031_8k.wav`: 4640 samples (0.58 s, 29 whole frames) of its 0.68 s
/// lead-in. This keeps the frame grid of the recording and leaves 480 samples (60 ms) before its first active frame.
pub(crate) const OSR_0031_TRIM_SAMPLES: usize = 4640;
/// The trimmed recording must keep at least this much (50 ms) before its first active frame.
pub(crate) const MINIMUM_LEAD_IN_SAMPLES: usize = 400;

/// Clean and noisy signals of one scenario.
#[derive(Debug, Clone)]
pub(crate) struct Scenario {
    pub(crate) name: &'static str,
    pub(crate) description: String,
    pub(crate) clean: Vec<f64>,
    pub(crate) noisy: Vec<f64>,
    /// Gain applied to the noise excerpt to reach the target SNR, in dB.
    pub(crate) noise_gain_db: f64,
    /// Gain applied to the whole mix to respect the peak limit (0 when no attenuation was needed), in dB.
    pub(crate) headroom_gain_db: f64,
}

/// The four evaluation recordings, at 8 kHz.
pub(crate) struct Sources {
    pub(crate) speech_0030: Vec<f64>,
    pub(crate) speech_0031: Vec<f64>,
    pub(crate) office_noise: Vec<f64>,
    pub(crate) cafeteria_noise: Vec<f64>,
}

/// Builds the four scoped scenarios.
pub(crate) fn scenarios(sources: &Sources) -> Result<Vec<Scenario>, EvalError> {
    let intro = INTRO_SECONDS * SAMPLE_RATE;
    let office = mix(
        "office-5db",
        "OSR_us_000_0030 over DEMAND OOFFICE, 6 s intro, 5 dB",
        &sources.speech_0030,
        excerpt("DEMAND OOFFICE", &sources.office_noise, intro + sources.speech_0030.len())?,
        intro,
        SNR_DB,
    );
    let cafeteria = mix(
        "cafeteria-5db",
        "OSR_us_000_0030 over DEMAND PCAFETER, 6 s intro, 5 dB",
        &sources.speech_0030,
        excerpt("DEMAND PCAFETER", &sources.cafeteria_noise, intro + sources.speech_0030.len())?,
        intro,
        SNR_DB,
    );
    let trimmed = trim_lead_in("OSR_us_000_0031", &sources.speech_0031, OSR_0031_TRIM_SAMPLES)?;
    let after = mix(
        "speech-after-calibration",
        "OSR_us_000_0031 (lead-in trimmed) over DEMAND OOFFICE, 6 s intro, 5 dB",
        trimmed,
        excerpt("DEMAND OOFFICE", &sources.office_noise, intro + trimmed.len())?,
        intro,
        SNR_DB,
    );
    let during = without_intro(&after, "speech-during-calibration", intro);
    Ok(vec![office, cafeteria, after, during])
}

/// The first `length` samples of `noise` after the fixed offset.
fn excerpt<'a>(name: &'static str, noise: &'a [f64], length: usize) -> Result<&'a [f64], EvalError> {
    let start = NOISE_OFFSET_SECONDS * SAMPLE_RATE;
    noise.get(start..start + length).ok_or(EvalError::SourceTooShort {
        source_name: name,
        needed: start + length,
        available: noise.len(),
    })
}

/// Removes `samples` from the start of `speech`, checking that the documented margin before speech remains.
pub(crate) fn trim_lead_in<'a>(name: &'static str, speech: &'a [f64], samples: usize) -> Result<&'a [f64], EvalError> {
    let trimmed = speech.get(samples..).unwrap_or_default();
    let first_active = classify(trimmed).iter().position(|&class| class == FrameClass::Active).unwrap_or(0) * FRAME;
    if first_active < MINIMUM_LEAD_IN_SAMPLES {
        return Err(EvalError::LeadInTooShort {
            source_name: name,
            remaining: first_active,
            required: MINIMUM_LEAD_IN_SAMPLES,
        });
    }
    Ok(trimmed)
}

/// Mixes `speech` after `intro` samples of noise-only audio at `snr_db`. `excerpt` is the noise under the intro and
/// the speech.
pub(crate) fn mix(
    name: &'static str,
    description: &str,
    speech: &[f64],
    excerpt: &[f64],
    intro: usize,
    snr_db: f64,
) -> Scenario {
    debug_assert_eq!(excerpt.len(), intro + speech.len(), "one noise excerpt covers the intro and the speech");
    let noise_gain = snr_gain(speech, &excerpt[intro..], snr_db);
    let length = (intro + speech.len()).div_ceil(PACKET_SAMPLES) * PACKET_SAMPLES;
    let mut clean = vec![0.0; length];
    clean[intro..intro + speech.len()].copy_from_slice(speech);
    let mut noisy = vec![0.0; length];
    for ((mixed, &clean), &noise) in noisy.iter_mut().zip(&clean).zip(excerpt) {
        *mixed = clean + noise_gain * noise;
    }

    let peak = noisy.iter().fold(0.0_f64, |peak, sample| peak.max(sample.abs()));
    let limit = 10.0_f64.powf(PEAK_LIMIT_DBFS / 20.0);
    let headroom = if peak > limit { limit / peak } else { 1.0 };
    for sample in clean.iter_mut().chain(noisy.iter_mut()) {
        *sample *= headroom;
    }
    Scenario {
        name,
        description: description.to_owned(),
        clean,
        noisy,
        noise_gain_db: 20.0 * noise_gain.log10(),
        headroom_gain_db: 20.0 * headroom.log10(),
    }
}

/// The amplitude gain that brings `noise` to `snr_db` below `speech` over the samples of the speech's active frames.
pub(crate) fn snr_gain(speech: &[f64], noise: &[f64], snr_db: f64) -> f64 {
    let (speech_energy, noise_energy) = classify(speech)
        .iter()
        .enumerate()
        .filter(|&(_, &class)| class == FrameClass::Active)
        .map(|(frame, _)| frame * FRAME..(frame + 1) * FRAME)
        .fold((0.0, 0.0), |(speech_sum, noise_sum), range| {
            (speech_sum + energy(&speech[range.clone()]), noise_sum + energy(&noise[range]))
        });
    (speech_energy / (noise_energy * db_to_power(snr_db))).sqrt()
}

/// The scenario with its first `samples` removed: the same speech over the same noise, starting earlier in the call.
fn without_intro(scenario: &Scenario, name: &'static str, samples: usize) -> Scenario {
    Scenario {
        name,
        description: format!("{} with the first {} s removed", scenario.name, samples / SAMPLE_RATE),
        clean: scenario.clean[samples..].to_vec(),
        noisy: scenario.noisy[samples..].to_vec(),
        noise_gain_db: scenario.noise_gain_db,
        headroom_gain_db: scenario.headroom_gain_db,
    }
}

#[cfg(test)]
#[path = "../tests/unit/scenario.rs"]
mod tests;
