//! Rule to check spacing around commas in flow collections.

use super::RuleId;
use crate::config::RuleName;
use serde::{Deserialize, Serialize};

use crate::config::{Limit, RuleOptions};
use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Severity,
    SourceContext, Span, tokenizer::TokenType,
};

/// Linting rule for comma spacing.
///
/// Validates spacing before and after commas in flow collections.
///
/// Configuration options:
/// - `max-spaces-before`: integer (default: 0)
/// - `min-spaces-after`: integer (default: 1)
/// - `max-spaces-after`: integer (default: 1)
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{rules::CommasRule, rules::SourceRule, LintConfig};
/// use fast_yaml_core::Parser;
///
/// let rule = CommasRule;
/// let yaml = "list: [1, 2, 3]";
///
/// let config = LintConfig::default();
///
/// let diagnostics = rule.check(&fast_yaml_linter::LintContext::new(yaml), &config);
/// assert!(diagnostics.is_empty());
/// ```
pub struct CommasRule;

/// Options of the commas rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", default)]
pub struct CommasOptions {
    /// Maximum spaces before a comma.
    pub max_spaces_before: Limit,
    /// Minimum spaces after a comma.
    pub min_spaces_after: Limit,
    /// Maximum spaces after a comma.
    pub max_spaces_after: Limit,
}

impl Default for CommasOptions {
    fn default() -> Self {
        Self {
            max_spaces_before: Limit::Max(0),
            min_spaces_after: Limit::Max(1),
            max_spaces_after: Limit::Max(1),
        }
    }
}

impl RuleOptions for CommasOptions {}

impl super::LintRule for CommasRule {
    fn id(&self) -> RuleId<'_> {
        RuleId::BuiltIn(RuleName::Commas)
    }

    fn name(&self) -> &'static str {
        "Commas"
    }

    fn description(&self) -> &'static str {
        "Validates spacing around commas in flow collections"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }
}

impl super::SourceRule for CommasRule {
    fn check(&self, context: &LintContext, config: &LintConfig) -> Vec<Diagnostic> {
        let source = context.source();
        let source_context = context.source_context();
        let tokenizer = context.flow_tokenizer();

        let options = &config.rules.commas.options;
        let max_spaces_before = options.max_spaces_before;
        let min_spaces_after = options.min_spaces_after;
        let max_spaces_after = options.max_spaces_after;

        let mut diagnostics = Vec::new();
        let commas = tokenizer.tokens(TokenType::Comma);

        for comma in commas {
            // Check spaces before comma
            if let Some(diag) = check_spaces_before_comma(
                source,
                source_context,
                comma.span.start.offset,
                max_spaces_before,
                DiagnosticCode::COMMAS,
                config,
            ) {
                diagnostics.push(diag);
            }

            // Check spaces after comma
            if let Some(diag) = check_spaces_after_comma(
                source,
                source_context,
                comma.span.start.offset,
                min_spaces_after,
                max_spaces_after,
                DiagnosticCode::COMMAS,
                config,
            ) {
                diagnostics.push(diag);
            }
        }

        diagnostics
    }
}

/// Checks spaces before a comma.
fn check_spaces_before_comma(
    source: &str,
    source_context: &SourceContext<'_>,
    comma_offset: usize,
    max_spaces: Limit,
    code: &str,
    config: &LintConfig,
) -> Option<Diagnostic> {
    if comma_offset == 0 {
        return None;
    }

    // Count spaces before comma using byte indexing for O(n) instead of O(n²)
    let bytes = source.as_bytes();
    let mut spaces = 0;
    let mut offset = comma_offset;

    while offset > 0 {
        offset -= 1;
        if bytes.get(offset) == Some(&b' ') {
            spaces += 1;
        } else {
            break;
        }
    }

    if max_spaces.exceeded_by(spaces) {
        let severity = config.rules.commas.severity_or(Severity::Warning);
        let loc = source_context.offset_to_location(comma_offset);
        let span = Span::new(loc, loc);

        return Some(
            DiagnosticBuilder::new(
                code,
                severity,
                format!(
                    "too many spaces before comma (expected at most {max_spaces}, found {spaces})"
                ),
                span,
            )
            .build(),
        );
    }

    None
}

