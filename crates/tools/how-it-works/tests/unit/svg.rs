use super::render;
use crate::{
    diagrams,
    model::{Card, DAY, Diagram, Emphasis, NIGHT, Palette},
    validate::ValidationError,
};

fn palette_values(palette: &Palette) -> [&'static str; 12] {
    [
        palette.canvas,
        palette.grid,
        palette.frame,
        palette.card,
        palette.card_border,
        palette.text,
        palette.label,
        palette.muted_text,
        palette.guide,
        palette.connector,
        palette.primary,
        palette.secondary,
    ]
}

/// Splits an SVG into its hex color tokens (`#rgb` or `#rrggbb`) and the text between them.
fn split_colors(svg: &str) -> (Vec<&str>, Vec<&str>) {
    let bytes = svg.as_bytes();
    let (mut colors, mut rest, mut last, mut index) = (Vec::new(), Vec::new(), 0, 0);
    while index < bytes.len() {
        if bytes[index] == b'#' {
            let digits = bytes[index + 1..].iter().take_while(|byte| byte.is_ascii_hexdigit()).count();
            let followed_by_name = bytes.get(index + 1 + digits).is_some_and(u8::is_ascii_alphanumeric);
            if (digits == 3 || digits == 6) && !followed_by_name {
                rest.push(&svg[last..index]);
                colors.push(&svg[index..=index + digits]);
                last = index + 1 + digits;
                index = last;
                continue;
            }
        }
        index += 1;
    }
    rest.push(&svg[last..]);
    (colors, rest)
}

#[test]
fn day_and_night_differ_only_in_palette_values() {
    let day_values = palette_values(&DAY);
    let night_values = palette_values(&NIGHT);
    for diagram in diagrams::ALL {
        let day = render(diagram, &DAY).expect("shipped diagrams are valid");
        let night = render(diagram, &NIGHT).expect("shipped diagrams are valid");
        let (day_colors, day_rest) = split_colors(&day);
        let (night_colors, night_rest) = split_colors(&night);
        assert_eq!(day_rest, night_rest, "{}: everything but colors is identical", diagram.slug);
        assert_eq!(day_colors.len(), night_colors.len());
        for (day_color, night_color) in day_colors.iter().zip(&night_colors) {
            let field = day_values.iter().position(|value| value == day_color);
            assert!(field.is_some(), "{}: {day_color} is not a day palette value", diagram.slug);
            assert_eq!(field, night_values.iter().position(|value| value == night_color), "{}", diagram.slug);
        }
        // Every palette role that the grammar draws appears, so the themes really are palette-driven.
        for value in [DAY.canvas, DAY.grid, DAY.frame, DAY.card, DAY.card_border, DAY.text, DAY.connector] {
            assert!(day_colors.contains(&value), "{}: {value} is unused", diagram.slug);
        }
    }
}

#[test]
fn text_is_painted_after_all_geometry() {
    for diagram in diagrams::ALL {
        let svg = render(diagram, &DAY).expect("shipped diagrams are valid");
        let body = &svg[svg.find("</defs>").expect("the document has definitions")..];
        let last_geometry = ["<rect", "<path", "<line"].iter().filter_map(|tag| body.rfind(tag)).max();
        let first_text = body.find("<text");
        assert!(last_geometry < first_text, "{}", diagram.slug);
    }
}

#[test]
fn turns_are_drawn_as_soft_elbows() {
    let svg = render(&diagrams::processing_flow::DIAGRAM, &DAY).expect("shipped diagrams are valid");
    // The route from (316, 316) turns once at (48, 316): straight, a curve around the corner, then straight down.
    assert!(svg.contains(r#"<path d="M316 316L60 316Q48 316 48 328L48 612""#), "{svg}");
}

#[test]
fn escapes_markup_and_describes_the_diagram_for_assistive_technology() {
    const CARDS: &[Card] = &[Card {
        key: "card",
        label: "a<b",
        title: "Tom & \"Jerry\"",
        details: &["x > y"],
        meta: &[],
        x: 24,
        y: 24,
        width: 240,
        height: 100,
        emphasis: Emphasis::Primary,
    }];
    let diagram = Diagram {
        slug: "escape",
        title: "Q&A",
        description: "Uses <markup>",
        width: 288,
        height: 148,
        sections: &[],
        guides: &[],
        connectors: &[],
        cards: CARDS,
        notes: &[],
    };
    let svg = render(&diagram, &DAY).expect("the diagram is valid");
    assert!(svg.contains(r#"role="img" aria-labelledby="title description""#));
    assert!(svg.contains(r#"<title id="title">Q&amp;A</title>"#));
    assert!(svg.contains(r#"<desc id="description">Uses &lt;markup&gt;</desc>"#));
    assert!(svg.contains(">A&lt;B</text>"), "labels are capitalized and escaped");
    assert!(svg.contains(">Tom &amp; &quot;Jerry&quot;</text>"));
    assert!(svg.contains(">x &gt; y</text>"));
}

#[test]
fn refuses_to_render_an_invalid_diagram() {
    let mut invalid = diagrams::crate_map::DIAGRAM;
    invalid.width = 400;
    assert!(matches!(render(&invalid, &DAY), Err(ValidationError::CardLeavesCanvas { .. })));
}
