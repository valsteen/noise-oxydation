//! Objective noise-reduction and speech-preservation metrics on 20 ms frames.
//!
//! Every signal is aligned sample for sample with the clean reference (the packet API preserves alignment), and
//! frames are the 160-sample packets of the call. Frame classes come from the clean reference: a frame is *active*
//! when its energy is at least the loudest clean frame's energy minus 30 dB, and a *pause* when it is at most that
//! maximum minus 45 dB; frames in between belong to neither class.
//!
//! Over a range of frames:
//!
//! ```text
//! noise attenuation    = 10·log10(Σ noisy² / Σ enhanced²)                       over pause frames
//! segmental SNR (y)    = mean over active frames of clamp(10·log10(Σ clean² / Σ (y − clean)²), −10, 35)
//! speech level change  = 10·log10(Σ enhanced² / Σ clean²)                        over active frames
//! log-spectral distance = mean over active frames of sqrt(mean_k (10·log10 P_y[k] − 10·log10 P_clean[k])²)
//! ```
//!
//! The spectra `P` are 256-point Hann-windowed power spectra of the 256 samples centered on the frame, each floored
//! 60 dB below its own peak (and at 1e−20 so that an all-zero frame stays finite).

use std::ops::Range;

use noise_oxydation::PACKET_SAMPLES;

use crate::spectrum::{BINS, POINTS, SpectrumAnalyzer};

/// Frame length in samples (one 20 ms packet).
pub(crate) const FRAME: usize = PACKET_SAMPLES;
/// Frames per second.
pub(crate) const FRAMES_PER_SECOND: usize = 50;
/// Active frames are at most this far below the loudest clean frame.
pub(crate) const ACTIVE_BELOW_MAX_DB: f64 = 30.0;
/// Pause frames are at least this far below the loudest clean frame.
pub(crate) const PAUSE_BELOW_MAX_DB: f64 = 45.0;
/// Segmental SNR clamp.
pub(crate) const SEGMENTAL_SNR_RANGE_DB: (f64, f64) = (-10.0, 35.0);
/// Each log-spectral-distance spectrum is floored this far below its own peak.
pub(crate) const SPECTRUM_FLOOR_BELOW_PEAK_DB: f64 = 60.0;
const ABSOLUTE_SPECTRUM_FLOOR: f64 = 1e-20;

/// Class of one clean-reference frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameClass {
    Active,
    Pause,
    Other,
}

/// Energy of each complete frame.
pub(crate) fn frame_energies(signal: &[f64]) -> Vec<f64> {
    signal.as_chunks::<FRAME>().0.iter().map(|frame| energy(frame)).collect()
}

/// Sum of squares.
pub(crate) fn energy(samples: &[f64]) -> f64 {
    samples.iter().map(|sample| sample * sample).sum()
}

/// Classifies every complete frame of the clean reference.
pub(crate) fn classify(clean: &[f64]) -> Vec<FrameClass> {
    let energies = frame_energies(clean);
    let loudest = energies.iter().copied().fold(0.0, f64::max);
    let active_threshold = loudest * db_to_power(-ACTIVE_BELOW_MAX_DB);
    let pause_threshold = loudest * db_to_power(-PAUSE_BELOW_MAX_DB);
    energies
        .into_iter()
        .map(|frame_energy| {
            if loudest > 0.0 && frame_energy >= active_threshold {
                FrameClass::Active
            } else if frame_energy <= pause_threshold {
                FrameClass::Pause
            } else {
                FrameClass::Other
            }
        })
        .collect()
}

pub(crate) fn db_to_power(db: f64) -> f64 {
    10.0_f64.powf(db / 10.0)
}

pub(crate) fn power_ratio_db(numerator: f64, denominator: f64) -> f64 {
    10.0 * (numerator / denominator).log10()
}

/// The clean reference with its frame classes and the precomputed spectra the log-spectral distance needs.
pub(crate) struct Reference<'a> {
    clean: &'a [f64],
    classes: Vec<FrameClass>,
    analyzer: SpectrumAnalyzer,
    clean_log_spectra: Vec<Option<[f64; BINS]>>,
}

impl<'a> Reference<'a> {
    pub(crate) fn new(clean: &'a [f64]) -> Self {
        let classes = classify(clean);
        let analyzer = SpectrumAnalyzer::new();
        let clean_log_spectra = classes
            .iter()
            .enumerate()
            .map(|(frame, &class)| (class == FrameClass::Active).then(|| log_spectrum(&analyzer, clean, frame)))
            .collect();
        Self { clean, classes, analyzer, clean_log_spectra }
    }

    pub(crate) fn classes(&self) -> &[FrameClass] {
        &self.classes
    }