/// Checks spaces after a comma.
fn check_spaces_after_comma(
    source: &str,
    source_context: &SourceContext<'_>,
    comma_offset: usize,
    min_spaces: Limit,
    max_spaces: Limit,
    code: &str,
    config: &LintConfig,
) -> Option<Diagnostic> {
    let bytes = source.as_bytes();
    if comma_offset + 1 >= bytes.len() {
        return None;
    }

    // Count spaces after comma using byte indexing for O(n) instead of O(n²)
    let mut spaces = 0;
    let mut offset = comma_offset + 1;
    let mut has_newline = false;

    while offset < bytes.len() {
        if bytes.get(offset) == Some(&b' ') {
            spaces += 1;
            offset += 1;
        } else {
            if matches!(bytes.get(offset), Some(b'\n' | b'\r')) {
                has_newline = true;
            }
            break;
        }
    }

    // A comment is not a token for yamllint, which measures the gap to the next token on the
    // line; none follows, and `comments` already requires two spaces before the `#`.
    if spaces > 0 && bytes.get(offset) == Some(&b'#') {
        return None;
    }

    // Don't check min spaces if followed by newline
    if min_spaces.unmet_by(spaces) && !has_newline {
        let severity = config.rules.commas.severity_or(Severity::Warning);
        let loc = source_context.offset_to_location(comma_offset + 1);
        let span = Span::new(loc, loc);

        return Some(
            DiagnosticBuilder::new(
                code,
                severity,
                format!(
                    "too few spaces after comma (expected at least {min_spaces}, found {spaces})"
                ),
                span,
            )
            .build(),
        );
    }

    if max_spaces.exceeded_by(spaces) {
        let severity = config.rules.commas.severity_or(Severity::Warning);
        let loc = source_context.offset_to_location(comma_offset + 1);
        let span = Span::new(loc, loc);

        return Some(
            DiagnosticBuilder::new(
                code,
                severity,
                format!(
                    "too many spaces after comma (expected at most {max_spaces}, found {spaces})"
                ),
                span,
            )
            .build(),
        );
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{RuleName, test_support::config_with_rule},
        rules::SourceRule,
    };

    #[test]
    fn test_commas_default_valid() {
        let yaml = "list: [1, 2, 3]";

        let rule = CommasRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_commas_too_many_spaces_before() {
        let yaml = "list: [1 , 2 , 3]";

        let rule = CommasRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &config);
        assert_ne!(diagnostics, []);
        assert!(diagnostics[0].message.contains("too many spaces before"));
    }

    #[test]
    fn test_commas_too_few_spaces_after() {
        let yaml = "list: [1,2,3]";

        let rule = CommasRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &config);
        assert_ne!(diagnostics, []);
        assert!(diagnostics[0].message.contains("too few spaces after"));
    }

    #[test]
    fn test_commas_too_many_spaces_after() {
        let yaml = "list: [1,  2,  3]";

        let rule = CommasRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &config);
        assert_ne!(diagnostics, []);
        assert!(diagnostics[0].message.contains("too many spaces after"));
    }

    #[test]
    fn test_commas_allow_no_spaces_after() {
        let yaml = "list: [1,2,3]";

        let rule = CommasRule;
        let config = config_with_rule(
            RuleName::Commas,
            "{min-spaces-after: 0, max-spaces-after: 0}",
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_commas_allow_multiple_spaces_after() {
        let yaml = "list: [1,  2,  3]";

        let rule = CommasRule;
        let config = config_with_rule(RuleName::Commas, "{max-spaces-after: 2}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_commas_flow_mapping() {
        let yaml = "{name: John, age: 30}";

        let rule = CommasRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_commas_nested_flow() {
        let yaml = "data: [[1, 2], [3, 4]]";

        let rule = CommasRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_commas_multiline_flow() {
        let yaml = "list: [\n  1,\n  2,\n  3\n]";

        let rule = CommasRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &config);
        // Commas followed by newlines should be handled gracefully
        assert!(
            diagnostics.is_empty() || diagnostics.iter().all(|d| !d.message.contains("too few"))
        );
    }

    #[test]
    fn test_commas_correct_location() {
        // Violation at line 2, not line 1
        let yaml = "first: ok\nlist: [1,2,3]";

        let rule = CommasRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &config);
        assert_ne!(diagnostics, []);
        assert_eq!(
            diagnostics[0].span.start.line, 2,
            "violation should be on line 2, got: {}",
            diagnostics[0].span.start.line
        );
    }

    #[test]
    fn test_commas_multiple_violations() {
        let yaml = "list: [1 ,2,3 , 4]";

        let rule = CommasRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &config);
        // Should have multiple violations
        assert!(diagnostics.len() >= 2);
    }

    // Regression test for issue #116
    #[test]
    fn test_commas_no_false_positive_in_block_scalar() {
        let yaml = "run: |\n  echo \"a, b, c\"\n  for i in 1,2,3; do echo $i; done\n";

        let rule = CommasRule;
        let config = config_with_rule(
            RuleName::Commas,
            "{max-spaces-before: 0, min-spaces-after: 1}",
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &config);
        assert!(
            diagnostics.is_empty(),
            "no false positives in block scalar: {diagnostics:?}"
        );
    }
}
