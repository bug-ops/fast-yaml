//! Source context extraction for diagnostic display.

use crate::{
    Location, Span,
    comments::Comment,
    diagnostic::{ContextLine, DiagnosticContext},
    scan::{DocumentMarkers, IMPLICIT_DOCUMENT, KeyRepeat, SourceScan},
    source::offset::{ByteOffset, ByteRange},
    tokenizer::{FlowIndex, FlowTokenizer},
};
use fast_yaml_core::SourcePosition;
use saphyr_parser::{Marker, Span as SaphyrSpan};
use std::collections::HashMap;
use std::sync::OnceLock;

/// Extracts source code context for diagnostics.
///
/// Efficiently indexes source text to provide line-based access
/// and context extraction for error reporting. Uses binary search
/// for O(log n) location lookups.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{SourceContext, Location, Span};
///
/// let source = "line 1\nline 2\nline 3";
/// let ctx = SourceContext::new(source);
///
/// assert_eq!(ctx.get_line(1), Some("line 1"));
/// assert_eq!(ctx.get_line(2), Some("line 2"));
/// ```
pub struct SourceContext<'a> {
    source: &'a str,
    line_starts: Vec<usize>,
    line_ends: Vec<usize>,
    line_index: Vec<LineIndex>,
}

/// Per-line char/byte column mapping strategy.
enum LineIndex {
    /// All bytes are ASCII: char columns equal byte columns.
    Ascii,
    /// Contains multi-byte chars: table built lazily on first query.
    Wide(OnceLock<CharStarts>),
}

/// Byte offset of every char in a line, giving O(1) char column to byte and O(log n) back.
pub struct CharStarts(Box<[usize]>);

impl CharStarts {
    /// Builds the char-start table for one line.
    pub fn new(line: &str) -> Self {
        Self(line.char_indices().map(|(byte, _)| byte).collect())
    }

    /// Byte offset of the 0-indexed char `col`, or `line_len` when past the end.
    pub fn byte_of(&self, col: usize, line_len: usize) -> usize {
        self.0.get(col).copied().unwrap_or(line_len)
    }

    /// Number of chars starting strictly before byte `byte`.
    pub fn chars_before(&self, byte: usize) -> usize {
        self.0.partition_point(|&start| start < byte)
    }
}

/// Maximum number of chars of a source line kept in a [`ContextLine`].
///
/// Bounds the size of every diagnostic context regardless of source line length.
pub const MAX_CONTEXT_COLUMNS: usize = 120;

/// Char window of a source line, bounding the size of a diagnostic context.
struct ContextWindow<'a> {
    text: &'a str,
    column_offset: usize,
    truncated_end: bool,
    chars: usize,
}

impl<'a> ContextWindow<'a> {
    /// Windows `line`, centred on `anchor` (`(byte offset in line, 0-based char column)`)
    /// or starting at the line start when there is none.
    fn of(line: &'a str, anchor: Option<(Option<usize>, usize)>) -> Self {
        let half = MAX_CONTEXT_COLUMNS / 2;
        let (start_byte, column_offset) = match anchor {
            Some((Some(byte), column)) if line.is_char_boundary(byte) => {
                let (mut start_byte, mut walked) = (byte, 0);
                for (idx, _) in line
                    .get(..byte)
                    .unwrap_or_default()
                    .char_indices()
                    .rev()
                    .take(half)
                {
                    start_byte = idx;
                    walked += 1;
                }
                (start_byte, column.saturating_sub(walked))
            }
            Some((_, column)) => {
                let offset = column.saturating_sub(half);
                let byte = line
                    .char_indices()
                    .nth(offset)
                    .map_or(line.len(), |(byte, _)| byte);
                (byte, offset)
            }
            None => (0, 0),
        };

        let rest = line.get(start_byte..).unwrap_or_default();
        let (text, truncated_end) = match rest.char_indices().nth(MAX_CONTEXT_COLUMNS) {
            Some((end, _)) => (rest.get(..end).unwrap_or_default(), true),
            None => (rest, false),
        };

        Self {
            text,
            column_offset,
            truncated_end,
            chars: text.chars().count(),
        }
    }

    /// Clips the absolute 1-based column range `[start, end)` to this window.
    fn clip(&self, start: usize, end: usize) -> Option<(usize, usize)> {
        let lo = self.column_offset.saturating_add(1);
        let hi = self
            .column_offset
            .saturating_add(self.chars)
            .saturating_add(1);
        let (start, end) = (start.max(lo), end.min(hi));
        (start <= end).then_some((start, end))
    }
}

/// Yields `(start, end)` byte bounds of every line, excluding terminators.
///
/// Lines end at `\n`, `\r\n` or a lone `\r`, matching saphyr's line counting. A final empty
/// line after a trailing terminator (and the sole line of an empty source) is included.
/// This is the single source of truth for line splitting in the crate.
fn line_bounds(source: &str) -> impl Iterator<Item = (usize, usize)> + '_ {
    let bytes = source.as_bytes();
    let mut start = 0;
    let mut done = false;
    std::iter::from_fn(move || {
        if done {
            return None;
        }
        let end = bytes
            .get(start..)
            .unwrap_or_default()
            .iter()
            .position(|b| matches!(b, b'\n' | b'\r'))
            .map_or(bytes.len(), |rel| start + rel);
        let line_start = start;
        if end == bytes.len() {
            done = true;
        } else {
            let crlf = bytes.get(end) == Some(&b'\r') && bytes.get(end + 1) == Some(&b'\n');
            start = end + 1 + usize::from(crlf);
        }
        Some((line_start, end))
    })
}

/// Yields `(byte_start, line)` for each line, like `str::lines` but with [`line_bounds`] splitting.
///
/// Use this instead of `str::lines` so line indices always agree with [`SourceContext`].
pub fn source_lines(source: &str) -> impl Iterator<Item = (usize, &str)> {
    line_bounds(source)
        .filter(|&(start, _)| start < source.len())
        .filter_map(|(start, end)| Some((start, source.get(start..end)?)))
}

/// Line-only variant of [`source_lines`].
pub fn lines_of(source: &str) -> impl Iterator<Item = &str> {
    source_lines(source).map(|(_, line)| line)
}

