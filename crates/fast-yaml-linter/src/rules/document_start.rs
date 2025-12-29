//! Rule to check for document start marker (---).

use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Location, Severity,
    Span,
};
use fast_yaml_core::Value;

/// Linting rule for document start marker.
///
/// Requires, forbids, or allows the YAML document start marker `---`.
///
/// Configuration options:
/// - `present`: "required" | "forbidden" | "allowed" (default: "allowed")
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{rules::DocumentStartRule, rules::LintRule, LintConfig, config::RuleConfig};
///
/// let rule = DocumentStartRule;
/// let yaml = "---\nname: John";
/// let value = Parser::parse_str(yaml).unwrap().unwrap();
///
/// let config = LintConfig::new()
///     .with_rule_config("document-start", RuleConfig::new().with_option("present", "required"));
///
/// let diagnostics = rule.check(yaml, &value, &config);
/// assert!(diagnostics.is_empty());
/// ```
pub struct DocumentStartRule;

impl super::LintRule for DocumentStartRule {
    fn code(&self) -> &str {
        DiagnosticCode::DOCUMENT_START
    }

    fn name(&self) -> &'static str {
        "Document Start"
    }

    fn description(&self) -> &'static str {
        "Requires or forbids the YAML document start marker '---'"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, context: &LintContext, _value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        let source = context.source();
        let presence = config
            .get_rule_config(self.code())
            .and_then(|rc| rc.options.get_string("present"))
            .unwrap_or("allowed");

        match presence {
            "required" => check_required(source, config, self.code()),
            "forbidden" => check_forbidden(source, config, self.code()),
            _ => Vec::new(), // "allowed" = no checks
        }
    }
}

fn check_required(source: &str, config: &LintConfig, code: &str) -> Vec<Diagnostic> {
    if has_document_start_marker(source) {
        Vec::new()
    } else {
        let severity = config.get_effective_severity(code, Severity::Warning);
        vec![
            DiagnosticBuilder::new(
                code,
                severity,
                "missing document start marker '---'",
                Span::new(Location::new(1, 1, 0), Location::new(1, 1, 0)),
            )
            .with_suggestion(
                "Add '---' at the beginning",
                Span::new(Location::new(1, 1, 0), Location::new(1, 1, 0)),
                Some("---\n".to_string()),
            )
            .build(source),
        ]
    }
}

fn check_forbidden(source: &str, config: &LintConfig, code: &str) -> Vec<Diagnostic> {
    if let Some((_line_num, span)) = find_document_start_marker(source) {
        let severity = config.get_effective_severity(code, Severity::Warning);
        vec![
            DiagnosticBuilder::new(
                code,
                severity,
                "document start marker '---' is forbidden",
                span,
            )
            .with_suggestion("Remove '---'", span, None)
            .build(source),
        ]
    } else {
        Vec::new()
    }
}

fn has_document_start_marker(source: &str) -> bool {
    find_document_start_marker(source).is_some()
}

fn find_document_start_marker(source: &str) -> Option<(usize, Span)> {
    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.starts_with("---") {
            let col = line.len() - trimmed.len() + 1;
            let offset = source
                .lines()
                .take(line_num)
                .map(|l| l.len() + 1)
                .sum::<usize>();

            return Some((
                line_num + 1,
                Span::new(
                    Location::new(line_num + 1, col, offset + col - 1),
                    Location::new(line_num + 1, col + 3, offset + col + 2),
                ),
            ));
        }

        // Found content before marker
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
    fn test_document_start_required_present() {
        let yaml = "---\nname: John";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = DocumentStartRule;
        let config = LintConfig::new().with_rule_config(
            "document-start",
            RuleConfig::new().with_option("present", "required"),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_document_start_required_missing() {
        let yaml = "name: John";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = DocumentStartRule;
        let config = LintConfig::new().with_rule_config(
            "document-start",
            RuleConfig::new().with_option("present", "required"),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "missing document start marker '---'"
        );
    }

    #[test]
    fn test_document_start_forbidden() {
        let yaml = "---\nname: John";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = DocumentStartRule;
        let config = LintConfig::new().with_rule_config(
            "document-start",
            RuleConfig::new().with_option("present", "forbidden"),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "document start marker '---' is forbidden"
        );
    }

    #[test]
    fn test_document_start_with_comments() {
        let yaml = "# Comment\n---\nname: John";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = DocumentStartRule;
        let config = LintConfig::new().with_rule_config(
            "document-start",
            RuleConfig::new().with_option("present", "required"),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_document_start_allowed() {
        let yaml_with = "---\nname: John";
        let yaml_without = "name: John";

        let rule = DocumentStartRule;
        let config = LintConfig::new(); // Default is "allowed"

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
    fn test_find_document_start_marker() {
        assert!(find_document_start_marker("---\ntest: value").is_some());
        assert!(find_document_start_marker("# comment\n---\ntest: value").is_some());
        assert!(find_document_start_marker("  ---\ntest: value").is_some());
        assert!(find_document_start_marker("test: value").is_none());
        assert!(find_document_start_marker("").is_none());
    }

    #[test]
    fn test_severity_override() {
        let yaml = "name: John";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = DocumentStartRule;
        let config = LintConfig::new().with_rule_config(
            "document-start",
            RuleConfig::new()
                .with_option("present", "required")
                .with_severity(Severity::Error),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Error);
    }
}
