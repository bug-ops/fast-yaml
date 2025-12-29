//! Rule to check key ordering in mappings.

use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Location, Severity,
    Span,
};
use fast_yaml_core::Value;
use std::collections::HashSet;

/// Linting rule for key ordering.
///
/// Checks if keys in mappings are alphabetically ordered.
/// This helps maintain consistency and makes it easier to find keys in large YAML files.
///
/// Configuration options:
/// - `case-sensitive`: boolean (default: true)
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{rules::KeyOrderingRule, rules::LintRule, LintConfig};
/// use fast_yaml_core::Parser;
///
/// let rule = KeyOrderingRule;
/// let yaml = "age: 30\nname: John";
/// let value = Parser::parse_str(yaml).unwrap().unwrap();
///
/// let config = LintConfig::default();
/// let context = fast_yaml_linter::LintContext::new(yaml);
/// let diagnostics = rule.check(&context, &value, &config);
/// assert!(!diagnostics.is_empty());  // Keys are not in alphabetical order
/// ```
pub struct KeyOrderingRule;

impl super::LintRule for KeyOrderingRule {
    fn code(&self) -> &str {
        DiagnosticCode::KEY_ORDERING
    }

    fn name(&self) -> &'static str {
        "Key Ordering"
    }

    fn description(&self) -> &'static str {
        "Checks if keys in mappings are alphabetically ordered"
    }

    fn default_severity(&self) -> Severity {
        Severity::Info
    }

    fn check(&self, context: &LintContext, value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        let source = context.source();
        let rule_config = config.get_rule_config(DiagnosticCode::KEY_ORDERING);

        let case_sensitive = rule_config
            .and_then(|rc| rc.options.get_bool("case-sensitive"))
            .unwrap_or(true);

        let mut diagnostics = Vec::new();

        // Check the value tree recursively
        self.check_value(
            value,
            context,
            source,
            case_sensitive,
            config,
            &mut diagnostics,
        );

        diagnostics
    }
}

impl KeyOrderingRule {
    /// Recursively checks a value tree for key ordering issues.
    #[allow(
        clippy::only_used_in_recursion,
        clippy::self_only_used_in_recursion,
        clippy::too_many_lines
    )]
    fn check_value(
        &self,
        value: &Value,
        context: &LintContext,
        source: &str,
        case_sensitive: bool,
        config: &LintConfig,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match value {
            Value::Mapping(hash) => {
                // Pre-build HashSet of hash keys for O(1) lookup
                let hash_keys: HashSet<&str> = hash.keys().filter_map(|v| v.as_str()).collect();

                // Build a mapping of keys to their line numbers from the source
                let mut key_positions: Vec<(String, usize)> = Vec::new();

                // Use cached lines and metadata from context
                let lines = context.lines();
                let line_metadata = context.line_metadata();

                // Extract keys from the source to preserve order and get positions
                for (line_idx, (line, metadata)) in lines.iter().zip(line_metadata).enumerate() {
                    let line_num = line_idx + 1;

                    // Skip empty lines and comments using cached metadata
                    if metadata.is_empty || metadata.is_comment {
                        continue;
                    }

                    let trimmed = line.trim_start();

                    // Check if this is a key-value line (contains ':' but not in a list)
                    if let Some(colon_pos) = trimmed.find(':') {
                        // Skip if this is a list item value (line starts with '- ')
                        if trimmed.starts_with("- ") && colon_pos > 2 {
                            // This is a list item with a mapping, recurse into it
                            continue;
                        }

                        let key_part = &trimmed[..colon_pos].trim();

                        // Skip if key is quoted (extract the content)
                        #[allow(clippy::if_same_then_else)]
                        let key = if key_part.starts_with('\'') && key_part.ends_with('\'') {
                            &key_part[1..key_part.len() - 1]
                        } else if key_part.starts_with('"') && key_part.ends_with('"') {
                            &key_part[1..key_part.len() - 1]
                        } else {
                            key_part
                        };

                        // O(1) lookup in HashSet instead of O(n) hash.contains_key
                        if hash_keys.contains(key) {
                            key_positions.push((key.to_string(), line_num));
                        }
                    }
                }

                // Check if keys are in alphabetical order
                let mut prev_key: Option<&str> = None;
                let mut prev_line: Option<usize> = None;

                for (key, line_num) in &key_positions {
                    if let Some(prev) = prev_key {
                        let current_cmp = if case_sensitive {
                            key.as_str()
                        } else {
                            &key.to_lowercase()
                        };

                        let prev_cmp = if case_sensitive {
                            prev
                        } else {
                            &prev.to_lowercase()
                        };

                        if current_cmp < prev_cmp {
                            // Found a key that should come before the previous one
                            let severity = config.get_effective_severity(
                                DiagnosticCode::KEY_ORDERING,
                                Severity::Info,
                            );
                            let line_offset = context.source_context().get_line_offset(*line_num);

                            let location = Location::new(*line_num, 1, line_offset);
                            let span = Span::new(
                                location,
                                Location::new(*line_num, 1, line_offset + key.len()),
                            );

                            diagnostics.push(
                                DiagnosticBuilder::new(
                                    DiagnosticCode::KEY_ORDERING,
                                    severity,
                                    format!(
                                        "key '{}' should be ordered before '{}' (line {})",
                                        key,
                                        prev,
                                        prev_line.unwrap()
                                    ),
                                    span,
                                )
                                .build(source),
                            );
                        }
                    }

                    prev_key = Some(key);
                    prev_line = Some(*line_num);
                }

                // Recursively check nested values
                for (_, nested_value) in hash {
                    self.check_value(
                        nested_value,
                        context,
                        source,
                        case_sensitive,
                        config,
                        diagnostics,
                    );
                }
            }
            Value::Sequence(arr) => {
                // Recursively check array elements
                for item in arr {
                    self.check_value(item, context, source, case_sensitive, config, diagnostics);
                }
            }
            _ => {
                // Scalar values don't have keys to check
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
    fn test_key_ordering_sorted() {
        let yaml = "age: 30\nname: John\nzip: 12345";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = KeyOrderingRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_key_ordering_unsorted() {
        let yaml = "name: John\nage: 30";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = KeyOrderingRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("should be ordered before"));
    }

    #[test]
    fn test_key_ordering_case_insensitive() {
        let yaml = "Name: John\nage: 30";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = KeyOrderingRule;
        let config = LintConfig::new().with_rule_config(
            "key-ordering",
            RuleConfig::new().with_option("case-sensitive", false),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(!diagnostics.is_empty());
    }

    #[test]
    fn test_key_ordering_case_sensitive() {
        let yaml = "Name: John\nage: 30";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = KeyOrderingRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // 'N' < 'a' in ASCII, so this is sorted
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_key_ordering_nested() {
        let yaml = "person:\n  name: John\n  age: 30";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = KeyOrderingRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // Nested keys should be checked
        assert!(!diagnostics.is_empty());
    }

    #[test]
    fn test_key_ordering_multiple_violations() {
        let yaml = "z: 1\ny: 2\nx: 3";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = KeyOrderingRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn test_key_ordering_single_key() {
        let yaml = "name: John";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = KeyOrderingRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_key_ordering_array_not_checked() {
        let yaml = "items:\n  - name: John\n  - age: 30";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = KeyOrderingRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // Array items are not checked for ordering
        assert!(diagnostics.is_empty());
    }
}
