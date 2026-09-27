//! `how-it-works`: generates `HOW_IT_WORKS.md` and its day and night SVG diagrams from the seeds in `src/diagrams/`.
//!
//! `cargo run --locked -p how-it-works` regenerates the outputs; `cargo run --locked -p how-it-works -- --check` fails
//! when a committed output differs from what the seeds produce. Both first check the crate-map seed against the
//! workspace manifests. See `docs/how-it-works.md` for the visual grammar and the update procedure.

#![forbid(unsafe_code)]

mod diagrams;
mod error;
mod guide;
mod layout;
mod manifest;
mod model;
mod outputs;
mod svg;
mod validate;

use std::{
    error::Error,
    path::{Path, PathBuf},
    process::ExitCode,
};

use crate::error::GenerateError;

const REGENERATE: &str = "cargo run --locked -p how-it-works";

/// What the invocation should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Rewrite outputs whose content changed.
    Generate,
    /// Report outputs that differ from the seeds, changing nothing.
    Check,
}

fn main() -> ExitCode {
    let mut arguments = std::env::args().skip(1);
    let mode = match (arguments.next().as_deref(), arguments.next()) {
        (None, _) => Mode::Generate,
        (Some("--check"), None) => Mode::Check,
        _ => {
            eprintln!("usage: how-it-works [--check]");
            return ExitCode::from(2);
        }
    };
    match run(&repository_root(), mode) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error}");
            let mut source = error.source();
            while let Some(cause) = source {
                eprintln!("  caused by: {cause}");
                source = cause.source();
            }
            ExitCode::FAILURE
        }
    }
}

fn run(root: &Path, mode: Mode) -> Result<ExitCode, GenerateError> {
    let outputs = outputs::build(root)?;
    if mode == Mode::Generate {
        let changed = outputs::write(root, &outputs)?;
        println!("{} generated files, {changed} written or removed", outputs.len());
        return Ok(ExitCode::SUCCESS);
    }
    let staleness = outputs::staleness(root, &outputs)?;
    if staleness.is_current() {
        println!("HOW_IT_WORKS.md and its {} diagram files are current", outputs.len() - 1);
        return Ok(ExitCode::SUCCESS);
    }
    println!("the committed guide differs from its seeds:");
    for path in &staleness.differing {
        println!("  differs: {path}");
    }
    for path in &staleness.unexpected {
        println!("  not generated: {path}");
    }
    println!("regenerate and commit the outputs: {REGENERATE}");
    Ok(ExitCode::FAILURE)
}

/// The repository root: this crate lives in `crates/tools/how-it-works`.
fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
