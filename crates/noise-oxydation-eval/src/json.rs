//! Minimal JSON rendering for the metrics files (the crate has no serialization dependency).

use std::fmt::Write;

/// A JSON value.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    Null,
    Number(f64),
    Integer(u64),
    Text(String),
    Array(Vec<Value>),
    Object(Vec<(&'static str, Value)>),
}

impl Value {
    /// A number rounded to three decimals, or `null` when absent or not finite.
    pub(crate) fn decibels(value: Option<f64>) -> Self {
        value.filter(|value| value.is_finite()).map_or(Self::Null, Self::Number)
    }

    pub(crate) fn text(value: impl Into<String>) -> Self {
        Self::Text(value.into())
    }

    /// Pretty-printed JSON with two-space indentation and a trailing newline.
    pub(crate) fn render(&self) -> String {
        let mut output = String::new();
        self.write(&mut output, 0);
        output.push('\n');
        output
    }

    fn write(&self, output: &mut String, depth: usize) {
        match self {
            Self::Null => output.push_str("null"),
            Self::Number(value) => {
                let _ = write!(output, "{value:.3}");
            }
            Self::Integer(value) => {
                let _ = write!(output, "{value}");
            }
            Self::Text(text) => write_string(output, text),
            Self::Array(items) => {
                write_sequence(output, depth, '[', ']', items.iter().map(|item| (None, item)));
            }
            Self::Object(fields) => {
                write_sequence(output, depth, '{', '}', fields.iter().map(|(key, value)| (Some(*key), value)));
            }
        }
    }
}

fn write_sequence<'a>(
    output: &mut String,
    depth: usize,
    open: char,
    close: char,
    entries: impl ExactSizeIterator<Item = (Option<&'a str>, &'a Value)>,
) {
    if entries.len() == 0 {
        output.push(open);
        output.push(close);
        return;
    }
    output.push(open);
    for (index, (key, value)) in entries.enumerate() {
        if index > 0 {
            output.push(',');
        }
        output.push('\n');
        output.push_str(&"  ".repeat(depth + 1));
        if let Some(key) = key {
            write_string(output, key);
            output.push_str(": ");
        }
        value.write(output, depth + 1);
    }
    output.push('\n');
    output.push_str(&"  ".repeat(depth));
    output.push(close);
}

fn write_string(output: &mut String, text: &str) {
    output.push('"');
    for character in text.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            control if u32::from(control) < 0x20 => {
                let _ = write!(output, "\\u{:04x}", u32::from(control));
            }
            other => output.push(other),
        }
    }
    output.push('"');
}

#[cfg(test)]
#[path = "../tests/unit/json.rs"]
mod tests;
