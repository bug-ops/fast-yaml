//! Rule to check quoted string style.

use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Location, Severity,
    Span,
};
use fast_yaml_core::Value;

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

    #[allow(clippy::too_many_lines)]
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

        // Use cached lines and metadata from context
        let lines = context.lines();
        let line_metadata = context.line_metadata();

        for (line_idx, (line, metadata)) in lines.iter().zip(line_metadata).enumerate() {
            let line_num = line_idx + 1;
            let line_offset = context.source_context().get_line_offset(line_num);

            // Skip comment lines using cached metadata
            if metadata.is_comment {
                continue;
            }

            // Find quoted strings in the line
            // Use iterator directly instead of collecting into Vec
            let mut chars_iter = line.char_indices();

            while let Some((byte_idx, ch)) = chars_iter.next() {
                if ch == '\'' || ch == '"' {
                    let quote_char = ch;
                    let start_byte_idx = byte_idx;

                    // Find the closing quote
                    let mut end_byte_idx = start_byte_idx;
                    let mut escaped = false;

                    for (curr_byte_idx, c) in chars_iter.by_ref() {
                        if escaped {
                            escaped = false;
                            continue;
                        }

                        if c == '\\' {
                            escaped = true;
                            continue;
                        }

                        if c == quote_char {
                            end_byte_idx = curr_byte_idx;
                            break;
                        }
                    }

                    if end_byte_idx > start_byte_idx {
                        let string_content = &line[start_byte_idx + 1..end_byte_idx];

                        // Check quote type
                        if quote_type == "single" && quote_char == '"' {
                            let severity =
                                config.get_effective_severity(self.code(), self.default_severity());
                            let offset = line_offset + start_byte_idx;
                            let span_len = end_byte_idx - start_byte_idx + 1;

                            let location = Location::new(line_num, 1, offset);
                            let span =
                                Span::new(location, Location::new(line_num, 1, offset + span_len));

                            diagnostics.push(
                                DiagnosticBuilder::new(
                                    self.code(),
                                    severity,
                                    "string should use single quotes",
                                    span,
                                )
                                .build(source),
                            );
                        } else if quote_type == "double" && quote_char == '\'' {
                            let severity =
                                config.get_effective_severity(self.code(), self.default_severity());
                            let offset = line_offset + start_byte_idx;
                            let span_len = end_byte_idx - start_byte_idx + 1;

                            let location = Location::new(line_num, 1, offset);
                            let span =
                                Span::new(location, Location::new(line_num, 1, offset + span_len));

                            diagnostics.push(
                                DiagnosticBuilder::new(
                                    self.code(),
                                    severity,
                                    "string should use double quotes",
                                    span,
                                )
                                .build(source),
                            );
                        }

                        // Check if quotes are needed
                        if required == "only-when-needed" {
                            let needs_quotes = Self::needs_quotes(string_content);

                            if !needs_quotes
                                && !extra_required
                                    .iter()
                                    .any(|p| string_content.contains(p.as_str()))
                            {
                                let severity = config
                                    .get_effective_severity(self.code(), self.default_severity());
                                let offset = line_offset + start_byte_idx;
                                let span_len = end_byte_idx - start_byte_idx + 1;

                                let location = Location::new(line_num, 1, offset);
                                let span = Span::new(
                                    location,
                                    Location::new(line_num, 1, offset + span_len),
                                );

                                diagnostics.push(
                                    DiagnosticBuilder::new(
                                        self.code(),
                                        severity,
                                        "string does not need quotes",
                                        span,
                                    )
                                    .build(source),
                                );
                            }
                        } else if required == "never" {
                            let severity =
                                config.get_effective_severity(self.code(), self.default_severity());
                            let offset = line_offset + start_byte_idx;
                            let span_len = end_byte_idx - start_byte_idx + 1;

                            let location = Location::new(line_num, 1, offset);
                            let span =
                                Span::new(location, Location::new(line_num, 1, offset + span_len));

                            diagnostics.push(
                                DiagnosticBuilder::new(
                                    self.code(),
                                    severity,
                                    "string should not be quoted",
                                    span,
                                )
                                .build(source),
                            );
                        }
                    }
                }
            }

            // Check unquoted strings if required == "always"
            if required == "always"
                && let Some(colon_pos) = line.find(':')
            {
                let value_part = &line[colon_pos + 1..];
                let value_trimmed = value_part.trim();

                // Skip if empty, already quoted, or starts with flow collection markers
                if !value_trimmed.is_empty()
                    && !value_trimmed.starts_with('"')
                    && !value_trimmed.starts_with('\'')
                    && !value_trimmed.starts_with('[')
                    && !value_trimmed.starts_with('{')
                    && !value_trimmed.starts_with('&')
                    && !value_trimmed.starts_with('*')
                {
                    // Extract the value token (before any comment)
                    let value_token = value_trimmed
                        .split_whitespace()
                        .next()
                        .and_then(|s| s.split('#').next())
                        .unwrap_or(value_trimmed);

                    // Skip if it's in extra-allowed
                    if extra_allowed
                        .iter()
                        .any(|p| value_token.contains(p.as_str()))
                    {
                        continue;
                    }

                    // Check if this looks like a string value (not a number or boolean)
                    if !Self::is_scalar_literal(value_token) {
                        let value_start = line.find(value_token).unwrap_or(colon_pos + 1);
                        let offset = line_offset + value_start;
                        let severity =
                            config.get_effective_severity(self.code(), self.default_severity());

                        let location = Location::new(line_num, 1, offset);
                        let span = Span::new(
                            location,
                            Location::new(line_num, 1, offset + value_token.len()),
                        );

                        diagnostics.push(
                            DiagnosticBuilder::new(
                                self.code(),
                                severity,
                                "string should be quoted",
                                span,
                            )
                            .build(source),
                        );
                    }
                }
            }
        }

        diagnostics
    }
}

impl QuotedStringsRule {
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
        let first_char = s.chars().next().unwrap();
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

        // Strings with colons or spaces often need quotes
        if s.contains(':') || s.contains('#') {
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
}
