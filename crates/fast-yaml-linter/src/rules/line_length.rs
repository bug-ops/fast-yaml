//! Rule to check line length limits.

use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, Location, Severity, SourceContext,
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

    fn check(&self, source: &str, _value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        let Some(max_length) = config.max_line_length else {
            return Vec::new();
        };

        let mut diagnostics = Vec::new();
        let ctx = SourceContext::new(source);

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
        let diagnostics = rule.check(yaml, &value, &config);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_no_limit_configured() {
        let yaml = "key: this is a very long line that would normally exceed any reasonable limit but should not trigger warnings";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = LineLengthRule;
        let config = LintConfig::new().with_max_line_length(None);
        let diagnostics = rule.check(yaml, &value, &config);

        assert!(diagnostics.is_empty());
    }
}
