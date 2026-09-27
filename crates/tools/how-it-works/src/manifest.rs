//! Keeps the crate-map seed honest: the workspace manifests, the Go parity harness's `go.mod` and the crate-map
//! diagram must all agree with [`crate_map::WORKSPACE_CRATES`]. Generation and `--check` stop on any drift.
//!
//! The manifest reader understands the plain forms this workspace uses: `[package]` `name`, and `[dependencies]` entries
//! written as `name = "version"` or `name = { ... }` inline tables. Other dependency forms are rejected rather than
//! silently skipped, so a new form cannot hide a dependency from the diagram.

use std::{
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use crate::{
    diagrams::crate_map::{
        self, DependencySource, GO_PARITY_CARD, GO_REFERENCE_CARD, GO_REFERENCE_MODULE, GO_REFERENCE_VERSION,
        WorkspaceCrate,
    },
    model::{Diagram, Stroke},
};

/// The workspace member pattern the crate map assumes: thematic groups under `crates/`.
const MEMBERS_LINE: &str = r#"members = ["crates/*/*"]"#;
const GO_MANIFEST: &str = "tools/go-parity/go.mod";

/// A crate as its Cargo manifest declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManifestCrate {
    pub(crate) package: String,
    /// Directory relative to the repository root, with `/` separators.
    pub(crate) directory: String,
    pub(crate) dependencies: Vec<ManifestDependency>,
}

/// A normal dependency as a Cargo manifest declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManifestDependency {
    pub(crate) name: String,
    pub(crate) source: DependencySource,
}

/// One disagreement between the crate-map seed and what it describes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Drift {
    /// The seed lists a crate that no manifest declares.
    CrateOnlyInSeed(&'static str),
    /// A manifest declares a crate that the seed does not list.
    CrateOnlyInManifests(String),
    /// A crate lives in another directory than the seed says.
    DirectoryDiffers { package: String, seed: &'static str, manifest: String },
    /// A crate's normal dependencies differ from the seed.
    DependenciesDiffer { package: String, seed: Vec<String>, manifest: Vec<String> },
    /// The diagram has no card for a crate or dependency of the seed.
    CardMissing(&'static str),
    /// The diagram lacks the connector for a dependency, or draws it with the wrong stroke.
    ConnectorMissing { from: &'static str, to: &'static str },
    /// The diagram draws a connector between seed cards that is not a dependency of the seed.
    ConnectorUnexpected { from: &'static str, to: &'static str },
    /// `tools/go-parity/go.mod` does not require the Go reference version the diagram shows.
    GoReferenceDiffers { manifest: Option<String> },
}

impl fmt::Display for Drift {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CrateOnlyInSeed(package) => write!(formatter, "the seed lists {package}, which no manifest declares"),
            Self::CrateOnlyInManifests(package) => {
                write!(formatter, "the workspace declares {package}, which the seed does not list")
            }
            Self::DirectoryDiffers { package, seed, manifest } => {
                write!(formatter, "{package} lives in {manifest}, not {seed}")
            }
            Self::DependenciesDiffer { package, seed, manifest } => write!(
                formatter,
                "{package} depends on [{}] in its manifest but on [{}] in the seed",
                manifest.join(", "),
                seed.join(", ")
            ),
            Self::CardMissing(key) => write!(formatter, "the crate map has no card for {key}"),
            Self::ConnectorMissing { from, to } => {
                write!(formatter, "the crate map lacks the {from} -> {to} dependency (solid, or dashed when optional)")
            }
            Self::ConnectorUnexpected { from, to } => {
                write!(formatter, "the crate map draws {from} -> {to}, which is not a dependency")
            }
            Self::GoReferenceDiffers { manifest } => match manifest {
                Some(version) => write!(
                    formatter,
                    "{GO_MANIFEST} requires {GO_REFERENCE_MODULE} {version}, not {GO_REFERENCE_VERSION}"
                ),
                None => write!(formatter, "{GO_MANIFEST} does not require {GO_REFERENCE_MODULE}"),
            },
        }
    }
}

/// The crate map could not be checked, or it drifted.
#[derive(Debug)]
pub(crate) enum ManifestError {
    /// Reading a manifest or listing a directory failed.
    Io { path: PathBuf, source: io::Error },
    /// The root manifest does not declare the expected workspace members.
    UnexpectedMembers { path: PathBuf },
    /// A manifest uses a form this reader does not understand.
    UnsupportedLine { path: PathBuf, line: String },
    /// A crate manifest has no `[package]` name.
    MissingPackageName { path: PathBuf },
    /// The seed disagrees with the manifests or the diagram.
    Drift(Vec<Drift>),
}

