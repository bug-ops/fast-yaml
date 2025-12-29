//! Rule to check truthy value representations.

use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Location, Severity,
    Span,
};
use fast_yaml_core::Value;
use std::collections::HashSet;

/// Non-standard boolean representations to detect
const NON_STANDARD_BOOLS: &[&str] = &[
    "yes", "no", "Yes", "No", "YES", "NO", "on", "off", "On", "Off", "ON", "OFF", "y", "n", "Y",
    "N", "True", "False", "TRUE", "FALSE",
];

/// Linting rule for truthy values.
///
/// Validates boolean value representations to ensure consistent usage.
/// YAML 1.2 standardizes on `true` and `false`, but YAML 1.1 allowed
/// many alternatives (yes/no, on/off, y/n, etc.) which can cause confusion.
///
/// Configuration options:
/// - `allowed-values`: list of allowed truthy representations (default: `["true", "false"]`)
/// - `check-keys`: whether to check keys too (default: false)
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{rules::TruthyRule, rules::LintRule, LintConfig, config::RuleConfig};
/// use fast_yaml_core::Parser;
///
/// let rule = TruthyRule;
/// let yaml = "enabled: true";
/// let value = Parser::parse_str(yaml).unwrap().unwrap();
///
/// let config = LintConfig::default();
/// let context = fast_yaml_linter::LintContext::new(yaml);
/// let diagnostics = rule.check(&context, &value, &config);
/// assert!(diagnostics.is_empty());
/// ```
pub struct TruthyRule;

impl super::LintRule for TruthyRule {
    fn code(&self) -> &str {
        DiagnosticCode::TRUTHY
    }