impl<'a> SourceContext<'a> {
    /// Creates a new source context analyzer.
    ///
    /// Builds an index of line start positions for efficient lookup.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::SourceContext;
    ///
    /// let source = "line 1\nline 2\nline 3";
    /// let ctx = SourceContext::new(source);
    /// ```
    #[must_use]
    pub fn new(source: &'a str) -> Self {
        let (line_starts, line_ends): (Vec<usize>, Vec<usize>) = line_bounds(source).unzip();
        let line_index = line_starts
            .iter()
            .zip(&line_ends)
            .map(|(&start, &end)| {
                if source
                    .as_bytes()
                    .get(start..end)
                    .is_some_and(<[u8]>::is_ascii)
                {
                    LineIndex::Ascii
                } else {
                    LineIndex::Wide(OnceLock::new())
                }
            })
            .collect();

        Self {
            source,
            line_starts,
            line_ends,
            line_index,
        }
    }

    /// Char-start table of the 0-indexed line, built on first use; `None` for ASCII lines
    /// (and unknown lines), where char columns equal byte columns.
    pub(crate) fn char_starts(&self, line_idx: usize) -> Option<&CharStarts> {
        let LineIndex::Wide(cell) = self.line_index.get(line_idx)? else {
            return None;
        };
        Some(cell.get_or_init(|| CharStarts::new(self.get_line(line_idx + 1).unwrap_or_default())))
    }

