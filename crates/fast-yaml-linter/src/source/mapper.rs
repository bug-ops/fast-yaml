//! Source code position mapper for finding tokens and keys.

use crate::{Location, SourceContext, Span};
use std::collections::HashMap;

/// Maps YAML elements to their positions in source code.
///
/// Provides utilities to locate keys, values, and special characters in the source text.
/// Builds a full inverted index on first access for O(n) total lookup complexity.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::source::SourceMapper;
///
/// let yaml = "name: John\nage: 30";
/// let mut mapper = SourceMapper::new(yaml);
///
/// let key_span = mapper.find_key_span("name", 1);
/// assert!(key_span.is_some());
/// ```
pub struct SourceMapper<'a> {
    context: SourceContext<'a>,
    key_positions: HashMap<String, Vec<Span>>,
    index_built: bool,
}

impl<'a> SourceMapper<'a> {
    /// Creates a new source mapper for the given YAML source.
    pub fn new(source: &'a str) -> Self {
        Self {
            context: SourceContext::new(source),
            key_positions: HashMap::new(),
            index_built: false,
        }
    }

    /// Builds the full inverted key index by scanning all lines once.
    ///
    /// After this call, all key lookups are O(1). This is called lazily on
    /// the first call to [`find_key_span`] or [`find_all_key_spans`].
    fn build_index(&mut self) {
        if self.index_built {
            return;
        }
        self.index_built = true;

        for line_num in 1..=self.context.line_count() {
            let Some(line_content) = self.context.get_line(line_num) else {
                continue;
            };
            let Some((key, col)) = Self::extract_key_from_line(line_content) else {
                continue;
            };

            let line_start_offset = self.context.get_line_offset(line_num);
            let start = Location::new(line_num, col + 1, line_start_offset + col);
            let end = Location::new(
                line_num,
                col + key.len() + 1,
                line_start_offset + col + key.len(),
            );
            self.key_positions
                .entry(key.to_string())
                .or_default()
                .push(Span::new(start, end));
        }
    }

    /// Extracts a YAML mapping key from a line, if one is present.
    ///
    /// Handles block mapping syntax: `<whitespace><key>: <value>` or `<key>:`.
    /// Returns the key string and its byte column offset within the line.
    fn extract_key_from_line(line: &str) -> Option<(&str, usize)> {
        let trimmed_start = line.len() - line.trim_start().len();
        let content = &line[trimmed_start..];

        if content.is_empty() || content.starts_with('#') {
            return None;
        }

        let mut in_single = false;
        let mut in_double = false;

        for (i, ch) in content.char_indices() {
            match ch {
                '\'' if !in_double => in_single = !in_single,
                '"' if !in_single => in_double = !in_double,
                ':' if !in_single && !in_double => {
                    let key = &content[..i];
                    if key.is_empty() || key.contains(|c: char| c.is_whitespace()) {
                        return None;
                    }
                    let after = &content[i + 1..];
                    if after.is_empty() || after.starts_with(' ') || after.starts_with('\t') {
                        return Some((key, trimmed_start));
                    }
                    return None;
                }
                _ => {}
            }
        }

        None
    }

    /// Finds the span of a key in the source code.
    ///
    /// Uses a line hint to disambiguate when the same key appears multiple times.
    /// Builds the full source index on first call for O(n) total complexity.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::source::SourceMapper;
    ///
    /// let yaml = "name: John\nname: Jane";
    /// let mut mapper = SourceMapper::new(yaml);
    ///
    /// let first = mapper.find_key_span("name", 1);
    /// let second = mapper.find_key_span("name", 2);
    ///
    /// assert_ne!(first, second);
    /// ```
    pub fn find_key_span(&mut self, key: &str, line_hint: usize) -> Option<Span> {
        self.build_index();
        self.key_positions
            .get(key)?
            .iter()
            .find(|s| s.start.line == line_hint)
            .copied()
    }

    /// Finds all occurrences of a key in the source code.
    ///
    /// Returns a vector of all spans where the key appears.
    /// Builds the full source index on first call for O(n) total complexity.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::source::SourceMapper;
    ///
    /// let yaml = "name: John\nage: 30\nname: Jane";
    /// let mut mapper = SourceMapper::new(yaml);
    ///
    /// let spans = mapper.find_all_key_spans("name");
    /// assert_eq!(spans.len(), 2);
    /// ```
    pub fn find_all_key_spans(&mut self, key: &str) -> Vec<Span> {
        self.build_index();
        self.key_positions.get(key).cloned().unwrap_or_default()
    }

