//! Flow collection tokenizer for identifying YAML syntax tokens.

use crate::{Location, SourceContext, Span};
use saphyr_parser::{BufferedInput, Event, Parser as SaphyrParser, ScalarStyle};

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
    block_scalar_ranges: Vec<(usize, usize)>,
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
    pub fn new(source: &'a str, context: &'a SourceContext<'a>) -> Self {
        Self {
            _source: source,
            context,
            block_scalar_ranges: collect_block_scalar_ranges(source),
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

                let mut char_col = 0usize;
                for (byte_col, c) in line.char_indices() {
                    if c == ch && !Self::is_inside_string_at(line, byte_col) {
                        // For hyphen, only match at start of line or after whitespace
                        if token_type == TokenType::Hyphen
                            && !Self::is_list_item_hyphen(line, byte_col)
                        {
                            char_col += 1;
                            continue;
                        }

                        let offset = line_start_offset + byte_col;

                        // Skip tokens inside block scalar content (literal `|` or folded `>`)
                        if self.is_in_block_scalar(offset) {
                            char_col += 1;
                            continue;
                        }

                        // Skip braces/brackets that appear inside block-context plain scalars
                        // (e.g. template expressions like `${{ var }}`).
                        if matches!(
                            token_type,
                            TokenType::BraceOpen
                                | TokenType::BraceClose
                                | TokenType::BracketOpen
                                | TokenType::BracketClose
                        ) && Self::is_in_block_plain_scalar_at(line, char_col)
                        {
                            char_col += 1;
                            continue;
                        }
                        let start = Location::new(line_num, char_col + 1, offset);
                        let end = Location::new(line_num, char_col + 2, offset + 1);
                        tokens.push(Token::new(token_type, Span::new(start, end)));
                    }
                    char_col += 1;
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

                let mut char_col = 0usize;
                for (byte_col, c) in line.char_indices() {
                    let offset = line_start_offset + byte_col;

                    // Skip if outside span bounds
                    if offset < span.start.offset || offset >= span.end.offset {
                        char_col += 1;
                        continue;
                    }

                    // Skip tokens inside block scalar content
                    if self.is_in_block_scalar(offset) {
                        char_col += 1;
                        continue;
                    }

                    // Skip if inside string
                    if Self::is_inside_string_at(line, byte_col) {
                        char_col += 1;
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
                        '-' if Self::is_list_item_hyphen(line, byte_col) => Some(TokenType::Hyphen),
                        _ => None,
                    };

                    if let Some(tt) = token_type {
                        let start = Location::new(line_num, char_col + 1, offset);
                        let end = Location::new(line_num, char_col + 2, offset + 1);
                        tokens.push(Token::new(tt, Span::new(start, end)));
                    }
                    char_col += 1;
                }
            }
        }

        // Already sorted by scan order (left to right, top to bottom)
        tokens
    }

    /// Checks if a byte offset falls inside a block scalar range.
    fn is_in_block_scalar(&self, offset: usize) -> bool {
        self.block_scalar_ranges
            .iter()
            .any(|&(start, end)| offset >= start && offset < end)
    }

    /// Checks if a position is inside a block-context plain scalar.
    ///
    /// A plain scalar starts when the first non-whitespace character after a value
    /// separator (`: ` or `, `) is not a flow indicator (`{`, `[`, `"`, `'`).
    /// In block context (outside any flow collection), such a plain scalar
    /// continues to the end of the line, so any `{` or `[` inside it must not
    /// be treated as a YAML flow collection delimiter.
    ///
    /// This prevents false positives on template expressions like `${{ var }}`
    /// that appear as plain scalar values.
    fn is_in_block_plain_scalar_at(line: &str, col: usize) -> bool {
        let chars: Vec<char> = line.chars().collect();
        if col >= chars.len() {
            return false;
        }

        let mut i = 0usize;
        let mut in_double = false;
        let mut in_single = false;
        let mut escape_next = false;
        let mut flow_depth: usize = 0;
        let mut at_value_start = false;
        let mut in_plain_scalar = false;

        while i < col {
            let ch = chars[i];

            if escape_next {
                escape_next = false;
                i += 1;
                continue;
            }

            if in_double {
                if ch == '\\' {
                    escape_next = true;
                } else if ch == '"' {
                    in_double = false;
                }
                i += 1;
                continue;
            }

            if in_single {
                if ch == '\'' {
                    in_single = false;
                }
                i += 1;
                continue;
            }

            if in_plain_scalar {
                // Block-context plain scalar ends at flow terminators only when nested
                if flow_depth > 0 {
                    match ch {
                        ',' => {
                            in_plain_scalar = false;
                            at_value_start = true;
                        }
                        '}' | ']' => {
                            in_plain_scalar = false;
                            flow_depth = flow_depth.saturating_sub(1);
                        }
                        _ => {}
                    }
                }
                // In block context (flow_depth == 0) plain scalar runs to EOL — nothing ends it
                i += 1;
                continue;
            }

            if at_value_start {
                match ch {
                    ' ' | '\t' => {}
                    '"' => {
                        in_double = true;
                        at_value_start = false;
                    }
                    '\'' => {
                        in_single = true;
                        at_value_start = false;
                    }
                    '{' | '[' => {
                        flow_depth += 1;
                        at_value_start = false;
                    }
                    '#' => break,
                    _ => {
                        at_value_start = false;
                        if flow_depth == 0 {
                            // Block-context plain scalar — everything until EOL is scalar
                            in_plain_scalar = true;
                        }
                        // Flow-context plain scalars cannot contain `{`/`[`, so we do not
                        // set in_plain_scalar; any `{` encountered later will be treated
                        // as a nested flow collection (or invalid YAML).
                    }
                }
            } else {
                match ch {
                    '"' => in_double = true,
                    '\'' => in_single = true,
                    '{' | '[' => flow_depth += 1,
                    '}' | ']' => flow_depth = flow_depth.saturating_sub(1),
                    ':' if i + 1 < chars.len() && (chars[i + 1] == ' ' || chars[i + 1] == '\t') => {
                        at_value_start = true;
                        i += 2; // consume `: `
                        continue;
                    }
                    ',' if flow_depth > 0 => at_value_start = true,
                    '#' => break,
                    _ => {}
                }
            }

            i += 1;
        }

        in_plain_scalar
    }

    /// Checks if a position is inside a quoted string.
    ///
    /// Handles both single and double quotes with escape sequences.
    fn is_inside_string_at(line: &str, byte_col: usize) -> bool {
        let mut in_single = false;
        let mut in_double = false;
        let mut escape = false;

        for (byte_i, ch) in line.char_indices() {
            if byte_i >= byte_col {
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
    fn is_list_item_hyphen(line: &str, byte_col: usize) -> bool {
        if byte_col == 0 {
            return true;
        }

        // Check if all characters before the hyphen are whitespace
        line[..byte_col].chars().all(char::is_whitespace)
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

/// Collects byte ranges of all block scalar values (`|` literal, `>` folded) in `source`.
///
/// Returns a list of `(start_byte, end_byte)` pairs. The start byte points to the `|`/`>`
/// indicator character; the end byte is one past the last byte of scalar content.
/// On parse error, returns whatever ranges were collected before the error.
fn collect_block_scalar_ranges(source: &str) -> Vec<(usize, usize)> {
    let input = BufferedInput::new(source.chars());
    let mut parser = SaphyrParser::new(input);
    let mut ranges = Vec::new();

    while let Some(Ok((event, span))) = parser.next_event() {
        if let Event::Scalar(_, ScalarStyle::Literal | ScalarStyle::Folded, ..) = event {
            ranges.push((span.start.index(), span.end.index()));
        }
    }

    ranges
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

    // Issue #116: block scalar false positives
    #[test]
    fn test_block_scalar_literal_no_bracket_tokens() {
        // GitHub Actions YAML: bash double-bracket syntax inside `run: |`
        let yaml = "steps:\n  - name: Check result\n    run: |\n      if [[ \"$result\" != \"success\" ]]; then\n        exit 1\n      fi\n";
        let context = SourceContext::new(yaml);
        let tokenizer = FlowTokenizer::new(yaml, &context);

        let brackets = tokenizer.find_all(TokenType::BracketOpen);
        assert_eq!(
            brackets.len(),
            0,
            "brackets inside literal block scalar must not be tokenized"
        );

        let commas = tokenizer.find_all(TokenType::Comma);
        assert_eq!(
            commas.len(),
            0,
            "commas inside literal block scalar must not be tokenized"
        );
    }

    #[test]
    #[allow(clippy::literal_string_with_formatting_args)]
    fn test_block_scalar_folded_no_brace_tokens() {
        let yaml = "message: >\n  This has {braces} and [brackets] inside.\n";
        let context = SourceContext::new(yaml);
        let tokenizer = FlowTokenizer::new(yaml, &context);

        let braces = tokenizer.find_all(TokenType::BraceOpen);
        assert_eq!(
            braces.len(),
            0,
            "braces inside folded block scalar must not be tokenized"
        );

        let brackets = tokenizer.find_all(TokenType::BracketOpen);
        assert_eq!(
            brackets.len(),
            0,
            "brackets inside folded block scalar must not be tokenized"
        );
    }

    #[test]
    fn test_real_yaml_tokens_after_block_scalar_still_found() {
        // Tokens in real YAML after a block scalar must still be linted
        let yaml = "run: |\n  echo hello\nlist: [1, 2, 3]\n";
        let context = SourceContext::new(yaml);
        let tokenizer = FlowTokenizer::new(yaml, &context);

        // The block scalar body must not produce bracket tokens
        // The flow sequence on `list:` line must produce exactly 1 open bracket
        let brackets = tokenizer.find_all(TokenType::BracketOpen);
        assert_eq!(
            brackets.len(),
            1,
            "only the real YAML bracket should be found"
        );
        assert_eq!(brackets[0].span.start.line, 3);
    }

    // Regression tests for issue #167: byte vs char offset confusion for multibyte UTF-8
    #[test]
    fn test_non_ascii_no_false_positives_commas() {
        // é is 2 bytes — char index and byte offset diverge after it
        let yaml = "items:\n  - {données: 1, key: 2}";
        let context = SourceContext::new(yaml);
        let tokenizer = FlowTokenizer::new(yaml, &context);
        let commas = tokenizer.find_all(TokenType::Comma);
        assert_eq!(commas.len(), 1, "should find exactly 1 comma");
        assert_eq!(commas[0].span.start.line, 2);
    }

    #[test]
    fn test_non_ascii_hyphens_no_false_positives() {
        // ✓ is 3 bytes — list items after it must not trigger false positives
        let yaml = "items:\n  - note: \"contains ✓ checkmark\"\n  - item1\n  - item2";
        let context = SourceContext::new(yaml);
        let tokenizer = FlowTokenizer::new(yaml, &context);
        let hyphens = tokenizer.find_all(TokenType::Hyphen);
        assert_eq!(hyphens.len(), 3, "should find exactly 3 list item hyphens");
    }

    #[test]
    fn test_cjk_no_false_positives() {
        // CJK characters are 3 bytes each
        let yaml = "data: {名前: value, key: other}";
        let context = SourceContext::new(yaml);
        let tokenizer = FlowTokenizer::new(yaml, &context);
        let colons = tokenizer.find_all(TokenType::Colon);
        assert_eq!(
            colons.len(),
            3,
            "should find exactly 3 colons (data:, 名前:, key:)"
        );
        let commas = tokenizer.find_all(TokenType::Comma);
        assert_eq!(commas.len(), 1, "should find exactly 1 comma");
    }

    #[test]
    fn test_emoji_no_false_positives() {
        // Emoji are 4 bytes each
        let yaml = "data: {emoji: \"🎉\", key: value}";
        let context = SourceContext::new(yaml);
        let tokenizer = FlowTokenizer::new(yaml, &context);
        let commas = tokenizer.find_all(TokenType::Comma);
        assert_eq!(commas.len(), 1, "should find exactly 1 comma");
    }

    #[test]
    fn test_collect_block_scalar_ranges_literal() {
        let yaml = "key: |\n  content [bracket]\n";
        let ranges = collect_block_scalar_ranges(yaml);
        assert_eq!(ranges.len(), 1);
        // Range must cover the block scalar content
        let (start, end) = ranges[0];
        // The word "bracket" is inside the range
        let bracket_pos = yaml.find('[').unwrap();
        assert!(bracket_pos >= start && bracket_pos < end);
    }
}
