//! Rule to check quoted string style.

use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Location, Severity,
    SourceContext, Span,
};
use fast_yaml_core::Value;
use saphyr_parser::{BufferedInput, Event, Parser as SaphyrParser, ScalarStyle};

use super::LintRule;

/// Linting rule for quoted strings.
///
/// Validates string quoting style to ensure consistency.
/// Controls whether strings should be quoted, and if so, which quote style to use.
///
/// Configuration options:
/// - `quote-type`: "single", "double", "any" (default: "any")
/// - `required`: "always", "only-when-needed", "never" (default: "only-when-needed")
/// - `extra-required`: list of patterns that always need quotes (default: [])
/// - `extra-allowed`: list of patterns where quotes are optional (default: [])
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{rules::QuotedStringsRule, rules::LintRule, LintConfig};
/// use fast_yaml_core::Parser;
///
/// let rule = QuotedStringsRule;
/// let yaml = "name: 'John'";
/// let value = Parser::parse_str(yaml).unwrap().unwrap();
///
/// let config = LintConfig::default();
/// let context = fast_yaml_linter::LintContext::new(yaml);
/// let diagnostics = rule.check(&context, &value, &config);
/// assert!(diagnostics.is_empty());
/// ```
pub struct QuotedStringsRule;

/// Tracks whether the next scalar in a mapping scope is a key or a value.
enum ScopeKind {
    /// Inside a mapping; `expecting_key` alternates after each key/value.
    Mapping { expecting_key: bool },
    /// Inside a sequence; no key/value distinction.
    Sequence,
}

impl super::LintRule for QuotedStringsRule {
    fn code(&self) -> &str {
        DiagnosticCode::QUOTED_STRINGS
    }

    fn name(&self) -> &'static str {
        "Quoted Strings"
    }

    fn description(&self) -> &'static str {
        "Validates string quoting style (quote-type, required)"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, context: &LintContext, _value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        let source = context.source();
        let rule_config = config.get_rule_config(self.code());

        let quote_type = rule_config
            .and_then(|rc| rc.options.get_string("quote-type"))
            .unwrap_or("any");

        let required = rule_config
            .and_then(|rc| rc.options.get_string("required"))
            .unwrap_or("only-when-needed");

        let extra_required = rule_config
            .and_then(|rc| rc.options.get_string_list("extra-required"))
            .map(std::borrow::ToOwned::to_owned)
            .unwrap_or_default();

        let extra_allowed = rule_config
            .and_then(|rc| rc.options.get_string_list("extra-allowed"))
            .map(std::borrow::ToOwned::to_owned)
            .unwrap_or_default();

        let mut diagnostics = Vec::new();
        let mut scopes: Vec<ScopeKind> = Vec::new();

        let input = BufferedInput::new(source.chars());
        let mut parser = SaphyrParser::new(input);

        while let Some(Ok(ev)) = parser.next_event() {
            let (event, span) = ev;

            match event {
                Event::MappingStart(..) => {
                    scopes.push(ScopeKind::Mapping {
                        expecting_key: true,
                    });
                }
                Event::SequenceStart(..) => {
                    scopes.push(ScopeKind::Sequence);
                }
                Event::MappingEnd | Event::SequenceEnd => {
                    scopes.pop();
                    Self::advance_parent_to_key(&mut scopes);
                }
                Event::Scalar(ref value, style, ..) => {
                    let is_key = matches!(
                        scopes.last(),
                        Some(ScopeKind::Mapping {
                            expecting_key: true
                        })
                    );

                    // Advance scope: after a key comes a value, after a value comes next key.
                    if let Some(ScopeKind::Mapping { expecting_key }) = scopes.last_mut() {
                        *expecting_key = !*expecting_key;
                    }

                    // Build source location from parser marker.
                    // saphyr line() is 1-indexed; col() is 0-indexed.
                    let line = span.start.line();
                    let col = span.start.col();
                    let scalar_offset = context.source_context().get_line_offset(line) + col;

                    self.check_scalar(
                        source,
                        context.source_context(),
                        config,
                        &mut diagnostics,
                        value,
                        style,
                        is_key,
                        line,
                        col,
                        scalar_offset,
                        quote_type,
                        required,
                        &extra_required,
                        &extra_allowed,
                    );
                }

                _ => {}
            }
        }

        diagnostics
    }
}

impl QuotedStringsRule {
    /// Advances the innermost mapping scope to expect a key after a nested structure ends.
    const fn advance_parent_to_key(scopes: &mut [ScopeKind]) {
        if let Some(ScopeKind::Mapping { expecting_key }) = scopes.last_mut() {
            *expecting_key = true;
        }
    }