    /// Gets a specific line by number (1-indexed), without its line terminator.
    ///
    /// `\n`, `\r\n` and lone `\r` all terminate a line.
    ///
    /// Returns `None` if the line number is out of bounds.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::SourceContext;
    ///
    /// let source = "line 1\nline 2\nline 3";
    /// let ctx = SourceContext::new(source);
    ///
    /// assert_eq!(ctx.get_line(1), Some("line 1"));
    /// assert_eq!(ctx.get_line(2), Some("line 2"));
    /// assert_eq!(ctx.get_line(100), None);
    /// ```
    #[must_use]
    pub fn get_line(&self, line_number: usize) -> Option<&'a str> {
        let idx = line_number.checked_sub(1)?;
        self.source
            .get(*self.line_starts.get(idx)?..*self.line_ends.get(idx)?)
    }

    /// Extracts context lines around a span.
    ///
    /// Returns up to `context_lines` before and after the span,
    /// with highlighting information for the affected portions.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{SourceContext, Location, Span};
    ///
    /// let source = "line 1\nline 2\nline 3";
    /// let ctx = SourceContext::new(source);
    ///
    /// let span = Span::new(
    ///     Location::new(2, 1, 7),
    ///     Location::new(2, 6, 12)
    /// );
    ///
    /// let diagnostic_ctx = ctx.extract_context(span, 1);
    /// assert!(!diagnostic_ctx.lines.is_empty());
    /// ```
    #[must_use]
    pub fn extract_context(&self, span: Span, context_lines: usize) -> DiagnosticContext {
        let start_line = span.start.line;
        let end_line = span.end.line;

        let first_line = start_line.saturating_sub(context_lines).max(1);
        let last_line = (end_line + context_lines).min(self.line_starts.len());

        let mut lines = Vec::new();

        for line_num in first_line..=last_line {
            if let Some(content) = self.get_line(line_num) {
                let anchor = (line_num == start_line).then(|| {
                    let line_start = self.get_line_offset(line_num);
                    (
                        span.start.offset.checked_sub(line_start),
                        span.start.column.saturating_sub(1),
                    )
                });
                let window = ContextWindow::of(content, anchor);
                let mut highlights = Vec::new();

                if line_num >= start_line && line_num <= end_line {
                    let start_col = if line_num == start_line {
                        span.start.column
                    } else {
                        1
                    };

                    let end_col = if line_num == end_line {
                        span.end.column
                    } else {
                        usize::MAX
                    };

                    if let Some(highlight) = window.clip(start_col, end_col) {
                        highlights.push(highlight);
                    }
                }

                lines.push(ContextLine {
                    line_number: line_num,
                    content: window.text.to_string(),
                    column_offset: window.column_offset,
                    truncated_end: window.truncated_end,
                    highlights,
                });
            }
        }

        DiagnosticContext { lines }
    }

    /// Gets the source snippet for a span.
    ///
    /// Returns the exact text covered by the span.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{SourceContext, Location, Span};
    ///
    /// let source = "key: value";
    /// let ctx = SourceContext::new(source);
    ///
    /// let span = Span::new(
    ///     Location::new(1, 1, 0),
    ///     Location::new(1, 4, 3)
    /// );
    ///
    /// assert_eq!(ctx.get_snippet(span), "key");
    /// ```
    #[must_use]
    pub fn get_snippet(&self, span: Span) -> &'a str {
        let start = self.source.floor_char_boundary(span.start.offset);
        let end = self.source.floor_char_boundary(span.end.offset);
        self.source.get(start..end).unwrap_or_default()
    }

    /// Converts a byte offset to a Location.
    ///
    /// Uses binary search for efficient lookup.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::SourceContext;
    ///
    /// let source = "line 1\nline 2\nline 3";
    /// let ctx = SourceContext::new(source);
    ///
    /// let loc = ctx.offset_to_location(7);
    /// assert_eq!(loc.line, 2);
    /// assert_eq!(loc.column, 1);
    /// ```
    #[must_use]
    pub fn offset_to_location(&self, offset: usize) -> Location {
        self.location_at(ByteOffset::new(offset))
    }

    /// Typed, total form of [`offset_to_location`](Self::offset_to_location).
    ///
    /// Offsets past the end are clamped and offsets inside a multi-byte char are floored to
    /// the char start, so this never panics.
    pub(crate) fn location_at(&self, offset: ByteOffset) -> Location {
        let offset = self.source.floor_char_boundary(offset.get());

        let line_idx = match self.line_starts.binary_search(&offset) {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        };

        let line = line_idx + 1;
        let line_start = self.line_starts.get(line_idx).copied().unwrap_or_default();

        let prefix_len = offset.saturating_sub(line_start);
        let line_len = self
            .line_ends
            .get(line_idx)
            .map_or(0, |end| end - line_start);
        let column = self.char_starts(line_idx).map_or(prefix_len, |table| {
            table.chars_before(prefix_len) + prefix_len.saturating_sub(line_len)
        }) + 1;

        Location::new(line, column, offset)
    }

    /// Returns the total number of lines.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::SourceContext;
    ///
    /// let source = "line 1\nline 2\nline 3";
    /// let ctx = SourceContext::new(source);
    ///
    /// assert_eq!(ctx.line_count(), 3);
    /// ```
    #[must_use]
    pub const fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// Gets the byte offset where a line starts (1-indexed).
    ///
    /// Returns 0 for line 1, and the offset of the first character
    /// of each subsequent line. Returns 0 for invalid line numbers.
    ///
    /// Pre-computed during construction for O(1) access.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::SourceContext;
    ///
    /// let source = "line 1\nline 2\nline 3";
    /// let ctx = SourceContext::new(source);
    ///
    /// assert_eq!(ctx.get_line_offset(1), 0);
    /// assert_eq!(ctx.get_line_offset(2), 7);
    /// assert_eq!(ctx.get_line_offset(3), 14);
    /// ```
    #[must_use]
    pub fn get_line_offset(&self, line_num: usize) -> usize {
        line_num
            .checked_sub(1)
            .and_then(|idx| self.line_starts.get(idx))
            .copied()
            .unwrap_or_default()
    }

    /// Typed twin of [`get_line_offset`](Self::get_line_offset).
    pub(crate) fn line_start(&self, line_num: usize) -> ByteOffset {
        ByteOffset::new(self.get_line_offset(line_num))
    }

    /// Converts a saphyr marker (char-based column) to a byte offset.
    ///
    /// This is the only place that reads `Marker::col`; a marker past the end maps to the
    /// end of its line, or of the source when the line does not exist.
    #[expect(clippy::disallowed_methods)]
    pub(crate) fn byte_offset_of(&self, marker: Marker) -> ByteOffset {
        let Some(line) = self.get_line(marker.line()) else {
            return ByteOffset::new(self.source.len());
        };
        let byte_col = marker
            .line()
            .checked_sub(1)
            .and_then(|idx| self.char_starts(idx))
            .map_or_else(
                || marker.col().min(line.len()),
                |table| table.byte_of(marker.col(), line.len()),
            );
        self.line_start(marker.line()).add_bytes(byte_col)
    }

    /// Converts a saphyr marker to a [`Location`] with a 1-indexed char column.
    pub(crate) fn location_of(&self, marker: Marker) -> Location {
        let offset = self.byte_offset_of(marker);
        #[expect(clippy::disallowed_methods)]
        let column = marker.col() + 1;
        Location::new(marker.line(), column, offset.get())
    }

    /// Converts a saphyr span to a [`Span`].
    pub(crate) fn span_of(&self, span: SaphyrSpan) -> Span {
        Span::new(self.location_of(span.start), self.location_of(span.end))
    }

    /// Converts the positions of a parser event (1-indexed line and char column) to a [`Span`].
    pub(crate) fn span_between(&self, from: SourcePosition, to: SourcePosition) -> Span {
        Span::new(
            self.location_of(Self::marker_at(from)),
            self.location_of(Self::marker_at(to)),
        )
    }

    /// Converts the positions of a parser event to a byte range.
    pub(crate) fn byte_range_between(&self, from: SourcePosition, to: SourcePosition) -> ByteRange {
        ByteRange::new(
            self.byte_offset_of(Self::marker_at(from)),
            self.byte_offset_of(Self::marker_at(to)),
        )
    }

    fn marker_at(position: SourcePosition) -> Marker {
        Marker::new(0, position.line, position.column.saturating_sub(1))
    }

    /// Converts a byte range to a [`Span`] with line and char-column locations.
    pub(crate) fn span_of_bytes(&self, range: ByteRange) -> Span {
        Span::new(
            self.location_at(range.start()),
            self.location_at(range.end()),
        )
    }

    /// Builds a [`Span`] covering `len` bytes starting at `start`.
    pub(crate) fn span_at(&self, start: ByteOffset, len: usize) -> Span {
        self.span_of_bytes(ByteRange::new(start, start.add_bytes(len)))
    }

    /// Converts a saphyr span to a byte range.
    pub(crate) fn byte_range_of(&self, span: SaphyrSpan) -> ByteRange {
        ByteRange::new(
            self.byte_offset_of(span.start),
            self.byte_offset_of(span.end),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_offset(ctx: &SourceContext<'_>, line: usize, col: usize) -> usize {
        ctx.byte_offset_of(Marker::new(0, line, col)).get() - ctx.get_line_offset(line)
    }

    #[test]
    fn test_ascii_fast_path_matches_char_path() {
        let source = "ab: [1, 2]\nж: [1, 2]\r\nq: z";
        let ctx = SourceContext::new(source);
        for (line, col, byte) in [(1, 6, 6), (2, 4, 5), (3, 3, 3)] {
            assert_eq!(ctx_offset(&ctx, line, col), byte);
        }
        let offset = ctx.get_line_offset(2) + "ж: ".len();
        assert_eq!(ctx.offset_to_location(offset).column, 4);
        let offset = ctx.get_line_offset(1) + 6;
        assert_eq!(ctx.offset_to_location(offset).column, 7);
    }

    #[test]
    fn test_char_starts_only_for_non_ascii_lines() {
        let ctx = SourceContext::new("plain\r\nжж\r\nplain");
        assert!(ctx.char_starts(0).is_none());
        assert!(ctx.char_starts(1).is_some());
        assert!(ctx.char_starts(2).is_none());
        assert!(ctx.char_starts(99).is_none());
    }

    #[test]
    fn test_non_ascii_line_does_not_affect_ascii_lines() {
        let ctx = SourceContext::new("ж: 1\nabc: def\n");
        assert_eq!(ctx_offset(&ctx, 2, 5), 5);
        assert_eq!(ctx.offset_to_location(ctx.get_line_offset(2) + 5).column, 6);
    }

    #[test]
    fn test_marker_col_past_line_end_clamps_for_both_line_kinds() {
        let ctx = SourceContext::new("abc\nжжж\n");
        assert_eq!(ctx_offset(&ctx, 1, 50), 3);
        assert_eq!(ctx_offset(&ctx, 2, 50), "жжж".len());
        assert_eq!(ctx_offset(&ctx, 2, 3), "жжж".len());
    }

    #[test]
    fn test_marker_out_of_range_line_maps_to_source_end() {
        let ctx = SourceContext::new("ж\nb");
        let marker = Marker::new(0, 50, 0);
        assert_eq!(ctx.byte_offset_of(marker).get(), "ж\nb".len());
    }

    #[test]
    fn test_byte_offset_lookups_on_long_wide_line_are_linear() {
        let source = format!("k: [{}]", "ж ,".repeat(100_000));
        let ctx = SourceContext::new(&source);
        let chars = source.chars().count();
        let start = std::time::Instant::now();
        let total: usize = (0..chars)
            .step_by(3)
            .map(|col| ctx.byte_offset_of(Marker::new(0, 1, col)).get())
            .sum();
        assert!(total > 0);
        assert!(start.elapsed() < std::time::Duration::from_secs(10));
    }

    #[test]
    fn test_crlf_wide_line_columns() {
        let source = "яя: 1\r\nz: 2\r\n";
        let ctx = SourceContext::new(source);
        assert_eq!(ctx_offset(&ctx, 1, 2), "я".len() * 2);
        let terminator = ctx.get_line_offset(1) + "яя: 1".len();
        assert_eq!(ctx.offset_to_location(terminator).column, 6);
        assert_eq!(ctx.offset_to_location(terminator + 1).column, 7);
        assert_eq!(ctx.offset_to_location(ctx.get_line_offset(2)).line, 2);
    }

    #[test]
    fn test_wide_line_round_trip_for_every_char() {
        let line = "aж😀b́c";
        let ctx = SourceContext::new(line);
        for (col, (byte, _)) in line.char_indices().enumerate() {
            assert_eq!(ctx_offset(&ctx, 1, col), byte);
            assert_eq!(ctx.offset_to_location(byte).column, col + 1);
        }
    }

    #[test]
    fn test_new_empty() {
        let ctx = SourceContext::new("");
        assert_eq!(ctx.line_count(), 1);
    }

    #[test]
    fn test_new_single_line() {
        let ctx = SourceContext::new("single line");
        assert_eq!(ctx.line_count(), 1);
        assert_eq!(ctx.get_line(1), Some("single line"));
    }

    #[test]
    fn test_new_multiple_lines() {
        let ctx = SourceContext::new("line 1\nline 2\nline 3");
        assert_eq!(ctx.line_count(), 3);
    }

    #[test]
    fn test_get_line() {
        let ctx = SourceContext::new("line 1\nline 2\nline 3");

        assert_eq!(ctx.get_line(1), Some("line 1"));
        assert_eq!(ctx.get_line(2), Some("line 2"));
        assert_eq!(ctx.get_line(3), Some("line 3"));
        assert_eq!(ctx.get_line(0), None);
        assert_eq!(ctx.get_line(4), None);
    }

    #[test]
    fn test_get_line_no_trailing_newline() {
        let ctx = SourceContext::new("line 1\nline 2");
        assert_eq!(ctx.get_line(2), Some("line 2"));
    }

    #[test]
    fn test_offset_to_location() {
        let source = "line 1\nline 2\nline 3";
        let ctx = SourceContext::new(source);

        let loc = ctx.offset_to_location(0);
        assert_eq!(loc.line, 1);
        assert_eq!(loc.column, 1);

        let loc = ctx.offset_to_location(7);
        assert_eq!(loc.line, 2);
        assert_eq!(loc.column, 1);

        let loc = ctx.offset_to_location(10);
        assert_eq!(loc.line, 2);
        assert_eq!(loc.column, 4);
    }

    #[test]
    fn test_offset_to_location_utf8() {
        let source = "emoji: 😀\nline 2";
        let ctx = SourceContext::new(source);

        let loc = ctx.offset_to_location(7);
        assert_eq!(loc.line, 1);
        assert_eq!(loc.column, 8);
    }

    #[test]
    fn test_line_endings_lf_crlf_lone_cr() {
        for source in ["a\nб\nc", "a\r\nб\r\nc", "a\rб\rc"] {
            let ctx = SourceContext::new(source);
            assert_eq!(ctx.line_count(), 3, "{source:?}");
            assert_eq!(ctx.get_line(1), Some("a"));
            assert_eq!(ctx.get_line(2), Some("б"));
            assert_eq!(ctx.get_line(3), Some("c"));
            let loc = ctx.offset_to_location(ctx.get_line_offset(3));
            assert_eq!((loc.line, loc.column), (3, 1));
        }
        assert_eq!(SourceContext::new("a\r\nb").get_line_offset(2), 3);
        assert_eq!(SourceContext::new("a\rb").get_line_offset(2), 2);
    }

    #[test]
    fn test_lines_of_matches_str_lines_without_lone_cr() {
        for source in [
            "",
            "a",
            "a\n",
            "a\n\n",
            "a\r\nb\r\n",
            "\n",
            "a\r\n\r\nb",
            "é\nж\r\n",
        ] {
            let expected: Vec<&str> = source.lines().collect();
            let actual: Vec<&str> = lines_of(source).collect();
            assert_eq!(actual, expected, "{source:?}");
            assert_eq!(LintContext::new(source).lines(), expected.as_slice());
        }
    }

    #[test]
    fn test_source_lines_lone_cr_and_offsets() {
        let source = "a\rб\r\nc\n";
        let got: Vec<_> = source_lines(source).collect();
        assert_eq!(got, [(0, "a"), (2, "б"), (6, "c")]);
        let ctx = SourceContext::new(source);
        for (i, (start, line)) in got.iter().enumerate() {
            assert_eq!(ctx.get_line_offset(i + 1), *start);
            assert_eq!(ctx.get_line(i + 1), Some(*line));
        }
    }

    #[test]
    fn test_location_at_is_total() {
        let ctx = SourceContext::new("ключ: 1");
        assert_eq!(ctx.offset_to_location(3).column, 2);
        assert_eq!(ctx.offset_to_location(1000).column, 8);
        assert_eq!(ctx.span_at(ByteOffset::new(6), 100).end.offset, 11);
    }

    #[test]
    fn test_get_snippet() {
        let source = "key: value";
        let ctx = SourceContext::new(source);

        let span = Span::new(Location::new(1, 1, 0), Location::new(1, 4, 3));
        assert_eq!(ctx.get_snippet(span), "key");

        let span = Span::new(Location::new(1, 6, 5), Location::new(1, 11, 10));
        assert_eq!(ctx.get_snippet(span), "value");
    }

    #[test]
    fn test_extract_context_single_line() {
        let source = "line 1\nline 2\nline 3";
        let ctx = SourceContext::new(source);

        let span = Span::new(Location::new(2, 1, 7), Location::new(2, 6, 12));
        let diagnostic_ctx = ctx.extract_context(span, 1);

        assert_eq!(diagnostic_ctx.lines.len(), 3);
        assert_eq!(diagnostic_ctx.lines[0].line_number, 1);
        assert_eq!(diagnostic_ctx.lines[1].line_number, 2);
        assert_eq!(diagnostic_ctx.lines[2].line_number, 3);

        assert_eq!(diagnostic_ctx.lines[1].highlights, vec![(1, 6)]);
    }

    #[test]
    fn test_extract_context_short_line_unchanged() {
        let ctx = SourceContext::new("key: value");
        let span = Span::new(Location::new(1, 6, 5), Location::new(1, 11, 10));
        let line = &ctx.extract_context(span, 0).lines[0];

        assert_eq!(line.content, "key: value");
        assert_eq!(line.column_offset, 0);
        assert!(!line.truncated_end);
        assert_eq!(line.highlights, vec![(6, 11)]);
    }

    #[test]
    fn test_extract_context_long_line_is_windowed() {
        let source = "a".repeat(1000);
        let ctx = SourceContext::new(&source);
        let span = Span::new(Location::new(1, 501, 500), Location::new(1, 511, 510));
        let line = &ctx.extract_context(span, 0).lines[0];

        assert_eq!(line.content.chars().count(), MAX_CONTEXT_COLUMNS);
        assert_eq!(line.column_offset, 500 - MAX_CONTEXT_COLUMNS / 2);
        assert!(line.truncated_end);
        assert_eq!(line.highlights, vec![(501, 511)]);
    }

    #[test]
    fn test_extract_context_whole_line_highlight_is_clipped() {
        let source = format!("{}\nend", "a".repeat(1000));
        let ctx = SourceContext::new(&source);
        let span = Span::new(Location::new(1, 1, 0), Location::new(2, 4, 1004));
        let line = &ctx.extract_context(span, 0).lines[0];

        assert_eq!(line.column_offset, 0);
        assert_eq!(line.highlights, vec![(1, MAX_CONTEXT_COLUMNS + 1)]);
    }

    fn ascii_span(start_col: usize, end_col: usize) -> Span {
        Span::new(
            Location::new(1, start_col, start_col - 1),
            Location::new(1, end_col, end_col - 1),
        )
    }

    fn first_line(source: &str, span: Span) -> ContextLine {
        SourceContext::new(source)
            .extract_context(span, 0)
            .lines
            .remove(0)
    }

    #[test]
    fn test_extract_context_window_boundary_120_vs_121() {
        let exact = first_line(&"a".repeat(120), ascii_span(60, 70));
        assert_eq!(exact.content.chars().count(), 120);
        assert_eq!(exact.column_offset, 0);
        assert!(!exact.truncated_end);

        let over = first_line(&"a".repeat(121), ascii_span(60, 70));
        assert_eq!(over.content.chars().count(), 120);
        assert!(over.truncated_end);
    }

    #[test]
    fn test_extract_context_highlight_at_end_of_long_line() {
        let line = first_line(&"a".repeat(1000), ascii_span(995, 1001));
        assert_eq!(line.column_offset, 994 - MAX_CONTEXT_COLUMNS / 2);
        assert!(!line.truncated_end);
        assert_eq!(line.highlights, vec![(995, 1001)]);
    }

    #[test]
    fn test_extract_context_highlight_at_start_of_long_line() {
        let line = first_line(&"a".repeat(1000), ascii_span(1, 5));
        assert_eq!(line.column_offset, 0);
        assert!(line.truncated_end);
        assert_eq!(line.highlights, vec![(1, 5)]);
    }

    #[test]
    fn test_extract_context_highlight_wider_than_window_is_clipped() {
        let line = first_line(&"a".repeat(1000), ascii_span(200, 900));
        assert_eq!(line.column_offset, 199 - MAX_CONTEXT_COLUMNS / 2);
        assert_eq!(
            line.highlights,
            vec![(200, line.column_offset + MAX_CONTEXT_COLUMNS + 1)]
        );
    }

    #[test]
    fn test_extract_context_multi_line_windows_only_start_line_around_anchor() {
        let source = format!(
            "{}\n{}\n{}",
            "a".repeat(500),
            "b".repeat(500),
            "c".repeat(500)
        );
        let span = Span::new(
            Location::new(1, 300, 299),
            Location::new(3, 400, 1002 + 399),
        );
        let lines = SourceContext::new(&source).extract_context(span, 0).lines;

        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].column_offset, 299 - MAX_CONTEXT_COLUMNS / 2);
        assert_eq!(lines[1].column_offset, 0);
        assert_eq!(lines[2].column_offset, 0);
        assert_eq!(lines[1].highlights, vec![(1, MAX_CONTEXT_COLUMNS + 1)]);
        assert_eq!(lines[2].highlights, vec![(1, MAX_CONTEXT_COLUMNS + 1)]);
        assert!(
            lines
                .iter()
                .all(|l| l.content.chars().count() <= MAX_CONTEXT_COLUMNS)
        );
    }

    #[test]
    fn test_extract_context_combining_chars_and_tabs_count_as_chars() {
        let source = format!("\t{}", "e\u{301}".repeat(200));
        let line = first_line(&source, ascii_span(1, 2));
        assert_eq!(line.content.chars().count(), MAX_CONTEXT_COLUMNS);
        assert!(line.content.starts_with('\t'));
        assert_eq!(line.highlights, vec![(1, 2)]);
    }

    #[test]
    fn test_extract_context_empty_line_and_zero_width_highlight() {
        let empty = first_line("", ascii_span(1, 1));
        assert_eq!(empty.content, "");
        assert_eq!(empty.highlights, vec![(1, 1)]);

        let zero = first_line(&"a".repeat(1000), ascii_span(500, 500));
        assert_eq!(zero.highlights, vec![(500, 500)]);
    }

    #[test]
    fn test_extract_context_multibyte_does_not_panic() {
        let source = format!("{}\u{1F600}{}", "я".repeat(300), "ж".repeat(300));
        let ctx = SourceContext::new(&source);
        let byte = source.find('\u{1F600}').unwrap();
        let span = Span::new(Location::new(1, 301, byte), Location::new(1, 302, byte + 4));
        let line = &ctx.extract_context(span, 0).lines[0];

        assert!(line.content.contains('\u{1F600}'));
        assert_eq!(line.content.chars().count(), MAX_CONTEXT_COLUMNS);
        assert_eq!(line.highlights, vec![(301, 302)]);

        let inconsistent = Span::new(Location::new(1, 301, byte + 1), Location::new(1, 302, byte));
        let _ = ctx.extract_context(inconsistent, 0);
    }

    #[test]
    fn test_extract_context_multi_line() {
        let source = "line 1\nline 2\nline 3\nline 4";
        let ctx = SourceContext::new(source);

        let span = Span::new(Location::new(2, 3, 9), Location::new(3, 4, 17));
        let diagnostic_ctx = ctx.extract_context(span, 0);

        assert_eq!(diagnostic_ctx.lines.len(), 2);
        assert_eq!(diagnostic_ctx.lines[0].highlights, vec![(3, 7)]);
        assert_eq!(diagnostic_ctx.lines[1].highlights, vec![(1, 4)]);
    }

    #[test]
    fn test_extract_context_at_boundaries() {
        let source = "line 1\nline 2\nline 3";
        let ctx = SourceContext::new(source);

        let span = Span::new(Location::new(1, 1, 0), Location::new(1, 6, 5));
        let diagnostic_ctx = ctx.extract_context(span, 5);

        assert!(diagnostic_ctx.lines[0].line_number >= 1);
    }

    #[test]
    fn test_line_count() {
        assert_eq!(SourceContext::new("").line_count(), 1);
        assert_eq!(SourceContext::new("single").line_count(), 1);
        assert_eq!(SourceContext::new("line 1\nline 2").line_count(), 2);
        assert_eq!(SourceContext::new("line 1\nline 2\n").line_count(), 3);
    }

    #[test]
    fn test_get_line_offset() {
        let source = "line 1\nline 2\nline 3";
        let ctx = SourceContext::new(source);

        assert_eq!(ctx.get_line_offset(1), 0);
        assert_eq!(ctx.get_line_offset(2), 7);
        assert_eq!(ctx.get_line_offset(3), 14);
        assert_eq!(ctx.get_line_offset(0), 0);
        assert_eq!(ctx.get_line_offset(100), 0);
    }

    fn scalar_spans(source: &str) -> Vec<SaphyrSpan> {
        use saphyr_parser::{Event, Parser};
        let mut parser = Parser::new_from_str(source);
        let mut spans = Vec::new();
        while let Some(Ok((event, span))) = parser.next_event() {
            if matches!(event, Event::Scalar(..)) {
                spans.push(span);
            }
        }
        spans
    }

    #[test]
    fn test_span_of_scalars_with_multibyte_chars() {
        let source = "—: \"é\"\n🎉: x\n";
        let ctx = SourceContext::new(source);
        let spans: Vec<Span> = scalar_spans(source)
            .into_iter()
            .map(|s| ctx.span_of(s))
            .collect();
        let snippets: Vec<&str> = spans.iter().map(|&s| ctx.get_snippet(s)).collect();
        assert_eq!(snippets, ["—", "\"é\"", "🎉", "x"]);
        assert_eq!((spans[1].start.column, spans[1].start.offset), (4, 5));
        assert_eq!((spans[1].end.column, spans[1].end.offset), (7, 9));
        assert_eq!((spans[2].start.line, spans[2].start.column), (2, 1));
    }

    #[test]
    fn test_byte_offset_of_crlf_second_line() {
        let source = "a: 1\r\nb: é\r\n";
        let ctx = SourceContext::new(source);
        let starts: Vec<usize> = scalar_spans(source)
            .into_iter()
            .map(|s| ctx.byte_offset_of(s.start).get())
            .collect();
        assert_eq!(starts, [0, 3, 6, 9]);
    }

    #[test]
    fn test_byte_offset_of_past_end_clamps() {
        let ctx = SourceContext::new("é");
        let span = scalar_spans("é")[0];
        assert_eq!(ctx.byte_offset_of(span.end).get(), "é".len());
        let missing_line = Marker::new(99, 99, 99);
        assert_eq!(ctx.byte_offset_of(missing_line).get(), "é".len());
    }
}

