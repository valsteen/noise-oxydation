//! Diagram primitives and the named day and night palettes.
//!
//! A diagram is authored per seed as fixed geometry: cards, section headings, guide lines, connectors and notes. The
//! renderer receives one complete [`Palette`], so the day and night outputs of a diagram differ only in palette values.

/// A point in SVG user units (1 unit = 1 CSS px at the design width).
pub(crate) type Point = (i32, i32);

/// Every color a diagram uses. Drawing code reads colors only from here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Palette {
    pub(crate) canvas: &'static str,
    pub(crate) grid: &'static str,
    pub(crate) frame: &'static str,
    pub(crate) card: &'static str,
    pub(crate) card_border: &'static str,
    pub(crate) text: &'static str,
    pub(crate) label: &'static str,
    pub(crate) muted_text: &'static str,
    pub(crate) guide: &'static str,
    pub(crate) connector: &'static str,
    pub(crate) primary: &'static str,
    pub(crate) secondary: &'static str,
}

/// Warm paper tones for GitHub's light reading surface.
pub(crate) const DAY: Palette = Palette {
    canvas: "#fffdf7",
    grid: "#e8e1d5",
    frame: "#cec7b9",
    card: "#fff",
    card_border: "#aaa397",
    text: "#292721",
    label: "#6d665b",
    muted_text: "#625d53",
    guide: "#d3ccc0",
    connector: "#697487",
    primary: "#394a62",
    secondary: "#81796f",
};

/// Muted navy tones chosen against GitHub's dark reading surface.
pub(crate) const NIGHT: Palette = Palette {
    canvas: "#1c2127",
    grid: "#45535f",
    frame: "#45515c",
    card: "#242b32",
    card_border: "#5e6a74",
    text: "#edf1f2",
    label: "#b7c0c5",
    muted_text: "#c5cdd0",
    guide: "#333b42",
    connector: "#9ba8b1",
    primary: "#91a7bb",
    secondary: "#9b948b",
};

/// Which accent the card's left bar carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Emphasis {
    /// The subject of the diagram.
    Primary,
    /// Context around the subject, such as external dependencies.
    Secondary,
}

/// A flat rectangular card: a monospaced label, a bold title, quieter details and meta lines.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Card {
    /// Identifies the card for connector endpoints; unique within a diagram.
    pub(crate) key: &'static str,
    /// Small monospaced heading, rendered in capitals; empty for none.
    pub(crate) label: &'static str,
    pub(crate) title: &'static str,
    pub(crate) details: &'static [&'static str],
    /// Monospaced lines at the bottom of the card, such as a path or a type name.
    pub(crate) meta: &'static [&'static str],
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: i32,
    pub(crate) height: i32,
    pub(crate) emphasis: Emphasis,
}

/// A group heading on the canvas, with optional subtitle lines below it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Section {
    pub(crate) label: &'static str,
    pub(crate) subtitle: &'static [&'static str],
    pub(crate) x: i32,
    pub(crate) y: i32,
}

/// A thin line that separates groups.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Guide {
    pub(crate) start: Point,
    pub(crate) end: Point,
}

/// Solid or dashed connector stroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stroke {
    Solid,
    /// A relationship that exists only conditionally, such as an optional dependency or a skipped path.
    Dashed,
}

/// A connector label and the explicit position of its center baseline.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ConnectorLabel {
    pub(crate) text: &'static str,
    pub(crate) position: Point,
}

/// An orthogonal route between two card ports (or from or to open canvas), drawn with soft elbows.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Connector {
    pub(crate) points: &'static [Point],
    pub(crate) source: Option<&'static str>,
    pub(crate) target: Option<&'static str>,
    pub(crate) label: Option<ConnectorLabel>,
    pub(crate) stroke: Stroke,
    /// Whether the route ends in a filled arrowhead.
    pub(crate) arrow: bool,
}

/// Horizontal alignment of a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Anchor {
    Start,
    Middle,
}

/// Free-standing canvas text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NoteStyle {
    /// Quiet explanatory sentence.
    Plain,
    /// Bold monospaced marker, such as a lane heading.
    Meta,
}

/// A line of text on the canvas, painted over geometry with a canvas-colored halo.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Note {
    pub(crate) text: &'static str,
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) anchor: Anchor,
    pub(crate) style: NoteStyle,
}

/// One authored diagram.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Diagram {
    /// File stem of the generated SVGs.
    pub(crate) slug: &'static str,
    /// Accessible title of the SVG.
    pub(crate) title: &'static str,
    /// Accessible description of the SVG.
    pub(crate) description: &'static str,
    pub(crate) width: i32,
    pub(crate) height: i32,
    pub(crate) sections: &'static [Section],
    pub(crate) guides: &'static [Guide],
    pub(crate) connectors: &'static [Connector],
    pub(crate) cards: &'static [Card],
    pub(crate) notes: &'static [Note],
}
