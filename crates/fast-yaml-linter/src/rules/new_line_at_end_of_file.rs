//! Rule to check for newline at end of file.

use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Location, Severity,
    Span,
};
use fast_yaml_core::Value;

/// Linting rule for newline at end of file.
///
/// Requires that files end with a newline character.
///
/// This is a common convention in Unix-like systems and many coding standards.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{rules::NewLineAtEndOfFileRule, rules::LintRule, LintConfig};
///
/// let rule = NewLineAtEndOfFileRule;
/// let yaml = "name: John\n";  // Ends with newline - OK
/// let value = Parser::parse_str(yaml).unwrap().unwrap();
///
/// let diagnostics = rule.check(yaml, &value, &LintConfig::new());
/// assert!(diagnostics.is_empty());
/// ```
pub struct NewLineAtEndOfFileRule;

impl super::LintRule for NewLineAtEndOfFileRule {
    fn code(&self) -> &str {
        DiagnosticCode::NEW_LINE_AT_END_OF_FILE
    }

    fn name(&self) -> &'static str {
        "New Line at End of File"
    }

    fn description(&self) -> &'static str {
        "Requires files to end with a newline character"
    }

    fn default_severity(&self) -> Severity {
        Severity::Info
    }

    fn check(&self, context: &LintContext, _value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        let source = context.source();
        if source.is_empty() {
            return Vec::new();
        }

        if source.ends_with('\n') {
            Vec::new()
        } else {
            let severity = config.get_effective_severity(self.code(), Severity::Info);
            let last_line = source.lines().count().max(1);
            let last_offset = source.len();

            vec![
                DiagnosticBuilder::new(
                    self.code(),
                    severity,
                    "no newline at end of file",
                    Span::new(
                        Location::new(last_line, 1, last_offset),
                        Location::new(last_line, 1, last_offset),
                    ),
                )
                .with_suggestion(
                    "Add newline",
                    Span::new(
                        Location::new(last_line, 1, last_offset),
                        Location::new(last_line, 1, last_offset),
                    ),
                    Some("\n".to_string()),
                )
                .build(source),
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::RuleConfig, rules::LintRule};
    use fast_yaml_core::Parser;

    #[test]
    fn test_newline_present() {
        let yaml = "name: John\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = NewLineAtEndOfFileRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &LintConfig::new());

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_newline_missing() {
        let yaml = "name: John";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = NewLineAtEndOfFileRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &LintConfig::new());

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].message, "no newline at end of file");
    }

    #[test]
    fn test_empty_file() {
        use fast_yaml_core::Parser;

        let yaml = "";
        let value = Parser::parse_str("null").unwrap().unwrap();

        let rule = NewLineAtEndOfFileRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &LintConfig::new());

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_multiple_newlines() {
        let yaml = "name: John\n\n\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = NewLineAtEndOfFileRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &LintConfig::new());

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_windows_newline() {
        let yaml = "name: John\r\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = NewLineAtEndOfFileRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &LintConfig::new());

        // Ends with \n so it's OK
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_severity_override() {
        let yaml = "name: John";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = NewLineAtEndOfFileRule;
        let config = LintConfig::new().with_rule_config(
            "new-line-at-end-of-file",
            RuleConfig::new().with_severity(Severity::Error),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Error);
    }
}
