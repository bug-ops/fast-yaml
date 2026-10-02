//! Rule to check float value representations.

use super::RuleId;
use crate::config::RuleName;
use serde::{Deserialize, Serialize};

use super::node_roles::NodeRole;
use crate::config::RuleOptions;
use crate::nodes::{Node, TagKind};
use crate::{Finding, LintConfig, LintContext, Severity};
use fast_yaml_core::{ResolvedScalar, ScalarStyle, resolve_scalar};

/// Linting rule for float values.
///
/// Validates float number representations to ensure consistent formatting.
/// Helps avoid ambiguity and enforces clear numeric formatting conventions.
///
/// Configuration options:
/// - `require-numeral-before-decimal`: boolean (default: true) - reject ".5", require "0.5"
/// - `forbid-scientific-notation`: boolean (default: false)
/// - `forbid-nan`: boolean (default: false)
/// - `forbid-inf`: boolean (default: false)
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{rules::FloatValuesRule, rules::SourceRule, LintConfig};
/// use fast_yaml_core::Parser;
///
/// let rule = FloatValuesRule;
/// let yaml = "value: 0.5";
///
/// let config = LintConfig::default();
/// let context = fast_yaml_linter::LintContext::new(yaml);
/// let diagnostics = rule.diagnose(&context, &config);
/// assert!(diagnostics.is_empty());
/// ```
pub struct FloatValuesRule;

/// Options of the float-values rule.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", default)]
pub struct FloatValuesOptions {
    /// Require a digit before the decimal point.
    pub require_numeral_before_decimal: bool,
    /// Flag scientific notation.
    pub forbid_scientific_notation: bool,
    /// Flag `.nan`.
    pub forbid_nan: bool,
    /// Flag `.inf`.
    pub forbid_inf: bool,
}

impl Default for FloatValuesOptions {
    fn default() -> Self {
        Self {
            require_numeral_before_decimal: true,
            forbid_scientific_notation: false,
            forbid_nan: false,
            forbid_inf: false,
        }
    }
}

impl RuleOptions for FloatValuesOptions {}

impl super::LintRule for FloatValuesRule {
    fn id(&self) -> RuleId<'_> {
        RuleId::BuiltIn(RuleName::FloatValues)
    }

    fn name(&self) -> &'static str {
        "Float Values"
    }

    fn description(&self) -> &'static str {
        "Validates float number representations (decimal point, scientific notation, NaN, Inf)"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }
}

impl super::SourceRule for FloatValuesRule {
    fn check(&self, context: &LintContext, config: &LintConfig) -> Vec<Finding> {
        let options = &config.rules.float_values.options;
        let source_context = context.source_context();
        let index = context.nodes();

        let mut diagnostics = Vec::new();
        for node in index.nodes() {
            let Node::Scalar(scalar) = node else {
                continue;
            };
            if scalar.role == NodeRole::MappingKey
                || scalar.tag != TagKind::None
                || scalar.style != ScalarStyle::Plain
            {
                continue;
            }
            let text = index.text(scalar);
            let ResolvedScalar::Float(float) = resolve_scalar(text, ScalarStyle::Plain, None)
            else {
                continue;
            };
            let span = source_context.span_of_bytes(scalar.range);
            for msg in messages(text, float, options) {
                diagnostics.push(Finding::new(msg, span));
            }
        }

        diagnostics
    }
}

