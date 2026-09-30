//! Rule to check for document start marker (---).

use serde::{Deserialize, Serialize};

use crate::config::{MarkerPresence, RuleOptions};
use crate::context::source_lines;
use crate::source::offset::ByteOffset;
use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Location, Severity,
    SourceContext, Span,
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
/// use fast_yaml_core::Parser;
/// use fast_yaml_linter::{rules::DocumentStartRule, rules::LintRule, LintConfig};
///
/// let rule = DocumentStartRule;
/// let yaml = "---\nname: John";
/// let value = Parser::parse_str(yaml).unwrap().unwrap();
///
/// let config = LintConfig::default();
///
/// let diagnostics = rule.check(&fast_yaml_linter::LintContext::new(yaml), &value, &config);
/// assert!(diagnostics.is_empty());
/// ```
pub struct DocumentStartRule;

/// Options of the document-start rule.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", default)]
pub struct DocumentStartOptions {
    /// Whether `---` is required, forbidden or allowed.
    pub present: MarkerPresence,
}

impl RuleOptions for DocumentStartOptions {}

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
        let source_context = context.source_context();
        match config.rules.document_start.options.present {
            MarkerPresence::Required => check_required(source, source_context, config, self.code()),
            MarkerPresence::Forbidden => {
                check_forbidden(source, source_context, config, self.code())
            }
            MarkerPresence::Allowed => Vec::new(),
        }
    }
}

fn check_required(
    source: &str,
    source_context: &SourceContext<'_>,
    config: &LintConfig,
    code: &str,
) -> Vec<Diagnostic> {
    if has_document_start_marker(source) {
        Vec::new()
    } else {
        let severity = config.rules.document_start.severity_or(Severity::Warning);
        let start_span = source_context.span_at(ByteOffset::ZERO, 0);
        vec![
            DiagnosticBuilder::new(
                code,
                severity,
                "missing document start marker '---'",
                start_span,
            )
            .with_suggestion(
                "Add '---' at the beginning",
                start_span,
                Some("---\n".to_string()),
            )
            .build_with_context(source_context),
        ]
    }
}

fn check_forbidden(
    source: &str,
    source_context: &SourceContext<'_>,
    config: &LintConfig,
    code: &str,
) -> Vec<Diagnostic> {
    if let Some(DocumentStartMarker {
        span,
        after_directive: false,
    }) = find_document_start_marker(source)
    {
        let severity = config.rules.document_start.severity_or(Severity::Warning);
        vec![
            DiagnosticBuilder::new(
                code,
                severity,
                "document start marker '---' is forbidden",
                span,
            )
            .with_suggestion("Remove '---'", span, None)
            .build_with_context(source_context),
        ]
    } else {
        Vec::new()
    }
}

fn has_document_start_marker(source: &str) -> bool {
    find_document_start_marker(source).is_some()
}

/// Location of a `---` marker; `after_directive` marks markers the spec makes mandatory.
struct DocumentStartMarker {
    span: Span,
    after_directive: bool,
}

fn find_document_start_marker(source: &str) -> Option<DocumentStartMarker> {
    let mut after_directive = false;
    for (line_num, (offset, line)) in source_lines(source).enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if line.starts_with('%') {
            after_directive = true;
            continue;
        }

        if trimmed.starts_with("---") {
            let col = line.len() - trimmed.len() + 1;
            return Some(DocumentStartMarker {
                span: Span::new(
                    Location::new(line_num + 1, col, offset + col - 1),
                    Location::new(line_num + 1, col + 3, offset + col + 2),
                ),
                after_directive,
            });
        }

        break;
    }

    None
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
    fn test_document_start_required_present() {
        let yaml = "---\nname: John";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = DocumentStartRule;
        let config = config_with_rule(RuleName::DocumentStart, "{present: required}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_document_start_required_missing() {
        let yaml = "name: John";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = DocumentStartRule;
        let config = config_with_rule(RuleName::DocumentStart, "{present: required}");

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
        let config = config_with_rule(RuleName::DocumentStart, "{present: forbidden}");

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
        let config = config_with_rule(RuleName::DocumentStart, "{present: required}");

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

    const REQUIRED: &str = "{present: required}";
    const FORBIDDEN: &str = "{present: forbidden}";

    fn count(yaml: &str, cfg: &str) -> usize {
        let value = Parser::parse_str("a: 1").unwrap().unwrap();
        let config = config_with_rule(RuleName::DocumentStart, cfg);
        DocumentStartRule
            .check(&LintContext::new(yaml), &value, &config)
            .len()
    }

    #[test]
    fn test_directive_before_marker() {
        assert_eq!(count("%YAML 1.2\n---\na: 1\n", REQUIRED), 0);
        assert_eq!(count("%TAG ! tag:x,2000:\n---\na: 1\n", REQUIRED), 0);
        assert_eq!(count("# c\n%YAML 1.2\n---\na: 1\n", REQUIRED), 0);
        assert_eq!(count("%YAML 1.2\na: 1\n", REQUIRED), 1);
        assert_eq!(count("%YAML 1.2\n---\na: 1\n", FORBIDDEN), 0);
        assert_eq!(count("---\na: 1\n", FORBIDDEN), 1);
    }

    #[test]
    fn test_directives_yaml_and_tag_together() {
        let yaml = "%YAML 1.2\n%TAG !e! tag:example.com,2000:\n---\na: 1\n";
        assert_eq!(count(yaml, REQUIRED), 0);
        assert_eq!(count(yaml, FORBIDDEN), 0);
    }

    #[test]
    fn test_directives_with_crlf() {
        let yaml = "%YAML 1.2\r\n%TAG !e! tag:x,2000:\r\n---\r\na: 1\r\n";
        assert_eq!(count(yaml, REQUIRED), 0);
        assert_eq!(count(yaml, FORBIDDEN), 0);
        assert_eq!(count("---\r\na: 1\r\n", FORBIDDEN), 1);
    }

    #[test]
    fn test_indented_percent_line_is_not_a_directive() {
        assert_eq!(count("  %YAML 1.2\n---\na: 1\n", REQUIRED), 1);
        assert_eq!(count("  %YAML 1.2\n---\na: 1\n", FORBIDDEN), 0);
    }

    #[test]
    fn test_comment_only_file_with_required() {
        assert_eq!(count("# only a comment\n", REQUIRED), 1);
        assert_eq!(count("# only a comment\n", FORBIDDEN), 0);
    }

    #[test]
    fn test_marker_found_after_directive_reports_line_and_span() {
        let marker = find_document_start_marker("%YAML 1.2\n---\na: 1\n").unwrap();
        assert!(marker.after_directive);
        assert_eq!(marker.span.start.line, 2);
        assert_eq!(marker.span.start.column, 1);
        let plain = find_document_start_marker("---\na: 1\n").unwrap();
        assert!(!plain.after_directive);
    }

    #[test]
    fn test_bom_before_marker_is_not_skipped() {
        let with_bom = "\u{feff}---\na: 1\n";
        assert!(find_document_start_marker(with_bom).is_none());
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
        let config = config_with_rule(
            RuleName::DocumentStart,
            "{present: required, severity: error}",
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Error);
    }
}