    /// Finds a key in a line, accounting for YAML syntax.
    fn find_key_in_line(line: &str, key: &str) -> Option<usize> {
        // Skip leading whitespace
        let trimmed_start = line.len() - line.trim_start().len();
        let content = &line[trimmed_start..];

        // Check if line starts with the key followed by ':'
        if let Some(after_key) = content.strip_prefix(key)
            && (after_key.starts_with(':') || after_key.starts_with(' '))
        {
            return Some(trimmed_start);
        }

        // Look for the key elsewhere in the line (for flow mappings)
        // We need to find a complete word match, not a substring
        let mut search_pos = 0;
        while let Some(pos) = content[search_pos..].find(key) {
            let absolute_pos = search_pos + pos;

            // Check if it's a word boundary before the key
            let is_start_boundary = absolute_pos == 0 || {
                let char_before = content.chars().nth(absolute_pos - 1).unwrap();
                !char_before.is_alphanumeric() && char_before != '_'
            };

            if is_start_boundary {
                // Make sure it's followed by ':' or space
                let after = &content[absolute_pos + key.len()..];
                if after.trim_start().starts_with(':') {
                    return Some(trimmed_start + absolute_pos);
                }
            }

            search_pos = absolute_pos + 1;
        }

        None
    }

    /// Finds the colon position after a key.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{source::SourceMapper, Location, Span};
    ///
    /// let yaml = "name: John";
    /// let mapper = SourceMapper::new(yaml);
    ///
    /// let key_span = Span::new(
    ///     Location::new(1, 1, 0),
    ///     Location::new(1, 5, 4),
    /// );
    ///
    /// let colon = mapper.find_colon_after_key(key_span);
    /// assert!(colon.is_some());
    /// ```
    pub fn find_colon_after_key(&self, key_span: Span) -> Option<Location> {
        let line = self.context.get_line(key_span.end.line)?;
        let key_end_col = key_span.end.column.saturating_sub(1);

        if key_end_col >= line.len() {
            return None;
        }

        // Search for ':' after key
        let rest = &line[key_end_col..];
        rest.find(':').map(|offset| {
            Location::new(
                key_span.end.line,
                key_end_col + offset + 1,
                key_span.end.offset + offset,
            )
        })
    }

    /// Finds all occurrences of a specific character in the source.
    ///
    /// Useful for tokenization and finding delimiters.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::source::SourceMapper;
    ///
    /// let yaml = "name: John\nage: 30";
    /// let mapper = SourceMapper::new(yaml);
    ///
    /// let colons = mapper.find_all_chars(':');
    /// assert_eq!(colons.len(), 2);
    /// ```
    pub fn find_all_chars(&self, ch: char) -> Vec<Location> {
        let mut locations = Vec::new();

        for line_num in 1..=self.context.line_count() {
            if let Some(line) = self.context.get_line(line_num) {
                for (col, c) in line.chars().enumerate() {
                    if c == ch && !Self::is_inside_string_at(line, col) {
                        let offset = self.context.get_line_offset(line_num) + col;
                        locations.push(Location::new(line_num, col + 1, offset));
                    }
                }
            }
        }

        locations
    }

    /// Checks if a position is inside a quoted string.
    fn is_inside_string_at(line: &str, col: usize) -> bool {
        let mut in_single = false;
        let mut in_double = false;
        let mut escape = false;

        for (i, ch) in line.chars().enumerate() {
            if i >= col {
                break;
            }

            if escape {
                escape = false;
                continue;
            }

            match ch {
                '\\' if in_single || in_double => escape = true,
                '\'' if !in_double => in_single = !in_single,
                '"' if !in_single => in_double = !in_double,
                _ => {}
            }
        }

        in_single || in_double
    }

    /// Gets the byte offset where a line starts (1-indexed).
    ///
    /// Delegates to the pre-computed index in [`SourceContext`] for O(1) access.
    fn get_line_start_offset(&self, line_num: usize) -> usize {
        self.context.get_line_offset(line_num)
    }

    /// Gets the source context.
    pub const fn context(&self) -> &SourceContext<'a> {
        &self.context
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_mapper() {
        let source = "name: John";
        let mapper = SourceMapper::new(source);
        assert_eq!(mapper.context().line_count(), 1);
    }

    #[test]
    fn test_find_key_span() {
        let source = "name: John\nage: 30";
        let mut mapper = SourceMapper::new(source);

        let span = mapper.find_key_span("name", 1).unwrap();
        assert_eq!(span.start.line, 1);
        assert_eq!(span.start.column, 1);
    }

