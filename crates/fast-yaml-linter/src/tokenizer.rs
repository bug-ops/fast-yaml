//! Flow collection tokenizer for identifying YAML syntax tokens.

use crate::{
    Location, SourceContext, Span,
    source::offset::{ByteOffset, ByteRange},
};
use saphyr_parser::{Event, Parser as SaphyrParser, ScalarStyle};

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
    /// Location span in source
    pub span: Span,
}

impl Token {
    /// Creates a new token.
    #[must_use]
    pub const fn new(span: Span) -> Self {
        Self { span }
    }
}

/// Tokenizes flow collection syntax in YAML source.
///
/// Accurately identifies flow syntax elements while ignoring tokens
/// inside quoted strings and comments.
pub struct FlowTokenizer<'a> {
    _source: &'a str,
    context: &'a SourceContext<'a>,
    block_scalar_ranges: Vec<ByteRange>,
    flow_ranges: Vec<ByteRange>,
    masked_ranges: Vec<ByteRange>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Quote {
    None,
    Double,
    Single,
}

/// Left-to-right scanner answering "is column N inside a block-context plain scalar?".
///
/// State is carried across queries, so successive queries with increasing columns on one
/// line cost O(line length) in total.
struct PlainScalarScanner {
    chars: Vec<char>,
    i: usize,
    quote: Quote,
    escape_next: bool,
    flow_depth: usize,
    at_value_start: bool,
    in_plain_scalar: bool,
}

impl PlainScalarScanner {
    fn new(line: &str) -> Self {
        Self {
            chars: line.chars().collect(),
            i: 0,
            quote: Quote::None,
            escape_next: false,
            flow_depth: 0,
            at_value_start: false,
            in_plain_scalar: false,
        }
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
    fn contains(&mut self, col: usize) -> bool {
        if col >= self.chars.len() {
            return false;
        }

        while self.i < col
            && let Some(&ch) = self.chars.get(self.i)
        {
            if self.escape_next {
                self.escape_next = false;
                self.i += 1;
                continue;
            }

            if self.quote == Quote::Double {
                if ch == '\\' {
                    self.escape_next = true;
                } else if ch == '"' {
                    self.quote = Quote::None;
                }
                self.i += 1;
                continue;
            }

            if self.quote == Quote::Single {
                if ch == '\'' {
                    self.quote = Quote::None;
                }
                self.i += 1;
                continue;
            }

            if self.in_plain_scalar {
                // Block-context plain scalar ends at flow terminators only when nested
                if self.flow_depth > 0 {
                    match ch {
                        ',' => {
                            self.in_plain_scalar = false;
                            self.at_value_start = true;
                        }
                        '}' | ']' => {
                            self.in_plain_scalar = false;
                            self.flow_depth = self.flow_depth.saturating_sub(1);
                        }
                        _ => {}
                    }
                }
                // In block context (self.flow_depth == 0) plain scalar runs to EOL — nothing ends it
                self.i += 1;
                continue;
            }

            if self.at_value_start {
                match ch {
                    ' ' | '\t' => {}
                    '"' => {
                        self.quote = Quote::Double;
                        self.at_value_start = false;
                    }
                    '\'' => {
                        self.quote = Quote::Single;
                        self.at_value_start = false;
                    }
                    '{' | '[' => {
                        self.flow_depth += 1;
                        self.at_value_start = false;
                    }
                    '#' => {
                        self.i = self.chars.len();
                        break;
                    }
                    _ => {
                        self.at_value_start = false;
                        if self.flow_depth == 0 {
                            // Block-context plain scalar — everything until EOL is scalar
                            self.in_plain_scalar = true;
                        }
                        // Flow-context plain scalars cannot contain `{`/`[`, so we do not
                        // set self.in_plain_scalar; any `{` encountered later will be treated
                        // as a nested flow collection (or invalid YAML).
                    }
                }
            } else {
                match ch {
                    '"' => self.quote = Quote::Double,
                    '\'' => self.quote = Quote::Single,
                    '{' | '[' => self.flow_depth += 1,
                    '}' | ']' => self.flow_depth = self.flow_depth.saturating_sub(1),
                    ':' if matches!(self.chars.get(self.i + 1), Some(' ' | '\t')) => {
                        self.at_value_start = true;
                        self.i += 2; // consume `: `
                        continue;
                    }
                    ',' if self.flow_depth > 0 => self.at_value_start = true,
                    '#' => {
                        self.i = self.chars.len();
                        break;
                    }
                    _ => {}
                }
            }

            self.i += 1;
        }

        self.in_plain_scalar
    }
}

impl<'a> FlowTokenizer<'a> {
    /// Creates a new flow tokenizer.
    #[must_use]
    pub fn new(source: &'a str, context: &'a SourceContext<'a>) -> Self {
        let scalars = collect_scalar_ranges(source, context);
        let masked_ranges = collect_masked_ranges(source, &scalars);
        Self {
            _source: source,
            context,
            block_scalar_ranges: scalars.block,
            flow_ranges: scalars.flow,
            masked_ranges,
        }
    }

