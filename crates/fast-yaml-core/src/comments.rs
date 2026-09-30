//! Comment detection for YAML input.
//!
//! The formatter re-emits the parsed value tree and drops comments, so callers that must not
//! lose them need to know beforehand whether the input has any.

use saphyr_parser::{BufferedInput, Event, Parser as SaphyrParser, ScalarStyle};

use crate::error::ParseResult;
use crate::parser::strip_bom;

/// Returns `true` if `input` contains at least one YAML comment.
///
/// Comments are located from parser event spans: plain and block scalar content is opaque,
/// quoted scalars are skipped with an escape-aware scan from their opening quote, and any `#`
/// at line start or after whitespace in the remaining text is a comment.
///
/// # Errors
///
/// Returns a [`ParseError`](crate::ParseError) if `input` is not valid YAML.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::has_comments;
///
/// assert!(has_comments("key: value # note").unwrap());
/// assert!(!has_comments("url: \"http://x/ # not a comment\"").unwrap());
/// assert!(has_comments("a: foo\n  \"bar\nb: 1 # real comment\n").unwrap());
/// ```
pub fn has_comments(input: &str) -> ParseResult<bool> {
    let chars: Vec<char> = strip_bom(input).chars().collect();
    let mut parser = SaphyrParser::new(BufferedInput::new(chars.iter().copied()));
    let mut cursor = 0usize;

    while let Some(event) = parser.next_event() {
        let (event, span) = event?;
        let Event::Scalar(_, style, _, _) = event else {
            continue;
        };
        // `chars` is indexed by char offsets, which is what `Marker::index` yields
        #[allow(clippy::disallowed_methods)]
        let (start, end) = (span.start.index(), span.end.index());
        if start == end || start < cursor {
            continue;
        }
        if gap_has_comment(&chars, cursor, start) {
            return Ok(true);
        }
        cursor = match style {
            ScalarStyle::SingleQuoted => skip_quoted(&chars, start, '\''),
            ScalarStyle::DoubleQuoted => skip_quoted(&chars, start, '"'),
            _ => end,
        };
    }

    Ok(gap_has_comment(&chars, cursor, chars.len()))
}

fn gap_has_comment(chars: &[char], from: usize, to: usize) -> bool {
    let to = to.min(chars.len());
    (from..to)
        .any(|i| chars[i] == '#' && (i == 0 || matches!(chars[i - 1], ' ' | '\t' | '\n' | '\r')))
}

