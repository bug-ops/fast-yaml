//! Rule to detect trailing whitespace.

use super::RuleId;
use crate::config::RuleName;
use crate::{Finding, LintConfig, LintContext, Severity};

/// Rule to detect trailing whitespace.
pub struct TrailingWhitespaceRule;

impl super::LintRule for TrailingWhitespaceRule {
    fn id(&self) -> RuleId<'_> {
        RuleId::BuiltIn(RuleName::TrailingWhitespace)
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
}

impl super::SourceRule for TrailingWhitespaceRule {
    fn check(&self, context: &LintContext, _config: &LintConfig) -> Vec<Finding> {
        let mut diagnostics = Vec::new();
        let ctx = context.source_context();

        for line_num in 1..=ctx.line_count() {
            if let Some(line) = ctx.get_line(line_num) {
                let trimmed = line.trim_end();

                if trimmed.len() < line.len() {
                    let start = ctx.line_start(line_num).add_bytes(trimmed.len());
                    let span = ctx.span_at(start, line.len() - trimmed.len());

                    let diagnostic = Finding::new("trailing whitespace detected".to_string(), span)
                        .with_suggestion("remove trailing whitespace", span, None);

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
        rules::SourceRule,
    };

    #[test]
    fn test_no_trailing_whitespace() {
        let yaml = "key: value\nname: test\nage: 30";

        let rule = TrailingWhitespaceRule;
        let config = LintConfig::default();
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&lint_context, &config);

        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_trailing_space_detected() {
        let yaml = "key: value \nname: test";

        let rule = TrailingWhitespaceRule;
        let config = LintConfig::default();
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&lint_context, &config);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("trailing whitespace"));
    }

    #[test]
    fn test_trailing_tab_detected() {
        let yaml = "key: value\t\nname: test";

        let rule = TrailingWhitespaceRule;
        let config = LintConfig::default();
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&lint_context, &config);

        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn test_multiple_lines_with_trailing_whitespace() {
        let yaml = "key: value  \nname: test \nage: 30";

        let rule = TrailingWhitespaceRule;
        let config = LintConfig::default();
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&lint_context, &config);

        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn test_empty_line_no_trailing_whitespace() {
        let yaml = "key: value\n\nname: test";

        let rule = TrailingWhitespaceRule;
        let config = LintConfig::default();
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&lint_context, &config);

        // Empty lines should not trigger
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_crlf_line_endings_no_false_positive() {
        // CRLF files: \r should not be reported as trailing whitespace.
        let yaml = "key: value\r\nother: val\r\n";

        let rule = TrailingWhitespaceRule;
        let config = LintConfig::default();
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&lint_context, &config);

        assert!(
            diagnostics.is_empty(),
            "CRLF line endings must not trigger trailing-whitespace: {diagnostics:?}"
        );
    }

    #[test]
    fn test_crlf_with_real_trailing_whitespace() {
        // CRLF file that also has genuine trailing spaces — must still report.
        let yaml = "key: value  \r\nother: val\r\n";

        let rule = TrailingWhitespaceRule;
        let config = LintConfig::default();
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&lint_context, &config);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].span.start.line(), 1);
    }

    #[test]
    fn test_severity_override() {
        let yaml = "key: value \nname: test";

        let rule = TrailingWhitespaceRule;
        let config = config_with_rule(RuleName::TrailingWhitespace, "{severity: error}");
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&lint_context, &config);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Error);
    }

    #[test]
    fn test_trailing_whitespace_location() {
        let yaml = "line1: ok\nline2: has_space \nline3: ok";

        let rule = TrailingWhitespaceRule;
        let config = LintConfig::default();
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&lint_context, &config);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].span.start.line(), 2);
    }
}
