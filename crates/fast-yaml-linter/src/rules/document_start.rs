//! Rule to check for document start marker (---).

use serde::{Deserialize, Serialize};

use crate::config::{BoolOrName, RuleOptions, deserialize_bool_or_name};
use crate::context::source_lines;
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

/// Whether the document start marker `---` is required.
///
/// Accepts `true` (required), `false` (forbidden), `required`, `forbidden` and `allowed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DocumentStartPresence {
    /// The marker must be present.
    Required,
    /// The marker must be absent.
    Forbidden,
    /// Either form is accepted.
    #[default]
    Allowed,
}

impl BoolOrName for DocumentStartPresence {
    const EXPECTING: &'static str = "a boolean, 'required', 'forbidden' or 'allowed'";

    fn from_bool(value: bool) -> Result<Self, &'static str> {
        Ok(if value {
            Self::Required
        } else {
            Self::Forbidden
        })
    }

    fn from_name(name: &str) -> Option<Self> {
        match name {
            "required" => Some(Self::Required),
            "forbidden" => Some(Self::Forbidden),
            "allowed" => Some(Self::Allowed),
            _ => None,
        }
    }
}

impl Serialize for DocumentStartPresence {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Required => "required",
            Self::Forbidden => "forbidden",
            Self::Allowed => "allowed",
        })
    }
}

impl<'de> Deserialize<'de> for DocumentStartPresence {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserialize_bool_or_name(deserializer)
    }
}

/// Options of the document-start rule.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", default)]
pub struct DocumentStartOptions {
    /// Whether `---` is required, forbidden or allowed.
    pub present: DocumentStartPresence,
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
            DocumentStartPresence::Required => {
                check_required(source, source_context, config, self.code())
            }
            DocumentStartPresence::Forbidden => {
                check_forbidden(source, source_context, config, self.code())
            }
            DocumentStartPresence::Allowed => Vec::new(),
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
    if let Some((_line_num, span)) = find_document_start_marker(source) {
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

fn find_document_start_marker(source: &str) -> Option<(usize, Span)> {
    for (line_num, (offset, line)) in source_lines(source).enumerate() {
        let trimmed = line.trim_start();

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.starts_with("---") {
            let col = line.len() - trimmed.len() + 1;
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
