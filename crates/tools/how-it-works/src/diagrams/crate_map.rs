//! Crate map: the workspace crates grouped by theme, their dependencies in dependency direction, and the Go parity
//! harness beside the workspace.
//!
//! [`WORKSPACE_CRATES`] is the semantic seed. [`crate::manifest`] rejects generation when it differs from the Cargo
//! manifests or from the cards and connectors below.

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

pub(crate) const DIAGRAM: Diagram = Diagram {
    slug: "crate-map",
    title: "Crate map",
    description: "The workspace crates grouped into tools and core. The evaluation tool depends on the library and on \
                  hound; the library depends only on the optional log facade; the guide renderer has no dependency. \
                  Outside the workspace, the go-parity harness depends on the pinned Go reference module. Arrows \
                  point from a crate to what it depends on.",
    width: 880,
    height: 608,
    sections: &[
        Section { label: "Tools", subtitle: &[], x: 24, y: 44 },
        Section { label: "Core", subtitle: &[], x: 24, y: 252 },
        Section { label: "External", subtitle: &[], x: 24, y: 452 },
    ],
    guides: &[
        Guide { start: (84, 40), end: (856, 40) },
        Guide { start: (76, 248), end: (856, 248) },
        Guide { start: (108, 448), end: (856, 448) },
    ],
    connectors: &[
        Connector {
            points: &[(156, 208), (156, 276)],
            source: Some("noise-oxydation-eval"),
            target: Some("noise-oxydation"),
            label: None,
            stroke: Stroke::Solid,
            arrow: true,
        },
        Connector {
            points: &[(336, 208), (336, 476)],
            source: Some("noise-oxydation-eval"),
            target: Some("hound"),
            label: None,
            stroke: Stroke::Solid,
            arrow: true,
        },
        Connector {
            points: &[(144, 404), (144, 476)],
            source: Some("noise-oxydation"),
            target: Some("log"),
            label: Some(ConnectorLabel { text: "optional", position: (188, 434) }),
            stroke: Stroke::Dashed,
            arrow: true,
        },
        Connector {
            points: &[(800, 208), (800, 476)],
            source: Some(GO_PARITY_CARD),
            target: Some(GO_REFERENCE_CARD),
            label: None,
            stroke: Stroke::Solid,
            arrow: true,
        },
    ],
    cards: &[
        Card {
            key: "noise-oxydation-eval",
            label: "Binary · crates/tools",
            title: "noise-oxydation-eval",
            details: &["replays real speech, compares outputs,", "benchmarks the packet path"],
            meta: &["crates/tools/noise-oxydation-eval"],
            x: 24,
            y: 60,
            width: 336,
            height: 148,
            emphasis: Emphasis::Primary,
        },
        Card {
            key: "how-it-works",
            label: "Binary · crates/tools",
            title: "how-it-works",
            details: &["generates this guide and", "its day and night diagrams", "std only, no dependencies"],
            meta: &["crates/tools/how-it-works"],
            x: 384,
            y: 60,
            width: 216,
            height: 148,
            emphasis: Emphasis::Primary,
        },
        Card {
            key: GO_PARITY_CARD,
            label: "Go module · evidence",
            title: "go-parity",
            details: &["runs the Go reference for", "parity and timing evidence", "outside the Cargo workspace"],
            meta: &["tools/go-parity"],
            x: 624,
            y: 60,
            width: 232,
            height: 148,
            emphasis: Emphasis::Secondary,
        },
        Card {
            key: "noise-oxydation",
            label: "Library · crates/core",
            title: "noise-oxydation",
            details: &["one CallEnhancer per call", "every DSP stage is a module"],
            meta: &["crates/core/noise-oxydation"],
            x: 24,
            y: 276,
            width: 264,
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
            y: 476,
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
            x: 312,
            y: 476,
            width: 216,
            height: 108,
            emphasis: Emphasis::Secondary,
        },
        Card {
            key: GO_REFERENCE_CARD,
            label: "Go module · reference",
            title: "noise-cancelation",
            details: &["sghaida, pinned at cfc7520"],
            meta: &[GO_REFERENCE_VERSION],
            x: 576,
            y: 476,
            width: 280,
            height: 108,
            emphasis: Emphasis::Secondary,
        },
    ],
    notes: &[
        Note {
            text: "Arrows point from a crate to what it depends on.",
            x: 360,
            y: 316,
            anchor: Anchor::Start,
            style: NoteStyle::Plain,
        },
        Note {
            text: "Tools depend on the library, never the reverse.",
            x: 360,
            y: 338,
            anchor: Anchor::Start,
            style: NoteStyle::Plain,
        },
        Note {
            text: "go-parity drives the Go reference; nothing in the",
            x: 360,
            y: 368,
            anchor: Anchor::Start,
            style: NoteStyle::Plain,
        },
        Note { text: "workspace depends on it.", x: 360, y: 386, anchor: Anchor::Start, style: NoteStyle::Plain },
    ],
};
