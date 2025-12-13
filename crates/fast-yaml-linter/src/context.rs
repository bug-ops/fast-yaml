//! Source context extraction for diagnostic display.

use crate::{
    Location, Span,
    diagnostic::{ContextLine, DiagnosticContext},
};

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
        let mut line_starts = vec![0];

        for (idx, ch) in source.char_indices() {
            if ch == '\n' {
                line_starts.push(idx + 1);
            }
        }

        Self {
            source,
            line_starts,
        }
    }

    /// Gets a specific line by number (1-indexed).
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
        if line_number == 0 || line_number > self.line_starts.len() {
            return None;
        }

        let start = self.line_starts[line_number - 1];
        let end = if line_number < self.line_starts.len() {
            self.line_starts[line_number] - 1
        } else {
            self.source.len()
        };

        Some(&self.source[start..end])
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
                        content.len() + 1
                    };

                    if start_col <= end_col {
                        highlights.push((start_col, end_col));
                    }
                }

                lines.push(ContextLine {
                    line_number: line_num,
                    content: content.to_string(),
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
        let start = span.start.offset.min(self.source.len());
        let end = span.end.offset.min(self.source.len());
        &self.source[start..end]
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
        let offset = offset.min(self.source.len());

        let line_idx = match self.line_starts.binary_search(&offset) {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        };

        let line = line_idx + 1;
        let line_start = self.line_starts[line_idx];

        let column = self.source[line_start..offset].chars().count() + 1;

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
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
