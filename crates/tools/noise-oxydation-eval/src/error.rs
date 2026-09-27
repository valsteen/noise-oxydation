//! Errors of the evaluation workflows. Library errors are wrapped with their source preserved.

use std::{error::Error, fmt, io, path::PathBuf};

use noise_oxydation::{ConfigError, StreamError};

use crate::heap::HeapActivity;

/// A failed evaluation workflow.
#[derive(Debug)]
pub enum EvalError {
    /// The command line could not be understood; the message explains the expected usage.
    Usage(String),
    /// Reading or writing a file failed.
    Io {
        /// The file or directory involved.
        path: PathBuf,
        /// The underlying error.
        source: io::Error,
    },
    /// A WAV file could not be read or written.
    Wav {
        /// The WAV file.
        path: PathBuf,
        /// The underlying error.
        source: hound::Error,
    },
    /// A WAV file is not 16-bit integer mono audio at the expected sample rate.
    UnsupportedWav {
        /// The WAV file.
        path: PathBuf,
        /// Its channel count.
        channels: u16,
        /// Its bits per sample.
        bits_per_sample: u16,
        /// Its sample rate in hertz.
        sample_rate: u32,
        /// The sample rate the workflow needs.
        expected_sample_rate: u32,
    },
    /// A source recording is shorter than the excerpt a scenario needs.
    SourceTooShort {
        /// The source.
        source_name: &'static str,
        /// Samples needed.
        needed: usize,
        /// Samples available.
        available: usize,
    },
    /// Trimming a speech lead-in would leave less than the documented margin before the first active frame.
    LeadInTooShort {
        /// The speech source.
        source_name: &'static str,
        /// Samples left before the first active frame after trimming.
        remaining: usize,
        /// The required margin in samples.
        required: usize,
    },
    /// Two files that must have equal length do not.
    LengthMismatch {
        /// Length of the reference file.
        reference: usize,
        /// Length of the candidate file.
        candidate: usize,
    },
    /// A μ-law input file is empty or not a whole number of 160-byte packets.
    NotWholePackets {
        /// The file.
        path: PathBuf,
        /// Its length in bytes.
        bytes: usize,
    },
    /// The enhancer rejected a configuration.
    Config(ConfigError),
    /// The enhancer rejected a packet-path call.
    Stream(StreamError),
    /// The packet path allocated, which the library guarantees it never does.
    PacketPathAllocated {
        /// The configuration being measured.
        configuration: &'static str,
        /// The observed heap activity.
        activity: HeapActivity,
    },
}

impl fmt::Display for EvalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(message) => write!(formatter, "{message}"),
            Self::Io { path, .. } => write!(formatter, "cannot access {}", path.display()),
            Self::Wav { path, .. } => write!(formatter, "cannot process WAV file {}", path.display()),
            Self::UnsupportedWav { path, channels, bits_per_sample, sample_rate, expected_sample_rate } => write!(
                formatter,
                "{} has {channels} channel(s), {bits_per_sample}-bit samples at {sample_rate} Hz; expected 16-bit \
                 mono integer samples at {expected_sample_rate} Hz",
                path.display()
            ),
            Self::SourceTooShort { source_name, needed, available } => {
                write!(formatter, "{source_name} has {available} samples; the scenario needs {needed}")
            }
            Self::LeadInTooShort { source_name, remaining, required } => write!(
                formatter,
                "trimming {source_name} leaves {remaining} samples before the first active frame; at least {required} \
                 are required"
            ),
            Self::LengthMismatch { reference, candidate } => {
                write!(formatter, "the reference has {reference} bytes but the candidate has {candidate}")
            }
            Self::NotWholePackets { path, bytes } => write!(
                formatter,
                "{} has {bytes} bytes; expected a non-empty whole number of 160-byte μ-law packets",
                path.display()
            ),
            Self::Config(_) => write!(formatter, "the enhancer rejected the configuration"),
            Self::Stream(_) => write!(formatter, "the enhancer rejected a packet-path call"),
            Self::PacketPathAllocated { configuration, activity } => {
                write!(formatter, "the {configuration} packet path touched the heap: {activity:?}")
            }
        }
    }
}

impl Error for EvalError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Wav { source, .. } => Some(source),
            Self::Config(source) => Some(source),
            Self::Stream(source) => Some(source),
            _ => None,
        }
    }
}

impl From<ConfigError> for EvalError {
    fn from(error: ConfigError) -> Self {
        Self::Config(error)
    }
}

impl From<StreamError> for EvalError {
    fn from(error: StreamError) -> Self {
        Self::Stream(error)
    }
}
