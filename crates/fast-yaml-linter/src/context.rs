//! Source context extraction for diagnostic display.

use crate::{
    Location, Span,
    comment_parser::{Comment, CommentParser},
    diagnostic::{ContextLine, DiagnosticContext},
};
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
        if line_num == 0 || line_num > self.line_starts.len() {
            return 0;
        }
        self.line_starts[line_num - 1]
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
/// assert_eq!(context.lines().len(), 2);
/// assert_eq!(context.comments().len(), 1);
/// ```
pub struct LintContext<'a> {
    source: &'a str,
    source_context: SourceContext<'a>,
    comments: OnceLock<Vec<Comment>>,
    lines: OnceLock<Vec<&'a str>>,
    line_metadata: OnceLock<Vec<LineMetadata>>,
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
            comments: OnceLock::new(),
            lines: OnceLock::new(),
            line_metadata: OnceLock::new(),
        }
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

    /// Returns all comments found in the source.
    ///
    /// Comments are parsed and cached on first access. Subsequent calls
    /// return the same cached reference with no additional parsing.
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
    /// assert_eq!(comments[0].content, " comment");
    /// ```
    #[must_use]
    pub fn comments(&self) -> &[Comment] {
        self.comments.get_or_init(|| {
            let parser = CommentParser::new(self.source, &self.source_context);
            parser.find_all().to_vec()
        })
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
        self.lines.get_or_init(|| self.source.lines().collect())
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
            self.lines()
                .iter()
                .map(|line| {
                    let trimmed = line.trim_start();
                    LineMetadata {
                        indent: line.chars().take_while(|&c| c == ' ').count(),
                        is_empty: trimmed.is_empty(),
                        is_comment: trimmed.starts_with('#'),
                    }
                })
                .collect()
        })
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
        assert_eq!(comments[0].content, " Comment 1");
        assert_eq!(comments[1].content, " Comment 2");
        assert_eq!(comments[2].content, " Comment 3");
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
}