    #[test]
    fn test_find_key_span_second_line() {
        let source = "name: John\nage: 30";
        let mut mapper = SourceMapper::new(source);

        let span = mapper.find_key_span("age", 2).unwrap();
        assert_eq!(span.start.line, 2);
        assert_eq!(span.start.column, 1);
    }

    #[test]
    fn test_find_key_span_with_indent() {
        let source = "user:\n  name: John";
        let mut mapper = SourceMapper::new(source);

        let span = mapper.find_key_span("name", 2).unwrap();
        assert_eq!(span.start.line, 2);
        assert_eq!(span.start.column, 3); // After 2 spaces
    }

    #[test]
    fn test_find_colon_after_key() {
        let source = "name: John";
        let mapper = SourceMapper::new(source);

        let key_span = Span::new(Location::new(1, 1, 0), Location::new(1, 5, 4));
        let colon = mapper.find_colon_after_key(key_span).unwrap();
        assert_eq!(colon.column, 5);
    }

    #[test]
    fn test_find_all_chars() {
        let source = "name: John\nage: 30";
        let mapper = SourceMapper::new(source);

        let colons = mapper.find_all_chars(':');
        assert_eq!(colons.len(), 2);
        assert_eq!(colons[0].line, 1);
        assert_eq!(colons[1].line, 2);
    }

    #[test]
    fn test_find_all_chars_ignores_strings() {
        let source = r#"url: "http://example.com""#;
        let mapper = SourceMapper::new(source);

        let colons = mapper.find_all_chars(':');
        // Should only find the mapping colon, not the one in the URL
        assert_eq!(colons.len(), 1);
    }

    #[test]
    fn test_is_inside_string_at() {
        let line = r#"text: "hello: world""#;

        assert!(!SourceMapper::is_inside_string_at(line, 5)); // At first colon
        assert!(SourceMapper::is_inside_string_at(line, 13)); // At second colon (inside string)
    }

    #[test]
    fn test_get_line_start_offset() {
        let source = "line1\nline2\nline3";
        let mapper = SourceMapper::new(source);

        assert_eq!(mapper.get_line_start_offset(1), 0);
        assert_eq!(mapper.get_line_start_offset(2), 6); // "line1\n" = 6 bytes
        assert_eq!(mapper.get_line_start_offset(3), 12); // "line1\nline2\n" = 12 bytes
    }

    #[test]
    fn test_find_key_in_line() {
        assert_eq!(
            SourceMapper::find_key_in_line("name: John", "name"),
            Some(0)
        );
        assert_eq!(
            SourceMapper::find_key_in_line("  name: John", "name"),
            Some(2)
        );
        assert_eq!(
            SourceMapper::find_key_in_line("other: name: John", "name"),
            Some(7)
        );
        assert_eq!(
            SourceMapper::find_key_in_line("username: John", "name"),
            None
        );
    }

    #[test]
    fn test_find_duplicate_keys() {
        let source = "name: John\nage: 30\nname: Jane";
        let mut mapper = SourceMapper::new(source);

        let first = mapper.find_key_span("name", 1).unwrap();
        let second = mapper.find_key_span("name", 3).unwrap();

        assert_eq!(first.start.line, 1);
        assert_eq!(second.start.line, 3);
    }

    #[test]
    fn test_index_built_once() {
        let source = "name: John\nage: 30\nname: Jane";
        let mut mapper = SourceMapper::new(source);

        assert!(!mapper.index_built);
        let _ = mapper.find_key_span("name", 1);
        assert!(mapper.index_built);
        // Second call should use cache
        let _ = mapper.find_key_span("age", 2);
        assert!(mapper.index_built);
    }

    #[test]
    fn test_find_all_key_spans() {
        let source = "name: John\nage: 30\nname: Jane";
        let mut mapper = SourceMapper::new(source);

        let spans = mapper.find_all_key_spans("name");
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].start.line, 1);
        assert_eq!(spans[1].start.line, 3);
    }

    #[test]
    fn test_large_file_performance() {
        // Verify index is built once even with many unique keys
        let mut lines = Vec::new();
        for i in 0..1000 {
            lines.push(format!("key_{i}: value_{i}"));
        }
        let source = lines.join("\n");
        let mut mapper = SourceMapper::new(&source);

        // Looking up many keys should not rebuild the index
        let _ = mapper.find_key_span("key_0", 1);
        assert!(mapper.index_built);
        let _ = mapper.find_key_span("key_999", 1000);
        let _ = mapper.find_all_key_spans("key_500");
    }
}
