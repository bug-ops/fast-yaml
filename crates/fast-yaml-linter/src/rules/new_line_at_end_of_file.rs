//! Rule to check for newline at end of file.

use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Severity, Span,
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
/// use fast_yaml_core::Parser;
/// use fast_yaml_linter::{rules::NewLineAtEndOfFileRule, rules::LintRule, LintConfig};
///
/// let rule = NewLineAtEndOfFileRule;
/// let yaml = "name: John\n";  // Ends with newline - OK
/// let value = Parser::parse_str(yaml).unwrap().unwrap();
///
/// let diagnostics = rule.check(&fast_yaml_linter::LintContext::new(yaml), &value, &LintConfig::new());
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

        if source.ends_with(['\n', '\r']) {
            Vec::new()
        } else {
            let severity = config
                .rules
                .new_line_at_end_of_file
                .severity_or(Severity::Info);
            let eof = context.source_context().offset_to_location(source.len());

            vec![
                DiagnosticBuilder::new(
                    self.code(),
                    severity,
                    "no newline at end of file",
                    Span::new(eof, eof),
                )
                .with_suggestion("Add newline", Span::new(eof, eof), Some("\n".to_string()))
                .build_with_context(context.source_context()),
            ]
        }
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
    fn test_newline_present() {
        let yaml = "name: John\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = NewLineAtEndOfFileRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &LintConfig::new());

        assert_eq!(diagnostics, []);
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

        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_multiple_newlines() {
        let yaml = "name: John\n\n\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = NewLineAtEndOfFileRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &LintConfig::new());

        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_windows_newline() {
        let yaml = "name: John\r\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = NewLineAtEndOfFileRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &LintConfig::new());

        // Ends with \n so it's OK
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_severity_override() {
        let yaml = "name: John";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = NewLineAtEndOfFileRule;
        let config = config_with_rule(RuleName::NewLineAtEndOfFile, "{severity: error}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Error);
    }
}