/// Pre-computed metadata about a line for efficient access.
///
/// Used by [`LintContext`] to provide cached line analysis results
/// that multiple linting rules may need.
#[derive(Debug, Clone)]
pub struct LineMetadata {
    /// Number of leading spaces
    pub indent: usize,
    /// true if line is empty or only whitespace
    pub is_empty: bool,
    /// true if line starts with '#' (after trimming)
    pub is_comment: bool,
}

/// Shared caching layer for linting operations.
///
/// Provides efficient access to source analysis results that are expensive
/// to compute but shared across multiple linting rules. All cached data is
/// lazily initialized on first access and reused for subsequent calls.
///
/// This is the foundation for LSP integration, as it enables incremental
/// invalidation when source changes.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::LintContext;
///
/// let source = "key: value  # comment\n";
/// let context = LintContext::new(source);
///
/// // Access source and cached data
/// assert_eq!(context.source(), source);
/// assert_eq!(context.source_context().line_count(), 2);
/// assert_eq!(context.lines().len(), 1);
/// assert_eq!(context.comments().len(), 1);
/// ```
pub struct LintContext<'a> {
    source: &'a str,
    source_context: SourceContext<'a>,
    scan: OnceLock<SourceScan<'a>>,
    lines: OnceLock<Vec<&'a str>>,
    line_metadata: OnceLock<Vec<LineMetadata>>,
    key_index: OnceLock<KeyIndex<'a>>,
    flow_index: OnceLock<FlowIndex>,
    /// 1-based line number where the current document starts within `source`.
    doc_start_line: usize,
}