    /// Checks a single scalar event and appends diagnostics as needed.
    #[allow(clippy::too_many_arguments)]
    fn check_scalar(
        &self,
        source: &str,
        source_ctx: &SourceContext<'_>,
        config: &LintConfig,
        diagnostics: &mut Vec<Diagnostic>,
        value: &str,
        style: ScalarStyle,
        is_key: bool,
        line: usize,
        col: usize,
        scalar_offset: usize,
        quote_type: &str,
        required: &str,
        extra_required: &[String],
        extra_allowed: &[String],
    ) {
        match style {
            ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted => {
                let quote_char = if style == ScalarStyle::SingleQuoted {
                    '\''
                } else {
                    '"'
                };
                // value.len() + 2 for the surrounding quote characters.
                let scalar_span = Self::make_span(line, col, scalar_offset, value.len() + 2);

                if quote_type == "single" && quote_char == '"' {
                    let severity =
                        config.get_effective_severity(self.code(), self.default_severity());
                    diagnostics.push(
                        DiagnosticBuilder::new(
                            self.code(),
                            severity,
                            "string should use single quotes",
                            scalar_span,
                        )
                        .build_with_context(source_ctx),
                    );
                } else if quote_type == "double" && quote_char == '\'' {
                    let severity =
                        config.get_effective_severity(self.code(), self.default_severity());
                    diagnostics.push(
                        DiagnosticBuilder::new(
                            self.code(),
                            severity,
                            "string should use double quotes",
                            scalar_span,
                        )
                        .build_with_context(source_ctx),
                    );
                }

                if required == "only-when-needed" {
                    let has_escape = style == ScalarStyle::DoubleQuoted
                        && (Self::has_yaml_escape(value)
                            || Self::has_source_unicode_hex_escape(source, scalar_offset));
                    let needs = has_escape
                        || Self::needs_quotes(value)
                        || extra_required.iter().any(|p| value.contains(p.as_str()));
                    if !needs {
                        let severity =
                            config.get_effective_severity(self.code(), self.default_severity());
                        diagnostics.push(
                            DiagnosticBuilder::new(
                                self.code(),
                                severity,
                                "string does not need quotes",
                                scalar_span,
                            )
                            .build_with_context(source_ctx),
                        );
                    }
                } else if required == "never" {
                    let severity =
                        config.get_effective_severity(self.code(), self.default_severity());
                    diagnostics.push(
                        DiagnosticBuilder::new(
                            self.code(),
                            severity,
                            "string should not be quoted",
                            scalar_span,
                        )
                        .build_with_context(source_ctx),
                    );
                }
            }

            ScalarStyle::Plain => {
                // Plain scalars: only check when required == "always" and not a key.
                if required == "always"
                    && !is_key
                    && !Self::is_scalar_literal(value)
                    && !extra_allowed.iter().any(|p| value.contains(p.as_str()))
                {
                    let scalar_span = Self::make_span(line, col, scalar_offset, value.len());
                    let severity =
                        config.get_effective_severity(self.code(), self.default_severity());
                    diagnostics.push(
                        DiagnosticBuilder::new(
                            self.code(),
                            severity,
                            "string should be quoted",
                            scalar_span,
                        )
                        .build_with_context(source_ctx),
                    );
                }
            }

            // Literal and folded block scalars are intentional; skip.
            _ => {}
        }
    }

    /// Builds a [`Span`] covering `len` bytes starting at `offset` on `line`.
    ///
    /// `col` is 0-indexed (as returned by saphyr); `Location` column is 1-indexed.
    const fn make_span(line: usize, col: usize, offset: usize, len: usize) -> Span {
        Span::new(
            Location::new(line, col + 1, offset),
            Location::new(line, col + 1 + len, offset + len),
        )
    }

    /// Returns `true` if a double-quoted scalar's decoded value indicates it required escape
    /// sequences in the source.
    ///
    /// Saphyr provides the already-decoded scalar value. A double-quoted string that contained
    /// YAML escape sequences (`\n`, `\t`, `\r`, `\\`, `\"`, `\uXXXX`, etc.) will decode to
    /// a string that either contains a backslash (from `\\`) or contains control characters
    /// (from `\n`, `\t`, etc.). In either case, removing the quotes would produce a different
    /// value, so the quotes are necessary.
    fn has_yaml_escape(value: &str) -> bool {
        value.contains('\\')
            || value.chars().any(|c| {
                matches!(
                    c,
                    '\n' | '\r' | '\t' | '\x00' | '\x07' | '\x08' | '\x0C' | '\x0B' | '\x1B'
                )
            })
    }

