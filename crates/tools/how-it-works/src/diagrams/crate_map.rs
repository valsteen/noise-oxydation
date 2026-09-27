//! Crate map: the workspace crates grouped by theme, their dependencies in dependency direction, and the two Go modules
//! beside the workspace: the `noiseox` package that links the C ABI crate, and the Go parity harness.
//!
//! [`WORKSPACE_CRATES`] and the Go constants are the semantic seed. [`crate::manifest`] rejects generation when they
//! differ from the Cargo manifests, the Go modules, or the cards and connectors below.

use crate::model::{
    Anchor, Card, Connector, ConnectorLabel, Diagram, Emphasis, Guide, Note, NoteStyle, Section, Stroke,
};

/// Where a dependency comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DependencySource {
    /// Another workspace crate, by path.
    Workspace,
    /// A crates.io crate.
    Registry { optional: bool },
}

/// One normal (non-dev, non-build) dependency of a workspace crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Dependency {
    pub(crate) name: &'static str,
    pub(crate) source: DependencySource,
}

/// One workspace crate as the diagram shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorkspaceCrate {
    pub(crate) package: &'static str,
    /// Directory relative to the repository root.
    pub(crate) directory: &'static str,
    pub(crate) dependencies: &'static [Dependency],
}

/// The workspace crates and their normal dependencies, in the order the diagram lists them.
pub(crate) const WORKSPACE_CRATES: &[WorkspaceCrate] = &[
    WorkspaceCrate {
        package: "noise-oxydation",
        directory: "crates/core/noise-oxydation",
        dependencies: &[Dependency { name: "log", source: DependencySource::Registry { optional: true } }],
    },
    WorkspaceCrate {
        package: "noise-oxydation-capi",
        directory: "crates/bindings/noise-oxydation-capi",
        dependencies: &[Dependency { name: "noise-oxydation", source: DependencySource::Workspace }],
    },
    WorkspaceCrate {
        package: "noise-oxydation-eval",
        directory: "crates/tools/noise-oxydation-eval",
        dependencies: &[
            Dependency { name: "hound", source: DependencySource::Registry { optional: false } },
            Dependency { name: "noise-oxydation", source: DependencySource::Workspace },
        ],
    },
    WorkspaceCrate { package: "how-it-works", directory: "crates/tools/how-it-works", dependencies: &[] },
];

/// The Go module that `tools/go-parity` requires, and its pinned pseudo-version.
pub(crate) const GO_REFERENCE_MODULE: &str = "github.com/sghaida/noise-cancelation";
pub(crate) const GO_REFERENCE_VERSION: &str = "v0.0.0-20260920200827-cfc7520a0625";

/// Card key of the Go parity harness, which the Go manifest check reads.
pub(crate) const GO_PARITY_CARD: &str = "go-parity";
/// Card key of the Go reference module.
pub(crate) const GO_REFERENCE_CARD: &str = "go-reference";

/// Module path of the Go package in `go/`, as its `go.mod` declares it.
pub(crate) const GO_PACKAGE_MODULE: &str = "github.com/valsteen/noise-oxydation/go";
/// Card key of the Go package.
pub(crate) const GO_PACKAGE_CARD: &str = "go-package";
/// The workspace crate whose static library the Go package links through cgo.
pub(crate) const GO_PACKAGE_LINKS: &str = "noise-oxydation-capi";