impl<'a> LintContext<'a> {
    /// Creates a new lint context for the given source.
    ///
    /// The context immediately builds line offset indexes but defers
    /// parsing comments and computing line metadata until first access.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::LintContext;
    ///
    /// let source = "key: value";
    /// let context = LintContext::new(source);
    /// ```
    #[must_use]
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            source_context: SourceContext::new(source),
            scan: OnceLock::new(),
            lines: OnceLock::new(),
            line_metadata: OnceLock::new(),
            key_index: OnceLock::new(),
            flow_index: OnceLock::new(),
            doc_start_line: 1,
        }
    }

    /// Returns a copy of this context with `doc_start_line` set to `line`.
    ///
    /// Used by the linter when processing individual documents within a
    /// multi-document stream so that rules which scan forward from a cursor
    /// can start at the correct document boundary.
    #[must_use]
    pub const fn with_doc_start_line(mut self, line: usize) -> Self {
        self.doc_start_line = line;
        self
    }

    /// Sets the document start line in-place.
    ///
    /// Prefer this over [`with_doc_start_line`](Self::with_doc_start_line) when you already have
    /// a fully constructed `LintContext` and want to reuse it across multiple documents in a
    /// multi-document stream. Mutating only `doc_start_line` avoids rebuilding the underlying
    /// [`SourceContext`] and recomputing cached data, eliminating `O(source_len)` work per document.
    pub const fn set_doc_start_line(&mut self, line: usize) {
        self.doc_start_line = line;
    }

    /// Returns the 1-based line number where the current document begins.
    ///
    /// For a single-document source this is always 1. For multi-document
    /// streams the linter sets this to the document's actual start line so
    /// that rules can initialize their forward-scan cursors correctly.
    #[must_use]
    pub const fn doc_start_line(&self) -> usize {
        self.doc_start_line
    }

    /// Returns the original source text.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::LintContext;
    ///
    /// let source = "key: value";
    /// let context = LintContext::new(source);
    /// assert_eq!(context.source(), source);
    /// ```
    #[must_use]
    pub const fn source(&self) -> &'a str {
        self.source
    }

    /// Returns the source context for line-based operations.
    ///
    /// Provides efficient access to line offsets and location mapping.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::LintContext;
    ///
    /// let source = "line 1\nline 2";
    /// let context = LintContext::new(source);
    /// assert_eq!(context.source_context().line_count(), 2);
    /// assert_eq!(context.source_context().get_line(1), Some("line 1"));
    /// ```
    #[must_use]
    pub const fn source_context(&self) -> &SourceContext<'a> {
        &self.source_context
    }

    /// Returns a copy of this context whose comments and document markers are `scan`.
    ///
    /// `Linter::lint` fills them from the same parser pass that loads the documents.
    #[must_use]
    pub(crate) fn with_scan(self, scan: SourceScan<'a>) -> Self {
        // A fresh context has an empty cell
        let _ = self.scan.set(scan);
        self
    }

    fn scan(&self) -> &SourceScan<'a> {
        self.scan
            .get_or_init(|| SourceScan::of_source(self.source, &self.source_context))
    }

    /// Returns the keys that repeat an earlier key of their mapping.
    pub(crate) fn key_repeats(&self) -> &[KeyRepeat] {
        &self.scan().key_repeats
    }

    /// Returns the explicit markers and first line of every parsed document.
    pub(crate) fn documents(&self) -> &[DocumentMarkers] {
        &self.scan().documents
    }

    /// Like [`documents`](Self::documents), but a source with no parsed document (empty, only
    /// comments, or not valid YAML) counts as one implicit document.
    pub(crate) fn document_markers(&self) -> &[DocumentMarkers] {
        match self.documents() {
            [] => std::slice::from_ref(&IMPLICIT_DOCUMENT),
            documents => documents,
        }
    }

    /// Returns all comments found in the source, in source order.
    ///
    /// Comments come from the parser events, so `#` inside quoted and block scalars is never a
    /// comment. They are located on first access with one parser pass unless the linter already
    /// did it while loading the documents. A source that does not parse, or that is not its own
    /// normalized form (it starts a document with a BOM), has none.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::LintContext;
    ///
    /// let source = "key: value  # comment";
    /// let context = LintContext::new(source);
    /// let comments = context.comments();
    /// assert_eq!(comments.len(), 1);
    /// assert_eq!(comments[0].text, " comment");
    /// ```
    #[must_use]
    pub fn comments(&self) -> &[Comment<'a>] {
        &self.scan().comments
    }

    /// Returns all lines in the source as a slice of string slices.
    ///
    /// Lines are split and cached on first access. Subsequent calls
    /// return the same cached reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::LintContext;
    ///
    /// let source = "line 1\nline 2\nline 3";
    /// let context = LintContext::new(source);
    /// let lines = context.lines();
    /// assert_eq!(lines.len(), 3);
    /// assert_eq!(lines[0], "line 1");
    /// assert_eq!(lines[1], "line 2");
    /// ```
    #[must_use]
    pub fn lines(&self) -> &[&'a str] {
        self.lines.get_or_init(|| lines_of(self.source).collect())
    }

    /// Returns pre-computed metadata for each line.
    ///
    /// Line metadata (indent level, empty status, comment status) is
    /// computed and cached on first access. Subsequent calls return
    /// the same cached reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::LintContext;
    ///
    /// let source = "  indented\n\n# comment";
    /// let context = LintContext::new(source);
    /// let metadata = context.line_metadata();
    ///
    /// assert_eq!(metadata.len(), 3);
    /// assert_eq!(metadata[0].indent, 2);
    /// assert!(!metadata[0].is_empty);
    /// assert!(!metadata[0].is_comment);
    ///
    /// assert!(metadata[1].is_empty);
    ///
    /// assert!(metadata[2].is_comment);
    /// ```
    #[must_use]
    pub fn line_metadata(&self) -> &[LineMetadata] {
        self.line_metadata.get_or_init(|| {
            let mut comment_lines = vec![false; self.lines().len()];
            for comment in self.comments().iter().filter(|c| c.is_full_line()) {
                if let Some(flag) = comment_lines.get_mut(comment.span.start.line.wrapping_sub(1)) {
                    *flag = true;
                }
            }
            self.lines()
                .iter()
                .zip(comment_lines)
                .map(|(line, is_comment)| LineMetadata {
                    indent: line.chars().take_while(|&c| c == ' ').count(),
                    is_empty: line.trim_start().is_empty(),
                    is_comment,
                })
                .collect()
        })
    }

    /// Returns a flow tokenizer over this source.
    ///
    /// The [`FlowIndex`] behind it is built on first use and shared by every flow rule and
    /// every document of the run.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{tokenizer::TokenType, LintContext};
    ///
    /// let context = LintContext::new("object: {key: value}");
    /// let braces = context.flow_tokenizer().find_all(TokenType::BraceOpen);
    /// assert_eq!(braces.len(), 1);
    /// ```
    #[must_use]
    pub fn flow_tokenizer(&self) -> FlowTokenizer<'_> {
        let index = self
            .flow_index
            .get_or_init(|| FlowIndex::new(self.source, &self.source_context));
        FlowTokenizer::new(index, &self.source_context)
    }

    /// Returns the lazily built first-key-per-line index, shared by all documents of a run.
    pub(crate) fn key_index(&self) -> &KeyIndex<'a> {
        self.key_index.get_or_init(|| KeyIndex::build(self))
    }
}

