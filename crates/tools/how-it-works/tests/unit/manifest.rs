use std::path::Path;

use super::{
    Drift, ManifestCrate, ManifestDependency, ManifestError, check_crate_map, diagram_drift, manifest_drift,
    parse_manifest, read_workspace, required_version,
};
use crate::{
    diagrams::crate_map::{DIAGRAM, DependencySource, WORKSPACE_CRATES},
    model::{Connector, Stroke},
};

const EVAL_MANIFEST: &str = r#"
[package]
name = "noise-oxydation-eval"
description = "a = b"

[[bin]]
name = "noise-oxydation-eval"

[features]
stage-timing = ["noise-oxydation/stage-timing"]

[dependencies]
hound = "3.5"
noise-oxydation = { path = "../../core/noise-oxydation" }
log = { version = "0.4", optional = true }

[lints]
workspace = true
"#;

fn manifests() -> Vec<ManifestCrate> {
    read_workspace(&crate::repository_root()).expect("the workspace manifests are readable")
}

#[test]
fn the_committed_workspace_matches_the_crate_map() {
    check_crate_map(&crate::repository_root()).expect("seed, manifests, go.mod and diagram agree");
}

#[test]
fn parses_package_names_and_normal_dependencies() {
    let parsed = parse_manifest(Path::new("Cargo.toml"), EVAL_MANIFEST, "crates/tools/eval".to_owned())
        .expect("the manifest uses supported forms");
    assert_eq!(parsed.package, "noise-oxydation-eval");
    assert_eq!(
        parsed.dependencies,
        [
            ManifestDependency { name: "hound".to_owned(), source: DependencySource::Registry { optional: false } },
            ManifestDependency { name: "noise-oxydation".to_owned(), source: DependencySource::Workspace },
            ManifestDependency { name: "log".to_owned(), source: DependencySource::Registry { optional: true } },
        ]
    );
}

#[test]
fn rejects_dependency_forms_it_cannot_read() {
    for text in [
        "[package]\nname = \"x\"\n[dependencies.hound]\nversion = \"3.5\"\n",
        "[package]\nname = \"x\"\n[target.'cfg(unix)'.dependencies]\nlibc = \"0.2\"\n",
        "[package]\nname = \"x\"\n[dependencies]\nlog = { workspace = true }\n",
    ] {
        let result = parse_manifest(Path::new("Cargo.toml"), text, "crates/x/x".to_owned());
        assert!(matches!(result, Err(ManifestError::UnsupportedLine { .. })), "{text}");
    }
    let unnamed = parse_manifest(Path::new("Cargo.toml"), "[dependencies]\n", "crates/x/x".to_owned());
    assert!(matches!(unnamed, Err(ManifestError::MissingPackageName { .. })));
}

#[test]
fn detects_a_new_dependency_edge_in_a_manifest() {
    let mut manifests = manifests();
    let library = manifests.iter_mut().find(|entry| entry.package == "noise-oxydation").expect("the library exists");
    library
        .dependencies
        .push(ManifestDependency { name: "hound".to_owned(), source: DependencySource::Registry { optional: false } });
    assert_eq!(
        manifest_drift(WORKSPACE_CRATES, &manifests),
        [Drift::DependenciesDiffer {
            package: "noise-oxydation".to_owned(),
            seed: vec!["log (optional)".to_owned()],
            manifest: vec!["hound".to_owned(), "log (optional)".to_owned()],
        }]
    );
}

#[test]
fn detects_added_removed_and_moved_crates() {
    let mut manifests = manifests();
    manifests.retain(|entry| entry.package != "how-it-works");
    manifests.push(ManifestCrate {
        package: "noise-oxydation-capi".to_owned(),
        directory: "crates/bindings/noise-oxydation-capi".to_owned(),
        dependencies: Vec::new(),
    });
    let eval = manifests.iter_mut().find(|entry| entry.package == "noise-oxydation-eval").expect("eval exists");
    eval.directory = "crates/noise-oxydation-eval".to_owned();
    assert_eq!(
        manifest_drift(WORKSPACE_CRATES, &manifests),
        [
            Drift::DirectoryDiffers {
                package: "noise-oxydation-eval".to_owned(),
                seed: "crates/tools/noise-oxydation-eval",
                manifest: "crates/noise-oxydation-eval".to_owned(),
            },
            Drift::CrateOnlyInSeed("how-it-works"),
            Drift::CrateOnlyInManifests("noise-oxydation-capi".to_owned()),
        ]
    );
}

#[test]
fn detects_a_diagram_that_drifts_from_the_seed() {
    // Drawing the optional log dependency solid, and the library depending on the evaluation tool.
    const CONNECTORS: &[Connector] = &[
        Connector { stroke: Stroke::Solid, ..DIAGRAM.connectors[2] },
        Connector {
            source: Some("noise-oxydation"),
            target: Some("noise-oxydation-eval"),
            label: None,
            ..DIAGRAM.connectors[0]
        },
    ];
    assert_eq!(diagram_drift(WORKSPACE_CRATES, &DIAGRAM), []);
    let mut drifted = DIAGRAM;
    drifted.connectors = CONNECTORS;
    drifted.cards = &DIAGRAM.cards[..DIAGRAM.cards.len() - 1];
    assert_eq!(
        diagram_drift(WORKSPACE_CRATES, &drifted),
        [
            Drift::CardMissing("go-reference"),
            Drift::ConnectorMissing { from: "noise-oxydation", to: "log" },
            Drift::ConnectorMissing { from: "noise-oxydation-eval", to: "hound" },
            Drift::ConnectorMissing { from: "noise-oxydation-eval", to: "noise-oxydation" },
            Drift::ConnectorMissing { from: "go-parity", to: "go-reference" },
            Drift::ConnectorUnexpected { from: "noise-oxydation", to: "noise-oxydation-eval" },
        ]
    );
}

#[test]
fn reads_go_module_requirements() {
    let single = "module m\n\ngo 1.26.6\n\nrequire github.com/a/b v1.2.3\n";
    assert_eq!(required_version(single, "github.com/a/b").as_deref(), Some("v1.2.3"));
    let block = "module m\n\nrequire (\n\tgithub.com/c/d v0.1.0 // indirect\n\tgithub.com/a/b v2.0.0\n)\n";
    assert_eq!(required_version(block, "github.com/a/b").as_deref(), Some("v2.0.0"));
    assert_eq!(required_version(block, "github.com/x/y"), None);
}
