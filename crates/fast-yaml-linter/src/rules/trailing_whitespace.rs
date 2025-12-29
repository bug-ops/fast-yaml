//! Rule to detect trailing whitespace.

use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Location, Severity,
    Span,
};
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

    fn check(
        &self,
        context: &LintContext,
        _value: &Value,
        _config: &LintConfig,
    ) -> Vec<Diagnostic> {
        let source = context.source();
        let mut diagnostics = Vec::new();
        let ctx = context.source_context();

        for line_num in 1..=ctx.line_count() {
            if let Some(line) = ctx.get_line(line_num) {
                // Check for trailing whitespace (excluding final newline)
                let trimmed = line.trim_end();

                if trimmed.len() < line.len() {
                    // Has trailing whitespace
                    let ws_start_col = trimmed.len() + 1;

                    // Calculate byte offset for the start of trailing whitespace
                    let line_start_offset = (1..line_num)
                        .filter_map(|ln| ctx.get_line(ln))
                        .map(|l| l.len() + 1) // +1 for newline
                        .sum::<usize>();

                    let ws_start_offset = line_start_offset + trimmed.len();
                    let ws_end_offset = line_start_offset + line.len();

                    let span = Span::new(
                        Location::new(line_num, ws_start_col, ws_start_offset),
                        Location::new(line_num, line.len() + 1, ws_end_offset),
                    );

                    let diagnostic = DiagnosticBuilder::new(
                        DiagnosticCode::TRAILING_WHITESPACE,
                        Severity::Hint,
                        "trailing whitespace detected".to_string(),
                        span,
                    )
                    .with_suggestion("remove trailing whitespace", span, None)
                    .build(source);

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
    use crate::rules::LintRule;
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