impl fmt::Display for ManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, .. } => write!(formatter, "cannot read {}", path.display()),
            Self::UnexpectedMembers { path } => {
                write!(formatter, "{} does not declare `{MEMBERS_LINE}`", path.display())
            }
            Self::UnsupportedLine { path, line } => {
                write!(formatter, "{} uses a form the crate-map check does not read: {line}", path.display())
            }
            Self::MissingPackageName { path } => write!(formatter, "{} has no [package] name", path.display()),
            Self::Drift(drifts) => {
                write!(formatter, "the crate-map seed drifted:")?;
                for drift in drifts {
                    write!(formatter, "\n  - {drift}")?;
                }
                Ok(())
            }
        }
    }
}

impl Error for ManifestError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::UnexpectedMembers { .. }
            | Self::UnsupportedLine { .. }
            | Self::MissingPackageName { .. }
            | Self::Drift(_) => None,
        }
    }
}

/// Reads the workspace under `root` and fails unless the crate-map seed, the manifests and the diagram agree.
pub(crate) fn check_crate_map(root: &Path) -> Result<(), ManifestError> {
    let manifests = read_workspace(root)?;
    let go_manifest = read(&root.join(GO_MANIFEST))?;
    let mut drifts = manifest_drift(crate_map::WORKSPACE_CRATES, &manifests);
    drifts.extend(diagram_drift(crate_map::WORKSPACE_CRATES, &crate_map::DIAGRAM));
    let go_reference = required_version(&go_manifest, GO_REFERENCE_MODULE);
    if go_reference.as_deref() != Some(GO_REFERENCE_VERSION) {
        drifts.push(Drift::GoReferenceDiffers { manifest: go_reference });
    }
    if drifts.is_empty() { Ok(()) } else { Err(ManifestError::Drift(drifts)) }
}

/// Reads every crate manifest matched by the workspace member pattern `crates/*/*`, sorted by directory.
pub(crate) fn read_workspace(root: &Path) -> Result<Vec<ManifestCrate>, ManifestError> {
    let root_manifest = root.join("Cargo.toml");
    if !read(&root_manifest)?.lines().any(|line| line.trim() == MEMBERS_LINE) {
        return Err(ManifestError::UnexpectedMembers { path: root_manifest });
    }
    let mut crates = Vec::new();
    for group in sorted_directories(&root.join("crates"))? {
        for directory in sorted_directories(&group)? {
            let path = directory.join("Cargo.toml");
            if path.is_file() {
                let relative = directory.strip_prefix(root).unwrap_or(&directory);
                let relative = relative.components().map(|part| part.as_os_str().to_string_lossy()).collect::<Vec<_>>();
                crates.push(parse_manifest(&path, &read(&path)?, relative.join("/"))?);
            }
        }
    }
    Ok(crates)
}

/// Parses one crate manifest whose directory, relative to the repository root, is `directory`.
pub(crate) fn parse_manifest(path: &Path, text: &str, directory: String) -> Result<ManifestCrate, ManifestError> {
    let unsupported = |line: &str| ManifestError::UnsupportedLine { path: path.to_path_buf(), line: line.to_owned() };
    let mut table = String::new();
    let mut package = None;
    let mut dependencies = Vec::new();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#')) {
        if let Some(header) = line.strip_prefix('[') {
            header.trim_end_matches(']').trim().clone_into(&mut table);
            if table.starts_with("dependencies.") || table.starts_with("target.") {
                return Err(unsupported(line));
            }
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let (key, value) = (key.trim(), value.trim());
        if table == "package" && key == "name" {
            package = Some(value.trim_matches('"').to_owned());
        } else if table == "dependencies" {
            if value.contains("workspace") {
                return Err(unsupported(line));
            }
            let source = if value.contains("path") {
                DependencySource::Workspace
            } else {
                DependencySource::Registry { optional: value.contains("optional = true") }
            };
            dependencies.push(ManifestDependency { name: key.to_owned(), source });
        }
    }
    let package = package.ok_or_else(|| ManifestError::MissingPackageName { path: path.to_path_buf() })?;
    Ok(ManifestCrate { package, directory, dependencies })
}

