//! Scalar presentation: the text and style a [`Value`] scalar is written in.
//!
//! One function decides it for every position, so the block and flow writers can never disagree
//! about what needs quoting.

use std::borrow::Cow;

use saphyr_parser::ScalarStyle;

use crate::scalar::{ResolvedScalar, is_c_printable, resolve_scalar};
use crate::value::Value;

/// Where a scalar is written, which decides whether a literal block is possible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    /// A block-context value, sequence entry or root: may be a literal block.
    Block,
    /// A block mapping key or set member: always single-line.
    Key,
    /// A node inside a flow collection.
    Flow,
    /// A mapping key or set member inside a flow collection.
    FlowKey,
}

/// A scalar's text and the style it is written in.
pub type Presented<'a> = (Cow<'a, str>, ScalarStyle);

/// Returns how `value` is written, or `None` when it is a collection.
///
/// Strings are plain unless quoting is needed to read them back as the same string, and a
/// multiline string is a literal block when `multiline` asks for it and a block can hold it.
pub fn present(value: &Value, position: Position, multiline: bool) -> Option<Presented<'_>> {
    Some(match value {
        Value::Null => plain(Cow::Borrowed("~")),
        Value::Bool(true) => plain(Cow::Borrowed("true")),
        Value::Bool(false) => plain(Cow::Borrowed("false")),
        Value::Int(i) => plain(Cow::Owned(i.to_string())),
        Value::BigInt(big) => plain(Cow::Borrowed(big.canonical())),
        Value::Float(f) => plain(
            f.spelling()
                .map_or_else(|| Cow::Owned(f.to_string()), Cow::Borrowed),
        ),
        Value::String(s) => present_string(s, position, multiline),
        Value::Sequence(_) | Value::Mapping(_) | Value::Set(_) => return None,
    })
}

const fn plain(text: Cow<'_, str>) -> Presented<'_> {
    (text, ScalarStyle::Plain)
}

fn present_string(s: &str, position: Position, multiline: bool) -> Presented<'_> {
    let literal = multiline
        && position == Position::Block
        && s.contains('\n')
        && !has_reason_to_escape(s)
        && literal_block_keeps(s);
    let in_flow = matches!(position, Position::Flow | Position::FlowKey);
    let style = if literal {
        ScalarStyle::Literal
    } else if needs_quotes(s) || (in_flow && (s.ends_with(" -") || s.contains('?'))) {
        ScalarStyle::DoubleQuoted
    } else {
        ScalarStyle::Plain
    };
    (Cow::Borrowed(s), style)
}

/// Reasons beyond line breaks that make a string unwritable as a block scalar or plain scalar.
fn has_reason_to_escape(s: &str) -> bool {
    reads_as_non_string(s)
        || reads_as_yaml_11_number_or_date(s)
        || s == "<<"
        || s.contains(['\u{FEFF}', '\u{85}', '\u{2028}', '\u{2029}'])
        || !s.chars().all(is_c_printable)
}

/// Whether a YAML 1.1 reader (`PyYAML`) resolves the plain scalar `s` to a number or timestamp
/// that the core schema leaves a string: `1_000`, `1_0.5`, `0b1010`, `2001-12-14`.
fn reads_as_yaml_11_number_or_date(s: &str) -> bool {
    let unsigned = s.strip_prefix(['-', '+']).unwrap_or(s);
    let underscored_number = unsigned.starts_with(|c: char| c.is_ascii_digit())
        && unsigned.contains('_')
        && unsigned
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '_' | '.' | 'e' | 'E' | '+' | '-'));
    let date = s.get(..10).is_some_and(|head| {
        head.char_indices().all(|(i, c)| match i {
            4 | 7 => c == '-',
            _ => c.is_ascii_digit(),
        })
    });
    let binary = unsigned
        .strip_prefix("0b")
        .or_else(|| unsigned.strip_prefix("0B"))
        .is_some_and(|digits| {
            !digits.is_empty() && digits.chars().all(|c| matches!(c, '0' | '1' | '_'))
        });
    underscored_number || binary || date
}

/// Whether the plain scalar `s` would be resolved to something other than a string.
fn reads_as_non_string(s: &str) -> bool {
    !matches!(
        resolve_scalar(s, crate::events::ScalarStyle::Plain, None),
        ResolvedScalar::Str(_)
    )
}

/// Whether a literal block scalar reads back as exactly `s`: it cannot carry a `\r`, leading
/// whitespace, more than one trailing line break without indicators the formatter does not
/// write, or a line that looks like a document marker or directive at the root.
fn literal_block_keeps(s: &str) -> bool {
    !s.contains('\r')
        && !s.starts_with(char::is_whitespace)
        && !s.ends_with("\n\n")
        && !s
            .lines()
            .any(|line| line.starts_with("---") || line.starts_with("...") || line.starts_with('%'))
}

/// Whether a string must be double-quoted to be read back as the same string.
///
/// The check is [`ported_need_quotes`] plus what it misses: a spelling the core schema resolves
/// to a non-string (`+.inf`), a plain `<<` (a merge key), characters a plain scalar cannot carry
/// (BOM, non-printables, U+2028/U+2029, which YAML 1.1 readers treat as line breaks).
pub fn needs_quotes(s: &str) -> bool {
    ported_need_quotes(s) || has_reason_to_escape(s)
}

