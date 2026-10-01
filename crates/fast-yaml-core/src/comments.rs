//! Comment detection for YAML input.
//!
//! The formatter re-emits the parsed value tree and drops comments, so callers that must not
//! lose them need to know beforehand whether the input has any.

use std::ops::{ControlFlow, Range};

use crate::error::{ParseResult, SourcePosition};
use crate::events::{self, Event, EventItem, ScalarStyle};
use crate::input::NormalizedInput;
use crate::limits::MaxScanAhead;

/// Returns `true` if `input` contains at least one YAML comment.
///
/// `max` bounds how far the scanner may read past a node, see [`MaxScanAhead`].
///
/// Comments are located from parser event spans: plain and block scalar content is opaque,
/// quoted scalars are skipped with an escape-aware scan from their opening quote, and any `#`
/// at line start or after whitespace in the remaining text is a comment.
///
/// # Errors
///
/// Returns a [`ParseError`](crate::ParseError) if `input` is not valid YAML or exceeds `max`.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::{MaxScanAhead, has_comments};
///
/// let max = MaxScanAhead::DEFAULT;
/// assert!(has_comments("key: value # note", max).unwrap());
/// assert!(!has_comments("url: \"http://x/ # not a comment\"", max).unwrap());
/// assert!(has_comments("a: foo\n  \"bar\nb: 1 # real comment\n", max).unwrap());
/// ```
pub fn has_comments(input: &str, max: MaxScanAhead) -> ParseResult<bool> {
    has_comments_normalized(&NormalizedInput::new(input)?, max)
}

/// Returns whether already validated `input` contains a YAML comment.
///
/// Same detection as [`has_comments`], without normalizing the text again.
///
/// # Errors
///
/// Returns a [`ParseError`](crate::ParseError) if `input` is not valid YAML or exceeds `max`.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::{MaxScanAhead, NormalizedInput, has_comments_normalized};
///
/// let input = NormalizedInput::new("a: 1 # note\n")?;
/// assert!(has_comments_normalized(&input, MaxScanAhead::DEFAULT)?);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn has_comments_normalized(
    input: &NormalizedInput<'_>,
    max: MaxScanAhead,
) -> ParseResult<bool> {
    let mut scanner = CommentScanner::new(input);
    drive(input, max, |item| scanner.observe(item))?;
    Ok(scanner.found_any() || !scanner.finish().is_empty())
}

/// Returns the byte range of every YAML comment in `input`, from `#` to the end of its line.
///
/// Ranges exclude the line terminator, are relative to `input` (a leading BOM is counted), and
/// come in source order. Detection follows the same rules as [`has_comments`].
///
/// # Errors
///
/// Returns a [`ParseError`](crate::ParseError) if `input` is not valid YAML or exceeds `max`.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::{MaxScanAhead, find_comments};
///
/// let input = "a: 1 # one\nb: \"# not\"\n# two\n";
/// let found: Vec<&str> = find_comments(input, MaxScanAhead::DEFAULT)
///     .unwrap()
///     .into_iter()
///     .map(|r| &input[r])
///     .collect();
/// assert_eq!(found, ["# one", "# two"]);
/// ```
pub fn find_comments(input: &str, max: MaxScanAhead) -> ParseResult<Vec<Range<usize>>> {
    let normalized = NormalizedInput::new(input)?;
    let mut scanner = CommentScanner::new(&normalized);
    drive(&normalized, max, |item| {
        let _ = scanner.observe(item);
        ControlFlow::Continue(())
    })?;
    Ok(scanner
        .finish()
        .into_iter()
        .map(|r| normalized.original_offset(r.start)..normalized.original_offset(r.end))
        .collect())
}

/// Locates comments from the parser events of one normalized text.
///
/// Feed it every [`EventItem`], in order, then call [`finish`](Self::finish). It lets a
/// caller that already drives a parser find the comments in the same pass instead of parsing
/// the text again.
///
/// The scanner works on the exact text it was built from; building it from a
/// [`NormalizedInput`] guarantees that this is the text the parser sees (saphyr does not skip a
/// leading BOM). It keeps a `Vec<char>` copy of the text, about four times its size.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::{CommentScanner, NormalizedInput};
/// use fast_yaml_core::events::EventStream;
/// use fast_yaml_core::limits::ParseLimits;
///
/// let input = NormalizedInput::new("a: 1 # one\nb: \"# not\"\n")?;
/// let mut scanner = CommentScanner::new(&input);
/// for item in EventStream::new(&input, ParseLimits::default()) {
///     let _ = scanner.observe(&item?);
/// }
/// let ranges = scanner.finish();
/// assert_eq!(&input.as_str()[ranges[0].clone()], "# one");
/// assert_eq!(ranges.len(), 1);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug)]
pub struct CommentScanner {
    chars: Vec<char>,
    line_starts: Vec<usize>,
    cursor: usize,
    // (char index, byte offset) of the last range end; hits arrive in source order
    pos: (usize, usize),
    ranges: Vec<Range<usize>>,
}

