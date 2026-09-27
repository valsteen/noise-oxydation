//! The mechanically checkable rules of the visual grammar. Every diagram is validated before it is rendered.
//!
//! - Cards and canvas text stay inside the canvas.
//! - Card text fits its card vertically and keeps a right margin, by a conservative width estimate.
//! - Connector routes are orthogonal, have no zero-length segment, and end in a straight approach to the arrowhead.
//! - Connectors meet the middle region of a card edge, never a corner.
//! - Every connector segment keeps a clearance corridor around cards it does not start or end at.

use std::{collections::HashSet, error::Error, fmt};

use crate::{
    layout::{
        ARROW_APPROACH, BOTTOM_GAP, CARD_CLEARANCE, CORNER_RADIUS, META_GAP, PORT_MARGIN, TextStyle,
        available_text_millipixels, estimated_width_millipixels, last_body_baseline, meta_baseline,
    },
    model::{Anchor, Card, Connector, Diagram, NoteStyle, Point},
};

/// Free-standing text keeps at least this distance from the canvas edge.
const CANVAS_TEXT_MARGIN: i32 = 8;

/// A diagram that breaks a rule of the visual grammar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ValidationError {
    /// Two cards share a key.
    DuplicateCard { diagram: &'static str, card: &'static str },
    /// A card extends beyond the canvas.
    CardLeavesCanvas { diagram: &'static str, card: &'static str },
    /// A card's lines do not fit its height.
    CardTextTooTall { diagram: &'static str, card: &'static str },
    /// A line of card text does not leave the right margin.
    CardTextTooWide { diagram: &'static str, card: &'static str, text: &'static str },
    /// A note, section heading or connector label reaches beyond the canvas margin.
    TextLeavesCanvas { diagram: &'static str, text: &'static str },
    /// A connector has fewer than two points.
    ConnectorTooShort { diagram: &'static str, connector: usize },
    /// A connector segment is diagonal.
    DiagonalSegment { diagram: &'static str, connector: usize },
    /// A connector repeats a point.
    ZeroLengthSegment { diagram: &'static str, connector: usize },
    /// The last segment is too short for the arrowhead (and the elbow before it).
    NoStraightApproach { diagram: &'static str, connector: usize },
    /// A connector names a card that does not exist.
    UnknownCard { diagram: &'static str, connector: usize, card: &'static str },
    /// A connector end is not on the middle region of its card's edge.
    UnstablePort { diagram: &'static str, connector: usize, card: &'static str },
    /// A connector segment enters the clearance corridor of a card it does not start or end at.
    ClearanceViolation { diagram: &'static str, connector: usize, card: &'static str },
}

impl fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateCard { diagram, card } => write!(formatter, "{diagram}: more than one card is keyed {card}"),
            Self::CardLeavesCanvas { diagram, card } => write!(formatter, "{diagram}: card {card} leaves the canvas"),
            Self::CardTextTooTall { diagram, card } => {
                write!(formatter, "{diagram}: the text of card {card} does not fit its height")
            }
            Self::CardTextTooWide { diagram, card, text } => {
                write!(formatter, "{diagram}: card {card} needs a right margin after {text:?}")
            }
            Self::TextLeavesCanvas { diagram, text } => write!(formatter, "{diagram}: {text:?} leaves the canvas"),
            Self::ConnectorTooShort { diagram, connector } => {
                write!(formatter, "{diagram}: connector {connector} needs at least two points")
            }
            Self::DiagonalSegment { diagram, connector } => {
                write!(formatter, "{diagram}: connector {connector} has a segment that is not horizontal or vertical")
            }
            Self::ZeroLengthSegment { diagram, connector } => {
                write!(formatter, "{diagram}: connector {connector} has a zero-length segment")
            }
            Self::NoStraightApproach { diagram, connector } => {
                write!(formatter, "{diagram}: connector {connector} needs a straight approach before its arrowhead")
            }
            Self::UnknownCard { diagram, connector, card } => {
                write!(formatter, "{diagram}: connector {connector} names the unknown card {card}")
            }
            Self::UnstablePort { diagram, connector, card } => {
                write!(
                    formatter,
                    "{diagram}: connector {connector} does not meet the middle region of an edge of {card}"
                )
            }
            Self::ClearanceViolation { diagram, connector, card } => {
                write!(formatter, "{diagram}: connector {connector} needs clearance from card {card}")
            }
        }
    }
}

impl Error for ValidationError {}

/// Checks every rule of the visual grammar and reports the first violation.
pub(crate) fn validate(diagram: &Diagram) -> Result<(), ValidationError> {
    let mut keys = HashSet::new();
    for card in diagram.cards {
        if !keys.insert(card.key) {
            return Err(ValidationError::DuplicateCard { diagram: diagram.slug, card: card.key });
        }
        validate_card(diagram, card)?;
    }
    validate_canvas_text(diagram)?;
    for (index, connector) in diagram.connectors.iter().enumerate() {
        validate_connector(diagram, index, connector)?;
    }
    Ok(())
}

fn validate_card(diagram: &Diagram, card: &Card) -> Result<(), ValidationError> {
    if card.x < 0 || card.y < 0 || card.x + card.width > diagram.width || card.y + card.height > diagram.height {
        return Err(ValidationError::CardLeavesCanvas { diagram: diagram.slug, card: card.key });
    }
    let last_body = last_body_baseline(card);
    let fits = if card.meta.is_empty() {
        last_body <= card.height - BOTTOM_GAP
    } else {
        last_body <= meta_baseline(card, 0) - META_GAP
    };
    if !fits {
        return Err(ValidationError::CardTextTooTall { diagram: diagram.slug, card: card.key });
    }
    let label = (!card.label.is_empty()).then_some((card.label, TextStyle::Label));
    let lines = label
        .into_iter()
        .chain([(card.title, TextStyle::Title)])
        .chain(card.details.iter().map(|&line| (line, TextStyle::Body)))
        .chain(card.meta.iter().map(|&line| (line, TextStyle::Meta)));
    for (text, style) in lines {
        if estimated_width_millipixels(text, style) > available_text_millipixels(card) {
            return Err(ValidationError::CardTextTooWide { diagram: diagram.slug, card: card.key, text });
        }
    }
    Ok(())
}

fn validate_canvas_text(diagram: &Diagram) -> Result<(), ValidationError> {
    let sections = diagram.sections.iter().flat_map(|section| {
        let subtitles = section.subtitle.iter().map(move |&line| (line, section.x, TextStyle::Body));
        [(section.label, section.x, TextStyle::Label)].into_iter().chain(subtitles)
    });
    let labels = diagram.connectors.iter().filter_map(|connector| connector.label).map(|label| {
        let half = estimated_width_millipixels(label.text, TextStyle::Meta) / 2000;
        (label.text, label.position.0 - i32::try_from(half).unwrap_or(i32::MAX), TextStyle::Meta)
    });
    let notes = diagram.notes.iter().map(|note| {
        // Lane markers are letter-spaced monospace, so they take the label advance.
        let style = if note.style == NoteStyle::Meta { TextStyle::Label } else { TextStyle::Body };
        let start = if note.anchor == Anchor::Middle {
            let half = estimated_width_millipixels(note.text, style) / 2000;
            note.x - i32::try_from(half).unwrap_or(i32::MAX)
        } else {
            note.x
        };
        (note.text, start, style)
    });
    for (text, start, style) in sections.chain(labels).chain(notes) {
        let end = i64::from(start) * 1000 + estimated_width_millipixels(text, style);
        if start < CANVAS_TEXT_MARGIN || end > i64::from(diagram.width - CANVAS_TEXT_MARGIN) * 1000 {
            return Err(ValidationError::TextLeavesCanvas { diagram: diagram.slug, text });
        }
    }
    Ok(())
}

fn validate_connector(diagram: &Diagram, connector_index: usize, connector: &Connector) -> Result<(), ValidationError> {
    let slug = diagram.slug;
    if connector.points.len() < 2 {
        return Err(ValidationError::ConnectorTooShort { diagram: slug, connector: connector_index });
    }
    let segments: Vec<(Point, Point)> = connector.points.windows(2).map(|pair| (pair[0], pair[1])).collect();
    for &(start, end) in &segments {
        if start == end {
            return Err(ValidationError::ZeroLengthSegment { diagram: slug, connector: connector_index });
        }
        if start.0 != end.0 && start.1 != end.1 {
            return Err(ValidationError::DiagonalSegment { diagram: slug, connector: connector_index });
        }
    }
    if connector.arrow {
        let (start, end) = segments[segments.len() - 1];
        let needed = ARROW_APPROACH + if segments.len() == 1 { 0 } else { CORNER_RADIUS };
        if length(start, end) < needed {
            return Err(ValidationError::NoStraightApproach { diagram: slug, connector: connector_index });
        }
    }
    for (end, point) in [(connector.source, connector.points[0]), (connector.target, connector.points[segments.len()])]
    {
        if let Some(key) = end {
            let card = find_card(diagram, connector_index, key)?;
            if !is_stable_port(card, point) {
                return Err(ValidationError::UnstablePort { diagram: slug, connector: connector_index, card: key });
            }
        }
    }
    for card in diagram.cards {
        for (position, &(start, end)) in segments.iter().enumerate() {
            let leaves_source = connector.source == Some(card.key) && position == 0;
            let enters_target = connector.target == Some(card.key) && position == segments.len() - 1;
            if !leaves_source && !enters_target && enters_clearance(start, end, card) {
                return Err(ValidationError::ClearanceViolation {
                    diagram: slug,
                    connector: connector_index,
                    card: card.key,
                });
            }
        }
    }
    Ok(())
}

fn find_card<'diagram>(
    diagram: &'diagram Diagram,
    connector: usize,
    key: &'static str,
) -> Result<&'diagram Card, ValidationError> {
    diagram.cards.iter().find(|card| card.key == key).ok_or(ValidationError::UnknownCard {
        diagram: diagram.slug,
        connector,
        card: key,
    })
}

fn length(start: Point, end: Point) -> i32 {
    (end.0 - start.0).abs() + (end.1 - start.1).abs()
}

fn is_stable_port(card: &Card, (x, y): Point) -> bool {
    let on_horizontal_edge = (y == card.y || y == card.y + card.height)
        && (card.x + PORT_MARGIN..=card.x + card.width - PORT_MARGIN).contains(&x);
    let on_vertical_edge = (x == card.x || x == card.x + card.width)
        && (card.y + PORT_MARGIN..=card.y + card.height - PORT_MARGIN).contains(&y);
    on_horizontal_edge || on_vertical_edge
}

fn enters_clearance(start: Point, end: Point, card: &Card) -> bool {
    let left = card.x - CARD_CLEARANCE;
    let right = card.x + card.width + CARD_CLEARANCE;
    let top = card.y - CARD_CLEARANCE;
    let bottom = card.y + card.height + CARD_CLEARANCE;
    let (low_x, high_x) = (start.0.min(end.0), start.0.max(end.0));
    let (low_y, high_y) = (start.1.min(end.1), start.1.max(end.1));
    low_x <= right && high_x >= left && low_y <= bottom && high_y >= top
}

#[cfg(test)]
#[path = "../tests/unit/validate.rs"]
mod tests;
