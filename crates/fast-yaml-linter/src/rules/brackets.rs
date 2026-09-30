//! Rule to check flow sequence brackets `[]` formatting.

use crate::{
    Diagnostic, DiagnosticCode, LintConfig, LintContext, Severity,
    rules::flow_common::{FlowCollection, check_flow_collection},
};
use fast_yaml_core::Value;

/// Linting rule for flow sequence brackets.
///
/// Validates spacing and usage of flow sequences `[]`.
///
/// Configuration options:
/// - `forbid`: "no" | "non-empty" | "all" (default: "no")
/// - `min-spaces-inside`: integer (default: 0)
/// - `max-spaces-inside`: integer (default: 0)
/// - `min-spaces-inside-empty`: integer (default: -1, disabled)
/// - `max-spaces-inside-empty`: integer (default: -1, disabled)
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{rules::BracketsRule, rules::LintRule, LintConfig, LintContext, config::RuleConfig};
/// use fast_yaml_core::Parser;
///
/// let rule = BracketsRule;
/// let yaml = "list: [1, 2, 3]";
/// let value = Parser::parse_str(yaml).unwrap().unwrap();
///
/// let config = LintConfig::new()
///     .with_rule_config("brackets", RuleConfig::new().with_option("forbid", "no"));
///
/// let diagnostics = rule.check(&LintContext::new(yaml), &value, &config);
/// assert!(diagnostics.is_empty());
/// ```
pub struct BracketsRule;

impl super::LintRule for BracketsRule {
    fn code(&self) -> &str {
        DiagnosticCode::BRACKETS
    }

    fn name(&self) -> &'static str {
        "Brackets"
    }

    fn description(&self) -> &'static str {
        "Validates spacing and usage of flow sequence brackets []"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, context: &LintContext, _value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        check_flow_collection(
            context,
            config,
            self.code(),
            self.default_severity(),
            FlowCollection::Sequence,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::RuleConfig, rules::LintRule};
    use fast_yaml_core::Parser;

    #[test]
    fn test_brackets_default_valid() {
        let yaml = "list: [1, 2, 3]";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = BracketsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_brackets_forbid_all() {
        let yaml = "list: [1, 2, 3]";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = BracketsRule;
        let config = LintConfig::new()
            .with_rule_config("brackets", RuleConfig::new().with_option("forbid", "all"));

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("forbidden"));
    }

    #[test]
    fn test_brackets_forbid_non_empty() {
        let yaml = "list: [1, 2]";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = BracketsRule;
        let config = LintConfig::new().with_rule_config(
            "brackets",
            RuleConfig::new().with_option("forbid", "non-empty"),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("non-empty"));
    }

    #[test]
    fn test_brackets_forbid_non_empty_allows_empty() {
        let yaml = "list: []";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = BracketsRule;
        let config = LintConfig::new().with_rule_config(
            "brackets",
            RuleConfig::new().with_option("forbid", "non-empty"),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_brackets_min_spaces_inside() {
        let yaml = "list: [1, 2, 3]";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = BracketsRule;
        let config = LintConfig::new().with_rule_config(
            "brackets",
            RuleConfig::new().with_option("min-spaces-inside", 1i64),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("too few spaces"));
    }

    #[test]
    fn test_brackets_max_spaces_inside() {
        let yaml = "list: [  1, 2, 3  ]";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = BracketsRule;
        let config = LintConfig::new().with_rule_config(
            "brackets",
            RuleConfig::new().with_option("max-spaces-inside", 0i64),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("too many spaces"));
    }

    #[test]
    fn test_brackets_valid_with_spaces() {
        let yaml = "list: [ 1, 2, 3 ]";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = BracketsRule;
        let config = LintConfig::new().with_rule_config(
            "brackets",
            RuleConfig::new()
                .with_option("min-spaces-inside", 1i64)
                .with_option("max-spaces-inside", 1i64),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_brackets_empty_sequence() {
        let yaml = "list: []";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = BracketsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_brackets_empty_with_spaces() {
        let yaml = "list: [ ]";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = BracketsRule;
        let config = LintConfig::new().with_rule_config(
            "brackets",
            RuleConfig::new()
                .with_option("min-spaces-inside-empty", 1i64)
                .with_option("max-spaces-inside-empty", 1i64),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_brackets_nested() {
        let yaml = "list: [[1, 2], [3, 4]]";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = BracketsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    // Regression test for issue #116
    #[test]
    fn test_brackets_no_false_positive_in_block_scalar() {
        let yaml = "steps:\n  - name: Check result\n    run: |\n      if [[ \"$result\" != \"success\" ]]; then\n        exit 1\n      fi\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = BracketsRule;
        let config = LintConfig::new().with_rule_config(
            "brackets",
            RuleConfig::new()
                .with_option("min-spaces-inside", 0i64)
                .with_option("max-spaces-inside", 0i64),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(
            diagnostics.is_empty(),
            "no false positives in block scalar: {diagnostics:?}"
        );
    }

    // Regression test for issue #302
    #[test]
    fn test_brackets_non_ascii_prefix_block_scalar_no_panic() {
        let yaml = "# ———\nrun: |\n  echo\n  ok\n  tail ]\nc: [a, b]\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();
        let context = LintContext::new(yaml);
        let diagnostics = BracketsRule.check(&context, &value, &LintConfig::default());
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn test_brackets_nested_pairing() {
        let yaml = "a: [[]]\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();
        let context = LintContext::new(yaml);
        let config = LintConfig::new().with_rule_config(
            "brackets",
            RuleConfig::new().with_option("forbid", "non-empty"),
        );
        let diagnostics = BracketsRule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].span.start.column, 4);
    }

    #[test]
    fn test_brackets_empty_min_spaces_reported() {
        let yaml = "b: []\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();
        let context = LintContext::new(yaml);
        let config = LintConfig::new().with_rule_config(
            "brackets",
            RuleConfig::new().with_option("min-spaces-inside-empty", 1i64),
        );
        let diagnostics = BracketsRule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    }

    #[test]
    fn test_brackets_non_ascii_key_location() {
        let yaml = "—: [ 1 ]\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();
        let config = LintConfig::new().with_rule_config(
            "brackets",
            RuleConfig::new().with_option("forbid", "non-empty"),
        );
        let diagnostics = BracketsRule.check(&LintContext::new(yaml), &value, &config);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        let start = diagnostics[0].span.start;
        assert_eq!((start.column, start.offset), (4, 5));
    }

    #[test]
    fn test_brackets_stray_bracket_in_comment_no_panic() {
        let yaml = "k: [a,\n  b]\n# x ]\nz: [ 1 ]\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();
        let context = LintContext::new(yaml);
        let diagnostics = BracketsRule.check(&context, &value, &LintConfig::default());
        assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
        assert!(diagnostics.iter().all(|d| d.span.start.line == 4));
    }
}
