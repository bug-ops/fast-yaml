//! Flow collection tokenizer for identifying YAML syntax tokens.

use crate::{Location, SourceContext, Span};

/// Types of tokens in YAML flow syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenType {
    /// Opening brace `{`
    BraceOpen,
    /// Closing brace `}`
    BraceClose,
    /// Opening bracket `[`
    BracketOpen,
    /// Closing bracket `]`
    BracketClose,
    /// Colon `:`
    Colon,
    /// Comma `,`
    Comma,
    /// Hyphen `-` (list item marker)
    Hyphen,
}

/// A token with its location in source.
#[derive(Debug, Clone)]
pub struct Token {
    /// Type of token
    pub token_type: TokenType,
    /// Location span in source
    pub span: Span,
}

impl Token {
    /// Creates a new token.
    #[must_use]
    pub const fn new(token_type: TokenType, span: Span) -> Self {
        Self { token_type, span }
    }
}

/// Tokenizes flow collection syntax in YAML source.
///
/// Accurately identifies flow syntax elements while ignoring tokens
/// inside quoted strings.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{tokenizer::{FlowTokenizer, TokenType}, SourceContext};
///
/// let yaml = "object: {key: value}";
/// let context = SourceContext::new(yaml);
/// let tokenizer = FlowTokenizer::new(yaml, &context);
///
/// let braces = tokenizer.find_all(TokenType::BraceOpen);
/// assert_eq!(braces.len(), 1);
/// ```
pub struct FlowTokenizer<'a> {
    _source: &'a str,
    context: &'a SourceContext<'a>,
}

impl<'a> FlowTokenizer<'a> {
    /// Creates a new flow tokenizer.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{tokenizer::FlowTokenizer, SourceContext};
    ///
    /// let yaml = "{key: value}";
    /// let context = SourceContext::new(yaml);
    /// let tokenizer = FlowTokenizer::new(yaml, &context);
    /// ```
    #[must_use]
    pub const fn new(source: &'a str, context: &'a SourceContext<'a>) -> Self {
        Self {
            _source: source,
            context,
        }
    }

    /// Finds all tokens of a specific type in the source.
    ///
    /// Ignores tokens inside quoted strings.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{tokenizer::{FlowTokenizer, TokenType}, SourceContext};
    ///
    /// let yaml = "list: [1, 2, 3]";
    /// let context = SourceContext::new(yaml);
    /// let tokenizer = FlowTokenizer::new(yaml, &context);
    ///
    /// let brackets = tokenizer.find_all(TokenType::BracketOpen);
    /// assert_eq!(brackets.len(), 1);
    /// ```
    #[must_use]
    pub fn find_all(&self, token_type: TokenType) -> Vec<Token> {
        let ch = Self::token_char(token_type);
        let mut tokens = Vec::new();

        for line_num in 1..=self.context.line_count() {
            if let Some(line) = self.context.get_line(line_num) {
                let line_start_offset = self.get_line_start_offset(line_num);

                for (col, c) in line.chars().enumerate() {
                    if c == ch && !Self::is_inside_string_at(line, col) {
                        // For hyphen, only match at start of line or after whitespace
                        if token_type == TokenType::Hyphen && !Self::is_list_item_hyphen(line, col)
                        {
                            continue;
                        }

                        let offset = line_start_offset + col;
                        let start = Location::new(line_num, col + 1, offset);
                        let end = Location::new(line_num, col + 2, offset + 1);
                        tokens.push(Token::new(token_type, Span::new(start, end)));
                    }
                }
            }
        }

        tokens
    }