/// Extracts the unquoted key preceding the first colon of a source line.
///
/// Quotes are stripped only when the key is a complete quoted scalar.
pub fn line_key(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let raw_key = trimmed.split_once(':')?.0.trim();
    let unquoted = ['"', '\'']
        .into_iter()
        .find_map(|q| raw_key.strip_prefix(q)?.strip_suffix(q))
        .unwrap_or(raw_key);
    Some(unquoted)
}

/// Per-lint-run index from key text to the ascending 1-based lines whose first-colon key equals it.
pub struct KeyIndex<'a>(HashMap<&'a str, Vec<usize>>);

impl<'a> KeyIndex<'a> {
    fn build(context: &LintContext<'a>) -> Self {
        let mut map: HashMap<&str, Vec<usize>> = HashMap::new();
        for (line_idx, (line, metadata)) in context
            .lines()
            .iter()
            .zip(context.line_metadata())
            .enumerate()
        {
            if metadata.is_empty || metadata.is_comment {
                continue;
            }
            if let Some(key) = line_key(line) {
                map.entry(key).or_default().push(line_idx + 1);
            }
        }
        Self(map)
    }

    /// Locates `key` on the first indexed line at or after `*cursor` and advances `*cursor` past it.
    pub(crate) fn locate(&self, key: &str, cursor: &mut usize) -> Option<usize> {
        let lines = self.0.get(key)?;
        let line_num = *lines.get(lines.partition_point(|&l| l < *cursor))?;
        *cursor = line_num + 1;
        Some(line_num)
    }
}