    /// Finds all tokens of a specific type in the source.
    ///
    /// Ignores tokens inside quoted strings and comments; commas are only reported inside
    /// flow collections.
    #[must_use]
    pub fn find_all(&self, token_type: TokenType) -> Vec<Token> {
        let ch = Self::token_char(token_type);
        let mut tokens = Vec::new();

        for line_num in 1..=self.context.line_count() {
            if let Some(line) = self.context.get_line(line_num) {
                let line_start = self.context.line_start(line_num);
                let mut scanner: Option<PlainScalarScanner> = None;

                for (char_col, (byte_col, c)) in line.char_indices().enumerate() {
                    let offset = line_start.add_bytes(byte_col);
                    if c != ch || self.is_masked(offset) {
                        continue;
                    }

                    if token_type == TokenType::Comma && !self.is_in_flow(offset) {
                        continue;
                    }

                    // For hyphen, only match at start of line or after whitespace
                    if token_type == TokenType::Hyphen && !Self::is_list_item_hyphen(line, byte_col)
                    {
                        continue;
                    }

                    let offset = line_start.add_bytes(byte_col);

                    // Skip tokens inside block scalar content (literal `|` or folded `>`)
                    if self.is_in_block_scalar(offset) {
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
                    ) && scanner
                        .get_or_insert_with(|| PlainScalarScanner::new(line))
                        .contains(char_col)
                    {
                        continue;
                    }

                    tokens.push(Self::single_char_token(line_num, char_col, offset));
                }
            }
        }

        tokens
    }

    /// Checks if a byte offset falls inside a block scalar range.
    fn is_in_block_scalar(&self, offset: ByteOffset) -> bool {
        Self::in_ranges(&self.block_scalar_ranges, offset)
    }

    /// Checks if a byte offset falls inside an outermost flow collection.
    fn is_in_flow(&self, offset: ByteOffset) -> bool {
        Self::in_ranges(&self.flow_ranges, offset)
    }

    /// Checks if `offset` falls inside one of the sorted, disjoint `ranges`.
    fn in_ranges(ranges: &[ByteRange], offset: ByteOffset) -> bool {
        let idx = ranges.partition_point(|range| range.start() <= offset);
        idx.checked_sub(1)
            .and_then(|prev| ranges.get(prev))
            .is_some_and(|range| range.contains(offset))
    }

    /// Builds a one-character token at 0-indexed `char_col` on `line`.
    const fn single_char_token(line: usize, char_col: usize, offset: ByteOffset) -> Token {
        let start = Location::new(line, char_col + 1, offset.get());
        let end = Location::new(line, char_col + 2, offset.get() + 1);
        Token::new(Span::new(start, end))
    }

    /// Checks if a byte offset falls inside a comment or quoted scalar.
    fn is_masked(&self, offset: ByteOffset) -> bool {
        Self::in_ranges(&self.masked_ranges, offset)
    }

    /// Checks if a hyphen at a position is a list item marker.
    ///
    /// Returns true if hyphen is at start of line or preceded by whitespace.
    fn is_list_item_hyphen(line: &str, byte_col: usize) -> bool {
        if byte_col == 0 {
            return true;
        }

        // Check if all characters before the hyphen are whitespace
        line.get(..byte_col)
            .is_some_and(|before| before.chars().all(char::is_whitespace))
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
}

/// Byte ranges of scalars that the parser reports as block or quoted, and of flow collections.
struct ScalarRanges {
    block: Vec<ByteRange>,
    quoted: Vec<ByteRange>,
    /// Outermost flow collections, from the opening to the closing indicator. One that the
    /// parser never closed extends to the end of the source.
    flow: Vec<ByteRange>,
    /// Byte offset from which the parser produced no events because of a syntax error.
    unparsed_from: Option<ByteOffset>,
}

/// Collects byte ranges of block scalars (`|` literal, `>` folded) and quoted scalars.
///
/// Each range covers the scalar content (end is one past its last byte).
/// On parse error, returns the ranges collected before the error and records where
/// parsing stopped.
fn collect_scalar_ranges(source: &str, context: &SourceContext<'_>) -> ScalarRanges {
    #[allow(
        clippy::disallowed_methods,
        reason = "source passed the guarded parse in the same lint call"
    )]
    let mut parser = SaphyrParser::new_from_str(source);
    let mut ranges = ScalarRanges {
        block: Vec::new(),
        quoted: Vec::new(),
        flow: Vec::new(),
        unparsed_from: None,
    };

