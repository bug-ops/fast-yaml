//! Comment detection and parsing utilities.

use crate::{SourceContext, Span};
use std::sync::OnceLock;

/// A comment in YAML source.
///
/// Represents a comment with its content, location, and metadata.
#[derive(Debug, Clone)]
pub struct Comment {
    /// Comment text without '#'
    pub content: String,
    /// Location in source
    pub span: Span,
    /// true if on same line as content
    pub is_inline: bool,
    /// true if #!/...
    pub is_shebang: bool,
}

/// Parses comments from YAML source.
///
/// Identifies comments while distinguishing them from string content.
/// Comments are cached on first parse to avoid repeated processing when
/// `find_on_line()` or `is_comment()` are called multiple times.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{comment_parser::CommentParser, SourceContext};
///
/// let yaml = "# This is a comment\nkey: value";
/// let context = SourceContext::new(yaml);
/// let parser = CommentParser::new(yaml, &context);
///
/// let comments = parser.find_all();
/// assert_eq!(comments.len(), 1);
/// assert_eq!(comments[0].content, " This is a comment");
/// ```
pub struct CommentParser<'a> {
    source: &'a str,
    context: &'a SourceContext<'a>,
    cache: OnceLock<Vec<Comment>>,
}

impl<'a> CommentParser<'a> {
    /// Creates a new comment parser.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{comment_parser::CommentParser, SourceContext};
    ///
    /// let yaml = "key: value";
    /// let context = SourceContext::new(yaml);
    /// let parser = CommentParser::new(yaml, &context);
    /// ```
    #[must_use]
    pub const fn new(source: &'a str, context: &'a SourceContext<'a>) -> Self {
        Self {
            source,
            context,
            cache: OnceLock::new(),
        }
    }

    /// Finds all comments in source.
    ///
    /// Returns a reference to all comments found, excluding comments
    /// that appear within quoted strings. Results are cached on first call.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{comment_parser::CommentParser, SourceContext};
    ///
    /// let yaml = "# Comment 1\nkey: value  # Comment 2";
    /// let context = SourceContext::new(yaml);
    /// let parser = CommentParser::new(yaml, &context);
    ///
    /// let comments = parser.find_all();
    /// assert_eq!(comments.len(), 2);
    /// ```
    #[must_use]
    pub fn find_all(&self) -> &[Comment] {
        self.cache.get_or_init(|| self.parse_comments())
    }

    /// Internal method to parse all comments from source.
    ///
    /// This is called once and cached by `find_all()`.
    fn parse_comments(&self) -> Vec<Comment> {
        let mut comments = Vec::new();
        let mut in_string = false;
        let mut string_delimiter = b'"';
        let mut escape_next = false;

        for (line_idx, line) in self.source.lines().enumerate() {
            let line_start_offset = self.context.get_line_offset(line_idx + 1);

            for (col_idx, ch) in line.char_indices() {
                let offset = line_start_offset + col_idx;

                if escape_next {
                    escape_next = false;
                    continue;
                }

                // Only handle escapes for double-quoted strings
                // Single-quoted strings in YAML don't support backslash escapes
                if ch == '\\' && in_string && string_delimiter == b'"' {
                    escape_next = true;
                    continue;
                }

                // Track string boundaries
                if (ch == '"' || ch == '\'') && !in_string {
                    in_string = true;
                    string_delimiter = ch as u8;
                    continue;
                }

                if in_string && ch as u8 == string_delimiter {
                    in_string = false;
                    continue;
                }

                // Found comment outside string
                if ch == '#' && !in_string {
                    let comment_start = offset;
                    let comment_content = &line[col_idx + 1..];

                    // Check if it's a shebang
                    let is_shebang =
                        line_idx == 0 && col_idx == 0 && comment_content.starts_with('!');

                    // Check if it's inline (has content before it)
                    let is_inline = col_idx > 0 && !line[..col_idx].trim().is_empty();

                    let location_start = self.context.offset_to_location(comment_start);
                    let location_end = self
                        .context
                        .offset_to_location(line_start_offset + line.len());

                    comments.push(Comment {
                        content: comment_content.to_string(),
                        span: Span::new(location_start, location_end),
                        is_inline,
                        is_shebang,
                    });

                    // Rest of line is comment, skip to next line
                    break;
                }
            }

            // Reset string state at end of line for comment detection purposes.
            // While YAML supports multiline strings (block scalars, folded/literal),
            // those are processed differently by the parser. For comment detection,
            // we only need to track inline quoted strings within a single line.
            // This ensures we don't miss comments on subsequent lines.
            in_string = false;
        }

        comments
    }