impl CommentScanner {
    /// Creates a scanner for `input`, the text whose events will be observed.
    #[must_use]
    pub fn new(input: &NormalizedInput<'_>) -> Self {
        let chars: Vec<char> = input.as_str().chars().collect();
        let line_starts = line_starts(&chars);
        Self {
            chars,
            line_starts,
            cursor: 0,
            pos: (0, 0),
            ranges: Vec::new(),
        }
    }

    /// Processes the next parser event.
    ///
    /// Returns `Break` as soon as at least one comment has been located, so a caller that only
    /// asks whether a comment exists can stop parsing; callers that want every comment ignore it.
    pub fn observe(&mut self, item: &EventItem<'_>) -> ControlFlow<()> {
        if let Event::Scalar { style, .. } = item.event {
            let (start, end) = (
                char_offset(&self.line_starts, item.at),
                char_offset(&self.line_starts, item.end),
            );
            if start != end && start >= self.cursor {
                self.scan_gap(self.cursor, start);
                self.cursor = match style {
                    ScalarStyle::SingleQuoted => skip_quoted(&self.chars, start, '\''),
                    ScalarStyle::DoubleQuoted => skip_quoted(&self.chars, start, '"'),
                    _ => end,
                };
            }
        }
        if self.found_any() {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }

    /// Whether a comment has been located so far.
    #[must_use]
    pub const fn found_any(&self) -> bool {
        !self.ranges.is_empty()
    }

    /// Scans the text after the last scalar and returns the byte range of every comment.
    ///
    /// Ranges run from `#` to the end of its line, exclude the line terminator, are relative to
    /// the text the scanner was built from and come in source order.
    #[must_use]
    pub fn finish(mut self) -> Vec<Range<usize>> {
        self.scan_gap(self.cursor, self.chars.len());
        self.ranges
    }

    fn scan_gap(&mut self, from: usize, to: usize) {
        let to = to.min(self.chars.len());
        let mut i = from;
        while i < to {
            let chars = &self.chars;
            if chars[i] == '#' && (i == 0 || matches!(chars[i - 1], ' ' | '\t' | '\n' | '\r')) {
                let end = line_end(chars, i);
                let start_byte = self.pos.1 + byte_len(&chars[self.pos.0..i]);
                let end_byte = start_byte + byte_len(&chars[i..end]);
                self.ranges.push(start_byte..end_byte);
                self.pos = (end, end_byte);
                i = end;
            }
            i += 1;
        }
    }
}

/// Parses `input` event by event, feeding each event to `on_event` until it returns `Break`.
///
/// Unlike [`EventStream`](crate::events::EventStream) it applies only the scan-ahead bound, no
/// other limits or merge checks: finding comments must not fail on input the formatter would
/// reject later with its own error.
fn drive(
    input: &NormalizedInput<'_>,
    max: MaxScanAhead,
    mut on_event: impl FnMut(&EventItem<'_>) -> ControlFlow<()>,
) -> ParseResult<()> {
    let mut parser = input.scanner(max);
    while let Some(event) = parser.next_event() {
        let (event, span) = event?;
        if let Some(item) = events::item_of(&event, span, None)
            && on_event(&item).is_break()
        {
            break;
        }
    }
    Ok(())
}

/// Char offsets at which each line starts, splitting like saphyr (`\n`, `\r\n`, lone `\r`).
fn line_starts(chars: &[char]) -> Vec<usize> {
    let mut starts = vec![0];
    for (i, &c) in chars.iter().enumerate() {
        let ends_line = c == '\n' || (c == '\r' && chars.get(i + 1) != Some(&'\n'));
        if ends_line {
            starts.push(i + 1);
        }
    }
    starts
}

/// Converts a line and column to a char offset; the parser's own index is unusable because it
/// adds byte counts after non-ASCII directive names.
fn char_offset(line_starts: &[usize], position: SourcePosition) -> usize {
    line_starts
        .get(position.line.wrapping_sub(1))
        .map_or(usize::MAX, |start| {
            start + position.column.saturating_sub(1)
        })
}

fn byte_len(chars: &[char]) -> usize {
    chars.iter().map(|c| c.len_utf8()).sum()
}

/// Index of the line terminator at or after `from`, or `chars.len()`.
fn line_end(chars: &[char], from: usize) -> usize {
    chars[from..]
        .iter()
        .position(|&c| c == '\n' || c == '\r')
        .map_or(chars.len(), |p| from + p)
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
    use crate::events::EventStream;
    use crate::limits::ParseLimits;

    fn has(input: &str) -> bool {
        has_comments(input, MaxScanAhead::DEFAULT).unwrap()
    }

    #[test]
    fn scan_ahead_limit_applies_to_comment_scans() {
        let max = MaxScanAhead::new(8).unwrap();
        let input = "[1, 2, 3, 4, 5, 6, 7, 8, 9] # c";
        let limit_error = |error: Option<crate::ParseError>| {
            matches!(
                error,
                Some(crate::ParseError::LimitExceeded {
                    kind: crate::LimitKind::ScanAhead(_),
                    ..
                })
            )
        };
        assert!(limit_error(has_comments(input, max).err()));
        assert!(limit_error(find_comments(input, max).err()));
    }

    #[test]
    fn unterminated_directive_is_an_error() {
        assert!(has_comments("%", MaxScanAhead::DEFAULT).is_err());
        assert!(has_comments("a: 1\n%", MaxScanAhead::DEFAULT).is_err());
    }

    #[test]
    fn non_ascii_directive_does_not_shift_offsets() {
        assert!(has("%FOO ééééé\n---\na: b # c\n"));
        assert!(!has("%FOO ééééé\n---\na: \"x # y\"\n"));
        assert!(has("%ééééé x\n---\na: b # c\n"));
        assert!(!has("%FOO 日本語日本語\n---\na: 'x # y'\n"));
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
        assert!(has_comments("a: [", MaxScanAhead::DEFAULT).is_err());
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

    fn ranges(input: &str) -> Vec<&str> {
        find_comments(input, MaxScanAhead::DEFAULT)
            .unwrap()
            .into_iter()
            .map(|r| &input[r])
            .collect()
    }

    #[test]
    fn find_comments_byte_ranges_after_non_ascii() {
        assert_eq!(ranges("\u{e9}: \u{1F600} # c\nb: 1 # d\n"), ["# c", "# d"]);
    }

    #[test]
    fn find_comments_bom_offsets_are_input_relative() {
        let input = "\u{FEFF}a: 1 # c\n";
        let found = find_comments(input, MaxScanAhead::DEFAULT).unwrap();
        let hash = input.find('#').unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0], hash..input.len() - 1);
    }

    #[test]
    fn find_comments_skips_scalars() {
        assert!(ranges("a: 'x # y'\nb: \"p # q\"\nc: \"m\n  # n\"\n").is_empty());
        assert!(ranges("s: |\n  # body\n  text\n").is_empty());
    }

    #[test]
    fn find_comments_single_range_per_line() {
        assert_eq!(ranges("a: 1 # x # y\n"), ["# x # y"]);
    }

    #[test]
    fn find_comments_line_endings() {
        assert_eq!(ranges("a: 1 # x\r\nb: 2 # y\rc: 3\n"), ["# x", "# y"]);
    }

    #[test]
    fn plain_scalar_continuation() {
        assert!(has("a: one\n  two # c\n"));
        assert!(!has("a: one\n  two#three\n"));
    }

    #[test]
    fn scanner_breaks_once_a_comment_is_found() {
        let input = NormalizedInput::new("a: \"x\" # c\nb: 'y'\n").unwrap();
        let mut scanner = CommentScanner::new(&input);
        let mut broke_at = None;
        let mut seen = 0;
        for item in EventStream::new(&input, ParseLimits::default()) {
            seen += 1;
            if scanner.observe(&item.unwrap()).is_break() {
                broke_at = Some(seen);
                break;
            }
        }
        assert!(broke_at.is_some_and(|n| n < 10));
        assert!(scanner.found_any());
    }

    #[test]
    fn scanner_matches_find_comments_in_input_coordinates() {
        let text = "\u{e9}: 1 # a\n---\nb: \"# no\" # b\n";
        let input = NormalizedInput::new(text).unwrap();
        let mut scanner = CommentScanner::new(&input);
        for item in EventStream::new(&input, ParseLimits::default()) {
            let _ = scanner.observe(&item.unwrap());
        }
        assert_eq!(
            scanner.finish(),
            find_comments(text, MaxScanAhead::DEFAULT).unwrap()
        );
    }
}
