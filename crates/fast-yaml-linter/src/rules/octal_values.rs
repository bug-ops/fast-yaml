//! Rule to check octal value representations.

use serde::{Deserialize, Serialize};

use crate::config::RuleOptions;
use crate::context::lines_of;
use crate::source::offset::ByteOffset;
use crate::{Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Severity};
use fast_yaml_core::Value;

use super::LintRule as _;

/// Returns the portion of a YAML source line before any inline comment.
///
/// A `#` starts a comment only when preceded by whitespace (or at start of line).
/// This avoids stripping `#` characters inside quoted strings — the caller already
/// skips quoted values, so the approximation is sufficient for value scanning.
fn strip_inline_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let comment_start = bytes.iter().enumerate().position(|(i, &b)| {
        b == b'#'
            && i.checked_sub(1)
                .and_then(|prev| bytes.get(prev))
                .is_none_or(|&p| p == b' ' || p == b'\t')
    });
    if let Some(i) = comment_start {
        return line.get(..i).unwrap_or(line);
    }
    line
}

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
/// use fast_yaml_linter::{rules::OctalValuesRule, rules::LintRule, LintConfig};
/// use fast_yaml_core::Parser;
///
/// let rule = OctalValuesRule;
/// let yaml = "code: '010'";  // Quoted, so valid
/// let value = Parser::parse_str(yaml).unwrap().unwrap();
///
/// let config = LintConfig::default();
/// let diagnostics = rule.check(&fast_yaml_linter::LintContext::new(yaml), &value, &config);
/// assert!(diagnostics.is_empty());
/// ```
pub struct OctalValuesRule;

/// Options of the octal-values rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", default)]
pub struct OctalValuesOptions {
    /// Flag implicit octals such as `0755`.
    pub forbid_implicit_octal: bool,
    /// Flag explicit octals such as `0o755`.
    pub forbid_explicit_octal: bool,
}

impl Default for OctalValuesOptions {
    fn default() -> Self {
        Self {
            forbid_implicit_octal: true,
            forbid_explicit_octal: true,
        }
    }
}

impl RuleOptions for OctalValuesOptions {}

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
        let options = &config.rules.octal_values.options;
        let forbid_implicit = options.forbid_implicit_octal;
        let forbid_explicit = options.forbid_explicit_octal;

        if !forbid_implicit && !forbid_explicit {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for (line_idx, line) in lines_of(source).enumerate() {
            let line_num = line_idx + 1;
            let line_offset = context.source_context().get_line_offset(line_num);
            self.check_line(context, config, &mut diagnostics, line, line_offset);
        }
        diagnostics
    }
}

