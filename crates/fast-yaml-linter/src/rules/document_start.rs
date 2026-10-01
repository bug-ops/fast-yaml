//! Rule to check for document start marker (---).

use serde::{Deserialize, Serialize};

use crate::config::{MarkerPresence, RuleOptions};
use crate::scan::DocumentStart;
use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Severity, Span,
};
use fast_yaml_core::Value;

/// Linting rule for document start marker.
///
/// Requires, forbids, or allows the YAML document start marker `---`, in every document of the
/// stream: `required` flags each document that starts without it (also after `...`), `forbidden`
/// flags each `---`, also one that follows a `%` directive (yamllint does the same). A source
/// without any document (empty or comment-only) is never reported.
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
        let severity = config.rules.document_start.severity_or(Severity::Warning);
        if context.documents().is_empty() && context.scan_is_complete() {
            return Vec::new();
        }
        let documents = context.document_markers();
        match config.rules.document_start.options.present {
            MarkerPresence::Required => documents
                .iter()
                .filter_map(|document| match document.start {
                    DocumentStart::Implicit(first_token) => {
                        Some(missing(context, severity, first_token))
                    }
                    DocumentStart::Explicit(_) => None,
                })
                .collect(),
            MarkerPresence::Forbidden => documents
                .iter()
                .filter_map(|document| document.start.marker())
                .map(|span| forbidden(context, severity, span))
                .collect(),
            MarkerPresence::Allowed => Vec::new(),
        }
    }
}

/// Reports a document that starts without `---`, at the line of its first token.
fn missing(context: &LintContext<'_>, severity: Severity, first_token: Span) -> Diagnostic {
    let source_context = context.source_context();
    let span = source_context.span_at(source_context.line_start(first_token.start.line), 0);
    DiagnosticBuilder::new(
        DiagnosticCode::DOCUMENT_START,
        severity,
        "missing document start marker '---'",
        span,
    )
    .with_suggestion(
        "Add '---' before this document",
        span,
        Some("---\n".to_string()),
    )
    .build_with_context(source_context)
}

fn forbidden(context: &LintContext<'_>, severity: Severity, span: Span) -> Diagnostic {
    DiagnosticBuilder::new(
        DiagnosticCode::DOCUMENT_START,
        severity,
        "document start marker '---' is forbidden",
        span,
    )
    .with_suggestion("Remove '---'", span, None)
    .build_with_context(context.source_context())
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
        assert_eq!(diagnostics, []);
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
        assert_eq!(diagnostics, []);
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
        assert_eq!(diag_with, []);

        let value_without = Parser::parse_str(yaml_without).unwrap().unwrap();
        let context_without = LintContext::new(yaml_without);
        let diag_without = rule.check(&context_without, &value_without, &config);
        assert_eq!(diag_without, []);
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
        assert_eq!(count("%YAML 1.2\n---\na: 1\n", FORBIDDEN), 1);
        assert_eq!(count("---\na: 1\n", FORBIDDEN), 1);
    }

    #[test]
    fn test_directives_yaml_and_tag_together() {
        let yaml = "%YAML 1.2\n%TAG !e! tag:example.com,2000:\n---\na: 1\n";
        assert_eq!(count(yaml, REQUIRED), 0);
        assert_eq!(count(yaml, FORBIDDEN), 1);
    }

    #[test]
    fn test_directives_with_crlf() {
        let yaml = "%YAML 1.2\r\n%TAG !e! tag:x,2000:\r\n---\r\na: 1\r\n";
        assert_eq!(count(yaml, REQUIRED), 0);
        assert_eq!(count(yaml, FORBIDDEN), 1);
        assert_eq!(count("---\r\na: 1\r\n", FORBIDDEN), 1);
    }

    #[test]
    fn test_indented_percent_line_is_not_a_directive() {
        assert_eq!(count("  %YAML 1.2\n---\na: 1\n", REQUIRED), 1);
        assert_eq!(count("  %YAML 1.2\n---\na: 1\n", FORBIDDEN), 0);
    }

    #[test]
    fn test_comment_only_file_with_required() {
        assert_eq!(count("# only a comment\n", REQUIRED), 0);
        assert_eq!(count("# only a comment\n", FORBIDDEN), 0);
        assert_eq!(count("", REQUIRED), 0);
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

    fn diagnostics(yaml: &str, cfg: &str) -> Vec<Diagnostic> {
        let value = Parser::parse_str("a: 1").unwrap().unwrap();
        let config = config_with_rule(RuleName::DocumentStart, cfg);
        DocumentStartRule.check(&LintContext::new(yaml), &value, &config)
    }

    fn lines(yaml: &str, cfg: &str) -> Vec<usize> {
        diagnostics(yaml, cfg)
            .iter()
            .map(|d| d.span.start.line)
            .collect()
    }

    #[test]
    fn test_required_checks_every_document() {
        assert_eq!(
            lines("---\na: 1\n---\nb: 2\n", REQUIRED),
            Vec::<usize>::new()
        );
        assert_eq!(lines("a: 1\n---\nb: 2\n", REQUIRED), [1]);
        assert_eq!(lines("---\na: 1\n...\nb: 2\n", REQUIRED), [4]);
        assert_eq!(lines("a: 1\n...\n# c\nb: 2\n", REQUIRED), [1, 4]);
    }

    #[test]
    fn test_required_suggestion_inserts_marker_at_the_document_line() {
        let found = diagnostics("---\na: 1\n...\n# c\nb: 2\n", REQUIRED);
        assert_eq!(found.len(), 1);
        let suggestion = &found[0].suggestions[0];
        assert_eq!(suggestion.replacement.as_deref(), Some("---\n"));
        assert_eq!(suggestion.span.start.offset, "---\na: 1\n...\n# c\n".len());
        assert_eq!(suggestion.span.start.offset, suggestion.span.end.offset);
    }

    #[test]
    fn test_forbidden_flags_every_marker() {
        assert_eq!(lines("---\na: 1\n---\nb: 2\n", FORBIDDEN), [1, 3]);
        assert_eq!(lines("a: 1\n---\nb: 2\n", FORBIDDEN), [2]);
        assert_eq!(lines("--- # c\na: 1\n", FORBIDDEN), [1]);
        assert_eq!(lines("a: 1\n", FORBIDDEN), [] as [usize; 0]);
    }

    #[test]
    fn test_forbidden_flags_markers_after_directives() {
        assert_eq!(
            lines("%YAML 1.2\n---\na: 1\n---\nb: 2\n", FORBIDDEN),
            [2, 4]
        );
        assert_eq!(lines("%YAML 1.2\n# c\n\n---\na: 1\n", FORBIDDEN), [4]);
    }

    #[test]
    fn test_marker_text_inside_scalars_is_not_a_marker() {
        assert_eq!(lines("a: |\n  ---\n  text\n", FORBIDDEN), [] as [usize; 0]);
        assert_eq!(lines("a: \"x\n  --- y\"\n", FORBIDDEN), [] as [usize; 0]);
    }
}