    /// Frames of `class` inside `frames`.
    pub(crate) fn frames_of(&self, class: FrameClass, frames: Range<usize>) -> impl Iterator<Item = usize> + '_ {
        let end = frames.end.min(self.classes.len());
        (frames.start.min(end)..end).filter(move |&frame| self.classes[frame] == class)
    }

    pub(crate) fn count(&self, class: FrameClass, frames: Range<usize>) -> usize {
        self.frames_of(class, frames).count()
    }

    /// Noise attenuation in dB over pause frames, or `None` without pause frames.
    pub(crate) fn noise_attenuation(&self, noisy: &[f64], enhanced: &[f64], frames: Range<usize>) -> Option<f64> {
        let (noisy_energy, enhanced_energy, count) =
            self.frames_of(FrameClass::Pause, frames).fold((0.0, 0.0, 0), |(noisy_sum, enhanced_sum, count), frame| {
                (
                    noisy_sum + energy(frame_of(noisy, frame)),
                    enhanced_sum + energy(frame_of(enhanced, frame)),
                    count + 1,
                )
            });
        (count > 0).then(|| power_ratio_db(noisy_energy, enhanced_energy))
    }

    /// Mean clamped per-frame SNR of `signal` over active frames, or `None` without active frames.
    pub(crate) fn segmental_snr(&self, signal: &[f64], frames: Range<usize>) -> Option<f64> {
        let (low, high) = SEGMENTAL_SNR_RANGE_DB;
        mean(self.frames_of(FrameClass::Active, frames).map(|frame| {
            let clean = frame_of(self.clean, frame);
            let error: f64 = clean.iter().zip(frame_of(signal, frame)).map(|(c, y)| (y - c) * (y - c)).sum();
            // Identical frames have an infinite SNR, which the clamp maps to its upper bound.
            let snr = if error == 0.0 { high } else { power_ratio_db(energy(clean), error) };
            snr.clamp(low, high)
        }))
    }

    /// Level of `signal` relative to the clean reference over active frames, in dB, or `None` without active frames.
    pub(crate) fn speech_level_change(&self, signal: &[f64], frames: Range<usize>) -> Option<f64> {
        let (signal_energy, clean_energy, count) =
            self.frames_of(FrameClass::Active, frames).fold((0.0, 0.0, 0), |(signal_sum, clean_sum, count), frame| {
                (
                    signal_sum + energy(frame_of(signal, frame)),
                    clean_sum + energy(frame_of(self.clean, frame)),
                    count + 1,
                )
            });
        (count > 0).then(|| power_ratio_db(signal_energy, clean_energy))
    }

    /// Mean log-spectral distance of `signal` from the clean reference over active frames, in dB.
    pub(crate) fn log_spectral_distance(&self, signal: &[f64], frames: Range<usize>) -> Option<f64> {
        mean(self.frames_of(FrameClass::Active, frames).map(|frame| {
            let clean = self.clean_log_spectra[frame].as_ref().expect("active frames have a clean spectrum");
            let other = log_spectrum(&self.analyzer, signal, frame);
            let squared: f64 = clean.iter().zip(&other).map(|(c, y)| (y - c) * (y - c)).sum();
            (squared / f64::from(u32::try_from(BINS).expect("small"))).sqrt()
        }))
    }
}

fn frame_of(signal: &[f64], frame: usize) -> &[f64] {
    &signal[frame * FRAME..(frame + 1) * FRAME]
}

/// Floored log power spectrum (dB) of the 256 samples centered on `frame`; samples outside the signal count as zero.
fn log_spectrum(analyzer: &SpectrumAnalyzer, signal: &[f64], frame: usize) -> [f64; BINS] {
    let mut segment = [0.0; POINTS];
    let center = frame * FRAME + FRAME / 2;
    for (offset, value) in segment.iter_mut().enumerate() {
        if let Some(&sample) = (center + offset).checked_sub(POINTS / 2).and_then(|position| signal.get(position)) {
            *value = sample;
        }
    }
    let power = analyzer.power(&segment);
    let peak = power.iter().copied().fold(0.0, f64::max);
    let floor = (peak * db_to_power(-SPECTRUM_FLOOR_BELOW_PEAK_DB)).max(ABSOLUTE_SPECTRUM_FLOOR);
    power.map(|value| 10.0 * value.max(floor).log10())
}

fn mean(values: impl Iterator<Item = f64>) -> Option<f64> {
    let (sum, count) = values.fold((0.0, 0_u32), |(sum, count), value| (sum + value, count + 1));
    (count > 0).then(|| sum / f64::from(count))
}

/// Metrics of one signal over the evaluated frames, in dB (`None` when the frames contain no qualifying class).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SignalMetrics {
    pub(crate) noise_attenuation: Option<f64>,
    pub(crate) segmental_snr: Option<f64>,
    pub(crate) speech_level_change: Option<f64>,
    pub(crate) log_spectral_distance: Option<f64>,
}

impl SignalMetrics {
    pub(crate) fn measure(reference: &Reference<'_>, noisy: &[f64], signal: &[f64], frames: Range<usize>) -> Self {
        Self {
            noise_attenuation: reference.noise_attenuation(noisy, signal, frames.clone()),
            segmental_snr: reference.segmental_snr(signal, frames.clone()),
            speech_level_change: reference.speech_level_change(signal, frames.clone()),
            log_spectral_distance: reference.log_spectral_distance(signal, frames),
        }
    }
}

/// Speech level change and noise attenuation (dB) within one window of the calibration comparison.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct WindowMetrics {
    pub(crate) speech_level_change: Option<f64>,
    pub(crate) noise_attenuation: Option<f64>,
}

impl WindowMetrics {
    pub(crate) fn measure(reference: &Reference<'_>, noisy: &[f64], enhanced: &[f64], frames: Range<usize>) -> Self {
        Self {
            speech_level_change: reference.speech_level_change(enhanced, frames.clone()),
            noise_attenuation: reference.noise_attenuation(noisy, enhanced, frames),
        }
    }
}

/// Largest speech level difference between the two runs that still counts as recovered.
pub(crate) const RECOVERY_TOLERANCE_DB: f64 = 1.0;

/// The smallest window index `k` such that, in every window from `k` on where both runs have a speech level, the
/// runs differ by at most [`RECOVERY_TOLERANCE_DB`]; `None` when no window qualifies.
pub(crate) fn recovery_window(pairs: &[(WindowMetrics, WindowMetrics)]) -> Option<usize> {
    (0..pairs.len()).find(|&start| {
        pairs[start..].iter().all(|(during, after)| match (during.speech_level_change, after.speech_level_change) {
            (Some(during), Some(after)) => (during - after).abs() <= RECOVERY_TOLERANCE_DB,
            _ => true,
        })
    })
}

#[cfg(test)]
#[path = "../tests/unit/metrics.rs"]
mod tests;
