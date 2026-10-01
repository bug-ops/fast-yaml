//! Rule to check for document end marker (...).

use serde::{Deserialize, Serialize};

use crate::config::{MarkerPresence, RuleOptions};
use crate::context::source_lines;
use crate::source::offset::ByteOffset;
use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Location, Severity,
    SourceContext, Span,
};
use fast_yaml_core::Value;

/// Linting rule for document end marker.
///
/// Requires, forbids, or allows the YAML document end marker `...`. `required` checks every
/// document of the stream, `forbidden` flags every `...` at column 0. A source without any
/// document (empty or comment-only) never lacks a required marker.
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

/// Flags each document that is not closed by `...`: the ones followed by another document at that
/// document's `---` line, and the last one at the end of the file.
fn check_required(context: &LintContext, config: &LintConfig, code: &str) -> Vec<Diagnostic> {
    if context.documents().is_empty() && context.scan_is_complete() {
        return Vec::new();
    }
    let source = context.source();
    let source_context = context.source_context();
    let severity = config.rules.document_end.severity_or(Severity::Warning);
    let documents = context.document_markers();

    documents
        .iter()
        .enumerate()
        .filter(|(_, document)| document.end.is_none())
        .map(|(index, _)| {
            let next_marker = documents
                .get(index + 1)
                .and_then(|next| next.start.marker());
            let (span, replacement) = next_marker.map_or_else(
                || {
                    let eof = source_context.span_at(ByteOffset::new(source.len()), 0);
                    let marker = if source.is_empty() || source.ends_with(['\n', '\r']) {
                        "..."
                    } else {
                        "\n..."
                    };
                    (eof, marker)
                },
                |marker| {
                    let at =
                        source_context.span_at(source_context.line_start(marker.start.line), 0);
                    (at, "...\n")
                },
            );
            DiagnosticBuilder::new(code, severity, "missing document end marker '...'", span)
                .with_suggestion(
                    "Add '...' to close this document",
                    span,
                    Some(replacement.to_owned()),
                )
                .build_with_context(source_context)
        })
        .collect()
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
        assert_eq!(diagnostics, []);
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
        assert_eq!(diag_with, []);

        let value_without = Parser::parse_str(yaml_without).unwrap().unwrap();
        let context_without = LintContext::new(yaml_without);
        let diag_without = rule.check(&context_without, &value_without, &config);
        assert_eq!(diag_without, []);
    }

    #[test]
    fn test_document_end_with_comments_after() {
        let yaml = "name: John\n...\n# comment";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = DocumentEndRule;
        let config = config_with_rule(RuleName::DocumentEnd, "{present: true}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics, []);
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
        assert_eq!(forbidden("a: 1\n"), []);
        assert_eq!(forbidden("a: 1\n....\n"), []);
        assert_eq!(forbidden("a: ...\n"), []);
        assert_eq!(forbidden("a: |\n  ...\n  text\n"), []);
        assert_eq!(forbidden("a: 1\n  ...\n"), []);
        assert_eq!(forbidden("a: 1\n...x\n"), []);
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

    fn required(yaml: &str) -> Vec<Diagnostic> {
        let value = Parser::parse_str("a: 1").unwrap().unwrap();
        let config = config_with_rule(RuleName::DocumentEnd, "{present: true}");
        DocumentEndRule.check(&LintContext::new(yaml), &value, &config)
    }

    fn required_lines(yaml: &str) -> Vec<(usize, usize)> {
        required(yaml)
            .iter()
            .map(|d| (d.span.start.line, d.span.start.column))
            .collect()
    }

    #[test]
    fn required_checks_every_document() {
        assert_eq!(required("a: 1\n...\n---\nb: 2\n...\n"), []);
        assert_eq!(required_lines("a: 1\n---\nb: 2\n..."), [(2, 1)]);
        assert_eq!(required_lines("a: 1\n...\n---\nb: 2\n"), [(5, 1)]);
        assert_eq!(
            required_lines("a: 1\n---\nb: 2\n---\nc: 3\n"),
            [(2, 1), (4, 1), (6, 1)]
        );
    }

    #[test]
    fn required_accepts_a_marker_with_a_comment() {
        assert_eq!(required("a: 1\n... # end\n"), []);
        assert_eq!(required("--- # c\na: 1\n... # e\n"), []);
    }

    #[test]
    fn required_does_not_take_an_indented_or_scalar_marker_for_an_end() {
        assert_eq!(required("a: 1\n  ...\n").len(), 1);
        assert_eq!(required("a: |\n  ...\n").len(), 1);
    }

    #[test]
    fn required_suggestions_close_the_document() {
        let found = required("a: 1\n---\nb: 2\n");
        assert_eq!(
            found[0].suggestions[0].replacement.as_deref(),
            Some("...\n")
        );
        assert_eq!(found[0].suggestions[0].span.start.offset, "a: 1\n".len());
        assert_eq!(found[1].suggestions[0].replacement.as_deref(), Some("..."));
    }

    #[test]
    fn required_comment_only_and_empty_sources_have_no_document() {
        assert_eq!(required("# c\n"), []);
        assert_eq!(required(""), []);
    }
}
