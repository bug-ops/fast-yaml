//! Rule to check octal value representations.

use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Location, Severity,
    Span,
};
use fast_yaml_core::Value;

/// Linting rule for octal values.
///
/// Forbids unquoted octal numbers to prevent ambiguity:
/// - Implicit octal: `010` (YAML 1.1 style, leading zero)
/// - Explicit octal: `0o10` (YAML 1.2 style, 0o prefix)
///
/// Configuration options:
/// - `forbid-implicit-octal`: bool (default: true)
/// - `forbid-explicit-octal`: bool (default: true)
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{rules::OctalValuesRule, rules::LintRule, LintConfig, config::RuleConfig};
/// use fast_yaml_core::Parser;
///
/// let rule = OctalValuesRule;
/// let yaml = "code: '010'";  // Quoted, so valid
/// let value = Parser::parse_str(yaml).unwrap().unwrap();
///
/// let config = LintConfig::default();
/// let diagnostics = rule.check(yaml, &value, &config);
/// assert!(diagnostics.is_empty());
/// ```
pub struct OctalValuesRule;

impl super::LintRule for OctalValuesRule {
    fn code(&self) -> &str {
        DiagnosticCode::OCTAL_VALUES
    }

    fn name(&self) -> &'static str {
        "Octal Values"
    }

    fn description(&self) -> &'static str {
        "Forbids unquoted octal numbers (implicit 010, explicit 0o10)"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, context: &LintContext, _value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        let source = context.source();
        let rule_config = config.get_rule_config(self.code());
        let forbid_implicit = rule_config
            .and_then(|rc| rc.options.get_bool("forbid-implicit-octal"))
            .unwrap_or(true);

        let forbid_explicit = rule_config
            .and_then(|rc| rc.options.get_bool("forbid-explicit-octal"))
            .unwrap_or(true);

        if !forbid_implicit && !forbid_explicit {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        let _source_context = context.source_context();

        // Pattern: value part after colon or hyphen
        for (line_idx, line) in source.lines().enumerate() {
            let line_num = line_idx + 1;
            let line_offset = context.source_context().get_line_offset(line_num);

            // Find value parts (after : or -)
            let parts: Vec<_> = line.find(':').map_or_else(
                || {
                    if line.trim_start().starts_with('-') {
                        line.find('-')
                            .map_or_else(Vec::new, |hyphen_pos| vec![&line[hyphen_pos + 1..]])
                    } else {
                        vec![]
                    }
                },
                |colon_pos| vec![&line[colon_pos + 1..]],
            );

            for part in parts {
                let trimmed = part.trim();

                // Skip if empty or quoted
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

                // Check for explicit octal (0o prefix)
                if forbid_explicit
                    && value_token.starts_with("0o")
                    && let Some(rest) = value_token.strip_prefix("0o")
                    && rest.chars().all(|c| c.is_ascii_digit() && c < '8')
                {
                    let value_offset = line_offset + line.find(value_token).unwrap_or(0);
                    let severity =
                        config.get_effective_severity(self.code(), self.default_severity());

                    let location = Location::new(line_num, 1, value_offset);
                    let span = Span::new(
                        location,
                        Location::new(line_num, 1, value_offset + value_token.len()),
                    );

                    diagnostics.push(
                        DiagnosticBuilder::new(
                            self.code(),
                            severity,
                            format!(
                                "found explicit octal value '{value_token}' (use quoted string to avoid ambiguity)"
                            ),
                            span,
                        )
                        .build(source),
                    );
                }

                // Check for implicit octal (leading zero followed by digits)
                if forbid_implicit
                    && value_token.starts_with('0')
                    && value_token.len() > 1
                    && !value_token.starts_with("0o")
                    && !value_token.starts_with("0x")
                {
                    // Check if all chars after '0' are octal digits
                    let rest = &value_token[1..];
                    if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit() && c < '8') {
                        let value_offset = line_offset + line.find(value_token).unwrap_or(0);
                        let severity =
                            config.get_effective_severity(self.code(), self.default_severity());

                        let location = Location::new(line_num, 1, value_offset);
                        let span = Span::new(
                            location,
                            Location::new(line_num, 1, value_offset + value_token.len()),
                        );

                        diagnostics.push(
                            DiagnosticBuilder::new(
                                self.code(),
                                severity,
                                format!(
                                    "found implicit octal value '{value_token}' (use quoted string or explicit '0o' prefix)"
                                ),
                                span,
                            )
                            .build(source),
                        );
                    }
                }
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::RuleConfig, rules::LintRule};
    use fast_yaml_core::Parser;

    #[test]
    fn test_octal_values_quoted_valid() {
        let yaml = "code: '010'";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = OctalValuesRule;
        let config = LintConfig::default();

        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_octal_values_implicit_octal() {
        let yaml = "code: 010";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = OctalValuesRule;
        let config = LintConfig::default();

        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("implicit octal"));
    }

    #[test]
    fn test_octal_values_explicit_octal() {
        let yaml = "code: 0o10";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = OctalValuesRule;
        let config = LintConfig::default();

        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("explicit octal"));
    }

    #[test]
    fn test_octal_values_allow_implicit() {
        let yaml = "code: 010";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = OctalValuesRule;
        let config = LintConfig::new().with_rule_config(
            "octal-values",
            RuleConfig::new().with_option("forbid-implicit-octal", false),
        );

        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_octal_values_allow_explicit() {
        let yaml = "code: 0o10";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = OctalValuesRule;
        let config = LintConfig::new().with_rule_config(
            "octal-values",
            RuleConfig::new().with_option("forbid-explicit-octal", false),
        );

        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_octal_values_decimal_valid() {
        let yaml = "code: 10";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = OctalValuesRule;
        let config = LintConfig::default();

        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_octal_values_hex_valid() {
        let yaml = "code: 0x10";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = OctalValuesRule;
        let config = LintConfig::default();

        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_octal_values_zero_valid() {
        let yaml = "code: 0";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = OctalValuesRule;
        let config = LintConfig::default();

        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_octal_values_invalid_octal_digits() {
        let yaml = "code: 089";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = OctalValuesRule;
        let config = LintConfig::default();

        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);
        // 089 is not valid octal (8 and 9 are not octal digits), so should be allowed
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_octal_values_list_item() {
        let yaml = "items:\n  - 010";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = OctalValuesRule;
        let config = LintConfig::default();

        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("implicit octal"));
    }

    #[test]
    fn test_octal_values_with_comment() {
        let yaml = "code: 010  # This is a comment";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = OctalValuesRule;
        let config = LintConfig::default();

        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("implicit octal"));
    }

    #[test]
    fn test_octal_values_multiple() {
        let yaml = "code1: 010\ncode2: 0o20";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = OctalValuesRule;
        let config = LintConfig::default();

        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);
        assert_eq!(diagnostics.len(), 2);
    }
}
