use std::path::Path;

use super::{
    Drift, ManifestCrate, ManifestDependency, ManifestError, check_crate_map, diagram_drift, links_library,
    manifest_drift, module_path, parse_manifest, read_workspace, required_version,
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
fn classifies_inline_tables_by_their_keys_not_by_substrings() {
    let text = r#"
[package]
name = "x"

[dependencies]
serde = { version = "1", features = ["path", "optional = true"] }
pathfinder = { version = "0.5", optional = true }
library = { path = "../library", default-features = false }
"#;
    let parsed = parse_manifest(Path::new("Cargo.toml"), text, "crates/x/x".to_owned()).expect("supported forms");
    assert_eq!(
        parsed.dependencies,
        [
            ManifestDependency { name: "serde".to_owned(), source: DependencySource::Registry { optional: false } },
            ManifestDependency { name: "pathfinder".to_owned(), source: DependencySource::Registry { optional: true } },
            ManifestDependency { name: "library".to_owned(), source: DependencySource::Workspace },
        ]
    );
}

#[test]
fn rejects_dependency_forms_it_cannot_read() {
    for text in [
        "[package]\nname = \"x\"\n[dependencies.hound]\nversion = \"3.5\"\n",
        "[package]\nname = \"x\"\n[target.'cfg(unix)'.dependencies]\nlibc = \"0.2\"\n",
        "[package]\nname = \"x\"\n[dependencies]\nlog = { workspace = true }\n",
        "[package]\nname = \"x\"\n[dependencies]\nlog = { version = \"0.4\", features = [\"std\" }\n",
        "[package]\nname = \"x\"\n[dependencies]\nlog = {\n",
        "[package]\nname = \"x\"\n[dependencies]\nlog = 4\n",
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
        package: "noise-oxydation-wasm".to_owned(),
        directory: "crates/bindings/noise-oxydation-wasm".to_owned(),
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
            Drift::CrateOnlyInManifests("noise-oxydation-wasm".to_owned()),
        ]
    );
}

#[test]
fn detects_a_diagram_that_drifts_from_the_seed() {
    // Drawing the optional log dependency solid, and the library depending on the evaluation tool.
    const CONNECTORS: &[Connector] = &[
        Connector { stroke: Stroke::Solid, ..DIAGRAM.connectors[4] },
        Connector {
            source: Some("noise-oxydation"),
            target: Some("noise-oxydation-eval"),
            label: None,
            ..DIAGRAM.connectors[2]
        },
    ];
    assert_eq!(diagram_drift(WORKSPACE_CRATES, &DIAGRAM), []);
    let mut drifted = DIAGRAM;
    drifted.connectors = CONNECTORS;
    drifted.cards = &DIAGRAM.cards[..DIAGRAM.cards.len() - 1];
    assert_eq!(
        diagram_drift(WORKSPACE_CRATES, &drifted),
        [
            Drift::CardMissing("hound"),
            Drift::ConnectorMissing { from: "noise-oxydation", to: "log" },
            Drift::ConnectorMissing { from: "noise-oxydation-capi", to: "noise-oxydation" },
            Drift::ConnectorMissing { from: "noise-oxydation-eval", to: "hound" },
            Drift::ConnectorMissing { from: "noise-oxydation-eval", to: "noise-oxydation" },
            Drift::ConnectorMissing { from: "go-parity", to: "go-reference" },
            Drift::ConnectorMissing { from: "go-package", to: "noise-oxydation-capi" },
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

#[test]
fn reads_go_module_paths_and_cgo_link_flags() {
    assert_eq!(module_path("// comment\nmodule example.com/m\n\ngo 1.24\n").as_deref(), Some("example.com/m"));
    assert_eq!(module_path("go 1.24\n"), None);

    let source = "package p\n\n/*\n#cgo CFLAGS: -I${SRCDIR}\n#cgo linux LDFLAGS: -lm\n#cgo LDFLAGS: -L${SRCDIR}/lib \
                  -lnoise_oxydation_capi\n*/\nimport \"C\"\n"
        .to_owned();
    assert!(links_library(std::slice::from_ref(&source), "noise_oxydation_capi"));
    assert!(links_library(&["#cgo darwin LDFLAGS: -lfoo -lm".to_owned()], "foo"));
    assert!(!links_library(std::slice::from_ref(&source), "noise_oxydation"));
    assert!(!links_library(&["#cgo CFLAGS: -lnoise_oxydation_capi".to_owned()], "noise_oxydation_capi"));
    assert!(!links_library(&["// -lnoise_oxydation_capi".to_owned()], "noise_oxydation_capi"));
}