    /// Finds comment on a specific line.
    ///
    /// Returns the comment on the given line number (1-indexed),
    /// or None if no comment exists on that line.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{comment_parser::CommentParser, SourceContext};
    ///
    /// let yaml = "key: value  # inline comment";
    /// let context = SourceContext::new(yaml);
    /// let parser = CommentParser::new(yaml, &context);
    ///
    /// let comment = parser.find_on_line(1);
    /// assert!(comment.is_some());
    /// assert_eq!(comment.unwrap().content, " inline comment");
    /// ```
    #[must_use]
    pub fn find_on_line(&self, line: usize) -> Option<&Comment> {
        self.find_all().iter().find(|c| c.span.start.line == line)
    }

    /// Checks if position is inside a comment.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{comment_parser::CommentParser, SourceContext};
    ///
    /// let yaml = "key: value  # comment";
    /// let context = SourceContext::new(yaml);
    /// let parser = CommentParser::new(yaml, &context);
    ///
    /// // Position 14 is inside the comment
    /// assert!(parser.is_comment(14));
    /// // Position 5 is not
    /// assert!(!parser.is_comment(5));
    /// ```
    #[must_use]
    pub fn is_comment(&self, offset: usize) -> bool {
        self.find_all()
            .iter()
            .any(|c| c.span.start.offset <= offset && offset <= c.span.end.offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_comment_parser_standalone() {
        let yaml = "# This is a comment\nkey: value";
        let context = SourceContext::new(yaml);
        let parser = CommentParser::new(yaml, &context);

        let comments = parser.find_all();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].content, " This is a comment");
        assert!(!comments[0].is_inline);
        assert!(!comments[0].is_shebang);
    }