    let mut parsed_until = ByteOffset::ZERO;
    let mut flow_open = Vec::<ByteOffset>::new();
    loop {
        match parser.next_event() {
            Some(Ok((event, span))) => {
                let range = context.byte_range_of(span);
                parsed_until = range.end();
                // Block collection events are empty; End events may also cover trailing
                // whitespace and comments, so the indicator is read from the source.
                let indicator = (range.start() != range.end())
                    .then(|| source.as_bytes().get(range.start().get()))
                    .flatten();
                match (&event, indicator) {
                    (Event::SequenceStart(..) | Event::MappingStart(..), Some(b'[' | b'{')) => {
                        flow_open.push(range.start());
                    }
                    (Event::SequenceEnd | Event::MappingEnd, Some(b']' | b'}')) => {
                        if let Some(open) = flow_open.pop()
                            && flow_open.is_empty()
                        {
                            ranges
                                .flow
                                .push(ByteRange::new(open, range.start().add_bytes(1)));
                        }
                    }
                    _ => {}
                }
                if let Event::Scalar(_, style, ..) = event {
                    match style {
                        ScalarStyle::Literal | ScalarStyle::Folded => {
                            debug_assert!(
                                ranges
                                    .block
                                    .last()
                                    .is_none_or(|last| last.end() <= range.start()),
                                "block scalar ranges must be sorted and disjoint"
                            );
                            ranges.block.push(range);
                        }
                        ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted => {
                            ranges.quoted.push(range);
                        }
                        ScalarStyle::Plain => {}
                    }
                }
            }
            Some(Err(_)) => {
                ranges.unparsed_from = Some(parsed_until);
                if let Some(&open) = flow_open.first() {
                    ranges
                        .flow
                        .push(ByteRange::new(open, ByteOffset::new(source.len())));
                }
                break;
            }
            None => break,
        }
    }

    ranges
}

/// Scanner state of [`collect_masked_ranges`].
#[derive(Clone, Copy)]
enum Mode {
    Code,
    Comment,
    Single,
    Double,
    VerbatimTag,
}

/// Returns the masking mode opened by `ch`, given the previous char.
fn opening_mode(ch: char, prev: Option<char>, guess_quotes: bool) -> Option<Mode> {
    let at_boundary = prev.is_none_or(char::is_whitespace);
    match ch {
        '#' if at_boundary => Some(Mode::Comment),
        '%' if prev.is_none_or(|p| matches!(p, '\n' | '\r')) => Some(Mode::Comment),
        '\'' | '"' if guess_quotes && (at_boundary || prev.is_some_and(|p| "[{,:".contains(p))) => {
            Some(if ch == '"' {
                Mode::Double
            } else {
                Mode::Single
            })
        }
        _ => None,
    }
}

/// Returns the byte length of the verbatim tag `!<...>` that starts `text`, if `prev` allows
/// one to start there and its `>` comes before any whitespace or `<`.
pub fn verbatim_tag_len(prev: Option<char>, text: &str) -> Option<usize> {
    let boundary = prev.is_none_or(|p| p.is_whitespace() || "[{,".contains(p));
    let body = text.strip_prefix("!<").filter(|_| boundary)?;
    let end = body.find(|c: char| matches!(c, '>' | '<') || c.is_whitespace())?;
    body.get(end..)?.starts_with('>').then_some(end + 3)
}

