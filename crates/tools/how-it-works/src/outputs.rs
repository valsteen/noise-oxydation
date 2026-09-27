//! The generated files, their paths, and the content-based freshness check.
//!
//! Freshness is the exact generated content: an output is current when the committed file holds exactly the bytes the
//! renderer produces. Regeneration rewrites only files whose content changed.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::{diagrams, error::GenerateError, guide, manifest, model, svg};

/// The guide, relative to the repository root.
pub(crate) const GUIDE_PATH: &str = "HOW_IT_WORKS.md";
/// Directory of the generated SVGs, relative to the repository root. It holds nothing else apart from hidden files.
pub(crate) const ASSET_DIRECTORY: &str = "docs/assets/how-it-works";

/// Path of a diagram's day SVG, relative to the repository root.
pub(crate) fn day_path(slug: &str) -> String {
    format!("{ASSET_DIRECTORY}/{slug}.svg")
}

/// Path of a diagram's night SVG, relative to the repository root.
pub(crate) fn night_path(slug: &str) -> String {
    format!("{ASSET_DIRECTORY}/{slug}-dark.svg")
}

/// One generated file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Output {
    /// Path relative to the repository root, with `/` separators.
    pub(crate) path: String,
    pub(crate) content: String,
}

/// Committed files that differ from the generated ones.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Staleness {
    /// Outputs that are missing or whose content differs.
    pub(crate) differing: Vec<String>,
    /// Files in the asset directory that the renderer does not produce.
    pub(crate) unexpected: Vec<String>,
}

impl Staleness {
    pub(crate) fn is_current(&self) -> bool {
        self.differing.is_empty() && self.unexpected.is_empty()
    }
}

/// Checks the crate-map seed against the workspace under `root`, then renders every output.
pub(crate) fn build(root: &Path) -> Result<Vec<Output>, GenerateError> {
    manifest::check_crate_map(root)?;
    Ok(render_all()?)
}

/// Renders every diagram in both palettes and the guide, sorted by path.
pub(crate) fn render_all() -> Result<Vec<Output>, crate::validate::ValidationError> {
    let mut outputs = Vec::new();
    for diagram in diagrams::ALL {
        outputs.push(Output { path: day_path(diagram.slug), content: svg::render(diagram, &model::DAY)? });
        outputs.push(Output { path: night_path(diagram.slug), content: svg::render(diagram, &model::NIGHT)? });
    }
    outputs.push(Output { path: GUIDE_PATH.to_owned(), content: guide::guide() });
    outputs.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(outputs)
}

/// Compares the files under `root` with `outputs`.
pub(crate) fn staleness(root: &Path, outputs: &[Output]) -> Result<Staleness, GenerateError> {
    let mut report = Staleness::default();
    for output in outputs {
        let path = root.join(&output.path);
        match fs::read_to_string(&path) {
            Ok(committed) if committed == output.content => {}
            Ok(_) => report.differing.push(output.path.clone()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => report.differing.push(output.path.clone()),
            Err(source) => return Err(GenerateError::Io { path, source }),
        }
    }
    report.unexpected = unexpected_assets(root, outputs)?;
    Ok(report)
}

/// Writes every output whose content changed and removes files in the asset directory that are no longer generated.
/// Returns the number of files written or removed.
pub(crate) fn write(root: &Path, outputs: &[Output]) -> Result<usize, GenerateError> {
    let report = staleness(root, outputs)?;
    for output in outputs.iter().filter(|output| report.differing.contains(&output.path)) {
        let path = root.join(&output.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| GenerateError::Io { path: parent.to_path_buf(), source })?;
        }
        fs::write(&path, &output.content).map_err(|source| GenerateError::Io { path, source })?;
    }
    for unexpected in &report.unexpected {
        let path = root.join(unexpected);
        fs::remove_file(&path).map_err(|source| GenerateError::Io { path, source })?;
    }
    Ok(report.differing.len() + report.unexpected.len())
}

fn unexpected_assets(root: &Path, outputs: &[Output]) -> Result<Vec<String>, GenerateError> {
    let directory: PathBuf = root.join(ASSET_DIRECTORY);
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(GenerateError::Io { path: directory, source }),
    };
    let mut unexpected = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| GenerateError::Io { path: directory.clone(), source })?;
        let name = entry.file_name().to_string_lossy().into_owned();
        // Hidden files such as Finder's `.DS_Store` are local clutter that Git ignores, not stale outputs.
        if name.starts_with('.') {
            continue;
        }
        let path = format!("{ASSET_DIRECTORY}/{name}");
        if !outputs.iter().any(|output| output.path == path) {
            unexpected.push(path);
        }
    }
    unexpected.sort();
    Ok(unexpected)
}

#[cfg(test)]
#[path = "../tests/unit/outputs.rs"]
mod tests;
