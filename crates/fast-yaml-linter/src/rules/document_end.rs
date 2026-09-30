//! Rule to check for document end marker (...).

use serde::{Deserialize, Serialize};

use crate::config::{MarkerPresence, RuleOptions};
use crate::context::{lines_of, source_lines};
use crate::source::offset::ByteOffset;
use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Location, Severity,
    SourceContext, Span,
};
use fast_yaml_core::Value;

/// Linting rule for document end marker.
///
/// Requires, forbids, or allows the YAML document end marker `...`.
///
/// Configuration options:
/// - `present`: "required" | "forbidden" | "allowed" (default: "allowed"); `true` and `false`
///   are accepted as `required` and `forbidden`
///
/// # Examples
///
/// ```
/// use fast_yaml_core::Parser;
/// use fast_yaml_linter::{rules::DocumentEndRule, rules::LintRule, LintConfig};
///
/// let rule = DocumentEndRule;
/// let yaml = "name: John\n...";
/// let value = Parser::parse_str(yaml).unwrap().unwrap();
///
/// let config = LintConfig::default();
///
/// let diagnostics = rule.check(&fast_yaml_linter::LintContext::new(yaml), &value, &config);
/// assert!(diagnostics.is_empty());
/// ```
pub struct DocumentEndRule;

/// Options of the document-end rule.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", default)]
pub struct DocumentEndOptions {
    /// Whether `...` is required, forbidden or allowed.
    pub present: MarkerPresence,
}

impl RuleOptions for DocumentEndOptions {}

impl super::LintRule for DocumentEndRule {
    fn code(&self) -> &str {
        DiagnosticCode::DOCUMENT_END
    }

    fn name(&self) -> &'static str {
        "Document End"
    }

    fn description(&self) -> &'static str {
        "Requires or forbids the YAML document end marker '...'"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, context: &LintContext, _value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        match config.rules.document_end.options.present {
            MarkerPresence::Allowed => Vec::new(),
            MarkerPresence::Required => check_required(context, config, self.code()),
            MarkerPresence::Forbidden => check_forbidden(
                context.source(),
                context.source_context(),
                config,
                self.code(),
            ),
        }
    }
}

fn check_required(context: &LintContext, config: &LintConfig, code: &str) -> Vec<Diagnostic> {
    let source = context.source();
    if has_document_end_marker(source) {
        return Vec::new();
    }
    let severity = config.rules.document_end.severity_or(Severity::Warning);
    let eof_span = context
        .source_context()
        .span_at(ByteOffset::new(source.len()), 0);

    let marker = if source.is_empty() || source.ends_with(['\n', '\r']) {
        "..."
    } else {
        "\n..."
    };

    vec![
        DiagnosticBuilder::new(
            code,
            severity,
            "missing document end marker '...'",
            eof_span,
        )
        .with_suggestion("Add '...' at the end", eof_span, Some(marker.to_string()))
        .build_with_context(context.source_context()),
    ]
}

/// Flags every `...` at column 0; YAML makes such a line end the document even inside scalars.
fn check_forbidden(
    source: &str,
    source_context: &SourceContext<'_>,
    config: &LintConfig,
    code: &str,
) -> Vec<Diagnostic> {
    let severity = config.rules.document_end.severity_or(Severity::Warning);
    source_lines(source)
        .enumerate()
        .filter(|(_, (_, line))| {
            line.strip_prefix("...")
                .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t']))
        })
        .map(|(line_num, (offset, _))| {
            let span = Span::new(
                Location::new(line_num + 1, 1, offset),
                Location::new(line_num + 1, 4, offset + 3),
            );
            DiagnosticBuilder::new(
                code,
                severity,
                "document end marker '...' is forbidden",
                span,
            )
            .with_suggestion("Remove '...'", span, None)
            .build_with_context(source_context)
        })
        .collect()
}

fn has_document_end_marker(source: &str) -> bool {
    lines_of(source)
        .map(str::trim)
        .filter(|trimmed| !trimmed.is_empty() && !trimmed.starts_with('#'))
        .last()
        == Some("...")
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
    fn test_document_end_required_present() {
        let yaml = "name: John\n...";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = DocumentEndRule;
        let config = config_with_rule(RuleName::DocumentEnd, "{present: true}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_document_end_required_missing() {
        let yaml = "name: John";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = DocumentEndRule;
        let config = config_with_rule(RuleName::DocumentEnd, "{present: true}");

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
        let config = config_with_rule(RuleName::DocumentEnd, "{present: true}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_has_document_end_marker() {
        assert!(has_document_end_marker("test: value\n..."));
        assert!(has_document_end_marker("test: value\n...  \n# comment"));
        assert!(has_document_end_marker("test: value\n...\n\n"));
        assert!(has_document_end_marker("é: 1\r...\r"));
        assert!(has_document_end_marker("é: 1\r\n  ...\n"));
        assert!(!has_document_end_marker("test: value"));
        assert!(!has_document_end_marker(""));
    }

    #[test]
    fn test_severity_override() {
        let yaml = "name: John";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = DocumentEndRule;
        let config = config_with_rule(RuleName::DocumentEnd, "{present: true, severity: error}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Error);
    }

    fn forbidden(yaml: &str) -> Vec<Diagnostic> {
        let value = Parser::parse_str("a: 1").unwrap().unwrap();
        let config = config_with_rule(RuleName::DocumentEnd, "{present: false}");
        DocumentEndRule.check(&LintContext::new(yaml), &value, &config)
    }

    #[test]
    fn forbidden_flags_every_column_zero_marker() {
        let diagnostics = forbidden("a: 1\n...\n---\nb: 2\n...\n");
        assert_eq!(diagnostics.len(), 2);
        assert_eq!(
            diagnostics[0].message,
            "document end marker '...' is forbidden"
        );
        assert_eq!(diagnostics[0].span.start.line, 2);
        assert_eq!(diagnostics[0].span.start.column, 1);
        assert_eq!(diagnostics[0].span.end.column, 4);
        assert_eq!(diagnostics[1].span.start.line, 5);
    }

    #[test]
    fn forbidden_accepts_marker_with_trailing_comment_and_eof() {
        assert_eq!(forbidden("a: 1\n... # end\n").len(), 1);
        assert_eq!(forbidden("a: 1\n...").len(), 1);
        assert_eq!(forbidden("a: 1\n...\t\n").len(), 1);
    }

    #[test]
    fn forbidden_ignores_non_markers() {
        assert!(forbidden("a: 1\n").is_empty());
        assert!(forbidden("a: 1\n....\n").is_empty());
        assert!(forbidden("a: ...\n").is_empty());
        assert!(forbidden("a: |\n  ...\n  text\n").is_empty());
        assert!(forbidden("a: 1\n  ...\n").is_empty());
        assert!(forbidden("a: 1\n...x\n").is_empty());
    }

    #[test]
    fn forbidden_flags_marker_after_block_scalar() {
        assert_eq!(forbidden("a: |\n  text\n...\n").len(), 1);
    }

    #[test]
    fn forbidden_reports_byte_offsets_after_multibyte_text() {
        let diagnostics = forbidden("é: 1\n...\n");
        assert_eq!(diagnostics[0].span.start.offset, "é: 1\n".len());
        assert_eq!(diagnostics[0].span.end.offset, "é: 1\n...".len());
    }

    #[test]
    fn forbidden_offers_removal_suggestion() {
        let diagnostics = forbidden("a: 1\n...\n");
        assert_eq!(diagnostics[0].suggestions.len(), 1);
        assert_eq!(diagnostics[0].suggestions[0].replacement, None);
    }
}