/// Returns the index just past the closing quote of the scalar opened at `open`.
fn skip_quoted(chars: &[char], open: usize, quote: char) -> usize {
    let mut i = open + 1;
    while i < chars.len() {
        match chars[i] {
            '\\' if quote == '"' => i += 1,
            c if c == quote => {
                if quote == '\'' && chars.get(i + 1) == Some(&'\'') {
                    i += 1;
                } else {
                    return i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    chars.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has(input: &str) -> bool {
        has_comments(input).unwrap()
    }

    #[test]
    fn detects_inline() {
        assert!(has("key: value # inline"));
    }

    #[test]
    fn ignores_hash_in_string() {
        assert!(!has("key: \"value # not a comment\""));
        assert!(!has("key: 'value # not a comment'"));
    }

    #[test]
    fn apostrophe_in_plain_scalar() {
        assert!(!has("msg: don't\n"));
        assert!(has("msg: don't # note\n"));
    }

    #[test]
    fn escaped_quote() {
        assert!(has("x: \"a\\\"b\" # c\n"));
        assert!(!has("x: \"a\\\" # b\"\n"));
        assert!(has("x: 'it''s' # c\n"));
        assert!(!has("x: 'it''s # b'\n"));
    }

    #[test]
    fn hash_without_space() {
        assert!(!has("url: http://x/#frag\n"));
        assert!(has("url: http://x/#frag # c\n"));
    }

    #[test]
    fn block_scalar() {
        assert!(!has("s: |\n  # not a comment\n  text\nk: v\n"));
        assert!(has("s: |\n  # not a comment\nk: v # real\n"));
        assert!(has("s: | # header comment\n  text\n"));
        assert!(!has("- |\n  # x\n- a\n"));
        assert!(has("- key: >-\n    # x\n  other: 1 # c\n"));
        assert!(has("s: |\n  text\n# after\n"));
    }

    #[test]
    fn flow_and_multiline_quotes() {
        assert!(!has("a: [\"x # y\", 'z # w']\n"));
        assert!(has("a: [\"x\", 'z'] # c\n"));
        assert!(!has("a: \"line one\n  # inside\n  end\"\n"));
    }

    #[test]
    fn bom_prefix() {
        assert!(has("\u{FEFF}# header\nkey: v\n"));
        assert!(!has("\u{FEFF}key: v\n"));
    }

    #[test]
    fn compact_flow() {
        assert!(!has("{\"a\":\"x # y\"}\n"));
        assert!(has("{\"a\":\"x # y\"} # c\n"));
    }

    #[test]
    fn tab_and_crlf() {
        assert!(has("key: v\t# c\n"));
        assert!(has("key: v\r\n# c\r\n"));
        assert!(!has("key: v\r\nb: 1\r\n"));
    }

    #[test]
    fn no_comment() {
        assert!(!has("key: value\nother: 123"));
    }

    #[test]
    fn multiline_quote_continuation_then_real_comment() {
        assert!(has("a: foo\n  \"bar\nb: 1 # real comment\n"));
    }

    #[test]
    fn multi_document() {
        assert!(!has("---\na: 1\n...\n---\nb: \"# x\"\n"));
        assert!(has("---\na: 1\n--- # c\nb: 2\n"));
        assert!(has("a: 1\n...\n# trailing\n"));
    }

    #[test]
    fn empty_values_and_comments() {
        assert!(has("a: # c\nb: 1\n"));
        assert!(!has("a:\nb: 1\n"));
        assert!(has("? k # c\n: v\n"));
    }

    #[test]
    fn anchors_and_aliases() {
        assert!(!has("a: &x#y 1\nb: *x#y\n"));
        assert!(has("a: &x 1 # c\nb: *x\n"));
    }

    #[test]
    fn invalid_yaml_is_error() {
        assert!(has_comments("a: [").is_err());
    }

    #[test]
    fn multiline_flow_collection() {
        assert!(has("a: [1,\n  2, # c\n  3]\n"));
        assert!(!has("a: [1,\n  2,\n  3]\n"));
        assert!(has("a: {x: 1,\n  # c\n  y: 2}\n"));
    }

    #[test]
    fn directive_comment() {
        assert!(has("%YAML 1.2 # c\n---\na: 1\n"));
        assert!(!has("%YAML 1.2\n---\na: 1\n"));
    }

    #[test]
    fn block_scalar_indicators() {
        assert!(!has("s: |2\n   # body\n  x\nk: v\n"));
        assert!(has("s: |2 # c\n   body\n"));
        assert!(!has("s: |+\n  # body\n\nk: v\n"));
        assert!(has("s: |+\n  body\n\n# after\nk: v\n"));
    }

    #[test]
    fn tag_containing_hash() {
        assert!(!has("a: !foo#bar x\n"));
        assert!(has("a: !foo#bar x # c\n"));
    }

    #[test]
    fn non_ascii_before_comment() {
        assert!(has("\u{e9}: \"\u{fc}\" # c\n"));
        assert!(!has("\u{e9}: \"\u{fc} # x\"\n"));
        assert!(has("k: \u{1F600}\u{1F600} # c\n"));
        assert!(!has("k: \"\u{1F600} # x\"\nz: 1\n"));
    }

    #[test]
    fn plain_scalar_continuation() {
        assert!(has("a: one\n  two # c\n"));
        assert!(!has("a: one\n  two#three\n"));
    }
}
