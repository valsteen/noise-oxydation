//! WAV (16-bit mono PCM) and headerless μ-law file access.

use std::{
    fs,
    path::{Path, PathBuf},
};

use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use noise_oxydation::PACKET_SAMPLES;

use crate::error::EvalError;

/// Reads a 16-bit integer mono WAV file recorded at `expected_sample_rate` and returns its normalized samples
/// (PCM / 32768).
pub(crate) fn read_wav(path: &Path, expected_sample_rate: u32) -> Result<Vec<f64>, EvalError> {
    let wav_error = |source| EvalError::Wav { path: path.to_path_buf(), source };
    let reader = WavReader::open(path).map_err(wav_error)?;
    let spec = reader.spec();
    if spec.channels != 1
        || spec.bits_per_sample != 16
        || spec.sample_format != SampleFormat::Int
        || spec.sample_rate != expected_sample_rate
    {
        return Err(EvalError::UnsupportedWav {
            path: path.to_path_buf(),
            channels: spec.channels,
            bits_per_sample: spec.bits_per_sample,
            sample_rate: spec.sample_rate,
            expected_sample_rate,
        });
    }
    reader
        .into_samples::<i16>()
        .map(|sample| sample.map(|value| f64::from(value) / 32768.0))
        .collect::<Result<_, _>>()
        .map_err(wav_error)
}

/// Writes 16-bit mono PCM samples as a WAV file.
pub(crate) fn write_wav(path: &Path, sample_rate: u32, samples: &[i16]) -> Result<(), EvalError> {
    let wav_error = |source| EvalError::Wav { path: path.to_path_buf(), source };
    let spec = WavSpec { channels: 1, sample_rate, bits_per_sample: 16, sample_format: SampleFormat::Int };
    let mut writer = WavWriter::create(path, spec).map_err(wav_error)?;
    for &sample in samples {
        writer.write_sample(sample).map_err(wav_error)?;
    }
    writer.finalize().map_err(wav_error)
}

/// Reads a headerless μ-law file that holds a non-empty whole number of packets.
pub(crate) fn read_mulaw_packets(path: &Path) -> Result<Vec<u8>, EvalError> {
    let bytes = read_bytes(path)?;
    if bytes.is_empty() || bytes.len() % PACKET_SAMPLES != 0 {
        return Err(EvalError::NotWholePackets { path: path.to_path_buf(), bytes: bytes.len() });
    }
    Ok(bytes)
}

/// Reads a whole file.
pub(crate) fn read_bytes(path: &Path) -> Result<Vec<u8>, EvalError> {
    fs::read(path).map_err(|source| EvalError::Io { path: path.to_path_buf(), source })
}

/// Writes a whole file.
pub(crate) fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), EvalError> {
    fs::write(path, bytes).map_err(|source| EvalError::Io { path: path.to_path_buf(), source })
}

/// Creates a directory and its parents.
pub(crate) fn create_dir(path: &Path) -> Result<PathBuf, EvalError> {
    fs::create_dir_all(path).map_err(|source| EvalError::Io { path: path.to_path_buf(), source })?;
    Ok(path.to_path_buf())
}

#[cfg(test)]
#[path = "../tests/unit/audio_io.rs"]
mod tests;