/// Collects sorted byte ranges of comments, directives, verbatim tags and quoted scalars
/// in `source`.
///
/// Quoted scalars come from the parser. A `#` starts a comment only at the start of
/// a line or after whitespace. Past a parse error, quotes are detected heuristically
/// (opening after whitespace or a flow indicator, closing at the latest at end of
/// line) so that stray quotes cannot mask the rest of the file.
fn collect_masked_ranges(source: &str, scalars: &ScalarRanges) -> Vec<ByteRange> {
    let mut ranges = Vec::new();
    let mut mode = Mode::Code;
    let mut start = ByteOffset::ZERO;
    let mut prev = None::<char>;
    let mut block_idx = 0usize;
    let mut quoted_idx = 0usize;
    let mut chars = source.char_indices().peekable();

    while let Some((byte, ch)) = chars.next() {
        let offset = ByteOffset::new(byte);
        match mode {
            Mode::Code => {
                while scalars
                    .block
                    .get(block_idx)
                    .is_some_and(|range| range.end() <= offset)
                {
                    block_idx += 1;
                }
                if scalars
                    .block
                    .get(block_idx)
                    .is_some_and(|range| range.start() <= offset)
                {
                    prev = None;
                    continue;
                }

                while scalars
                    .quoted
                    .get(quoted_idx)
                    .is_some_and(|range| range.end() <= offset)
                {
                    quoted_idx += 1;
                }
                if let Some(&quoted) = scalars.quoted.get(quoted_idx)
                    && quoted.start() <= offset
                {
                    if quoted.start() == offset {
                        ranges.push(quoted);
                    }
                    prev = Some(ch);
                    continue;
                }

                let guess_quotes = scalars.unparsed_from.is_some_and(|from| offset >= from);
                let opened = if source
                    .get(byte..)
                    .is_some_and(|text| verbatim_tag_len(prev, text).is_some())
                {
                    Some(Mode::VerbatimTag)
                } else {
                    opening_mode(ch, prev, guess_quotes)
                };
                if let Some(opened) = opened {
                    mode = opened;
                    start = offset;
                }
                prev = Some(ch);
            }
            Mode::Comment | Mode::Single | Mode::Double | Mode::VerbatimTag if ch == '\n' => {
                ranges.push(ByteRange::new(start, offset));
                mode = Mode::Code;
                prev = Some(ch);
            }
            Mode::Comment => {}
            Mode::VerbatimTag => {
                if ch == '>' {
                    ranges.push(ByteRange::new(start, offset.add_bytes(1)));
                    mode = Mode::Code;
                    prev = Some(ch);
                }
            }
            Mode::Single => {
                if ch == '\'' && chars.next_if(|&(_, next)| next == '\'').is_none() {
                    ranges.push(ByteRange::new(start, offset.add_bytes(1)));
                    mode = Mode::Code;
                    prev = Some(ch);
                }
            }
            Mode::Double => match ch {
                '\\' => {
                    chars.next();
                }
                '"' => {
                    ranges.push(ByteRange::new(start, offset.add_bytes(1)));
                    mode = Mode::Code;
                    prev = Some(ch);
                }
                _ => {}
            },
        }
    }

    if !matches!(mode, Mode::Code) {
        ranges.push(ByteRange::new(start, ByteOffset::new(source.len())));
    }

    ranges
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Location, Span};

    impl FlowTokenizer<'_> {
        /// Finds all tokens within a specific span.
        ///
        /// Single-pass implementation that scans only the span range once
        /// to find all token types, avoiding redundant full-source scans.
        pub fn find_in_span(&self, span: Span) -> Vec<Token> {
            let mut tokens = Vec::new();

            // Single-pass scan of only the span range
            for line_num in span.start.line..=span.end.line {
                if let Some(line) = self.context.get_line(line_num) {
                    let line_start = self.context.line_start(line_num);

                    for (char_col, (byte_col, c)) in line.char_indices().enumerate() {
                        let offset = line_start.add_bytes(byte_col);

                        // Skip if outside span bounds
                        if offset.get() < span.start.offset || offset.get() >= span.end.offset {
                            continue;
                        }

                        // Skip tokens inside block scalar content
                        if self.is_in_block_scalar(offset) {
                            continue;
                        }

                        // Skip if inside string
                        if self.is_masked(offset) {
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
                            '-' if Self::is_list_item_hyphen(line, byte_col) => {
                                Some(TokenType::Hyphen)
                            }
                            _ => None,
                        };

                        if token_type.is_some() {
                            tokens.push(Self::single_char_token(line_num, char_col, offset));
                        }
                    }
                }
            }

            // Already sorted by scan order (left to right, top to bottom)
            tokens
        }
    }

    #[test]
    fn test_find_all_ignores_tokens_in_quotes_and_comments() {
        let yaml = "list: [1, 2, 3]";
        let context = SourceContext::new(yaml);
        let tokenizer = FlowTokenizer::new(yaml, &context);
        assert_eq!(tokenizer.find_all(TokenType::BracketOpen).len(), 1);
    }

    #[test]
    fn test_find_in_span_scans_only_the_span() {
        let yaml = "a: b\nc: {d: e}";
        let context = SourceContext::new(yaml);
        let tokenizer = FlowTokenizer::new(yaml, &context);
        let span = Span::new(Location::new(2, 1, 5), Location::new(2, 10, 14));
        assert_eq!(tokenizer.find_in_span(span).len(), 4);
    }

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

        // Should find `:`, `{`, `:`, `}`
        assert_eq!(tokens.len(), 4);
    }

    fn count(yaml: &str, token_type: TokenType) -> usize {
        let context = SourceContext::new(yaml);
        FlowTokenizer::new(yaml, &context)
            .find_all(token_type)
            .len()
    }

    #[test]
    fn test_plain_scalar_scanner_carries_state_across_queries() {
        let yaml = "k: ${{ a }} {b} [c]";
        assert_eq!(count(yaml, TokenType::BraceOpen), 0);
        assert_eq!(count(yaml, TokenType::BracketOpen), 0);
        assert_eq!(count("k: [{a: 1}, {b: 2}]", TokenType::BraceOpen), 2);
        assert_eq!(count("k: \"q\" # {x}\nm: {y: 1}", TokenType::BraceOpen), 1);
    }

    #[test]
    fn test_many_braces_on_one_line_are_all_found() {
        let yaml = format!("k: [{}{{b: 2}}]", "{a: 1}, ".repeat(20_000));
        assert_eq!(count(&yaml, TokenType::BraceOpen), 20_001);
        assert_eq!(count(&yaml, TokenType::BraceClose), 20_001);
    }

    #[test]
    fn test_many_quoted_scalars_on_one_wide_line_are_linear() {
        let yaml = format!("k: [{}\"ж\"]", "\"ж\", ".repeat(50_000));
        let context = SourceContext::new(&yaml);
        let start = std::time::Instant::now();
        let ranges = collect_scalar_ranges(&yaml, &context);
        assert_eq!(ranges.quoted.len(), 50_001);
        assert!(start.elapsed() < std::time::Duration::from_secs(10));
    }

    fn scan(line: &str, cols: &[usize]) -> Vec<bool> {
        let mut scanner = PlainScalarScanner::new(line);
        cols.iter().map(|&col| scanner.contains(col)).collect()
    }

    fn char_cols(line: &str, ch: char) -> Vec<usize> {
        line.chars()
            .enumerate()
            .filter_map(|(i, c)| (c == ch).then_some(i))
            .collect()
    }

    #[test]
    fn test_scanner_plain_scalar_value_covers_braces() {
        let line = "k: a {x} b";
        assert_eq!(scan(line, &[5, 7]), vec![true, true]);
    }

    #[test]
    fn test_scanner_flow_value_is_not_plain() {
        let line = "k: {x: 1}";
        assert_eq!(scan(line, &[3]), vec![false]);
    }

    #[test]
    fn test_scanner_nested_flow_sequence_with_inner_brace() {
        let line = "k: [a, b {c}, d]";
        let cols = char_cols(line, '{');
        assert_eq!(scan(line, &cols), vec![false]);
    }

    #[test]
    fn test_scanner_quoted_braces_do_not_start_plain() {
        let line = r#"k: "a { \" } b" {c}"#;
        let cols = char_cols(line, '{');
        assert_eq!(cols.len(), 2);
        assert_eq!(scan(line, &cols[1..]), vec![false]);
    }

    #[test]
    fn test_scanner_trailing_comment_stops_scan() {
        let line = r#"k: "v" # {x} [y]"#;
        let mut cols = char_cols(line, '{');
        cols.extend(char_cols(line, '['));
        cols.sort_unstable();
        assert_eq!(scan(line, &cols), vec![false, false]);
    }

    #[test]
    fn test_scanner_nested_mapping_value_in_plain_scalar() {
        let line = "k: a: b {x}";
        let cols = char_cols(line, '{');
        assert_eq!(scan(line, &cols), vec![true]);
    }

    #[test]
    fn test_scanner_multibyte_before_brace_uses_char_columns() {
        let line = "ключ: значение {x}";
        let cols = char_cols(line, '{');
        assert_eq!(cols, vec![15]);
        assert_eq!(scan(line, &cols), vec![true]);
    }

    #[test]
    fn test_scanner_query_at_or_past_line_end_is_false() {
        let line = "k: a {x}";
        let len = line.chars().count();
        assert_eq!(scan(line, &[len, len + 5]), vec![false, false]);
        assert_eq!(scan("", &[0]), vec![false]);
    }

    #[test]
    fn test_scanner_state_matches_fresh_scan_for_every_column() {
        let line = r#"a: [x, "y, {z}", {k: v}, w {q}] # {c}"#;
        let len = line.chars().count();
        let cols: Vec<usize> = (0..len).collect();
        let carried = scan(line, &cols);
        let fresh: Vec<bool> = cols
            .iter()
            .map(|&col| PlainScalarScanner::new(line).contains(col))
            .collect();
        assert_eq!(carried, fresh);
    }

    #[test]
    fn test_comment_delimiters_ignored() {
        assert_eq!(count("# a { b\nkey: value", TokenType::BraceOpen), 0);
        assert_eq!(
            count("key: value  # [ not a bracket", TokenType::BracketOpen),
            0
        );
        assert_eq!(
            count("key: [a, b] # ] trailing", TokenType::BracketClose),
            1
        );
    }

    #[test]
    fn test_comment_inside_multiline_flow_sequence() {
        let yaml = "key: [\n  a, # open [ here\n  b\n]\n";
        assert_eq!(count(yaml, TokenType::BracketOpen), 1);
        assert_eq!(count(yaml, TokenType::BracketClose), 1);
    }

    #[test]
    fn test_hash_without_whitespace_is_not_comment() {
        assert_eq!(count("k: [a#b, c]", TokenType::Comma), 1);
    }

    #[test]
    fn test_hash_inside_quotes_is_not_comment() {
        let yaml = "a: \"x # y\" # z\nb: {c: d}";
        assert_eq!(count(yaml, TokenType::BraceOpen), 1);
        assert_eq!(count(yaml, TokenType::Colon), 3);
    }

    #[test]
    fn test_quoted_delimiters_ignored() {
        assert_eq!(count("a: \"x { y\"", TokenType::BraceOpen), 0);
        assert_eq!(count("a: 'x [ y'", TokenType::BracketOpen), 0);
        assert_eq!(count("a: 'it''s { here'", TokenType::BraceOpen), 0);
        assert_eq!(count(r#"a: "say \" { done""#, TokenType::BraceOpen), 0);
    }

    #[test]
    fn test_multiline_quoted_scalars() {
        assert_eq!(
            count("a: \"first {\n  second\"\nb: {c: d}", TokenType::BraceOpen),
            1
        );
        assert_eq!(
            count("a: 'first [\n  second'\nb: [1]", TokenType::BracketOpen),
            1
        );
    }

    #[test]
    fn test_apostrophe_in_plain_scalar_does_not_open_quote() {
        let yaml = "a: it's fine\nb: {c: d}";
        assert_eq!(count(yaml, TokenType::BraceOpen), 1);
    }

    #[test]
    fn test_non_ascii_comments_and_quotes() {
        let yaml = "# héllo { 名前\nk: \"日本 [ 語\" # ✓ }\nl: {é: 1}";
        assert_eq!(count(yaml, TokenType::BraceOpen), 1);
        assert_eq!(count(yaml, TokenType::BraceClose), 1);
        assert_eq!(count(yaml, TokenType::BracketOpen), 0);
        let context = SourceContext::new(yaml);
        let braces = FlowTokenizer::new(yaml, &context).find_all(TokenType::BraceOpen);
        assert_eq!(braces[0].span.start.line, 3);
        assert_eq!(braces[0].span.start.column, 4);
    }

    #[test]
    fn test_find_in_span_ignores_comments_and_quotes() {
        let yaml = "a: {b: \"c, d\"} # e, f }";
        let context = SourceContext::new(yaml);
        let tokenizer = FlowTokenizer::new(yaml, &context);
        let span = Span::new(
            Location::new(1, 1, 0),
            Location::new(1, yaml.len() + 1, yaml.len()),
        );
        let commas = tokenizer
            .find_in_span(span)
            .into_iter()
            .filter(|t| yaml[t.span.start.offset..].starts_with(','))
            .count();
        assert_eq!(commas, 0);
    }

    #[test]
    fn test_mid_plain_scalar_quote_does_not_mask_rest() {
        for head in [
            "a: foo \"bar {\n",
            "a: don 't {x\n",
            "note: the '90s were great\n",
            "a: foo\n  'bar\n",
        ] {
            let yaml = format!("{head}b: [1,2 ,3]\n");
            assert_eq!(count(&yaml, TokenType::Comma), 2, "{yaml}");
        }
    }

    #[test]
    fn test_unterminated_quote_after_parse_error_stops_at_line_end() {
        let yaml = "a: [x, 'oops\nb: {c: d}\n";
        assert_eq!(count(yaml, TokenType::BraceOpen), 1);
    }

    #[test]
    fn test_crlf_comment() {
        assert_eq!(count("# c {\r\nk: {a: 1}\r\n", TokenType::BraceOpen), 1);
    }

    #[test]
    fn test_comment_at_eof_without_newline() {
        assert_eq!(count("k: v # {", TokenType::BraceOpen), 0);
    }

    #[test]
    fn test_double_quote_escaped_backslash_before_closing_quote() {
        assert_eq!(count(r#"k: "a\\" # {"#, TokenType::BraceOpen), 0);
        assert_eq!(
            count("k: \"a \\\n  { b\"\nm: {c: d}", TokenType::BraceOpen),
            1
        );
    }

    #[test]
    fn test_quotes_after_flow_indicators() {
        assert_eq!(count(r#"k: {"a":1}"#, TokenType::Colon), 2);
        assert_eq!(count("k: [a,'b ,c',d]", TokenType::Comma), 2);
    }

    #[test]
    fn test_hash_in_url_and_tab_before_comment() {
        assert_eq!(count("u: http://x/#frag'{", TokenType::BraceOpen), 0);
        assert_eq!(count("k: [a]\t# ] [", TokenType::BracketClose), 1);
    }

    #[test]
    fn test_tag_and_anchor_prefixed_quotes() {
        assert_eq!(count("a: !!str 'x {'", TokenType::BraceOpen), 0);
        assert_eq!(count("a: &a \"y [\"", TokenType::BracketOpen), 0);
    }

    #[test]
    fn test_block_scalar_variants_with_quotes() {
        for header in ["|+", ">", "|-"] {
            let yaml = format!("a: {header}\n  it 'x {{\n  \"y\n\nb: [1, 2]\n");
            assert_eq!(count(&yaml, TokenType::BracketOpen), 1, "{yaml}");
        }
        let yaml = "- |\n  echo 'x {\n- [1]\n";
        assert_eq!(count(yaml, TokenType::BracketOpen), 1);
    }

    #[test]
    fn test_find_in_span_starting_inside_multiline_quote() {
        let yaml = "a: \"first {\n  second, }\"\nb: {c: d}";
        let context = SourceContext::new(yaml);
        let tokenizer = FlowTokenizer::new(yaml, &context);
        let start = yaml.find("second").unwrap();
        let span = Span::new(Location::new(2, 3, start), Location::new(3, 9, yaml.len()));
        let tokens = tokenizer.find_in_span(span);
        assert!(tokens.iter().all(|t| t.span.start.line == 3));
        assert_eq!(tokens.len(), 4);
    }

    #[test]
    fn test_quote_in_block_scalar_does_not_mask_following_yaml() {
        let yaml = "run: |\n  echo 'unterminated\nlist: [1, 2]\n";
        assert_eq!(count(yaml, TokenType::BracketOpen), 1);
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
        let ranges = collect_scalar_ranges(yaml, &SourceContext::new(yaml)).block;
        assert_eq!(ranges.len(), 1);
        let bracket_pos = yaml.find('[').unwrap();
        assert!(ranges[0].contains(ByteOffset::new(bracket_pos)));
    }

    #[test]
    fn test_block_scalar_ranges_are_byte_offsets_with_non_ascii_prefix() {
        let yaml = "# ———\nrun: |\n  echo\n  tail }\nc: {a: b}\n";
        let ranges = collect_scalar_ranges(yaml, &SourceContext::new(yaml)).block;
        assert_eq!(ranges.len(), 1);
        let range = ranges[0];
        assert!(yaml[range.start().get()..].starts_with("echo"));
        assert!(range.end().get() <= yaml.len());
        let stray = yaml.find("tail }").unwrap() + 5;
        assert!(range.contains(ByteOffset::new(stray)));
    }

    #[test]
    fn test_is_in_block_scalar_lookup_across_multiple_ranges() {
        let yaml = "a: |\n  {x}\nb: {y: 1}\nc: >\n  [z]\nd: [w]\n";
        let context = SourceContext::new(yaml);
        let tokenizer = FlowTokenizer::new(yaml, &context);
        let inside = |needle: &str| {
            tokenizer.is_in_block_scalar(ByteOffset::new(yaml.find(needle).unwrap()))
        };
        assert!(inside("{x}"));
        assert!(inside("[z]"));
        assert!(!inside("{y"));
        assert!(!inside("[w]"));
        assert!(!inside("a:"));
        assert_eq!(tokenizer.find_all(TokenType::BraceOpen).len(), 1);
        assert_eq!(tokenizer.find_all(TokenType::BracketOpen).len(), 1);
    }

    #[test]
    fn test_is_in_block_scalar_range_boundaries() {
        let yaml = "a: |\n  {x}\nb: >\n  [y]\n";
        let context = SourceContext::new(yaml);
        let tokenizer = FlowTokenizer::new(yaml, &context);
        let ranges = collect_scalar_ranges(yaml, &context).block;
        assert_eq!(ranges.len(), 2);
        for range in &ranges {
            assert!(tokenizer.is_in_block_scalar(range.start()));
            assert!(!tokenizer.is_in_block_scalar(range.end()));
        }
        assert!(!tokenizer.is_in_block_scalar(ByteOffset::ZERO));
    }

    #[test]
    fn test_is_in_block_scalar_empty_block() {
        let yaml = "a: |\nb: {c: 1}\n";
        let context = SourceContext::new(yaml);
        let tokenizer = FlowTokenizer::new(yaml, &context);
        assert!(!tokenizer.is_in_block_scalar(ByteOffset::new(yaml.find('{').unwrap())));
        assert_eq!(tokenizer.find_all(TokenType::BraceOpen).len(), 1);
    }

    #[test]
    fn test_non_ascii_prefix_block_scalar_no_brace_tokens() {
        let yaml = "# ———\nrun: |\n  echo\n  ok\n  tail }\n  [ x { y\n";
        let context = SourceContext::new(yaml);
        let tokenizer = FlowTokenizer::new(yaml, &context);
        for token_type in [
            TokenType::BraceOpen,
            TokenType::BraceClose,
            TokenType::BracketOpen,
        ] {
            assert!(tokenizer.find_all(token_type).is_empty(), "{token_type:?}");
        }
    }

    #[test]
    fn test_commas_only_inside_flow_collections() {
        assert_eq!(count("{a: 1}: x,y\n", TokenType::Comma), 0);
        assert_eq!(count("k: {a: [1,2], b: 3}\n", TokenType::Comma), 2);
    }

    #[test]
    fn test_flow_range_survives_trailing_comment_and_spaces() {
        for yaml in [
            "k: [a,b]  # c\nj: [c,d]\n",
            "k: {a: 1,b: 2}  # z\nj: {c: 3,d: 4}\n",
            "k: [a,b]   \nj: [c,d]\n",
            "[a]: b\nj: [c,d]\n",
        ] {
            let expected = yaml.matches(',').count();
            assert_eq!(count(yaml, TokenType::Comma), expected, "{yaml:?}");
        }
    }

    #[test]
    fn test_unclosed_flow_extends_to_end_of_source() {
        assert_eq!(count("k: [a,b\nj: c,d\n", TokenType::Comma), 2);
    }

    #[test]
    fn test_many_unterminated_verbatim_tags_are_linear() {
        let yaml = format!("a: x[{}\n", "!<[".repeat(40_000));
        let context = SourceContext::new(&yaml);
        let started = std::time::Instant::now();
        let _ = FlowTokenizer::new(&yaml, &context);
        assert!(started.elapsed().as_secs() < 2);
    }

    #[test]
    fn test_verbatim_tag_len() {
        assert_eq!(verbatim_tag_len(None, "!<a#b,c> x"), Some(8));
        assert_eq!(verbatim_tag_len(Some('['), "!<a>]"), Some(4));
        assert_eq!(verbatim_tag_len(Some(' '), "!<x,b]"), None);
        assert_eq!(verbatim_tag_len(Some(' '), "!<x ,b>"), None);
        assert_eq!(verbatim_tag_len(Some(' '), "!<[!<x>"), None);
        assert_eq!(verbatim_tag_len(Some('a'), "!<x>"), None);
        assert_eq!(verbatim_tag_len(None, "!x>"), None);
    }
}
