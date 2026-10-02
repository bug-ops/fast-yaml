//! Rule to check flow mapping braces `{}` formatting.

use super::RuleId;
use crate::config::RuleName;
use crate::{
    Finding, LintConfig, LintContext, Severity,
    rules::flow_common::{FlowCollection, check_flow_collection},
};

/// Linting rule for flow mapping braces.
///
/// Validates spacing and usage of flow mappings `{}`.
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
/// use fast_yaml_linter::{rules::BracesRule, rules::SourceRule, LintConfig, LintContext};
/// use fast_yaml_core::Parser;
///
/// let rule = BracesRule;
/// let yaml = "object: {key: value}";
///
/// let config = LintConfig::default();
///
/// let diagnostics = rule.diagnose(&LintContext::new(yaml), &config);
/// assert!(diagnostics.is_empty());
/// ```
pub struct BracesRule;

impl super::LintRule for BracesRule {
    fn id(&self) -> RuleId<'_> {
        RuleId::BuiltIn(RuleName::Braces)
    }

    fn name(&self) -> &'static str {
        "Braces"
    }

    fn description(&self) -> &'static str {
        "Validates spacing and usage of flow mapping braces {}"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }
}

impl super::SourceRule for BracesRule {
    fn check(&self, context: &LintContext, config: &LintConfig) -> Vec<Finding> {
        check_flow_collection(context, &config.rules.braces, FlowCollection::Mapping)
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
    fn test_braces_default_valid() {
        let yaml = "object: {key: value}";

        let rule = BracesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_braces_forbid_all() {
        let yaml = "object: {key: value}";

        let rule = BracesRule;
        let config = config_with_rule(RuleName::Braces, "{forbid: all}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("forbidden"));
    }

    #[test]
    fn test_braces_forbid_non_empty() {
        let yaml = "object: {key: value}";

        let rule = BracesRule;
        let config = config_with_rule(RuleName::Braces, "{forbid: non-empty}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("non-empty"));
    }

    #[test]
    fn test_braces_forbid_non_empty_allows_empty() {
        let yaml = "object: {}";

        let rule = BracesRule;
        let config = config_with_rule(RuleName::Braces, "{forbid: non-empty}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_braces_min_spaces_inside() {
        let yaml = "object: {key: value}";

        let rule = BracesRule;
        let config = config_with_rule(RuleName::Braces, "{min-spaces-inside: 1}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_ne!(diagnostics, []);
        assert!(diagnostics[0].message.contains("too few spaces"));
    }

    #[test]
    fn test_braces_max_spaces_inside() {
        let yaml = "object: {  key: value  }";

        let rule = BracesRule;
        let config = config_with_rule(RuleName::Braces, "{max-spaces-inside: 0}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_ne!(diagnostics, []);
        assert!(diagnostics[0].message.contains("too many spaces"));
    }

    #[test]
    fn test_braces_valid_with_spaces() {
        let yaml = "object: { key: value }";

        let rule = BracesRule;
        let config = config_with_rule(
            RuleName::Braces,
            "{min-spaces-inside: 1, max-spaces-inside: 1}",
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_braces_empty_mapping() {
        let yaml = "object: {}";

        let rule = BracesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_braces_empty_with_spaces() {
        let yaml = "object: { }";

        let rule = BracesRule;
        let config = config_with_rule(
            RuleName::Braces,
            "{min-spaces-inside-empty: 1, max-spaces-inside-empty: 1}",
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_braces_nested() {
        let yaml = "object: {a: {b: c}}";

        let rule = BracesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_is_empty_mapping() {
        use crate::rules::flow_common::is_empty_collection;
        assert!(is_empty_collection("", 0, 0));
        assert!(is_empty_collection("{}", 1, 1));
        assert!(is_empty_collection("{  }", 1, 3));
        assert!(!is_empty_collection("{a}", 1, 2));
    }

    // Regression test for issue #102: location must not be hardcoded to 1:1
    #[test]
    fn test_braces_correct_location() {
        let yaml = "key: {  a: 1  }";

        let rule = BracesRule;
        let config = config_with_rule(RuleName::Braces, "{max-spaces-inside: 0}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_ne!(diagnostics, []);
        // The opening-side diagnostic must point to the `{`, not line 1 col 1
        let open_diag = diagnostics.iter().find(|d| d.span.start.offset() > 0);
        assert!(open_diag.is_some(), "diagnostic must have non-zero offset");
        // None of the diagnostics should report line 1 col 1 (offset 0)
        for d in &diagnostics {
            assert!(
                d.span.start.offset() > 0,
                "diagnostic at wrong location: {:?}",
                d.span,
            );
        }
    }

    // Regression test for issue #102: each violation fires only once
    #[test]
    fn test_braces_no_duplicate_diagnostics() {
        let yaml = "key: { a: 1 }";

        let rule = BracesRule;
        let config = config_with_rule(RuleName::Braces, "{max-spaces-inside: 0}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        // Two violations: space after `{` and space before `}`
        assert_eq!(diagnostics.len(), 2);
        // They must point to different locations
        assert_ne!(
            diagnostics[0].span.start.offset(),
            diagnostics[1].span.start.offset()
        );
    }

    // Regression test for issue #103: no false positives on template expressions
    #[test]
    fn test_braces_no_false_positive_on_template_expression() {
        let yaml = "key: ${{ github.ref }}";

        let rule = BracesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics on template expression: {diagnostics:?}"
        );
    }

    #[test]
    fn test_braces_no_false_positive_jinja2() {
        let yaml = "template: \"{{ variable }}\"";

        let rule = BracesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics on Jinja2-style template: {diagnostics:?}"
        );
    }

    // Regression test for issue #116
    #[test]
    fn test_braces_no_false_positive_in_block_scalar() {
        let yaml = "message: >\n  This has {braces} and more {braces} inside.\n";

        let rule = BracesRule;
        let config = config_with_rule(
            RuleName::Braces,
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
    fn test_braces_non_ascii_prefix_block_scalar_no_panic() {
        let yaml = "# ———\nrun: |\n  echo\n  ok\n  tail }\nc: {a: b}\n";
        let context = LintContext::new(yaml);
        let diagnostics = BracesRule.diagnose(&context, &LintConfig::default());
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn test_braces_nested_pairing() {
        let yaml = "a: {b: {}}\n";
        let context = LintContext::new(yaml);
        let config = config_with_rule(RuleName::Braces, "{forbid: non-empty}");
        let diagnostics = BracesRule.diagnose(&context, &config);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].span.start.column(), 4);
    }

    #[test]
    fn test_braces_empty_min_spaces_reported() {
        let yaml = "a: {}\n";
        let context = LintContext::new(yaml);
        let config = config_with_rule(RuleName::Braces, "{min-spaces-inside: 1}");
        let diagnostics = BracesRule.diagnose(&context, &config);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    }

    #[test]
    fn test_braces_non_ascii_key_location() {
        let yaml = "—: { a: 1 }\n";
        let config = config_with_rule(RuleName::Braces, "{forbid: non-empty}");
        let diagnostics = BracesRule.diagnose(&LintContext::new(yaml), &config);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        let start = diagnostics[0].span.start;
        assert_eq!((start.column(), start.offset()), (4, 5));
    }

    #[test]
    fn test_braces_multiline_pairs() {
        let yaml = "a: {\n  b: 1}\nc: {d: 2}\n";
        let context = LintContext::new(yaml);
        let diagnostics = BracesRule.diagnose(&context, &LintConfig::default());
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }
}