/// Every difference between the seed and the manifests.
pub(crate) fn manifest_drift(seed: &[WorkspaceCrate], manifests: &[ManifestCrate]) -> Vec<Drift> {
    let mut drifts = Vec::new();
    for entry in seed {
        let Some(manifest) = manifests.iter().find(|manifest| manifest.package == entry.package) else {
            drifts.push(Drift::CrateOnlyInSeed(entry.package));
            continue;
        };
        if manifest.directory != entry.directory {
            drifts.push(Drift::DirectoryDiffers {
                package: manifest.package.clone(),
                seed: entry.directory,
                manifest: manifest.directory.clone(),
            });
        }
        let seed_dependencies =
            sorted(entry.dependencies.iter().map(|dependency| describe(dependency.name, dependency.source)));
        let manifest_dependencies =
            sorted(manifest.dependencies.iter().map(|dependency| describe(&dependency.name, dependency.source)));
        if seed_dependencies != manifest_dependencies {
            drifts.push(Drift::DependenciesDiffer {
                package: manifest.package.clone(),
                seed: seed_dependencies,
                manifest: manifest_dependencies,
            });
        }
    }
    for manifest in manifests {
        if !seed.iter().any(|entry| entry.package == manifest.package) {
            drifts.push(Drift::CrateOnlyInManifests(manifest.package.clone()));
        }
    }
    drifts
}

/// Every difference between the seed and the crate-map diagram's cards and connectors.
///
/// Each crate and each dependency needs a card keyed by its name, and each dependency a connector from the crate's card
/// to the dependency's card, dashed exactly when the dependency is optional. No other connector may join two of those
/// cards. The Go harness and the Go reference module are joined by one solid connector.
pub(crate) fn diagram_drift(seed: &[WorkspaceCrate], diagram: &Diagram) -> Vec<Drift> {
    let mut expected = Vec::new();
    for entry in seed {
        for dependency in entry.dependencies {
            let optional = dependency.source == (DependencySource::Registry { optional: true });
            expected.push((entry.package, dependency.name, if optional { Stroke::Dashed } else { Stroke::Solid }));
        }
    }
    expected.push((GO_PARITY_CARD, GO_REFERENCE_CARD, Stroke::Solid));
    let mut keys: Vec<&'static str> = expected.iter().flat_map(|&(from, to, _)| [from, to]).collect();
    keys.extend(seed.iter().map(|entry| entry.package));
    keys.sort_unstable();
    keys.dedup();
    let mut drifts: Vec<Drift> = keys
        .iter()
        .filter(|&&key| !diagram.cards.iter().any(|card| card.key == key))
        .map(|&key| Drift::CardMissing(key))
        .collect();
    for &(from, to, stroke) in &expected {
        let drawn = diagram.connectors.iter().any(|connector| {
            connector.source == Some(from) && connector.target == Some(to) && connector.stroke == stroke
        });
        if !drawn {
            drifts.push(Drift::ConnectorMissing { from, to });
        }
    }
    for connector in diagram.connectors {
        if let (Some(from), Some(to)) = (connector.source, connector.target) {
            let joins_seed_cards = keys.contains(&from) && keys.contains(&to);
            if joins_seed_cards && !expected.iter().any(|&(source, target, _)| source == from && target == to) {
                drifts.push(Drift::ConnectorUnexpected { from, to });
            }
        }
    }
    drifts
}

/// The version with which a `go.mod` requires `module`, in a single-line `require` or inside a `require ( ... )` block.
pub(crate) fn required_version(go_manifest: &str, module: &str) -> Option<String> {
    go_manifest.lines().find_map(|line| {
        let mut words = line.split_whitespace();
        let first = words.next()?;
        let name = if first == "require" { words.next()? } else { first };
        if name == module { words.next().map(str::to_owned) } else { None }
    })
}

fn describe(name: &str, source: DependencySource) -> String {
    match source {
        DependencySource::Workspace => format!("{name} (workspace)"),
        DependencySource::Registry { optional: true } => format!("{name} (optional)"),
        DependencySource::Registry { optional: false } => name.to_owned(),
    }
}

fn sorted(items: impl Iterator<Item = String>) -> Vec<String> {
    let mut items: Vec<String> = items.collect();
    items.sort();
    items
}

fn sorted_directories(path: &Path) -> Result<Vec<PathBuf>, ManifestError> {
    let entries = fs::read_dir(path).map_err(|source| ManifestError::Io { path: path.to_path_buf(), source })?;
    let mut directories = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| ManifestError::Io { path: path.to_path_buf(), source })?;
        if entry.path().is_dir() {
            directories.push(entry.path());
        }
    }
    directories.sort();
    Ok(directories)
}

fn read(path: &Path) -> Result<String, ManifestError> {
    fs::read_to_string(path).map_err(|source| ManifestError::Io { path: path.to_path_buf(), source })
}

#[cfg(test)]
#[path = "../tests/unit/manifest.rs"]
mod tests;
