//! Rule to check float value representations.

use serde::{Deserialize, Serialize};

use crate::config::RuleOptions;
use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Location, Severity,
    Span,
};
use fast_yaml_core::Value;

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
/// use fast_yaml_linter::{rules::FloatValuesRule, rules::LintRule, LintConfig};
/// use fast_yaml_core::Parser;
///
/// let rule = FloatValuesRule;
/// let yaml = "value: 0.5";
/// let value = Parser::parse_str(yaml).unwrap().unwrap();
///
/// let config = LintConfig::default();
/// let context = fast_yaml_linter::LintContext::new(yaml);
/// let diagnostics = rule.check(&context, &value, &config);
/// assert!(diagnostics.is_empty());
/// ```
pub struct FloatValuesRule;

fn value_start(line: &str, part_offset: usize, token: &str) -> usize {
    part_offset
        + line
            .get(part_offset..)
            .and_then(|rest| rest.find(token))
            .unwrap_or(0)
}

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
    fn code(&self) -> &str {
        DiagnosticCode::FLOAT_VALUES
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

    #[allow(clippy::too_many_lines)]
    fn check(&self, context: &LintContext, _value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        let options = &config.rules.float_values.options;
        let require_numeral_before_decimal = options.require_numeral_before_decimal;
        let forbid_scientific_notation = options.forbid_scientific_notation;
        let forbid_nan = options.forbid_nan;
        let forbid_inf = options.forbid_inf;

        let mut diagnostics = Vec::new();

        // Use cached lines and metadata from context
        let lines = context.lines();
        let line_metadata = context.line_metadata();

        for (line_idx, (line, metadata)) in lines.iter().zip(line_metadata).enumerate() {
            let line_num = line_idx + 1;
            let line_offset = context.source_context().get_line_offset(line_num);

            // Skip comment lines using cached metadata
            if metadata.is_comment {
                continue;
            }

            // Find value parts (after : or -)
            let parts: Vec<(&str, usize)> = line
                .split_once(':')
                .or_else(|| line.split_once('-'))
                .map(|(before, after)| vec![(after, before.len() + 1)])
                .unwrap_or_default();

            for (part, part_offset) in parts {
                let trimmed = part.trim();

                // Skip if empty, quoted, or starts with flow collection markers
                if trimmed.is_empty()
                    || trimmed.starts_with('"')
                    || trimmed.starts_with('\'')
                    || trimmed.starts_with('[')
                    || trimmed.starts_with('{')
                {
                    continue;
                }

                // Extract the value token (before any comment or space)
                let value_token = trimmed
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.split('#').next())
                    .unwrap_or(trimmed);

                // Pre-compute lowercase once for all checks
                let value_lower = value_token.to_lowercase();

                // Check for missing numeral before decimal point
                let bare = value_token.trim_start_matches(['-', '+']);
                if require_numeral_before_decimal && bare.starts_with('.') {
                    // Check if it's a valid float starting with '.' (e.g. .5, -.5, +.5)
                    if bare.chars().nth(1).is_some_and(|c| c.is_ascii_digit()) {
                        let value_start = value_start(line, part_offset, value_token);
                        let offset = line_offset + value_start;
                        let severity = config
                            .rules
                            .float_values
                            .severity_or(self.default_severity());

                        let location = Location::new(line_num, 1, offset);
                        let span = Span::new(
                            location,
                            Location::new(line_num, 1, offset + value_token.len()),
                        );

                        let suggestion = match value_token.split_at_checked(1) {
                            Some((sign @ ("-" | "+"), rest)) => format!("{sign}0{rest}"),
                            _ => format!("0{value_token}"),
                        };
                        diagnostics.push(
                            DiagnosticBuilder::new(
                                self.code(),
                                severity,
                                format!(
                                    "float value '{value_token}' should have a numeral before the decimal point (e.g., '{suggestion}')"
                                ),
                                span,
                            )
                            .build_with_context(context.source_context()),
                        );
                    }
                }

                // Check for scientific notation
                if forbid_scientific_notation
                    && ((value_lower.contains('e') && value_token.parse::<f64>().is_ok())
                        || value_lower.ends_with("e+")
                        || value_lower.ends_with("e-"))
                {
                    let value_start = value_start(line, part_offset, value_token);
                    let offset = line_offset + value_start;
                    let severity = config
                        .rules
                        .float_values
                        .severity_or(self.default_severity());

                    let location = Location::new(line_num, 1, offset);
                    let span = Span::new(
                        location,
                        Location::new(line_num, 1, offset + value_token.len()),
                    );

                    diagnostics.push(
                        DiagnosticBuilder::new(
                            self.code(),
                            severity,
                            format!("scientific notation '{value_token}' is forbidden"),
                            span,
                        )
                        .build_with_context(context.source_context()),
                    );
                }

                // Check for NaN
                if forbid_nan && matches!(value_lower.as_str(), ".nan" | "nan") {
                    let value_start = value_start(line, part_offset, value_token);
                    let offset = line_offset + value_start;
                    let severity = config
                        .rules
                        .float_values
                        .severity_or(self.default_severity());

                    let location = Location::new(line_num, 1, offset);
                    let span = Span::new(
                        location,
                        Location::new(line_num, 1, offset + value_token.len()),
                    );

                    diagnostics.push(
                        DiagnosticBuilder::new(
                            self.code(),
                            severity,
                            "NaN (not a number) is forbidden",
                            span,
                        )
                        .build_with_context(context.source_context()),
                    );
                }

                // Check for Infinity
                if forbid_inf
                    && matches!(
                        value_lower.as_str(),
                        ".inf" | "-.inf" | "+.inf" | "inf" | "-inf" | "+inf"
                    )
                {
                    let value_start = value_start(line, part_offset, value_token);
                    let offset = line_offset + value_start;
                    let severity = config
                        .rules
                        .float_values
                        .severity_or(self.default_severity());

                    let location = Location::new(line_num, 1, offset);
                    let span = Span::new(
                        location,
                        Location::new(line_num, 1, offset + value_token.len()),
                    );

                    diagnostics.push(
                        DiagnosticBuilder::new(
                            self.code(),
                            severity,
                            "Infinity is forbidden",
                            span,
                        )
                        .build_with_context(context.source_context()),
                    );
                }
            }
        }

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
    fn test_float_values_valid() {
        let yaml = "value: 0.5\npi: 3.14159";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = FloatValuesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_float_values_missing_numeral() {
        let yaml = "value: .5";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = FloatValuesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(!diagnostics.is_empty());
        assert!(
            diagnostics[0]
                .message
                .contains("numeral before the decimal point")
        );
    }

    #[test]
    fn test_float_values_allow_missing_numeral() {
        let yaml = "value: .5";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = FloatValuesRule;
        let config = config_with_rule(
            RuleName::FloatValues,
            "{require-numeral-before-decimal: false}",
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_float_values_scientific_notation() {
        let yaml = "value: 1.5e10\nanother: 3.14e-5";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = FloatValuesRule;
        let config = config_with_rule(RuleName::FloatValues, "{forbid-scientific-notation: true}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics[0].message.contains("scientific notation"));
    }

    #[test]
    fn test_float_values_allow_scientific_notation() {
        let yaml = "value: 1.5e10";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = FloatValuesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_float_values_nan() {
        let yaml = "value: .nan\nanother: NaN";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = FloatValuesRule;
        let config = config_with_rule(RuleName::FloatValues, "{forbid-nan: true}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics[0].message.contains("NaN"));
    }

    #[test]
    fn test_float_values_infinity() {
        let yaml = "value: .inf\nneg: -.inf\npos: +.inf";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = FloatValuesRule;
        let config = config_with_rule(RuleName::FloatValues, "{forbid-inf: true}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 3);
        assert!(diagnostics[0].message.contains("Infinity"));
    }

    #[test]
    fn test_float_values_allow_nan_inf() {
        let yaml = "value: .nan\ninf: .inf";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = FloatValuesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_float_values_quoted() {
        let yaml = "value: '.5'\nscientific: \"1.5e10\"";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = FloatValuesRule;
        let config = config_with_rule(
            RuleName::FloatValues,
            "{require-numeral-before-decimal: true, forbid-scientific-notation: true}",
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // Quoted values should be ignored
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_float_values_list_item() {
        let yaml = "items:\n  - .5\n  - 1.5e10";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = FloatValuesRule;
        let config = config_with_rule(
            RuleName::FloatValues,
            "{require-numeral-before-decimal: true, forbid-scientific-notation: true}",
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn test_float_values_with_comment() {
        let yaml = "value: .5  # half";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = FloatValuesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(!diagnostics.is_empty());
    }

    #[test]
    fn test_float_values_signed_missing_numeral() {
        let yaml = "bad1: .5\nbad2: -.5\nbad3: +.5\ngood: 0.5";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = FloatValuesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(
            diagnostics.len(),
            3,
            "expected diagnostics for .5, -.5, +.5"
        );
    }

    #[test]
    fn test_float_values_signed_suggestion() {
        let yaml = "neg: -.5\npos: +.5";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = FloatValuesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
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
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = FloatValuesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }
}
