//! Rule to check line length limits.

use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Location, Severity,
    Span,
};
use fast_yaml_core::Value;

/// Rule to check line length limits.
pub struct LineLengthRule;

impl super::LintRule for LineLengthRule {
    fn code(&self) -> &str {
        DiagnosticCode::LINE_LENGTH
    }

    fn name(&self) -> &'static str {
        "Line Length"
    }

    fn description(&self) -> &'static str {
        "Checks that lines do not exceed the configured maximum length"
    }

    fn default_severity(&self) -> Severity {
        Severity::Info
    }

    fn check(&self, context: &LintContext, _value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        let source = context.source();
        let Some(max_length) = config.max_line_length else {
            return Vec::new();
        };

        let mut diagnostics = Vec::new();
        let ctx = context.source_context();

        for line_num in 1..=ctx.line_count() {
            if let Some(line_content) = ctx.get_line(line_num) {
                let line_len = line_content.chars().count();
                if line_len > max_length {
                    let line_start = ctx.offset_to_location(
                        ctx.get_snippet(Span::new(
                            Location::new(line_num, 1, 0),
                            Location::new(line_num, 1, 0),
                        ))
                        .as_ptr() as usize
                            - source.as_ptr() as usize,
                    );

                    let span = Span::new(
                        Location::new(line_num, 1, line_start.offset),
                        Location::new(
                            line_num,
                            line_len + 1,
                            line_start.offset + line_content.len(),
                        ),
                    );

                    let diagnostic = DiagnosticBuilder::new(
                        DiagnosticCode::LINE_LENGTH,
                        Severity::Info,
                        format!(
                            "line exceeds maximum length of {max_length} characters (current: {line_len})"
                        ),
                        span,
                    )
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
    fn test_line_within_limit() {
        let yaml = "key: value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = LineLengthRule;
        let config = LintConfig::default();
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_no_limit_configured() {
        let yaml = "key: this is a very long line that would normally exceed any reasonable limit but should not trigger warnings";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = LineLengthRule;
        let config = LintConfig::new().with_max_line_length(None);
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_line_exceeds_limit() {
        let yaml = "key: this is a very long value that definitely exceeds eighty characters without any doubt whatsoever";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = LineLengthRule;
        let config = LintConfig::new().with_max_line_length(Some(80));
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("exceeds maximum length"));
        assert!(diagnostics[0].message.contains("80"));
    }

    #[test]
    fn test_line_at_exact_limit() {
        // This line is exactly 77 characters long
        let yaml = "name: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = LineLengthRule;
        let config = LintConfig::new().with_max_line_length(Some(77));
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);

        // Exactly at limit should not trigger
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_line_one_over_limit() {
        // This line is 78 characters long
        let yaml = "name: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = LineLengthRule;
        let config = LintConfig::new().with_max_line_length(Some(77));
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);

        // One over should trigger
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn test_multiple_long_lines() {
        let yaml = "first: this is a very long line that exceeds the maximum character limit\n\
                    second: another extremely long line that also exceeds the character limit\n\
                    short: ok";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = LineLengthRule;
        let config = LintConfig::new().with_max_line_length(Some(50));
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);

        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn test_utf8_multibyte_characters() {
        // 5 Japanese characters (日本語日本語日本語日本語日本語) + "key: " = ~29 chars
        let yaml = "key: 日本語日本語日本語日本語日本語日本語日本語日本語";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = LineLengthRule;
        let config = LintConfig::new().with_max_line_length(Some(20));
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);

        // Should count characters, not bytes
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn test_empty_lines_ignored() {
        let yaml = "key: value\n\n\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = LineLengthRule;
        let config = LintConfig::new().with_max_line_length(Some(5));
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);

        // Should only report the first line (10 chars), not the empty lines
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn test_diagnostic_location_accuracy() {
        let yaml = "first: ok\nvery_long_key_name: this is a very long value that definitely exceeds fifty chars\nthird: ok";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = LineLengthRule;
        let config = LintConfig::new().with_max_line_length(Some(50));
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].span.start.line, 2); // Second line
    }
}
