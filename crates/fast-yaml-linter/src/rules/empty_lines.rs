//! Rule to check empty lines.

use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Location, Severity,
    Span,
};
use fast_yaml_core::Value;

/// Linting rule for empty lines.
///
/// Limits consecutive empty lines in YAML documents:
/// - `max`: Maximum consecutive empty lines anywhere
/// - `max-start`: Maximum empty lines at document start
/// - `max-end`: Maximum empty lines at document end
///
/// Configuration options:
/// - `max`: integer (default: 2)
/// - `max-start`: integer (default: 0)
/// - `max-end`: integer (default: 0)
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{rules::EmptyLinesRule, rules::LintRule, LintConfig, config::RuleConfig};
/// use fast_yaml_core::Parser;
///
/// let rule = EmptyLinesRule;
/// let yaml = "key: value\n\nanother: value";
/// let value = Parser::parse_str(yaml).unwrap().unwrap();
///
/// let config = LintConfig::default();
/// let diagnostics = rule.check(yaml, &value, &config);
/// assert!(diagnostics.is_empty());
/// ```
pub struct EmptyLinesRule;

impl super::LintRule for EmptyLinesRule {
    fn code(&self) -> &str {
        DiagnosticCode::EMPTY_LINES
    }

    fn name(&self) -> &'static str {
        "Empty Lines"
    }

    fn description(&self) -> &'static str {
        "Limits consecutive empty lines in document"
    }

    fn default_severity(&self) -> Severity {
        Severity::Info
    }

    fn check(&self, context: &LintContext, _value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        let source = context.source();
        let rule_config = config.get_rule_config(self.code());
        let max = rule_config
            .and_then(|rc| rc.options.get_int("max"))
            .unwrap_or(2);

        let max_start = rule_config
            .and_then(|rc| rc.options.get_int("max-start"))
            .unwrap_or(0);

        let max_end = rule_config
            .and_then(|rc| rc.options.get_int("max-end"))
            .unwrap_or(0);

        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = source.lines().collect();

        if lines.is_empty() {
            return diagnostics;
        }

        // Track consecutive empty lines
        let mut empty_count = 0;
        let mut empty_start_line = 0;
        let mut offset = 0;

        for (idx, line) in lines.iter().enumerate() {
            let line_num = idx + 1;

            if line.trim().is_empty() {
                if empty_count == 0 {
                    empty_start_line = line_num;
                }
                empty_count += 1;
            } else {
                // Check if we exceeded limits
                if empty_count > 0 {
                    let limit = if empty_start_line == 1 {
                        max_start
                    } else {
                        max
                    };

                    #[allow(
                        clippy::cast_possible_truncation,
                        clippy::cast_possible_wrap,
                        clippy::cast_lossless
                    )]
                    let empty_count_i64 = empty_count as i64;

                    if limit >= 0 && empty_count_i64 > limit {
                        let severity =
                            config.get_effective_severity(self.code(), self.default_severity());

                        let location = Location::new(empty_start_line, 1, offset - empty_count);
                        let span = Span::new(location, location);

                        let position = if empty_start_line == 1 {
                            "at document start"
                        } else {
                            "in document"
                        };

                        diagnostics.push(
                            DiagnosticBuilder::new(
                                self.code(),
                                severity,
                                format!(
                                    "too many consecutive empty lines {position} (expected at most {limit}, found {empty_count})"
                                ),
                                span,
                            )
                            .build(source),
                        );
                    }

                    empty_count = 0;
                }
            }

            offset += line.len() + 1; // +1 for newline
        }

        // Check trailing empty lines at end
        if empty_count > 0 {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_possible_wrap,
                clippy::cast_lossless
            )]
            let empty_count_i64 = empty_count as i64;

            if max_end >= 0 && empty_count_i64 > max_end {
                let severity = config.get_effective_severity(self.code(), self.default_severity());

                let location = Location::new(empty_start_line, 1, offset - empty_count);
                let span = Span::new(location, location);

                diagnostics.push(
                    DiagnosticBuilder::new(
                        self.code(),
                        severity,
                        format!(
                            "too many consecutive empty lines at document end (expected at most {max_end}, found {empty_count})"
                        ),
                        span,
                    )
                    .build(source),
                );
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::RuleConfig, rules::LintRule};
    use fast_yaml_core::Parser;

    #[test]
    fn test_empty_lines_valid() {
        let yaml = "key: value\n\nanother: value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = EmptyLinesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_empty_lines_too_many() {
        let yaml = "key: value\n\n\n\nanother: value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = EmptyLinesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(!diagnostics.is_empty());
        assert!(
            diagnostics[0]
                .message
                .contains("too many consecutive empty lines")
        );
    }

    #[test]
    fn test_empty_lines_custom_max() {
        let yaml = "key: value\n\n\n\nanother: value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = EmptyLinesRule;
        let config = LintConfig::new()
            .with_rule_config("empty-lines", RuleConfig::new().with_option("max", 5i64));

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_empty_lines_at_start() {
        let yaml = "\n\nkey: value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = EmptyLinesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("at document start"));
    }

    #[test]
    fn test_empty_lines_at_start_allowed() {
        let yaml = "\n\nkey: value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = EmptyLinesRule;
        let config = LintConfig::new().with_rule_config(
            "empty-lines",
            RuleConfig::new().with_option("max-start", 2i64),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_empty_lines_at_end() {
        let yaml = "key: value\n\n\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = EmptyLinesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("at document end"));
    }

    #[test]
    fn test_empty_lines_at_end_allowed() {
        let yaml = "key: value\n\n\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = EmptyLinesRule;
        let config = LintConfig::new().with_rule_config(
            "empty-lines",
            RuleConfig::new().with_option("max-end", 3i64),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_empty_lines_no_empty() {
        let yaml = "key: value\nanother: value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = EmptyLinesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_empty_lines_single_empty() {
        let yaml = "key: value\n\nanother: value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = EmptyLinesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_empty_lines_max_zero() {
        let yaml = "key: value\n\nanother: value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = EmptyLinesRule;
        let config = LintConfig::new()
            .with_rule_config("empty-lines", RuleConfig::new().with_option("max", 0i64));

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(!diagnostics.is_empty());
    }

    #[test]
    fn test_empty_lines_multiple_blocks() {
        let yaml = "key1: value1\n\n\n\nkey2: value2\n\n\n\nkey3: value3";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = EmptyLinesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // Should report 2 violations (two blocks with 3 empty lines each)
        assert_eq!(diagnostics.len(), 2);
    }
}