    /// Finds all tokens within a specific span.
    ///
    /// Single-pass implementation that scans only the span range once
    /// to find all token types, avoiding redundant full-source scans.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{tokenizer::FlowTokenizer, SourceContext, Location, Span};
    ///
    /// let yaml = "a: b\nc: {d: e}";
    /// let context = SourceContext::new(yaml);
    /// let tokenizer = FlowTokenizer::new(yaml, &context);
    ///
    /// // Search only in line 2
    /// let span = Span::new(Location::new(2, 1, 5), Location::new(2, 10, 14));
    /// let tokens = tokenizer.find_in_span(span);
    ///
    /// // Should find {, :, }
    /// assert_eq!(tokens.len(), 3);
    /// ```
    #[must_use]
    pub fn find_in_span(&self, span: Span) -> Vec<Token> {
        let mut tokens = Vec::new();

        // Single-pass scan of only the span range
        for line_num in span.start.line..=span.end.line {
            if let Some(line) = self.context.get_line(line_num) {
                let line_start_offset = self.get_line_start_offset(line_num);

                for (col, c) in line.chars().enumerate() {
                    let offset = line_start_offset + col;

                    // Skip if outside span bounds
                    if offset < span.start.offset || offset >= span.end.offset {
                        continue;
                    }

                    // Skip if inside string
                    if Self::is_inside_string_at(line, col) {
                        continue;
                    }

                    // Match all token types in single pass
                    let token_type = match c {
                        '{' => Some(TokenType::BraceOpen),
                        '}' => Some(TokenType::BraceClose),
                        '[' => Some(TokenType::BracketOpen),
                        ']' => Some(TokenType::BracketClose),
                        ':' => Some(TokenType::Colon),
                        ',' => Some(TokenType::Comma),
                        '-' if Self::is_list_item_hyphen(line, col) => Some(TokenType::Hyphen),
                        _ => None,
                    };

                    if let Some(tt) = token_type {
                        let start = Location::new(line_num, col + 1, offset);
                        let end = Location::new(line_num, col + 2, offset + 1);
                        tokens.push(Token::new(tt, Span::new(start, end)));
                    }
                }
            }
        }

        // Already sorted by scan order (left to right, top to bottom)
        tokens
    }

    /// Checks if a position is inside a quoted string.
    ///
    /// Handles both single and double quotes with escape sequences.
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

    /// Checks if a hyphen at a position is a list item marker.
    ///
    /// Returns true if hyphen is at start of line or preceded by whitespace.
    fn is_list_item_hyphen(line: &str, col: usize) -> bool {
        if col == 0 {
            return true;
        }

        // Check if all characters before the hyphen are whitespace
        line.chars().take(col).all(char::is_whitespace)
    }

    /// Maps token type to its character representation.
    const fn token_char(token_type: TokenType) -> char {
        match token_type {
            TokenType::BraceOpen => '{',
            TokenType::BraceClose => '}',
            TokenType::BracketOpen => '[',
            TokenType::BracketClose => ']',
            TokenType::Colon => ':',
            TokenType::Comma => ',',
            TokenType::Hyphen => '-',
        }
    }

