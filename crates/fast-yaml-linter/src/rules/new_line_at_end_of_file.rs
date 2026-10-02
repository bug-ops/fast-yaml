//! Rule to check for newline at end of file.

use super::RuleId;
use crate::config::RuleName;
use crate::{Finding, LintConfig, LintContext, Severity, Span};

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
/// use fast_yaml_linter::{rules::NewLineAtEndOfFileRule, rules::SourceRule, LintConfig};
///
/// let rule = NewLineAtEndOfFileRule;
/// let yaml = "name: John\n";  // Ends with newline - OK
///
/// let diagnostics = rule.diagnose(&fast_yaml_linter::LintContext::new(yaml), &LintConfig::new());
/// assert!(diagnostics.is_empty());
/// ```
pub struct NewLineAtEndOfFileRule;

impl super::LintRule for NewLineAtEndOfFileRule {
    fn id(&self) -> RuleId<'_> {
        RuleId::BuiltIn(RuleName::NewLineAtEndOfFile)
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
}

impl super::SourceRule for NewLineAtEndOfFileRule {
    fn check(&self, context: &LintContext, _config: &LintConfig) -> Vec<Finding> {
        let source = context.source();
        if source.is_empty() {
            return Vec::new();
        }

        if source.ends_with(['\n', '\r']) {
            Vec::new()
        } else {
            let eof = context.source_context().offset_to_location(source.len());

            vec![
                Finding::new("no newline at end of file", Span::new(eof, eof)).with_suggestion(
                    "Add newline",
                    Span::new(eof, eof),
                    Some("\n".to_string()),
                ),
            ]
        }
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
    fn test_newline_present() {
        let yaml = "name: John\n";

        let rule = NewLineAtEndOfFileRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &LintConfig::new());

        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_newline_missing() {
        let yaml = "name: John";

        let rule = NewLineAtEndOfFileRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &LintConfig::new());

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].message, "no newline at end of file");
    }

    #[test]
    fn test_empty_file() {
        let yaml = "";

        let rule = NewLineAtEndOfFileRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &LintConfig::new());

        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_multiple_newlines() {
        let yaml = "name: John\n\n\n";

        let rule = NewLineAtEndOfFileRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &LintConfig::new());

        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_windows_newline() {
        let yaml = "name: John\r\n";

        let rule = NewLineAtEndOfFileRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &LintConfig::new());

        // Ends with \n so it's OK
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_severity_override() {
        let yaml = "name: John";

        let rule = NewLineAtEndOfFileRule;
        let config = config_with_rule(RuleName::NewLineAtEndOfFile, "{severity: error}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Error);
    }
}
