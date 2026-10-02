//! Rule to check flow sequence brackets `[]` formatting.

use super::RuleId;
use crate::config::RuleName;
use crate::{
    Finding, LintConfig, LintContext, Severity,
    rules::flow_common::{FlowCollection, check_flow_collection},
};

/// Linting rule for flow sequence brackets.
///
/// Validates spacing and usage of flow sequences `[]`.
///
/// Configuration options (see [`FlowCollectionOptions`](super::FlowCollectionOptions)):
/// - `forbid`: `false` | `true` | "non-empty" | "all" (default: `false`)
/// - `min-spaces-inside`: integer, -1 disables (default: 0)
/// - `max-spaces-inside`: integer, -1 disables (default: 0)
/// - `min-spaces-inside-empty`: integer, -1 inherits `min-spaces-inside` (default: -1)
/// - `max-spaces-inside-empty`: integer, -1 inherits `max-spaces-inside` (default: -1)
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{rules::BracketsRule, rules::SourceRule, LintConfig, LintContext};
/// use fast_yaml_core::Parser;
///
/// let rule = BracketsRule;
/// let yaml = "list: [1, 2, 3]";
///
/// let config = LintConfig::default();
///
/// let diagnostics = rule.diagnose(&LintContext::new(yaml), &config);
/// assert!(diagnostics.is_empty());
/// ```
pub struct BracketsRule;

impl super::LintRule for BracketsRule {
    fn id(&self) -> RuleId<'_> {
        RuleId::BuiltIn(RuleName::Brackets)
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
}

impl super::SourceRule for BracketsRule {
    fn check(&self, context: &LintContext, config: &LintConfig) -> Vec<Finding> {
        check_flow_collection(context, &config.rules.brackets, FlowCollection::Sequence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{RuleName, test_support::config_with_rule},
        rules::SourceRule,
    };

    #[test]
    fn test_brackets_default_valid() {
        let yaml = "list: [1, 2, 3]";

        let rule = BracketsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_brackets_forbid_all() {
        let yaml = "list: [1, 2, 3]";

        let rule = BracketsRule;
        let config = config_with_rule(RuleName::Brackets, "{forbid: all}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("forbidden"));
    }

    #[test]
    fn test_brackets_forbid_non_empty() {
        let yaml = "list: [1, 2]";

        let rule = BracketsRule;
        let config = config_with_rule(RuleName::Brackets, "{forbid: non-empty}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("non-empty"));
    }

    #[test]
    fn test_brackets_forbid_non_empty_allows_empty() {
        let yaml = "list: []";

        let rule = BracketsRule;
        let config = config_with_rule(RuleName::Brackets, "{forbid: non-empty}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_brackets_min_spaces_inside() {
        let yaml = "list: [1, 2, 3]";

        let rule = BracketsRule;
        let config = config_with_rule(RuleName::Brackets, "{min-spaces-inside: 1}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_ne!(diagnostics, []);
        assert!(diagnostics[0].message.contains("too few spaces"));
    }

    #[test]
    fn test_brackets_max_spaces_inside() {
        let yaml = "list: [  1, 2, 3  ]";

        let rule = BracketsRule;
        let config = config_with_rule(RuleName::Brackets, "{max-spaces-inside: 0}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_ne!(diagnostics, []);
        assert!(diagnostics[0].message.contains("too many spaces"));
    }

    #[test]
    fn test_brackets_valid_with_spaces() {
        let yaml = "list: [ 1, 2, 3 ]";

        let rule = BracketsRule;
        let config = config_with_rule(
            RuleName::Brackets,
            "{min-spaces-inside: 1, max-spaces-inside: 1}",
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_brackets_empty_sequence() {
        let yaml = "list: []";

        let rule = BracketsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_brackets_empty_with_spaces() {
        let yaml = "list: [ ]";

        let rule = BracketsRule;
        let config = config_with_rule(
            RuleName::Brackets,
            "{min-spaces-inside-empty: 1, max-spaces-inside-empty: 1}",
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_brackets_nested() {
        let yaml = "list: [[1, 2], [3, 4]]";

        let rule = BracketsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    // Regression test for issue #116
    #[test]
    fn test_brackets_no_false_positive_in_block_scalar() {
        let yaml = "steps:\n  - name: Check result\n    run: |\n      if [[ \"$result\" != \"success\" ]]; then\n        exit 1\n      fi\n";

        let rule = BracketsRule;
        let config = config_with_rule(
            RuleName::Brackets,
            "{min-spaces-inside: 0, max-spaces-inside: 0}",
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert!(
            diagnostics.is_empty(),
            "no false positives in block scalar: {diagnostics:?}"
        );
    }

    // Regression test for issue #302
    #[test]
    fn test_brackets_non_ascii_prefix_block_scalar_no_panic() {
        let yaml = "# ———\nrun: |\n  echo\n  ok\n  tail ]\nc: [a, b]\n";
        let context = LintContext::new(yaml);
        let diagnostics = BracketsRule.diagnose(&context, &LintConfig::default());
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn test_brackets_nested_pairing() {
        let yaml = "a: [[]]\n";
        let context = LintContext::new(yaml);
        let config = config_with_rule(RuleName::Brackets, "{forbid: non-empty}");
        let diagnostics = BracketsRule.diagnose(&context, &config);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].span.start.column(), 4);
    }

    #[test]
    fn test_brackets_empty_min_spaces_reported() {
        let yaml = "b: []\n";
        let context = LintContext::new(yaml);
        let config = config_with_rule(RuleName::Brackets, "{min-spaces-inside-empty: 1}");
        let diagnostics = BracketsRule.diagnose(&context, &config);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    }

    #[test]
    fn test_brackets_non_ascii_key_location() {
        let yaml = "—: [ 1 ]\n";
        let config = config_with_rule(RuleName::Brackets, "{forbid: non-empty}");
        let diagnostics = BracketsRule.diagnose(&LintContext::new(yaml), &config);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        let start = diagnostics[0].span.start;
        assert_eq!((start.column(), start.offset()), (4, 5));
    }

    #[test]
    fn test_brackets_stray_bracket_in_comment_no_panic() {
        let yaml = "k: [a,\n  b]\n# x ]\nz: [ 1 ]\n";
        let context = LintContext::new(yaml);
        let diagnostics = BracketsRule.diagnose(&context, &LintConfig::default());
        assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
        assert!(diagnostics.iter().all(|d| d.span.start.line() == 4));
    }
}