    /// Gets the byte offset where a line starts.
    ///
    /// Uses pre-computed offsets from `SourceContext` for O(1) access.
    fn get_line_start_offset(&self, line_num: usize) -> usize {
        self.context.get_line_offset(line_num)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenizer_simple_braces() {
        let yaml = "object: {key: value}";
        let context = SourceContext::new(yaml);
        let tokenizer = FlowTokenizer::new(yaml, &context);

        let braces = tokenizer.find_all(TokenType::BraceOpen);
        assert_eq!(braces.len(), 1);
        assert_eq!(braces[0].span.start.column, 9);
    }

    #[test]
    fn test_tokenizer_nested_braces() {
        let yaml = "{a: {b: c}}";
        let context = SourceContext::new(yaml);
        let tokenizer = FlowTokenizer::new(yaml, &context);

        let open_braces = tokenizer.find_all(TokenType::BraceOpen);
        assert_eq!(open_braces.len(), 2);

        let close_braces = tokenizer.find_all(TokenType::BraceClose);
        assert_eq!(close_braces.len(), 2);
    }

    #[test]
    fn test_tokenizer_ignore_in_strings() {
        let yaml = r#"url: "http://example.com""#;
        let context = SourceContext::new(yaml);
        let tokenizer = FlowTokenizer::new(yaml, &context);

        let colons = tokenizer.find_all(TokenType::Colon);
        // Only the mapping separator, not the one in the URL
        assert_eq!(colons.len(), 1);
        assert_eq!(colons[0].span.start.column, 4);
    }

    #[test]
    fn test_tokenizer_brackets() {
        let yaml = "list: [1, 2, 3]";
        let context = SourceContext::new(yaml);
        let tokenizer = FlowTokenizer::new(yaml, &context);

        let open = tokenizer.find_all(TokenType::BracketOpen);
        assert_eq!(open.len(), 1);

        let close = tokenizer.find_all(TokenType::BracketClose);
        assert_eq!(close.len(), 1);
    }

    #[test]
    fn test_tokenizer_commas() {
        let yaml = "[a, b, c]";
        let context = SourceContext::new(yaml);
        let tokenizer = FlowTokenizer::new(yaml, &context);

        let commas = tokenizer.find_all(TokenType::Comma);
        assert_eq!(commas.len(), 2);
    }

    #[test]
    fn test_tokenizer_colons() {
        let yaml = "a: b\nc: d";
        let context = SourceContext::new(yaml);
        let tokenizer = FlowTokenizer::new(yaml, &context);

        let colons = tokenizer.find_all(TokenType::Colon);
        assert_eq!(colons.len(), 2);
    }

    #[test]
    fn test_tokenizer_hyphens() {
        let yaml = "- item1\n- item2";
        let context = SourceContext::new(yaml);
        let tokenizer = FlowTokenizer::new(yaml, &context);

        let hyphens = tokenizer.find_all(TokenType::Hyphen);
        assert_eq!(hyphens.len(), 2);
    }

    #[test]
    fn test_tokenizer_hyphen_not_in_middle() {
        let yaml = "key: some-value";
        let context = SourceContext::new(yaml);
        let tokenizer = FlowTokenizer::new(yaml, &context);

        let hyphens = tokenizer.find_all(TokenType::Hyphen);
        // Should not match the hyphen in "some-value"
        assert_eq!(hyphens.len(), 0);
    }

    #[test]
    fn test_tokenizer_find_in_span() {
        let yaml = "a: b\nc: {d: e}";
        let context = SourceContext::new(yaml);
        let tokenizer = FlowTokenizer::new(yaml, &context);

        // Search only in line 2
        let span = Span::new(Location::new(2, 1, 5), Location::new(2, 10, 14));
        let tokens = tokenizer.find_in_span(span);

        // Should find {, :, }
        assert!(tokens.len() >= 3);
    }

    #[test]
    fn test_is_inside_string_at() {
        let line = r#"text: "hello: world""#;

        assert!(!FlowTokenizer::is_inside_string_at(line, 5)); // At first colon
        assert!(FlowTokenizer::is_inside_string_at(line, 13)); // At second colon (inside string)
    }

    #[test]
    fn test_is_inside_string_single_quotes() {
        let line = "text: 'hello: world'";

        assert!(!FlowTokenizer::is_inside_string_at(line, 5)); // At first colon
        assert!(FlowTokenizer::is_inside_string_at(line, 13)); // At second colon (inside string)
    }

    #[test]
    fn test_is_inside_string_escaped() {
        let line = r#"text: "escaped \" quote: here""#;

        assert!(!FlowTokenizer::is_inside_string_at(line, 5)); // At first colon
        assert!(FlowTokenizer::is_inside_string_at(line, 24)); // At second colon (inside string)
    }

    #[test]
    fn test_is_list_item_hyphen() {
        assert!(FlowTokenizer::is_list_item_hyphen("- item", 0));
        assert!(FlowTokenizer::is_list_item_hyphen("  - item", 2));
        assert!(!FlowTokenizer::is_list_item_hyphen("some-value", 4));
    }

    #[test]
    fn test_multiline_flow_mapping() {
        let yaml = "{\n  key: value\n}";
        let context = SourceContext::new(yaml);
        let tokenizer = FlowTokenizer::new(yaml, &context);

        let open = tokenizer.find_all(TokenType::BraceOpen);
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].span.start.line, 1);

        let close = tokenizer.find_all(TokenType::BraceClose);
        assert_eq!(close.len(), 1);
        assert_eq!(close[0].span.start.line, 3);
    }

    #[test]
    fn test_empty_flow_collections() {
        let yaml = "{}\n[]";
        let context = SourceContext::new(yaml);
        let tokenizer = FlowTokenizer::new(yaml, &context);

        let braces = tokenizer.find_all(TokenType::BraceOpen);
        assert_eq!(braces.len(), 1);

        let brackets = tokenizer.find_all(TokenType::BracketOpen);
        assert_eq!(brackets.len(), 1);
    }
}