#[cfg(test)]
mod lint_context_tests {
    use super::*;

    #[test]
    fn test_lint_context_creation() {
        let source = "key: value\n# comment";
        let ctx = LintContext::new(source);

        assert_eq!(ctx.source(), source);
        assert_eq!(ctx.source_context().line_count(), 2);
    }

    #[test]
    fn test_comments_cached() {
        let source = "key: value  # comment";
        let ctx = LintContext::new(source);

        // First access computes
        let comments1 = ctx.comments();
        assert_eq!(comments1.len(), 1);

        // Second access should return same reference
        let comments2 = ctx.comments();
        assert_eq!(comments1.as_ptr(), comments2.as_ptr());
    }

    #[test]
    fn test_lines_cached() {
        let source = "line 1\nline 2\nline 3";
        let ctx = LintContext::new(source);

        let lines1 = ctx.lines();
        assert_eq!(lines1.len(), 3);

        let lines2 = ctx.lines();
        assert_eq!(lines1.as_ptr(), lines2.as_ptr());
    }

    #[test]
    fn test_line_metadata_computed() {
        let source = "  indented\n\n# comment";
        let ctx = LintContext::new(source);

        let metadata = ctx.line_metadata();
        assert_eq!(metadata.len(), 3);

        assert_eq!(metadata[0].indent, 2);
        assert!(!metadata[0].is_empty);
        assert!(!metadata[0].is_comment);

        assert!(metadata[1].is_empty);

        assert!(metadata[2].is_comment);
    }

