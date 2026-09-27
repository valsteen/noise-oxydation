//! Errors of generation and the freshness check.

use std::{error::Error, fmt, io, path::PathBuf};

use crate::{manifest::ManifestError, validate::ValidationError};

/// Why the guide could not be generated or checked.
#[derive(Debug)]
pub(crate) enum GenerateError {
    /// A diagram breaks the visual grammar.
    Validation(ValidationError),
    /// The crate-map seed could not be checked or drifted from the workspace.
    Manifest(ManifestError),
    /// Reading or writing an output failed.
    Io { path: PathBuf, source: io::Error },
}

impl fmt::Display for GenerateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(_) => write!(formatter, "a diagram breaks the visual grammar"),
            Self::Manifest(_) => write!(formatter, "the crate map cannot be checked against the workspace"),
            Self::Io { path, .. } => write!(formatter, "cannot access {}", path.display()),
        }
    }
}

impl Error for GenerateError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Validation(error) => Some(error),
            Self::Manifest(error) => Some(error),
            Self::Io { source, .. } => Some(source),
        }
    }
}

impl From<ValidationError> for GenerateError {
    fn from(error: ValidationError) -> Self {
        Self::Validation(error)
    }
}

impl From<ManifestError> for GenerateError {
    fn from(error: ManifestError) -> Self {
        Self::Manifest(error)
    }
}
