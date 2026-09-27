//! `compare`: byte and sample statistics between two headerless μ-law files of equal length.
//!
//! The first file is the reference. Statistics are reported over the whole file and separately for the calibration
//! region `[0, split)` and the enhanced region `[split, end)`. With the default 5 s calibration the split is output
//! sample 39808: frame 311 starts at 128·311 = 39808 and is the first enhanced frame, so no earlier output sample is
//! influenced by an enhanced frame.
//!
//! The tail lines measure reference concern U2 in the last 127 samples, where a drained output can come from a single
//! analysis frame: their peak relative to the preceding second of the same file, and the candidate's tail peak relative
//! to the reference's (for example a truncated call against the same samples of an uninterrupted call).

use std::{ops::Range, path::Path};

use crate::{audio_io::read_bytes, error::EvalError, metrics::power_ratio_db, mulaw};

/// First output sample influenced by an enhanced frame with the default 5 s calibration.
pub(crate) const DEFAULT_SPLIT: usize = 39_808;
/// Samples at the end of a call that can come from a single analysis frame.
pub(crate) const TAIL_SAMPLES: usize = 127;
/// Samples before the tail used as its comparison level.
pub(crate) const TAIL_CONTEXT_SAMPLES: usize = 8000;

/// Statistics of one region.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RegionStatistics {
    pub(crate) samples: usize,
    pub(crate) identical_bytes: usize,
    /// Largest absolute difference of the decoded 16-bit samples.
    pub(crate) max_abs_difference: u32,
    /// `10·log10(Σ (candidate − reference)² / Σ reference²)` of the decoded samples; `None` when the files are
    /// identical in the region (the ratio is −∞) or the reference is silent.
    pub(crate) difference_to_reference_db: Option<f64>,
}

impl RegionStatistics {
    pub(crate) fn identical_fraction(&self) -> f64 {
        if self.samples == 0 {
            1.0
        } else {
            f64::from(u32::try_from(self.identical_bytes).unwrap_or(u32::MAX))
                / f64::from(u32::try_from(self.samples).unwrap_or(u32::MAX))
        }
    }
}

/// Compares `candidate` with `reference` over `range`.
pub(crate) fn region_statistics(reference: &[u8], candidate: &[u8], range: Range<usize>) -> RegionStatistics {
    let pairs = reference[range.clone()].iter().zip(&candidate[range.clone()]);
    let (mut identical_bytes, mut max_abs_difference, mut difference_energy, mut reference_energy) = (0, 0, 0.0, 0.0);
    for (&expected, &actual) in pairs {
        if expected == actual {
            identical_bytes += 1;
        }
        let (expected, actual) = (i32::from(mulaw::decode(expected)), i32::from(mulaw::decode(actual)));
        max_abs_difference = max_abs_difference.max((actual - expected).unsigned_abs());
        difference_energy += f64::from(actual - expected).powi(2);
        reference_energy += f64::from(expected).powi(2);
    }
    let difference_to_reference_db = (difference_energy > 0.0 && reference_energy > 0.0)
        .then(|| power_ratio_db(difference_energy, reference_energy));
    RegionStatistics { samples: range.len(), identical_bytes, max_abs_difference, difference_to_reference_db }
}

/// Peak of the last [`TAIL_SAMPLES`] decoded samples relative to the peak of the [`TAIL_CONTEXT_SAMPLES`] before them,
/// in dB; `None` when the file is too short or the context is silent.
pub(crate) fn tail_peak_ratio_db(bytes: &[u8]) -> Option<f64> {
    let tail_start = bytes.len().checked_sub(TAIL_SAMPLES)?;
    let context_start = tail_start.checked_sub(TAIL_CONTEXT_SAMPLES)?;
    let context = peak(&bytes[context_start..tail_start]);
    (context > 0.0).then(|| 20.0 * (peak(&bytes[tail_start..]) / context).log10())
}

/// Peak of the last [`TAIL_SAMPLES`] decoded samples of `candidate` relative to the same samples of `reference`, in
/// dB; `None` when the lengths differ, the files are shorter than the tail, or the reference tail is silent. Comparing
/// a truncated call's output with the same samples of an uninterrupted call isolates the effect of draining.
pub(crate) fn tail_peak_change_db(reference: &[u8], candidate: &[u8]) -> Option<f64> {
    let start = reference.len().checked_sub(TAIL_SAMPLES)?;
    let expected = peak(&reference[start..]);
    (expected > 0.0 && candidate.len() == reference.len())
        .then(|| 20.0 * (peak(&candidate[start..]) / expected).log10())
}

/// Largest decoded magnitude.
fn peak(bytes: &[u8]) -> f64 {
    bytes.iter().map(|&byte| f64::from(mulaw::decode(byte)).abs()).fold(0.0, f64::max)
}

pub(crate) fn compare(reference_path: &Path, candidate_path: &Path, split: usize) -> Result<(), EvalError> {
    let reference = read_bytes(reference_path)?;
    let candidate = read_bytes(candidate_path)?;
    if reference.len() != candidate.len() {
        return Err(EvalError::LengthMismatch { reference: reference.len(), candidate: candidate.len() });
    }
    let length = reference.len();
    let split = split.min(length);
    println!("reference {}", reference_path.display());
    println!("candidate {}", candidate_path.display());
    println!("{length} samples, regions split at output sample {split}");
    println!(
        "  {:<24} {:>9} {:>16} {:>14} {:>22}",
        "region", "samples", "identical bytes", "max |diff|", "diff / reference energy"
    );
    for (name, range) in [("overall", 0..length), ("calibration", 0..split), ("enhanced", split..length)] {
        let statistics = region_statistics(&reference, &candidate, range.clone());
        let ratio = statistics.difference_to_reference_db.map_or_else(
            || if statistics.identical_bytes == statistics.samples { "identical".to_owned() } else { "n/a".to_owned() },
            |db| format!("{db:.2} dB"),
        );
        println!(
            "  {:<24} {:>9} {:>15.4}% {:>14} {:>22}",
            format!("{name} [{}, {})", range.start, range.end),
            statistics.samples,
            100.0 * statistics.identical_fraction(),
            statistics.max_abs_difference,
            ratio
        );
    }
    let tail = |bytes: &[u8]| tail_peak_ratio_db(bytes).map_or_else(|| "n/a".to_owned(), |db| format!("{db:.2} dB"));
    println!(
        "tail peak (last {TAIL_SAMPLES} samples) relative to the preceding {TAIL_CONTEXT_SAMPLES}-sample peak: \
         reference {}, candidate {}",
        tail(&reference),
        tail(&candidate)
    );
    let change =
        tail_peak_change_db(&reference, &candidate).map_or_else(|| "n/a".to_owned(), |db| format!("{db:.2} dB"));
    println!("tail peak (last {TAIL_SAMPLES} samples) of the candidate relative to the reference: {change}");
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/compare.rs"]
mod tests;