/// Builds the diagnostic messages that `options` raise for the float scalar `text`.
fn messages(text: &str, float: f64, options: &FloatValuesOptions) -> Vec<String> {
    let bare = text.trim_start_matches(['-', '+']);
    if float.is_nan() {
        return options
            .forbid_nan
            .then(|| "NaN (not a number) is forbidden".to_owned())
            .into_iter()
            .collect();
    }
    if float.is_infinite() && bare.starts_with('.') {
        return options
            .forbid_inf
            .then(|| "Infinity is forbidden".to_owned())
            .into_iter()
            .collect();
    }
    let mut found = Vec::new();
    if options.require_numeral_before_decimal && bare.starts_with('.') {
        let suggestion = match text.split_at_checked(1) {
            Some((sign @ ("-" | "+"), rest)) => format!("{sign}0{rest}"),
            _ => format!("0{text}"),
        };
        found.push(format!(
            "float value '{text}' should have a numeral before the decimal point (e.g., '{suggestion}')"
        ));
    }
    if options.forbid_scientific_notation && bare.contains(['e', 'E']) {
        found.push(format!("scientific notation '{text}' is forbidden"));
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{RuleName, test_support::config_with_rule},
        rules::SourceRule,
    };

    #[test]
    fn test_float_values_valid() {
        let yaml = "value: 0.5\npi: 3.14159";

        let rule = FloatValuesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_float_values_missing_numeral() {
        let yaml = "value: .5";

        let rule = FloatValuesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_ne!(diagnostics, []);
        assert!(
            diagnostics[0]
                .message
                .contains("numeral before the decimal point")
        );
    }

    #[test]
    fn test_float_values_allow_missing_numeral() {
        let yaml = "value: .5";

        let rule = FloatValuesRule;
        let config = config_with_rule(
            RuleName::FloatValues,
            "{require-numeral-before-decimal: false}",
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_float_values_scientific_notation() {
        let yaml = "value: 1.5e10\nanother: 3.14e-5";

        let rule = FloatValuesRule;
        let config = config_with_rule(RuleName::FloatValues, "{forbid-scientific-notation: true}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics[0].message.contains("scientific notation"));
    }

    #[test]
    fn test_float_values_allow_scientific_notation() {
        let yaml = "value: 1.5e10";

        let rule = FloatValuesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_float_values_nan() {
        let yaml = "value: .nan\nanother: .NaN";

        let rule = FloatValuesRule;
        let config = config_with_rule(RuleName::FloatValues, "{forbid-nan: true}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics[0].message.contains("NaN"));
    }

    #[test]
    fn test_float_values_infinity() {
        let yaml = "value: .inf\nneg: -.inf\npos: +.inf";

        let rule = FloatValuesRule;
        let config = config_with_rule(RuleName::FloatValues, "{forbid-inf: true}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics.len(), 3);
        assert!(diagnostics[0].message.contains("Infinity"));
    }

    #[test]
    fn test_float_values_allow_nan_inf() {
        let yaml = "value: .nan\ninf: .inf";

        let rule = FloatValuesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_float_values_quoted() {
        let yaml = "value: '.5'\nscientific: \"1.5e10\"";

        let rule = FloatValuesRule;
        let config = config_with_rule(
            RuleName::FloatValues,
            "{require-numeral-before-decimal: true, forbid-scientific-notation: true}",
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        // Quoted values should be ignored
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_float_values_list_item() {
        let yaml = "items:\n  - .5\n  - 1.5e10";

        let rule = FloatValuesRule;
        let config = config_with_rule(
            RuleName::FloatValues,
            "{require-numeral-before-decimal: true, forbid-scientific-notation: true}",
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn test_float_values_with_comment() {
        let yaml = "value: .5  # half";

        let rule = FloatValuesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_ne!(diagnostics, []);
    }

    #[test]
    fn test_float_values_signed_missing_numeral() {
        let yaml = "bad1: .5\nbad2: -.5\nbad3: +.5\ngood: 0.5";

        let rule = FloatValuesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(
            diagnostics.len(),
            3,
            "expected diagnostics for .5, -.5, +.5"
        );
    }

    #[test]
    fn test_float_values_signed_suggestion() {
        let yaml = "neg: -.5\npos: +.5";

        let rule = FloatValuesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics.len(), 2, "expected diagnostics for -.5 and +.5");
        assert!(
            diagnostics[0].message.contains("-0.5"),
            "suggestion for -.5 should be -0.5, got: {}",
            diagnostics[0].message
        );
        assert!(
            diagnostics[1].message.contains("+0.5"),
            "suggestion for +.5 should be +0.5, got: {}",
            diagnostics[1].message
        );
    }

    #[test]
    fn test_float_values_integer_not_flagged() {
        let yaml = "value: 5\nanother: 100";

        let rule = FloatValuesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    fn lint_messages(yaml: &str, options: &str) -> Vec<String> {
        let config = config_with_rule(RuleName::FloatValues, options);
        FloatValuesRule
            .diagnose(&LintContext::new(yaml), &config)
            .into_iter()
            .map(|d| d.message.into_owned())
            .collect()
    }

    #[test]
    fn strings_that_are_not_core_floats_are_ignored() {
        let options = "{forbid-nan: true, forbid-inf: true, forbid-scientific-notation: true}";
        assert_eq!(
            lint_messages("a: .5e\nb: 1e+\nc: NaN\nd: inf\ne: 1e\nf: .e5", options),
            [] as [String; 0]
        );
    }

    #[test]
    fn flow_items_and_root_floats_are_checked() {
        assert_eq!(lint_messages("[.5, 1.5]", "{}").len(), 1);
        assert_eq!(lint_messages(".5", "{}").len(), 1);
        assert_eq!(lint_messages("{a: .5}", "{}").len(), 1);
    }

    #[test]
    fn keys_are_not_checked() {
        assert_eq!(lint_messages(".5: x", "{}"), [] as [String; 0]);
    }

    #[test]
    fn every_applicable_diagnostic_is_reported() {
        let opts = "{forbid-scientific-notation: true}";
        assert_eq!(lint_messages("a: .5e3", opts).len(), 2);
        assert_eq!(lint_messages("a: -.5E-3", opts).len(), 2);
    }

    #[test]
    fn tagged_scalars_are_skipped() {
        let opts = "{forbid-nan: true, forbid-scientific-notation: true}";
        let yaml = "a: !!float .5\nb: !custom .5\nc: !!float .nan\nd: !!float 1e3";
        assert_eq!(lint_messages(yaml, opts), [] as [String; 0]);
    }

    #[test]
    fn negative_strings_and_alias_between_keys() {
        let opts = "{forbid-nan: true, forbid-scientific-notation: true}";
        assert_eq!(
            lint_messages("- -.5e\n- -NaN\n- -1e+\n- '.5'\n", opts),
            [] as [String; 0]
        );
        assert_eq!(lint_messages("x: &v 1.5\ny: *v\nz: .5\n", "{}").len(), 1);
        assert_eq!(lint_messages("[.5]", "{}").len(), 1);
    }
}
