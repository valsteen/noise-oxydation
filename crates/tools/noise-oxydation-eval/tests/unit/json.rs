use super::Value;

#[test]
fn values_render_as_indented_json() {
    let value = Value::Object(vec![
        ("name", Value::text("a \"quoted\"\\path\n")),
        ("count", Value::Integer(3)),
        ("level", Value::decibels(Some(-1.23456))),
        ("missing", Value::decibels(None)),
        ("infinite", Value::decibels(Some(f64::INFINITY))),
        ("items", Value::Array(vec![Value::Null, Value::Number(2.0)])),
        ("empty", Value::Array(Vec::new())),
    ]);
    let expected = "{\n  \"name\": \"a \\\"quoted\\\"\\\\path\\n\",\n  \"count\": 3,\n  \"level\": -1.235,\n  \
                    \"missing\": null,\n  \"infinite\": null,\n  \"items\": [\n    null,\n    2.000\n  ],\n  \
                    \"empty\": []\n}\n";
    assert_eq!(value.render(), expected);
}

#[test]
fn control_characters_are_escaped() {
    assert_eq!(Value::text("\u{1}").render(), "\"\\u0001\"\n");
}