    #[test]
    fn test_line_metadata_cached() {
        let source = "key: value";
        let ctx = LintContext::new(source);

        let meta1 = ctx.line_metadata();
        let meta2 = ctx.line_metadata();
        assert_eq!(meta1.as_ptr(), meta2.as_ptr());
    }

    #[test]
    fn test_multiple_comments() {
        let source = "# Comment 1\nkey: value  # Comment 2\n# Comment 3";
        let ctx = LintContext::new(source);

        let comments = ctx.comments();
        assert_eq!(comments.len(), 3);
        assert_eq!(comments[0].text, " Comment 1");
        assert_eq!(comments[1].text, " Comment 2");
        assert_eq!(comments[2].text, " Comment 3");
    }

    #[test]
    fn test_lines_no_trailing_newline() {
        let source = "line 1\nline 2";
        let ctx = LintContext::new(source);

        let lines = ctx.lines();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "line 1");
        assert_eq!(lines[1], "line 2");
    }

    #[test]
    fn test_empty_source() {
        let source = "";
        let ctx = LintContext::new(source);

        assert_eq!(ctx.source(), "");
        assert_eq!(ctx.lines().len(), 0);
        assert_eq!(ctx.comments().len(), 0);
        assert_eq!(ctx.line_metadata().len(), 0);
    }

    #[test]
    fn test_only_whitespace() {
        let source = "  \n\t\n  ";
        let ctx = LintContext::new(source);

        let metadata = ctx.line_metadata();
        assert_eq!(metadata.len(), 3);
        assert!(metadata[0].is_empty);
        assert!(metadata[1].is_empty);
        assert!(metadata[2].is_empty);
    }

    #[test]
    fn test_flow_index_is_built_once() {
        let ctx = LintContext::new("a: {b: 1}\nc: [1, 2]");
        let first = ctx.flow_tokenizer();
        let second = ctx.flow_tokenizer();
        assert!(std::ptr::eq(first.index, second.index));
    }

    #[test]
    fn test_line_metadata_ignores_hash_lines_inside_scalars() {
        let ctx = LintContext::new("a: \"x\n  #y\"\nb: |\n  #z\n# c\n");
        let flags: Vec<_> = ctx.line_metadata().iter().map(|m| m.is_comment).collect();
        assert_eq!(flags, [false, false, false, false, true]);
    }
}