    fn name(&self) -> &'static str {
        "Truthy Values"
    }

    fn description(&self) -> &'static str {
        "Forbids non-standard truthy value representations (yes/no, on/off, y/n, etc.)"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    #[allow(clippy::too_many_lines)]
    fn check(&self, context: &LintContext, _value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        let source = context.source();
        let rule_config = config.get_rule_config(self.code());

        let allowed_values = rule_config
            .and_then(|rc| rc.options.get_string_list("allowed-values"))
            .map_or_else(
                || vec!["true".to_string(), "false".to_string()],
                std::borrow::ToOwned::to_owned,
            );

        let check_keys = rule_config
            .and_then(|rc| rc.options.get_bool("check-keys"))
            .unwrap_or(false);

        // Pre-build HashSets for O(1) lookup
        let non_standard_set: HashSet<&str> = NON_STANDARD_BOOLS.iter().copied().collect();
        let allowed_set: HashSet<&str> = allowed_values.iter().map(String::as_str).collect();

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

            // Find key-value pairs (after ':')
            if let Some(colon_pos) = line.find(':') {
                let key_part = &line[..colon_pos];
                let value_part = &line[colon_pos + 1..];

                // Check key if configured
                if check_keys {
                    let key_trimmed = key_part.trim();
                    if non_standard_set.contains(key_trimmed) && !allowed_set.contains(key_trimmed)
                    {
                        let key_start = line.find(key_trimmed).unwrap_or(0);
                        let offset = line_offset + key_start;
                        let severity =
                            config.get_effective_severity(self.code(), self.default_severity());

                        let location = Location::new(line_num, 1, offset);
                        let span = Span::new(
                            location,
                            Location::new(line_num, 1, offset + key_trimmed.len()),
                        );

                        diagnostics.push(
                            DiagnosticBuilder::new(
                                self.code(),
                                severity,
                                format!(
                                    "found non-standard truthy key '{key_trimmed}' (use {})",
                                    allowed_values.join(" or ")
                                ),
                                span,
                            )
                            .build(source),
                        );
                    }
                }

                // Check value
                let value_trimmed = value_part.trim();

                // Skip if empty, quoted, or starts with flow collection markers
                if value_trimmed.is_empty()
                    || value_trimmed.starts_with('"')
                    || value_trimmed.starts_with('\'')
                    || value_trimmed.starts_with('[')
                    || value_trimmed.starts_with('{')
                {
                    continue;
                }

                // Extract the value token (before any comment or space)
                let value_token = value_trimmed
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.split('#').next())
                    .unwrap_or(value_trimmed);

                if non_standard_set.contains(value_token) && !allowed_set.contains(value_token) {
                    let value_start = line.find(value_token).unwrap_or(colon_pos + 1);
                    let offset = line_offset + value_start;
                    let severity =
                        config.get_effective_severity(self.code(), self.default_severity());

                    let location = Location::new(line_num, 1, offset);
                    let span = Span::new(
                        location,
                        Location::new(line_num, 1, offset + value_token.len()),
                    );

                    diagnostics.push(
                        DiagnosticBuilder::new(
                            self.code(),
                            severity,
                            format!(
                                "found non-standard truthy value '{value_token}' (use {})",
                                allowed_values.join(" or ")
                            ),
                            span,
                        )
                        .build(source),
                    );
                }
            }

            // Check list items (after '- ')
            if let Some(hyphen_pos) = line.find('-') {
                let after_hyphen = &line[hyphen_pos + 1..];
                let value_trimmed = after_hyphen.trim();

                // Skip if this is a mapping key (contains ':')
                if value_trimmed.contains(':') {
                    continue;
                }

                // Skip if empty, quoted, or starts with flow collection markers
                if value_trimmed.is_empty()
                    || value_trimmed.starts_with('"')
                    || value_trimmed.starts_with('\'')
                    || value_trimmed.starts_with('[')
                    || value_trimmed.starts_with('{')
                {
                    continue;
                }

                let value_token = value_trimmed
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.split('#').next())
                    .unwrap_or(value_trimmed);

                if non_standard_set.contains(value_token) && !allowed_set.contains(value_token) {
                    let value_start = line.find(value_token).unwrap_or(hyphen_pos + 1);
                    let offset = line_offset + value_start;
                    let severity =
                        config.get_effective_severity(self.code(), self.default_severity());

                    let location = Location::new(line_num, 1, offset);
                    let span = Span::new(
                        location,
                        Location::new(line_num, 1, offset + value_token.len()),
                    );

                    diagnostics.push(
                        DiagnosticBuilder::new(
                            self.code(),
                            severity,
                            format!(
                                "found non-standard truthy value '{value_token}' (use {})",
                                allowed_values.join(" or ")
                            ),
                            span,
                        )
                        .build(source),
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
    use crate::{config::RuleConfig, rules::LintRule};
    use fast_yaml_core::Parser;

    #[test]
    fn test_truthy_standard_values() {
        let yaml = "enabled: true\ndisabled: false";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = TruthyRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_truthy_yes_no() {
        let yaml = "enabled: yes\ndisabled: no";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = TruthyRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics[0].message.contains("yes"));
        assert!(diagnostics[1].message.contains("no"));
    }

    #[test]
    fn test_truthy_on_off() {
        let yaml = "enabled: on\ndisabled: off";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = TruthyRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics[0].message.contains("on"));
        assert!(diagnostics[1].message.contains("off"));
    }

    #[test]
    fn test_truthy_capitalized() {
        let yaml = "enabled: True\ndisabled: FALSE";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = TruthyRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn test_truthy_quoted_allowed() {
        let yaml = "enabled: 'yes'\ndisabled: \"no\"";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = TruthyRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_truthy_custom_allowed_values() {
        let yaml = "enabled: yes\ndisabled: no";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = TruthyRule;
        let config = LintConfig::new().with_rule_config(
            "truthy",
            RuleConfig::new()
                .with_option("allowed-values", vec!["yes".to_string(), "no".to_string()]),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_truthy_list_items() {
        let yaml = "items:\n  - yes\n  - no\n  - true";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = TruthyRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics[0].message.contains("yes"));
        assert!(diagnostics[1].message.contains("no"));
    }

    #[test]
    fn test_truthy_check_keys() {
        let yaml = "yes: value\nno: other";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = TruthyRule;
        let config = LintConfig::new()
            .with_rule_config("truthy", RuleConfig::new().with_option("check-keys", true));

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics[0].message.contains("key"));
        assert!(diagnostics[1].message.contains("key"));
    }

    #[test]
    fn test_truthy_ignore_keys_by_default() {
        let yaml = "yes: value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = TruthyRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_truthy_with_comment() {
        let yaml = "enabled: yes  # This is a comment";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = TruthyRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("yes"));
    }

    #[test]
    fn test_truthy_single_letter() {
        let yaml = "enabled: y\ndisabled: n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = TruthyRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 2);
    }
}
