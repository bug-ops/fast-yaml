//! Rule to check for document end marker (...).

use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Location, Severity,
    Span,
};
use fast_yaml_core::Value;

/// Linting rule for document end marker.
///
/// Requires or allows the YAML document end marker `...`.
///
/// Configuration options:
/// - `present`: bool (default: false) - whether the marker is required
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{rules::DocumentEndRule, rules::LintRule, LintConfig, config::RuleConfig};
///
/// let rule = DocumentEndRule;
/// let yaml = "name: John\n...";
/// let value = Parser::parse_str(yaml).unwrap().unwrap();
///
/// let config = LintConfig::new()
///     .with_rule_config("document-end", RuleConfig::new().with_option("present", true));
///
/// let diagnostics = rule.check(yaml, &value, &config);
/// assert!(diagnostics.is_empty());
/// ```
pub struct DocumentEndRule;

impl super::LintRule for DocumentEndRule {
    fn code(&self) -> &str {
        DiagnosticCode::DOCUMENT_END
    }

    fn name(&self) -> &'static str {
        "Document End"
    }

    fn description(&self) -> &'static str {
        "Requires or allows the YAML document end marker '...'"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, context: &LintContext, _value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        let source = context.source();
        let required = config
            .get_rule_config(self.code())
            .and_then(|rc| rc.options.get_bool("present"))
            .unwrap_or(false);

        if !required {
            return Vec::new();
        }

        if has_document_end_marker(source) {
            Vec::new()
        } else {
            let severity = config.get_effective_severity(self.code(), Severity::Warning);
            let last_line = source.lines().count().max(1);
            let last_offset = source.len();

            vec![
                DiagnosticBuilder::new(
                    self.code(),
                    severity,
                    "missing document end marker '...'",
                    Span::new(
                        Location::new(last_line, 1, last_offset),
                        Location::new(last_line, 1, last_offset),
                    ),
                )
                .with_suggestion(
                    "Add '...' at the end",
                    Span::new(
                        Location::new(last_line, 1, last_offset),
                        Location::new(last_line, 1, last_offset),
                    ),
                    Some("\n...".to_string()),
                )
                .build(source),
            ]
        }
    }
}

fn has_document_end_marker(source: &str) -> bool {
    find_document_end_marker(source).is_some()
}

fn find_document_end_marker(source: &str) -> Option<(usize, Span)> {
    let lines: Vec<&str> = source.lines().collect();

    // Check from the end, skipping empty lines and comments
    for (idx, line) in lines.iter().enumerate().rev() {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed == "..." {
            let line_num = idx + 1;
            let col = line.len() - trimmed.len() + 1;
            let offset: usize = lines.iter().take(idx).map(|l| l.len() + 1).sum();

            return Some((
                line_num,
                Span::new(
                    Location::new(line_num, col, offset + col - 1),
                    Location::new(line_num, col + 3, offset + col + 2),
                ),
            ));
        }

        // Found non-marker content
        break;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::RuleConfig, rules::LintRule};
    use fast_yaml_core::Parser;

    #[test]
    fn test_document_end_required_present() {
        let yaml = "name: John\n...";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = DocumentEndRule;
        let config = LintConfig::new().with_rule_config(
            "document-end",
            RuleConfig::new().with_option("present", true),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_document_end_required_missing() {
        let yaml = "name: John";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = DocumentEndRule;
        let config = LintConfig::new().with_rule_config(
            "document-end",
            RuleConfig::new().with_option("present", true),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].message, "missing document end marker '...'");
    }

    #[test]
    fn test_document_end_not_required() {
        let yaml_with = "name: John\n...";
        let yaml_without = "name: John";

        let rule = DocumentEndRule;
        let config = LintConfig::new(); // Default: not required

        let value_with = Parser::parse_str(yaml_with).unwrap().unwrap();
        let context_with = LintContext::new(yaml_with);
        let diag_with = rule.check(&context_with, &value_with, &config);
        assert!(diag_with.is_empty());

        let value_without = Parser::parse_str(yaml_without).unwrap().unwrap();
        let context_without = LintContext::new(yaml_without);
        let diag_without = rule.check(&context_without, &value_without, &config);
        assert!(diag_without.is_empty());
    }

    #[test]
    fn test_document_end_with_comments_after() {
        let yaml = "name: John\n...\n# comment";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = DocumentEndRule;
        let config = LintConfig::new().with_rule_config(
            "document-end",
            RuleConfig::new().with_option("present", true),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_find_document_end_marker() {
        assert!(find_document_end_marker("test: value\n...").is_some());
        assert!(find_document_end_marker("test: value\n...\n# comment").is_some());
        assert!(find_document_end_marker("test: value\n...\n\n").is_some());
        assert!(find_document_end_marker("test: value").is_none());
        assert!(find_document_end_marker("").is_none());
    }

    #[test]
    fn test_severity_override() {
        let yaml = "name: John";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = DocumentEndRule;
        let config = LintConfig::new().with_rule_config(
            "document-end",
            RuleConfig::new()
                .with_option("present", true)
                .with_severity(Severity::Error),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Error);
    }
}