pub(crate) const DIAGRAM: Diagram = Diagram {
    slug: "crate-map",
    title: "Crate map",
    description: "The Go modules, the workspace crates grouped into tools and bindings and core, and their external \
                  dependencies. The noiseox Go package links the noise-oxydation-capi static library through cgo, and \
                  the C ABI crate depends on the library with its default features off. The evaluation tool depends \
                  on the library and on hound; the library depends only on the optional log facade; the guide \
                  renderer has no dependency. The go-parity harness depends on the pinned Go reference module. Arrows \
                  point from a crate or module to what it depends on.",
    width: 880,
    height: 824,
    sections: &[
        Section { label: "Go modules", subtitle: &[], x: 24, y: 44 },
        Section { label: "Tools and bindings", subtitle: &[], x: 24, y: 252 },
        Section { label: "Core", subtitle: &[], x: 24, y: 468 },
        Section { label: "External crates", subtitle: &[], x: 24, y: 668 },
    ],
    guides: &[
        Guide { start: (124, 40), end: (856, 40) },
        Guide { start: (188, 248), end: (856, 248) },
        Guide { start: (76, 464), end: (856, 464) },
        Guide { start: (168, 664), end: (856, 664) },
    ],
    connectors: &[
        Connector {
            points: &[(144, 208), (144, 276)],
            source: Some(GO_PACKAGE_CARD),
            target: Some(GO_PACKAGE_LINKS),
            label: Some(ConnectorLabel { text: "cgo, static", position: (206, 232) }),
            stroke: Stroke::Solid,
            arrow: true,
        },
        Connector {
            points: &[(144, 424), (144, 492)],
            source: Some(GO_PACKAGE_LINKS),
            target: Some("noise-oxydation"),
            label: Some(ConnectorLabel { text: "no default features", position: (232, 448) }),
            stroke: Stroke::Solid,
            arrow: true,
        },
        Connector {
            points: &[(376, 424), (376, 492)],
            source: Some("noise-oxydation-eval"),
            target: Some("noise-oxydation"),
            label: None,
            stroke: Stroke::Solid,
            arrow: true,
        },
        Connector {
            points: &[(440, 424), (440, 692)],
            source: Some("noise-oxydation-eval"),
            target: Some("hound"),
            label: None,
            stroke: Stroke::Solid,
            arrow: true,
        },
        Connector {
            points: &[(144, 620), (144, 692)],
            source: Some("noise-oxydation"),
            target: Some("log"),
            label: Some(ConnectorLabel { text: "optional", position: (188, 650) }),
            stroke: Stroke::Dashed,
            arrow: true,
        },
        Connector {
            points: &[(560, 134), (592, 134)],
            source: Some(GO_PARITY_CARD),
            target: Some(GO_REFERENCE_CARD),
            label: None,
            stroke: Stroke::Solid,
            arrow: true,
        },
    ],
    cards: &[
        Card {
            key: GO_PACKAGE_CARD,
            label: "Go package · go/",
            title: "noiseox",
            details: &["one Call per call, cgo hidden", "no allocation per packet"],
            meta: &["github.com/valsteen/", "noise-oxydation/go"],
            x: 24,
            y: 60,
            width: 288,
            height: 148,
            emphasis: Emphasis::Primary,
        },
        Card {
            key: GO_PARITY_CARD,
            label: "Go module · evidence",
            title: "go-parity",
            details: &["runs the Go reference for", "parity and timing evidence"],
            meta: &["tools/go-parity"],
            x: 336,
            y: 60,
            width: 224,
            height: 148,
            emphasis: Emphasis::Secondary,
        },
        Card {
            key: GO_REFERENCE_CARD,
            label: "Go module · reference",
            title: "noise-cancelation",
            details: &["sghaida, pinned at cfc7520"],
            meta: &[GO_REFERENCE_VERSION],
            x: 592,
            y: 60,
            width: 264,
            height: 148,
            emphasis: Emphasis::Secondary,
        },
        Card {
            key: GO_PACKAGE_LINKS,
            label: "Static library",
            title: "noise-oxydation-capi",
            details: &["C ABI of the per-call packet API", "its unsafe code stays in this crate"],
            meta: &["crates/bindings/noise-oxydation-capi"],
            x: 24,
            y: 276,
            width: 288,
            height: 148,
            emphasis: Emphasis::Primary,
        },
        Card {
            key: "noise-oxydation-eval",
            label: "Binary · crates/tools",
            title: "noise-oxydation-eval",
            details: &["replays real speech, compares", "outputs, benchmarks packets"],
            meta: &["crates/tools/noise-oxydation-eval"],
            x: 336,
            y: 276,
            width: 264,
            height: 148,
            emphasis: Emphasis::Primary,
        },
        Card {
            key: "how-it-works",
            label: "Binary · crates/tools",
            title: "how-it-works",
            details: &["generates this guide and", "its day and night diagrams", "std only, no dependencies"],
            meta: &["crates/tools/how-it-works"],
            x: 624,
            y: 276,
            width: 232,
            height: 148,
            emphasis: Emphasis::Primary,
        },
        Card {
            key: "noise-oxydation",
            label: "Library · crates/core",
            title: "noise-oxydation",
            details: &["one CallEnhancer per call", "every DSP stage is a module"],
            meta: &["crates/core/noise-oxydation"],
            x: 24,
            y: 492,
            width: 392,
            height: 128,
            emphasis: Emphasis::Primary,
        },
        Card {
            key: "log",
            label: "Crate · optional",
            title: "log 0.4",
            details: &["facade behind the log feature"],
            meta: &[],
            x: 24,
            y: 692,
            width: 240,
            height: 108,
            emphasis: Emphasis::Secondary,
        },
        Card {
            key: "hound",
            label: "Crate",
            title: "hound 3.5",
            details: &["WAV reading and writing"],
            meta: &[],
            x: 336,
            y: 692,
            width: 216,
            height: 108,
            emphasis: Emphasis::Secondary,
        },
    ],
    notes: &[
        Note {
            text: "Arrows point from a crate or module to what it",
            x: 472,
            y: 516,
            anchor: Anchor::Start,
            style: NoteStyle::Plain,
        },
        Note {
            text: "depends on. Tools and bindings depend on the",
            x: 472,
            y: 534,
            anchor: Anchor::Start,
            style: NoteStyle::Plain,
        },
        Note { text: "library, never the reverse.", x: 472, y: 552, anchor: Anchor::Start, style: NoteStyle::Plain },
        Note {
            text: "go-parity drives the Go reference; nothing in the",
            x: 472,
            y: 582,
            anchor: Anchor::Start,
            style: NoteStyle::Plain,
        },
        Note { text: "workspace depends on it.", x: 472, y: 600, anchor: Anchor::Start, style: NoteStyle::Plain },
    ],
};