    /// Returns `true` if the raw double-quoted scalar in `source` at byte offset `start`
    /// contains a `\u`, `\U`, or `\x` escape sequence.
    ///
    /// These escape sequences decode to Unicode/ASCII characters whose decoded form is
    /// indistinguishable from plain text, so `has_yaml_escape` (which operates on the decoded
    /// value) cannot detect them. We must inspect the raw source instead.
    fn has_source_unicode_hex_escape(source: &str, start: usize) -> bool {
        let bytes = source.as_bytes();
        if start >= bytes.len() || bytes[start] != b'"' {
            return false;
        }
        let mut i = start + 1;
        while i < bytes.len() {
            match bytes[i] {
                b'"' => return false,
                b'\\' => {
                    if i + 1 >= bytes.len() {
                        return false;
                    }
                    if matches!(bytes[i + 1], b'u' | b'U' | b'x') {
                        return true;
                    }
                    i += 2;
                }
                _ => i += 1,
            }
        }
        false
    }

    /// Checks if a string needs quotes based on YAML syntax rules.
    fn needs_quotes(s: &str) -> bool {
        // Empty strings need quotes
        if s.is_empty() {
            return true;
        }

        // Strings that could be interpreted as special values need quotes
        let special_values = [
            "true", "false", "True", "False", "TRUE", "FALSE", "yes", "no", "Yes", "No", "YES",
            "NO", "on", "off", "On", "Off", "ON", "OFF", "null", "Null", "NULL", "~",
        ];

        if special_values.contains(&s) {
            return true;
        }

        // Numbers need quotes to be treated as strings
        if s.parse::<f64>().is_ok() {
            return true;
        }

        // Strings starting with special chars need quotes
        let first_char = s.chars().next().unwrap_or('\0');
        if matches!(
            first_char,
            '@' | '`'
                | '|'
                | '>'
                | '%'
                | '*'
                | '&'
                | '!'
                | '['
                | ']'
                | '{'
                | '}'
                | '#'
                | ':'
                | '-'
                | '?'
                | ','
        ) {
            return true;
        }

        // Strings with colons or hash signs need quotes
        if s.contains(':') || s.contains('#') {
            return true;
        }

        // Strings containing glob/template/cron special characters conventionally
        // benefit from quoting even when YAML could parse them unquoted. Flagging
        // them as "does not need quotes" produces false positives on real-world
        // YAML (GitHub Actions, Helm charts, Kubernetes manifests, cron schedules).
        if s.contains('*')
            || s.contains('?')
            || s.contains('{')
            || s.contains('}')
            || s.contains('[')
            || s.contains(']')
        {
            return true;
        }

        false
    }

    /// Checks if a token is a scalar literal (number, boolean, null).
    fn is_scalar_literal(s: &str) -> bool {
        // Boolean values
        if matches!(s, "true" | "false") {
            return true;
        }

        // Null values
        if matches!(s, "null" | "~") {
            return true;
        }

        // Numeric values
        s.parse::<f64>().is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::RuleConfig, rules::LintRule};
    use fast_yaml_core::Parser;

    #[test]
    fn test_quoted_strings_any_type() {
        let yaml = "name: 'John'\ncity: \"NYC\"";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // Both quotes should be flagged as unnecessary in only-when-needed mode
        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn test_quoted_strings_single_only() {
        let yaml = "name: \"John\"";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = LintConfig::new().with_rule_config(
            "quoted-strings",
            RuleConfig::new().with_option("quote-type", "single"),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("single quotes"));
    }

    #[test]
    fn test_quoted_strings_double_only() {
        let yaml = "name: 'John'";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = LintConfig::new().with_rule_config(
            "quoted-strings",
            RuleConfig::new().with_option("quote-type", "double"),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("double quotes"));
    }

    #[test]
    fn test_quoted_strings_only_when_needed() {
        let yaml = "name: 'simple'";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("does not need quotes"));
    }