/// The quoting check of the `saphyr` crate (`need_quotes` in its `emitter.rs`, MIT OR
/// Apache-2.0), ported verbatim so the emitter quotes at least everything it used to.
fn ported_need_quotes(string: &str) -> bool {
    fn need_quotes_spaces(string: &str) -> bool {
        string.starts_with(' ') || string.ends_with(' ')
    }

    string.is_empty()
        || need_quotes_spaces(string)
        || string.starts_with(|character: char| {
            matches!(
                character,
                '&' | '*' | '?' | '|' | '-' | '<' | '>' | '=' | '!' | '%' | '@'
            )
        })
        || string.contains(|character: char| {
            matches!(character, ':'
            | '{'
            | '}'
            | '['
            | ']'
            | ','
            | '#'
            | '`'
            | '\"'
            | '\''
            | '\\'
            | '\0'..='\x06'
            | '\t'
            | '\n'
            | '\r'
            | '\x0e'..='\x1a'
            | '\x1c'..='\x1f')
        })
        || [
            // http://yaml.org/type/bool.html
            // Note: 'y', 'Y', 'n', 'N', is not quoted deliberately, as in libyaml.
            "yes", "Yes", "YES", "no", "No", "NO", "True", "TRUE", "true", "False", "FALSE",
            "false", "on", "On", "ON", "off", "Off", "OFF",
            // http://yaml.org/type/null.html
            "null", "Null", "NULL", "~",
        ]
        .contains(&string)
        || string.starts_with('.')
        || string.starts_with("0x")
        || string.parse::<i64>().is_ok()
        || string.parse::<f64>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(s: &str, position: Position, multiline: bool) -> ScalarStyle {
        present(&Value::String(s.into()), position, multiline)
            .unwrap()
            .1
    }

    #[test]
    fn yaml_11_words_numbers_and_indicators_are_quoted() {
        for s in [
            "yes",
            "No",
            "ON",
            "off",
            "null",
            "~",
            "true",
            "",
            " a",
            "a ",
            "-a",
            "a: b",
            "a#b",
            "0x1F",
            "1.5",
            "1e3",
            ".inf",
            "+.inf",
            "<<",
            "a,b",
            "[x]",
            "tab\there",
            "\u{2028}",
            "\u{FEFF}x",
            "\u{7}",
            "1_000",
            "-1_0.5",
            "2001-12-14",
            "0b1010",
            "-0B1_1",
            "\u{85}",
            "a\u{85}b",
        ] {
            assert_eq!(
                style(s, Position::Key, false),
                ScalarStyle::DoubleQuoted,
                "{s:?}"
            );
        }
        for s in [
            "y",
            "n",
            "plain text",
            "a-b",
            "x.y",
            "2001-12",
            "1_a",
            "v1_0",
        ] {
            assert_eq!(style(s, Position::Key, false), ScalarStyle::Plain, "{s:?}");
        }
    }

    #[test]
    fn strings_ending_in_a_dash_are_quoted_in_flow_only() {
        for position in [Position::Flow, Position::FlowKey] {
            assert_eq!(style("a -", position, false), ScalarStyle::DoubleQuoted);
            assert_eq!(style("a-", position, false), ScalarStyle::Plain);
        }
        for position in [Position::Block, Position::Key] {
            assert_eq!(style("a -", position, false), ScalarStyle::Plain);
        }
    }

    #[test]
    fn question_marks_are_quoted_in_flow_only() {
        for position in [Position::Flow, Position::FlowKey] {
            assert_eq!(style("a?b", position, false), ScalarStyle::DoubleQuoted);
            assert_eq!(style("what ?", position, false), ScalarStyle::DoubleQuoted);
        }
        for position in [Position::Block, Position::Key] {
            assert_eq!(style("a?b", position, false), ScalarStyle::Plain);
        }
    }

    #[test]
    fn next_line_is_never_plain_or_literal() {
        for position in [
            Position::Block,
            Position::Key,
            Position::Flow,
            Position::FlowKey,
        ] {
            assert_eq!(style("a\u{85}b", position, true), ScalarStyle::DoubleQuoted);
            assert_eq!(
                style("l\n\u{85}x: 1", position, true),
                ScalarStyle::DoubleQuoted
            );
        }
    }

    #[test]
    fn multiline_literal_only_in_block_value_position() {
        assert_eq!(style("a\nb\n", Position::Block, true), ScalarStyle::Literal);
        assert_eq!(
            style("a\nb\n", Position::Block, false),
            ScalarStyle::DoubleQuoted
        );
        assert_eq!(
            style("a\nb\n", Position::Key, true),
            ScalarStyle::DoubleQuoted
        );
        assert_eq!(
            style("a\nb\n", Position::Flow, true),
            ScalarStyle::DoubleQuoted
        );
        assert_eq!(
            style("a\r\nb", Position::Block, true),
            ScalarStyle::DoubleQuoted
        );
        assert_eq!(
            style(" a\nb", Position::Block, true),
            ScalarStyle::DoubleQuoted
        );
        assert_eq!(
            style("a\n\n", Position::Block, true),
            ScalarStyle::DoubleQuoted
        );
    }

    #[test]
    fn collections_have_no_scalar_presentation() {
        assert!(present(&Value::Sequence(Vec::new()), Position::Block, false).is_none());
    }
}
