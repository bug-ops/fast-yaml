//! Rule to check spacing after list item hyphens.

use super::RuleId;
use crate::config::RuleName;
use serde::{Deserialize, Serialize};

use crate::config::{Limit, RuleOptions};
use crate::rules::token_stream::{
    scanner,
    tokens::{Kind, Token},
};
use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Severity, Span,
};
use fast_yaml_core::Value;

/// Linting rule for hyphen spacing.
///
/// Validates spacing after the hyphen of a block sequence entry, read from the token stream
/// like yamllint does: a `-` inside a plain scalar or a flow collection is not an entry, and
/// neither is `-item`.
///
/// Configuration options:
/// - `max-spaces-after`: integer (default: 1)
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{rules::HyphensRule, rules::LintRule, LintConfig};
/// use fast_yaml_core::Parser;
///
/// let rule = HyphensRule;
/// let yaml = "- item1\n- item2";
/// let value = Parser::parse_str(yaml).unwrap().unwrap();
///
/// let config = LintConfig::default();
///
/// let diagnostics = rule.check(&fast_yaml_linter::LintContext::new(yaml), &value, &config);
/// assert!(diagnostics.is_empty());
/// ```
pub struct HyphensRule;

/// Options of the hyphens rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", default)]
pub struct HyphensOptions {
    /// Maximum spaces after a sequence hyphen.
    pub max_spaces_after: Limit,
}

impl Default for HyphensOptions {
    fn default() -> Self {
        Self {
            max_spaces_after: Limit::Max(1),
        }
    }
}

impl RuleOptions for HyphensOptions {}

impl super::LintRule for HyphensRule {
    fn id(&self) -> RuleId<'_> {
        RuleId::BuiltIn(RuleName::Hyphens)
    }

    fn name(&self) -> &'static str {
        "Hyphens"
    }

    fn description(&self) -> &'static str {
        "Validates spacing after list item hyphens"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, context: &LintContext, _value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        let settings = &config.rules.hyphens;
        let severity = settings.severity_or(Severity::Warning);
        let max_spaces = settings.options.max_spaces_after;
        let source_context = context.source_context();

        let mut diagnostics = Vec::new();
        let mut entry: Option<Token> = None;
        scanner::scan(
            context.source(),
            context.nodes(),
            context.scan_is_complete(),
            |token| {
                if let Some(hyphen) = entry.take()
                    && hyphen.end.line == token.start.line
                {
                    let spaces = token.start.index - hyphen.end.index;
                    if max_spaces.exceeded_by(spaces) {
                        let loc = source_context.offset_to_location(hyphen.end.pointer);
                        diagnostics.push(
                            DiagnosticBuilder::new(
                                DiagnosticCode::HYPHENS,
                                severity,
                                format!(
                                    "too many spaces after hyphen (expected at most {max_spaces}, found {spaces})"
                                ),
                                Span::new(loc, loc),
                            )
                            .build(),
                        );
                    }
                }
                if token.kind == Kind::BlockEntry {
                    entry = Some(token);
                }
            },
        );
        diagnostics
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
    fn test_hyphens_default_valid() {
        let yaml = "- item1\n- item2\n- item3";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = HyphensRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_hyphens_without_space_is_a_plain_scalar() {
        let yaml = "-item1\n-item2";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = HyphensRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        assert_eq!(rule.check(&context, &value, &config), []);
    }

    #[test]
    fn test_hyphens_too_many_spaces() {
        let yaml = "-  item1\n-  item2";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = HyphensRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_ne!(diagnostics, []);
        assert!(diagnostics[0].message.contains("too many spaces"));
    }

    #[test]
    fn test_hyphens_allow_multiple_spaces() {
        let yaml = "-  item1\n-  item2";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = HyphensRule;
        let config = config_with_rule(RuleName::Hyphens, "{max-spaces-after: 2}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_hyphens_nested_lists() {
        let yaml = "- item1\n  - nested1\n  - nested2\n- item2";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = HyphensRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_hyphens_indented_lists() {
        let yaml = "list:\n  - item1\n  - item2";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = HyphensRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_hyphens_empty_list_item() {
        let yaml = "-\n-";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = HyphensRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // Empty list items (hyphen at end of line) are allowed
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_hyphens_list_with_mappings() {
        let yaml = "- name: John\n  age: 30\n- name: Jane\n  age: 25";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = HyphensRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_hyphens_no_false_positive_on_document_separator() {
        let yaml = "---\nkey: value\n---\nother: data";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = HyphensRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(
            diagnostics.is_empty(),
            "document separators should not trigger hyphens rule: {diagnostics:?}"
        );
    }

    #[test]
    fn test_hyphens_correct_location() {
        let yaml = "- a\n-  b\n-   c\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = HyphensRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        let found: Vec<_> = diagnostics
            .iter()
            .map(|d| (d.span.start.line, d.span.start.column))
            .collect();
        assert_eq!(found, [(2, 2), (3, 2)]);
    }

    #[test]
    fn test_hyphens_no_false_positive_after_multibyte_chars() {
        // ✓ is 3 bytes but 1 char; byte offset and char index diverge after it
        let yaml = "items:\n  - note: \"contains ✓ checkmark\"\n  - item1\n  - item2";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = HyphensRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(
            diagnostics.is_empty(),
            "valid list items after multibyte chars should not trigger hyphens rule: {diagnostics:?}"
        );
    }

    #[test]
    fn test_hyphens_ignores_dashes_that_are_not_entries() {
        let yaml =
            "---\nrun: security import \"x\"\n  -k \"y\"\n  -t cert\nd: [a,\n  -1]\ne: |\n  -x\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = HyphensRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        assert_eq!(rule.check(&context, &value, &config), []);
    }

    #[test]
    fn test_hyphens_nested_entry_and_comment() {
        let yaml = "-   - a\n-  # c\n   b\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = HyphensRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let lines: Vec<_> = rule
            .check(&context, &value, &config)
            .iter()
            .map(|d| d.span.start.line)
            .collect();
        assert_eq!(lines, [1]);
    }
}
