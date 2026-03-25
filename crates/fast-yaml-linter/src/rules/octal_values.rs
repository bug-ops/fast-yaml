//! Rule to check octal value representations.

use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Location, Severity,
    Span,
};
use fast_yaml_core::Value;

use super::LintRule as _;

/// Returns the portion of a YAML source line before any inline comment.
///
/// A `#` starts a comment only when preceded by whitespace (or at start of line).
/// This avoids stripping `#` characters inside quoted strings — the caller already
/// skips quoted values, so the approximation is sufficient for value scanning.
fn strip_inline_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'#' && (i == 0 || bytes[i - 1] == b' ' || bytes[i - 1] == b'\t') {
            return &line[..i];
        }
        i += 1;
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
        for (line_idx, line) in source.lines().enumerate() {
            let line_num = line_idx + 1;
            let line_offset = context.source_context().get_line_offset(line_num);
            self.check_line(
                context,
                config,
                &mut diagnostics,
                line,
                line_num,
                line_offset,
                forbid_implicit,
                forbid_explicit,
            );
        }
        diagnostics
    }
}

impl OctalValuesRule {
    #[allow(clippy::too_many_arguments)]
    fn check_line(
        &self,
        context: &LintContext,
        config: &LintConfig,
        diagnostics: &mut Vec<Diagnostic>,
        line: &str,
        line_num: usize,
        line_offset: usize,
        forbid_implicit: bool,
        forbid_explicit: bool,
    ) {
        // Strip inline comment before scanning for octal tokens.
        let line_without_comment = strip_inline_comment(line);

        // Skip pure comment lines.
        if line_without_comment.trim_start().starts_with('#') {
            return;
        }

        // Find value parts (after : or -)
        let parts: Vec<(usize, &str)> = line_without_comment.find(':').map_or_else(
            || {
                if line_without_comment.trim_start().starts_with('-') {
                    line_without_comment
                        .find('-')
                        .map_or_else(Vec::new, |hyphen_pos| {
                            vec![(hyphen_pos + 1, &line_without_comment[hyphen_pos + 1..])]
                        })
                } else {
                    vec![]
                }
            },
            |colon_pos| vec![(colon_pos + 1, &line_without_comment[colon_pos + 1..])],
        );

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
                let value_offset = line_offset + trim_offset_in_line;
                let col = trim_offset_in_line + 1;
                let severity = config.get_effective_severity(self.code(), self.default_severity());
                let span = Span::new(
                    Location::new(line_num, col, value_offset),
                    Location::new(
                        line_num,
                        col + value_token.len(),
                        value_offset + value_token.len(),
                    ),
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
                && value_token.starts_with('0')
                && value_token.len() > 1
                && !value_token.starts_with("0o")
                && !value_token.starts_with("0x")
            {
                let rest = &value_token[1..];
                if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit() && c < '8') {
                    let value_offset = line_offset + trim_offset_in_line;
                    let col = trim_offset_in_line + 1;
                    let severity =
                        config.get_effective_severity(self.code(), self.default_severity());
                    let span = Span::new(
                        Location::new(line_num, col, value_offset),
                        Location::new(
                            line_num,
                            col + value_token.len(),
                            value_offset + value_token.len(),
                        ),
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
