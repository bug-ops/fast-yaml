//! Rule to check empty lines.

use serde::{Deserialize, Serialize};

use crate::config::{Limit, RuleOptions};
use crate::context::source_lines;
use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Severity, Span,
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
/// use fast_yaml_linter::{rules::EmptyLinesRule, rules::LintRule, LintConfig};
/// use fast_yaml_core::Parser;
///
/// let rule = EmptyLinesRule;
/// let yaml = "key: value\n\nanother: value";
/// let value = Parser::parse_str(yaml).unwrap().unwrap();
///
/// let config = LintConfig::default();
/// let diagnostics = rule.check(&fast_yaml_linter::LintContext::new(yaml), &value, &config);
/// assert!(diagnostics.is_empty());
/// ```
pub struct EmptyLinesRule;

/// Options of the empty-lines rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", default)]
pub struct EmptyLinesOptions {
    /// Maximum consecutive empty lines inside the document.
    pub max: Limit,
    /// Maximum empty lines at the start of the document.
    pub max_start: Limit,
    /// Maximum empty lines at the end of the document.
    pub max_end: Limit,
}

impl Default for EmptyLinesOptions {
    fn default() -> Self {
        Self {
            max: Limit::Max(2),
            max_start: Limit::Max(0),
            max_end: Limit::Max(0),
        }
    }
}

impl RuleOptions for EmptyLinesOptions {}

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
        let options = &config.rules.empty_lines.options;
        let (max, max_start, max_end) = (options.max, options.max_start, options.max_end);

        let mut diagnostics = Vec::new();
        let lines: Vec<(usize, &str)> = source_lines(source).collect();

        if lines.is_empty() {
            return diagnostics;
        }
        let source_context = context.source_context();

        // Track consecutive empty lines
        let mut empty_count = 0;
        let mut empty_start_line = 0;

        for (idx, (_, line)) in lines.iter().enumerate() {
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

                    if limit.exceeded_by(empty_count) {
                        let severity = config
                            .rules
                            .empty_lines
                            .severity_or(self.default_severity());

                        let location =
                            source_context.location_at(source_context.line_start(empty_start_line));
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
                            .build_with_context(context.source_context()),
                        );
                    }

                    empty_count = 0;
                }
            }
        }

        // Check trailing empty lines at end
        if empty_count > 0 && max_end.exceeded_by(empty_count) {
            let severity = config
                .rules
                .empty_lines
                .severity_or(self.default_severity());

            let location = source_context.location_at(source_context.line_start(empty_start_line));
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
                .build_with_context(context.source_context()),
            );
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{RuleName, test_support::config_with_rule},
        rules::LintRule,
    };
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
        let config = config_with_rule(RuleName::EmptyLines, "{max: 5}");

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
        let config = config_with_rule(RuleName::EmptyLines, "{max-start: 2}");

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
        let config = config_with_rule(RuleName::EmptyLines, "{max-end: 3}");

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
        let config = config_with_rule(RuleName::EmptyLines, "{max: 0}");

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