impl OctalValuesRule {
    fn check_line(
        &self,
        context: &LintContext,
        config: &LintConfig,
        diagnostics: &mut Vec<Diagnostic>,
        line: &str,
        line_offset: usize,
    ) {
        let options = &config.rules.octal_values.options;
        let forbid_implicit = options.forbid_implicit_octal;
        let forbid_explicit = options.forbid_explicit_octal;

        // Strip inline comment before scanning for octal tokens.
        let line_without_comment = strip_inline_comment(line);

        // Skip pure comment lines.
        if line_without_comment.trim_start().starts_with('#') {
            return;
        }

        // Find value parts (after : or -)
        let parts: Vec<(usize, &str)> = line_without_comment
            .split_once(':')
            .or_else(|| {
                line_without_comment
                    .trim_start()
                    .starts_with('-')
                    .then(|| line_without_comment.split_once('-'))
                    .flatten()
            })
            .map(|(before, after)| vec![(before.len() + 1, after)])
            .unwrap_or_default();

        for (part_start_in_line, part) in parts {
            let trimmed = part.trim_start();

            // Skip if empty or quoted
            if trimmed.is_empty()
                || trimmed.starts_with('"')
                || trimmed.starts_with('\'')
                || trimmed.starts_with('[')
                || trimmed.starts_with('{')
            {
                continue;
            }

            // Byte offset of `trimmed` within `line`
            let trim_offset_in_line = part_start_in_line + (part.len() - part.trim_start().len());

            // Extract the value token (before any space)
            let value_token = trimmed.split_whitespace().next().unwrap_or(trimmed);

            // Check for explicit octal (0o prefix)
            if forbid_explicit
                && value_token.starts_with("0o")
                && let Some(rest) = value_token.strip_prefix("0o")
                && rest.chars().all(|c| c.is_ascii_digit() && c < '8')
            {
                let severity = config
                    .rules
                    .octal_values
                    .severity_or(self.default_severity());
                let span = context.source_context().span_at(
                    ByteOffset::new(line_offset + trim_offset_in_line),
                    value_token.len(),
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
                    .build_with_context(context.source_context()),
                );
            }

            // Check for implicit octal (leading zero followed by octal digits)
            if forbid_implicit
                && let Some(rest) = value_token.strip_prefix('0')
                && !rest.is_empty()
                && !value_token.starts_with("0o")
                && !value_token.starts_with("0x")
                && rest.chars().all(|c| c.is_ascii_digit() && c < '8')
            {
                let severity = config
                    .rules
                    .octal_values
                    .severity_or(self.default_severity());
                let span = context.source_context().span_at(
                    ByteOffset::new(line_offset + trim_offset_in_line),
                    value_token.len(),
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
                    .build_with_context(context.source_context()),
                );
            }
        }
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
    fn test_octal_values_quoted_valid() {
        let yaml = "code: '010'";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = OctalValuesRule;
        let config = LintConfig::default();

        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_octal_values_implicit_octal() {
        let yaml = "code: 010";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = OctalValuesRule;
        let config = LintConfig::default();

        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);
        assert_ne!(diagnostics, []);
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
        assert_ne!(diagnostics, []);
        assert!(diagnostics[0].message.contains("explicit octal"));
    }

    #[test]
    fn test_octal_values_allow_implicit() {
        let yaml = "code: 010";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = OctalValuesRule;
        let config = config_with_rule(RuleName::OctalValues, "{forbid-implicit-octal: false}");

        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_octal_values_allow_explicit() {
        let yaml = "code: 0o10";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = OctalValuesRule;
        let config = config_with_rule(RuleName::OctalValues, "{forbid-explicit-octal: false}");

        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_octal_values_decimal_valid() {
        let yaml = "code: 10";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = OctalValuesRule;
        let config = LintConfig::default();

        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_octal_values_hex_valid() {
        let yaml = "code: 0x10";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = OctalValuesRule;
        let config = LintConfig::default();

        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_octal_values_zero_valid() {
        let yaml = "code: 0";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = OctalValuesRule;
        let config = LintConfig::default();

        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);
        assert_eq!(diagnostics, []);
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
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_octal_values_list_item() {
        let yaml = "items:\n  - 010";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = OctalValuesRule;
        let config = LintConfig::default();

        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);
        assert_ne!(diagnostics, []);
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
        assert_ne!(diagnostics, []);
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

    // Regression tests for issue #176: false positive on octal patterns in comments.

    #[test]
    fn test_octal_values_no_false_positive_in_comment_line() {
        // A pure comment line containing 0o755 must not trigger a diagnostic.
        let yaml = "# permissions: 0o755 is octal\nmode: 7";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = OctalValuesRule;
        let config = LintConfig::default();

        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);
        assert!(
            diagnostics.is_empty(),
            "expected no diagnostics for octal pattern in comment line, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_octal_values_no_false_positive_in_inline_comment() {
        // An inline comment after a valid value must not trigger a diagnostic.
        let yaml = "mode: 7 # was 0o755 before";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = OctalValuesRule;
        let config = LintConfig::default();

        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);
        assert!(
            diagnostics.is_empty(),
            "expected no diagnostics for octal pattern in inline comment, got: {diagnostics:?}"
        );
    }

    // Regression tests for issue #177: diagnostic position must point to the value, not the key.

    #[test]
    fn test_octal_values_explicit_correct_position() {
        // "mode: 0o755" — the value '0o755' starts at column 7 (1-indexed), offset 6.
        let yaml = "mode: 0o755";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = OctalValuesRule;
        let config = LintConfig::default();

        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);
        assert!(!diagnostics.is_empty(), "expected a diagnostic for 0o755");
        let span = diagnostics[0].span;
        assert_eq!(
            span.start.column, 7,
            "expected column 7 for octal value, got {}",
            span.start.column
        );
        assert_eq!(
            span.start.offset, 6,
            "expected offset 6 for octal value, got {}",
            span.start.offset
        );
    }

    #[test]
    fn test_octal_values_implicit_correct_position() {
        // "perm: 0755" — the value '0755' starts at column 7 (1-indexed), offset 6.
        let yaml = "perm: 0755";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = OctalValuesRule;
        let config = LintConfig::default();

        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);
        assert!(!diagnostics.is_empty(), "expected a diagnostic for 0755");
        let span = diagnostics[0].span;
        assert_eq!(
            span.start.column, 7,
            "expected column 7 for octal value, got {}",
            span.start.column
        );
        assert_eq!(
            span.start.offset, 6,
            "expected offset 6 for octal value, got {}",
            span.start.offset
        );
    }
}
