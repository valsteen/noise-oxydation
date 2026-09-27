//! Shared geometry of the visual grammar: where card text sits, how wide text is estimated to be, and the distances
//! that ports, arrowheads and routes keep. The validator and the SVG writer both read these values, so a validated
//! layout is exactly the one that is drawn.

use crate::model::Card;

/// Radius of a soft elbow where a connector turns.
pub(crate) const CORNER_RADIUS: i32 = 12;
/// A port must sit at least this far from a card corner.
pub(crate) const PORT_MARGIN: i32 = 12;
/// Straight length an arrowhead needs before it, in addition to an elbow's radius when the route turns.
pub(crate) const ARROW_APPROACH: i32 = 16;
/// Empty corridor a route keeps around every card it does not start or end at.
pub(crate) const CARD_CLEARANCE: i32 = 12;
/// Left inset of card text from the card edge.
pub(crate) const TEXT_INSET: i32 = 16;
/// Space card text keeps from the right edge.
pub(crate) const TEXT_RIGHT_MARGIN: i32 = 16;
/// Width of a card's left accent bar.
pub(crate) const ACCENT_WIDTH: i32 = 3;

/// Baseline of the card label, relative to the card top.
pub(crate) const LABEL_BASELINE: i32 = 22;
const TITLE_BASELINE_WITH_LABEL: i32 = 49;
const TITLE_BASELINE_WITHOUT_LABEL: i32 = 28;
const DETAIL_SPACING: i32 = 21;
const META_SPACING: i32 = 14;
const META_BOTTOM_INSET: i32 = 14;
/// Space the last detail line keeps above the first meta line, so the monospaced meta reads as a separate block.
pub(crate) const META_GAP: i32 = 20;
pub(crate) const BOTTOM_GAP: i32 = 12;

/// Baseline of the card title, relative to the card top.
pub(crate) fn title_baseline(card: &Card) -> i32 {
    if card.label.is_empty() { TITLE_BASELINE_WITHOUT_LABEL } else { TITLE_BASELINE_WITH_LABEL }
}

/// Baseline of detail line `index` (0-based), relative to the card top.
pub(crate) fn detail_baseline(card: &Card, index: usize) -> i32 {
    title_baseline(card) + DETAIL_SPACING * (line_count(index) + 1)
}

/// Baseline of the last text line above the meta lines: the last detail line, or the title without details.
pub(crate) fn last_body_baseline(card: &Card) -> i32 {
    title_baseline(card) + DETAIL_SPACING * line_count(card.details.len())
}

/// Baseline of meta line `index` (0-based), relative to the card top. Meta lines are anchored to the card bottom.
pub(crate) fn meta_baseline(card: &Card, index: usize) -> i32 {
    card.height - META_BOTTOM_INSET - META_SPACING * (line_count(card.meta.len()) - 1).max(0)
        + META_SPACING * line_count(index)
}

fn line_count(lines: usize) -> i32 {
    i32::try_from(lines).unwrap_or(i32::MAX)
}

/// The typographic role of a line of text, which sets its size and face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextStyle {
    /// 15 px bold sans-serif card title.
    Title,
    /// 12 px sans-serif detail line, note or section subtitle.
    Body,
    /// 12 px bold monospaced capitals with letter spacing: card labels, section labels and lane markers.
    Label,
    /// 11 px monospaced meta line or 11 px bold connector label.
    Meta,
}

/// Estimated rendered width of `text`, in thousandths of a pixel.
///
/// Monospaced styles use a fixed advance per character. Proportional styles use an em-width estimate per glyph class,
/// generous enough for the common system sans-serif faces.
pub(crate) fn estimated_width_millipixels(text: &str, style: TextStyle) -> i64 {
    let characters = i64::try_from(text.chars().count()).unwrap_or(i64::MAX);
    match style {
        TextStyle::Label => characters.saturating_mul(8600),
        TextStyle::Meta => characters.saturating_mul(6800),
        TextStyle::Title => proportional_em_thousandths(text).saturating_mul(159) / 10,
        TextStyle::Body => proportional_em_thousandths(text).saturating_mul(12),
    }
}

/// Space a card leaves for one line of text, in thousandths of a pixel.
pub(crate) fn available_text_millipixels(card: &Card) -> i64 {
    i64::from(card.width - TEXT_INSET - TEXT_RIGHT_MARGIN) * 1000
}

fn proportional_em_thousandths(text: &str) -> i64 {
    text.chars()
        .map(|character| {
            if " !'(),.:;I[]`fijlt|·".contains(character) {
                340
            } else if "%&@GMOQVWmw—→−".contains(character) {
                780
            } else {
                560
            }
        })
        .sum()
}
