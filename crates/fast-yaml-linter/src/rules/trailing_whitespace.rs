//! Rule to detect trailing whitespace.

use crate::{Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Severity};
use fast_yaml_core::Value;

/// Rule to detect trailing whitespace.
pub struct TrailingWhitespaceRule;

impl super::LintRule for TrailingWhitespaceRule {
    fn code(&self) -> &str {
        DiagnosticCode::TRAILING_WHITESPACE
    }

    fn name(&self) -> &'static str {
        "Trailing Whitespace"
    }

    fn description(&self) -> &'static str {
        "Detects trailing whitespace at the end of lines"
    }

    fn default_severity(&self) -> Severity {
        Severity::Hint
    }

    fn check(&self, context: &LintContext, _value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let ctx = context.source_context();

        for line_num in 1..=ctx.line_count() {
            if let Some(line) = ctx.get_line(line_num) {
                let trimmed = line.trim_end();

                if trimmed.len() < line.len() {
                    let start = ctx.line_start(line_num).add_bytes(trimmed.len());
                    let span = ctx.span_at(start, line.len() - trimmed.len());

                    let diagnostic = DiagnosticBuilder::new(
                        DiagnosticCode::TRAILING_WHITESPACE,
                        config
                            .rules
                            .trailing_whitespace
                            .severity_or(self.default_severity()),
                        "trailing whitespace detected".to_string(),
                        span,
                    )
                    .with_suggestion("remove trailing whitespace", span, None)
                    .build_with_context(context.source_context());

                    diagnostics.push(diagnostic);
                }
            }
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
    fn test_no_trailing_whitespace() {
        let yaml = "key: value\nname: test\nage: 30";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = TrailingWhitespaceRule;
        let config = LintConfig::default();
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_trailing_space_detected() {
        let yaml = "key: value \nname: test";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = TrailingWhitespaceRule;
        let config = LintConfig::default();
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("trailing whitespace"));
    }

    #[test]
    fn test_trailing_tab_detected() {
        let yaml = "key: value\t\nname: test";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = TrailingWhitespaceRule;
        let config = LintConfig::default();
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);

        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn test_multiple_lines_with_trailing_whitespace() {
        let yaml = "key: value  \nname: test \nage: 30";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = TrailingWhitespaceRule;
        let config = LintConfig::default();
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);

        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn test_empty_line_no_trailing_whitespace() {
        let yaml = "key: value\n\nname: test";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = TrailingWhitespaceRule;
        let config = LintConfig::default();
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);

        // Empty lines should not trigger
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_crlf_line_endings_no_false_positive() {
        // CRLF files: \r should not be reported as trailing whitespace.
        let yaml = "key: value\r\nother: val\r\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = TrailingWhitespaceRule;
        let config = LintConfig::default();
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);

        assert!(
            diagnostics.is_empty(),
            "CRLF line endings must not trigger trailing-whitespace: {diagnostics:?}"
        );
    }

    #[test]
    fn test_crlf_with_real_trailing_whitespace() {
        // CRLF file that also has genuine trailing spaces — must still report.
        let yaml = "key: value  \r\nother: val\r\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = TrailingWhitespaceRule;
        let config = LintConfig::default();
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].span.start.line, 1);
    }

    #[test]
    fn test_severity_override() {
        let yaml = "key: value \nname: test";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = TrailingWhitespaceRule;
        let config = config_with_rule(RuleName::TrailingWhitespace, "{severity: error}");
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Error);
    }

    #[test]
    fn test_trailing_whitespace_location() {
        let yaml = "line1: ok\nline2: has_space \nline3: ok";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = TrailingWhitespaceRule;
        let config = LintConfig::default();
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].span.start.line, 2);
    }
}
