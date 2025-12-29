//! Source code position mapper for finding tokens and keys.

use crate::{Location, SourceContext, Span};
use std::collections::HashMap;

/// Maps YAML elements to their positions in source code.
///
/// Provides utilities to locate keys, values, and special characters in the source text.
/// Caches lookups for performance.
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
}

impl<'a> SourceMapper<'a> {
    /// Creates a new source mapper for the given YAML source.
    pub fn new(source: &'a str) -> Self {
        Self {
            context: SourceContext::new(source),
            key_positions: HashMap::new(),
        }
    }

    /// Finds the span of a key in the source code.
    ///
    /// Uses a line hint to disambiguate when the same key appears multiple times.
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
        // Check cache first
        if let Some(spans) = self.key_positions.get(key) {
            return spans.iter().find(|s| s.start.line == line_hint).copied();
        }

        // Search in source
        let mut found_spans = Vec::new();

        for line_num in 1..=self.context.line_count() {
            if let Some(line_content) = self.context.get_line(line_num) {
                // Look for the key at the beginning or after whitespace
                if let Some(col) = Self::find_key_in_line(line_content, key) {
                    let line_start_offset = self.get_line_start_offset(line_num);
                    let start = Location::new(line_num, col + 1, line_start_offset + col);
                    let end = Location::new(
                        line_num,
                        col + key.len() + 1,
                        line_start_offset + col + key.len(),
                    );
                    found_spans.push(Span::new(start, end));
                }
            }
        }

        self.key_positions
            .insert(key.to_string(), found_spans.clone());

        found_spans
            .iter()
            .find(|s| s.start.line == line_hint)
            .copied()
    }

    /// Finds all occurrences of a key in the source code.
    ///
    /// Returns a vector of all spans where the key appears.
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
        // Populate cache if needed by calling find_key_span with line hint 1
        if !self.key_positions.contains_key(key) {
            let _ = self.find_key_span(key, 1);
        }

        // Return cloned vector from cache
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
                        let offset = self.get_line_start_offset(line_num) + col;
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

    /// Gets the byte offset where a line starts.
    fn get_line_start_offset(&self, line_num: usize) -> usize {
        if line_num == 1 {
            return 0;
        }

        (1..line_num)
            .filter_map(|ln| self.context.get_line(ln))
            .map(|l| l.len() + 1) // +1 for newline
            .sum()
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
}