    #[test]
    fn test_quoted_strings_needed_for_special_values() {
        let yaml = "value: 'true'\nnumber: '123'";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // These should not be flagged as they need quotes
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_quoted_strings_always() {
        let yaml = "name: John\nage: 30";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = LintConfig::new().with_rule_config(
            "quoted-strings",
            RuleConfig::new().with_option("required", "always"),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // "John" should be flagged (not age: 30, it's a number)
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("should be quoted"));
    }

    #[test]
    fn test_quoted_strings_never() {
        let yaml = "name: 'John'";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = LintConfig::new().with_rule_config(
            "quoted-strings",
            RuleConfig::new().with_option("required", "never"),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("should not be quoted"));
    }

    #[test]
    fn test_quoted_strings_extra_required() {
        let yaml = "command: run-script";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = LintConfig::new().with_rule_config(
            "quoted-strings",
            RuleConfig::new().with_option("extra-required", vec!["-".to_string()]),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // Should not flag as unnecessary because it contains '-'
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_quoted_strings_with_colon() {
        let yaml = "url: 'http://example.com'";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // Quotes are needed because of the colon
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_quoted_strings_needs_quotes() {
        assert!(QuotedStringsRule::needs_quotes(""));
        assert!(QuotedStringsRule::needs_quotes("true"));
        assert!(QuotedStringsRule::needs_quotes("123"));
        assert!(QuotedStringsRule::needs_quotes("http://example.com"));
        assert!(QuotedStringsRule::needs_quotes("#comment"));

        assert!(!QuotedStringsRule::needs_quotes("simple"));
        assert!(!QuotedStringsRule::needs_quotes("hello_world"));
    }

    // Regression tests for issue #175: false positive on double-quoted strings with escapes.

    #[test]
    fn test_no_false_positive_escape_newline() {
        // "\n" is a newline escape — removing quotes would produce a literal 'n', not a newline.
        let yaml = "message: \"line1\\nline2\"";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(
            diagnostics.is_empty(),
            "expected no diagnostics for double-quoted string with \\n escape, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_no_false_positive_escape_tab() {
        let yaml = "data: \"col1\\tcol2\"";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(
            diagnostics.is_empty(),
            "expected no diagnostics for double-quoted string with \\t escape, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_no_false_positive_escape_backslash() {
        let yaml = r#"path: "C:\\Users\\foo""#;
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(
            diagnostics.is_empty(),
            "expected no diagnostics for double-quoted string with \\\\ escape, got: {diagnostics:?}"
        );
    }

    // Regression tests for issue #113: false positives on quotes inside plain scalars.

    #[test]
    fn test_no_false_positive_double_quotes_in_plain_scalar() {
        // The value `echo "hello"` is a plain scalar; the " chars are literal content.
        let yaml = r#"run: echo "hello""#;
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(
            diagnostics.is_empty(),
            "expected no diagnostics for plain scalar with embedded double quotes, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_no_false_positive_single_quotes_in_plain_scalar() {
        // The value `${{ github.event_name == 'push' }}` is a plain scalar.
        let yaml = "if: ${{ github.event_name == 'push' }}";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(
            diagnostics.is_empty(),
            "expected no diagnostics for plain scalar with embedded single quotes, got: {diagnostics:?}"
        );
    }

    // Regression tests for issue #153: incorrect column and offset in diagnostics.

    #[test]
    fn test_diagnostic_location_value_after_key() {
        // `key: "unnecessary"` — the quoted value starts at column 6 (1-indexed), offset 5.
        let yaml = r#"key: "unnecessary""#;
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(
            !diagnostics.is_empty(),
            "expected at least one diagnostic for unnecessarily quoted value"
        );
        let span = diagnostics[0].span;
        assert_eq!(
            span.start.column, 6,
            "expected column 6 for quoted value, got {}",
            span.start.column
        );
        assert_eq!(
            span.start.offset, 5,
            "expected offset 5 for quoted value, got {}",
            span.start.offset
        );
    }

    #[test]
    fn test_diagnostic_location_value_at_line_start() {
        // A quoted value at the start of a sequence: `- "val"` — value starts at column 3, offset 2.
        // Use a plain sequence to get a quoted scalar at a known offset.
        let yaml = "- \"val\"";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(
            !diagnostics.is_empty(),
            "expected diagnostic for unnecessarily quoted sequence value"
        );
        let span = diagnostics[0].span;
        assert_eq!(
            span.start.column, 3,
            "expected column 3 for quoted value after '- ', got {}",
            span.start.column
        );
        assert_eq!(
            span.start.offset, 2,
            "expected offset 2 for quoted value after '- ', got {}",
            span.start.offset
        );
    }

    // Regression tests for issue #182: false positives on unicode/hex escape sequences.

    #[test]
    fn test_no_false_positive_unicode_escape_u4() {
        // "\u0041BC" decodes to "ABC" — without quotes it becomes literal "\u0041BC"
        let yaml = r#"key: "\u0041BC""#;
        let value = Parser::parse_str(yaml).unwrap().unwrap();
        let rule = QuotedStringsRule;
        let config = LintConfig::default();
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(
            diagnostics.is_empty(),
            "expected no diagnostics for \\u escape, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_no_false_positive_unicode_escape_u8() {
        // "\U00000041BC" decodes to "ABC" — without quotes it becomes literal
        let yaml = r#"key: "\U00000041BC""#;
        let value = Parser::parse_str(yaml).unwrap().unwrap();
        let rule = QuotedStringsRule;
        let config = LintConfig::default();
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(
            diagnostics.is_empty(),
            "expected no diagnostics for \\U escape, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_no_false_positive_hex_escape() {
        // "\x41BC" decodes to "ABC" — without quotes it becomes literal "\x41BC"
        let yaml = r#"key: "\x41BC""#;
        let value = Parser::parse_str(yaml).unwrap().unwrap();
        let rule = QuotedStringsRule;
        let config = LintConfig::default();
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(
            diagnostics.is_empty(),
            "expected no diagnostics for \\x escape, got: {diagnostics:?}"
        );
    }
}
