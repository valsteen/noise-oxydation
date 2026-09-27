use super::{ValidationError, validate};
use crate::{
    diagrams,
    model::{Card, Connector, Diagram, Emphasis, Stroke},
};

/// Left card: x 24..224, y 24..124.
const LEFT: Card = Card {
    key: "left",
    label: "Source",
    title: "Left",
    details: &["one detail"],
    meta: &[],
    x: 24,
    y: 24,
    width: 200,
    height: 100,
    emphasis: Emphasis::Primary,
};

/// Right card: x 424..624, y 24..124.
const RIGHT: Card = Card { key: "right", title: "Right", x: 424, ..LEFT };

/// Middle card: x 284..364, y 150..250, below the straight route between LEFT and RIGHT.
const MIDDLE: Card = Card { key: "middle", label: "", title: "M", details: &[], x: 284, y: 150, width: 80, ..LEFT };

const LINK: Connector = Connector {
    points: &[(224, 74), (424, 74)],
    source: Some("left"),
    target: Some("right"),
    label: None,
    stroke: Stroke::Solid,
    arrow: true,
};

fn diagram(cards: &'static [Card], connectors: &'static [Connector]) -> Diagram {
    Diagram {
        slug: "test",
        title: "Test",
        description: "Test diagram",
        width: 648,
        height: 260,
        sections: &[],
        guides: &[],
        connectors,
        cards,
        notes: &[],
    }
}

#[test]
fn accepts_a_valid_diagram_and_every_shipped_diagram() {
    assert_eq!(validate(&diagram(&[LEFT, RIGHT, MIDDLE], &[LINK])), Ok(()));
    for shipped in diagrams::ALL {
        assert_eq!(validate(shipped), Ok(()), "{}", shipped.slug);
    }
}

#[test]
fn rejects_a_card_leaving_the_canvas() {
    const OUTSIDE: Card = Card { x: 600, ..RIGHT };
    assert_eq!(
        validate(&diagram(&[LEFT, OUTSIDE], &[])),
        Err(ValidationError::CardLeavesCanvas { diagram: "test", card: "right" })
    );
}

#[test]
fn rejects_text_without_a_right_margin() {
    const CROWDED: Card = Card { details: &["a detail line that is much too long for this card"], ..LEFT };
    assert_eq!(
        validate(&diagram(&[CROWDED], &[])),
        Err(ValidationError::CardTextTooWide {
            diagram: "test",
            card: "left",
            text: "a detail line that is much too long for this card",
        })
    );
}

#[test]
fn rejects_text_that_does_not_fit_the_card_height() {
    const TALL: Card = Card { details: &["one", "two", "three"], meta: &["meta"], ..LEFT };
    assert_eq!(
        validate(&diagram(&[TALL], &[])),
        Err(ValidationError::CardTextTooTall { diagram: "test", card: "left" })
    );
}

#[test]
fn rejects_a_diagonal_segment() {
    const ROUTE: &[Connector] = &[Connector { points: &[(224, 74), (424, 90)], ..LINK }];
    assert_eq!(
        validate(&diagram(&[LEFT, RIGHT], ROUTE)),
        Err(ValidationError::DiagonalSegment { diagram: "test", connector: 0 })
    );
}

#[test]
fn rejects_a_zero_length_segment() {
    const ROUTE: &[Connector] = &[Connector { points: &[(224, 74), (224, 74), (424, 74)], ..LINK }];
    assert_eq!(
        validate(&diagram(&[LEFT, RIGHT], ROUTE)),
        Err(ValidationError::ZeroLengthSegment { diagram: "test", connector: 0 })
    );
}

#[test]
fn rejects_an_arrowhead_without_a_straight_approach() {
    // The route turns 20 px before the target: less than the 16 px approach plus the 12 px elbow.
    const ROUTE: &[Connector] =
        &[Connector { points: &[(224, 74), (300, 74), (300, 144), (404, 144), (404, 60), (424, 60)], ..LINK }];
    // Without an arrowhead the same short final segment is acceptable.
    const PLAIN: &[Connector] = &[Connector { arrow: false, ..ROUTE[0] }];
    assert_eq!(
        validate(&diagram(&[LEFT, RIGHT], ROUTE)),
        Err(ValidationError::NoStraightApproach { diagram: "test", connector: 0 })
    );
    assert_eq!(validate(&diagram(&[LEFT, RIGHT], PLAIN)), Ok(()));
}

#[test]
fn rejects_a_connector_that_misses_a_stable_card_port() {
    // (224, 30) is on the right edge of LEFT but only 6 px from its top corner.
    const ROUTE: &[Connector] = &[Connector { points: &[(224, 30), (424, 30)], ..LINK }];
    // A point in open canvas is not a port of the named target either.
    const SHORT: &[Connector] = &[Connector { points: &[(224, 74), (400, 74)], ..LINK }];
    assert_eq!(
        validate(&diagram(&[LEFT, RIGHT], ROUTE)),
        Err(ValidationError::UnstablePort { diagram: "test", connector: 0, card: "left" })
    );
    assert_eq!(
        validate(&diagram(&[LEFT, RIGHT], SHORT)),
        Err(ValidationError::UnstablePort { diagram: "test", connector: 0, card: "right" })
    );
}

#[test]
fn rejects_a_route_entering_an_unrelated_cards_clearance() {
    // The detour runs 6 px above MIDDLE's top edge, inside its 12 px clearance.
    const ROUTE: &[Connector] =
        &[Connector { points: &[(224, 74), (254, 74), (254, 144), (394, 144), (394, 74), (424, 74)], ..LINK }];
    assert_eq!(
        validate(&diagram(&[LEFT, RIGHT, MIDDLE], ROUTE)),
        Err(ValidationError::ClearanceViolation { diagram: "test", connector: 0, card: "middle" })
    );
}

#[test]
fn rejects_duplicate_and_unknown_cards() {
    assert_eq!(
        validate(&diagram(&[LEFT, LEFT], &[])),
        Err(ValidationError::DuplicateCard { diagram: "test", card: "left" })
    );
    assert_eq!(
        validate(&diagram(&[LEFT], &[LINK])),
        Err(ValidationError::UnknownCard { diagram: "test", connector: 0, card: "right" })
    );
}