    #[test]
    fn test_comment_parser_inline() {
        let yaml = "key: value  # inline comment";
        let context = SourceContext::new(yaml);
        let parser = CommentParser::new(yaml, &context);

        let comments = parser.find_all();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].content, " inline comment");
        assert!(comments[0].is_inline);
        assert!(!comments[0].is_shebang);
    }

    #[test]
    fn test_comment_parser_shebang() {
        let yaml = "#!/usr/bin/env yaml\nkey: value";
        let context = SourceContext::new(yaml);
        let parser = CommentParser::new(yaml, &context);

        let comments = parser.find_all();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].content, "!/usr/bin/env yaml");
        assert!(comments[0].is_shebang);
        assert!(!comments[0].is_inline);
    }

    #[test]
    fn test_comment_parser_in_string() {
        let yaml = r#"text: "not # a comment""#;
        let context = SourceContext::new(yaml);
        let parser = CommentParser::new(yaml, &context);

        let comments = parser.find_all();
        assert!(comments.is_empty());
    }

    #[test]
    fn test_comment_parser_single_quote_string() {
        let yaml = "text: 'not # a comment'";
        let context = SourceContext::new(yaml);
        let parser = CommentParser::new(yaml, &context);

        let comments = parser.find_all();
        assert!(comments.is_empty());
    }

    #[test]
    fn test_comment_parser_escaped_quote() {
        let yaml = r#"text: "escaped \" quote"  # real comment"#;
        let context = SourceContext::new(yaml);
        let parser = CommentParser::new(yaml, &context);

        let comments = parser.find_all();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].content, " real comment");
    }

    #[test]
    fn test_comment_parser_multiple() {
        let yaml = "# Comment 1\nkey: value  # Comment 2\n# Comment 3";
        let context = SourceContext::new(yaml);
        let parser = CommentParser::new(yaml, &context);

        let comments = parser.find_all();
        assert_eq!(comments.len(), 3);
        assert_eq!(comments[0].content, " Comment 1");
        assert_eq!(comments[1].content, " Comment 2");
        assert_eq!(comments[2].content, " Comment 3");
    }

    #[test]
    fn test_find_on_line() {
        let yaml = "# Line 1\nkey: value\n# Line 3";
        let context = SourceContext::new(yaml);
        let parser = CommentParser::new(yaml, &context);

        let comment = parser.find_on_line(1);
        assert!(comment.is_some());
        assert_eq!(comment.unwrap().content, " Line 1");

        let no_comment = parser.find_on_line(2);
        assert!(no_comment.is_none());

        let comment3 = parser.find_on_line(3);
        assert!(comment3.is_some());
        assert_eq!(comment3.unwrap().content, " Line 3");
    }

    #[test]
    fn test_is_comment() {
        let yaml = "key: value  # comment";
        let context = SourceContext::new(yaml);
        let parser = CommentParser::new(yaml, &context);

        // Inside comment (after #)
        assert!(parser.is_comment(12));
        assert!(parser.is_comment(14));

        // Not in comment (before #)
        assert!(!parser.is_comment(0));
        assert!(!parser.is_comment(5));
    }

    #[test]
    fn test_empty_comment() {
        let yaml = "key: value  #";
        let context = SourceContext::new(yaml);
        let parser = CommentParser::new(yaml, &context);

        let comments = parser.find_all();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].content, "");
    }

    #[test]
    fn test_comment_no_space_after_hash() {
        let yaml = "#No space";
        let context = SourceContext::new(yaml);
        let parser = CommentParser::new(yaml, &context);

        let comments = parser.find_all();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].content, "No space");
    }

    #[test]
    fn test_block_scalar_with_hash() {
        // Block scalars can contain # without it being a comment
        let yaml = "text: |\n  This has a # in it\n  # This too";
        let context = SourceContext::new(yaml);
        let parser = CommentParser::new(yaml, &context);

        // Parser should not detect these as comments (they're in block scalar)
        // For now, our simple parser will detect them - this documents the limitation
        let comments = parser.find_all();
        // Current implementation treats these as comments (acceptable for Phase 3)
        assert_eq!(comments.len(), 2);
    }

    #[test]
    fn test_single_quote_with_hash() {
        // Single-quoted strings don't support escapes in YAML
        let yaml = r"text: 'has # hash'  # real comment";
        let context = SourceContext::new(yaml);
        let parser = CommentParser::new(yaml, &context);

        let comments = parser.find_all();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].content, " real comment");
        assert!(comments[0].is_inline);
    }

    #[test]
    fn test_single_quote_no_backslash_escape() {
        // Backslash has no special meaning in single-quoted strings
        let yaml = r"text: 'backslash \ here'  # comment";
        let context = SourceContext::new(yaml);
        let parser = CommentParser::new(yaml, &context);

        let comments = parser.find_all();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].content, " comment");
    }

    #[test]
    fn test_double_quote_escaped_single_quote() {
        // Escaped single quote in double-quoted string
        let yaml = r#"text: "has \' quote"  # comment"#;
        let context = SourceContext::new(yaml);
        let parser = CommentParser::new(yaml, &context);

        let comments = parser.find_all();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].content, " comment");
    }

    #[test]
    fn test_double_quote_escaped_hash() {
        // Escaped hash in double-quoted string (though # doesn't need escaping)
        let yaml = r#"text: "has \# hash"  # real comment"#;
        let context = SourceContext::new(yaml);
        let parser = CommentParser::new(yaml, &context);

        let comments = parser.find_all();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].content, " real comment");
    }
}
