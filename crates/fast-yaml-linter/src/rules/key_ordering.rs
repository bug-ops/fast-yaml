//! Rule to check key ordering in mappings.

use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Location, Severity,
    Span,
};
use fast_yaml_core::Value;

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
        let mut cursor = 1usize;

        check_value(
            value,
            context,
            source,
            case_sensitive,
            config,
            &mut diagnostics,
            &mut cursor,
        );
        diagnostics
    }
}

/// Recursively walks `value` and emits ordering diagnostics.
///
/// `cursor` is a 1-based source line index that advances after each key is
/// located. Searching forward from the cursor scopes each mapping's key search
/// to its own position in the document, preventing duplicate diagnostics when
/// the same key name appears in multiple mappings (#105).
#[allow(clippy::too_many_arguments)]
fn check_value(
    value: &Value,
    context: &LintContext<'_>,
    source: &str,
    case_sensitive: bool,
    config: &LintConfig,
    diagnostics: &mut Vec<Diagnostic>,
    cursor: &mut usize,
) {
    match value {
        Value::Mapping(hash) => {
            let key_positions = locate_keys(hash, context, cursor);
            emit_ordering_diagnostics(
                &key_positions,
                context,
                source,
                case_sensitive,
                config,
                diagnostics,
            );

            for (_, nested_value) in hash {
                check_value(
                    nested_value,
                    context,
                    source,
                    case_sensitive,
                    config,
                    diagnostics,
                    cursor,
                );
            }
        }
        Value::Sequence(arr) => {
            for item in arr {
                check_value(
                    item,
                    context,
                    source,
                    case_sensitive,
                    config,
                    diagnostics,
                    cursor,
                );
            }
        }
        _ => {}
    }
}

/// Locates each key of `hash` in the source, scanning forward from `*cursor`.
///
/// Returns `(key_name, line_number)` pairs in document order.
/// `*cursor` is advanced past each located key.
fn locate_keys(
    hash: &fast_yaml_core::Map,
    context: &LintContext<'_>,
    cursor: &mut usize,
) -> Vec<(String, usize)> {
    let lines = context.lines();
    let line_metadata = context.line_metadata();
    let mut positions: Vec<(String, usize)> = Vec::new();

    for key_value in hash.keys() {
        let Some(key) = key_value.as_str() else {
            continue;
        };

        for (line_idx, (line, metadata)) in lines
            .iter()
            .zip(line_metadata)
            .enumerate()
            .skip(*cursor - 1)
        {
            let line_num = line_idx + 1;

            if metadata.is_empty || metadata.is_comment {
                continue;
            }

            let trimmed = line.trim_start();
            let Some(colon_pos) = trimmed.find(':') else {
                continue;
            };

            let raw_key = trimmed[..colon_pos].trim();
            let unquoted = if (raw_key.starts_with('\'') && raw_key.ends_with('\''))
                || (raw_key.starts_with('"') && raw_key.ends_with('"'))
            {
                &raw_key[1..raw_key.len() - 1]
            } else {
                raw_key
            };

            if unquoted == key {
                positions.push((key.to_string(), line_num));
                *cursor = line_num + 1;
                break;
            }
        }
    }

    positions
}

/// Compares consecutive key pairs and pushes a diagnostic for each violation.
fn emit_ordering_diagnostics(
    key_positions: &[(String, usize)],
    context: &LintContext<'_>,
    source: &str,
    case_sensitive: bool,
    config: &LintConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut prev_key: Option<&str> = None;
    let mut prev_line: Option<usize> = None;

    for (key, line_num) in key_positions {
        if let Some(prev) = prev_key {
            let out_of_order = if case_sensitive {
                key.as_str() < prev
            } else {
                key.to_lowercase() < prev.to_lowercase()
            };

            if out_of_order {
                let severity =
                    config.get_effective_severity(DiagnosticCode::KEY_ORDERING, Severity::Info);
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
                            prev_line.unwrap_or(0)
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
        assert!(diagnostics.is_empty());
    }

    /// Regression test for #105: same key names across multiple mappings must
    /// each produce exactly one diagnostic, not N times.
    #[test]
    fn test_key_ordering_no_duplicate_diagnostics_across_mappings() {
        let yaml = "b: 1\na: 2\n---\nb: 3\na: 4\n";
        let values = fast_yaml_core::Parser::parse_all(yaml).unwrap();

        let rule = KeyOrderingRule;
        let config = LintConfig::default();
        let context = LintContext::new(yaml);

        let total: usize = values
            .iter()
            .map(|v| rule.check(&context, v, &config).len())
            .sum();
        // Each document contributes exactly 1 violation (a < b).
        assert_eq!(total, 2, "expected 2 diagnostics, got {total}");
    }

    /// Regression test for #105: repetitive nested structure (CI `with:` blocks)
    /// must not inflate diagnostic count.
    #[test]
    fn test_key_ordering_repetitive_nested_structure() {
        let yaml = "steps:\n  - with:\n      b: 1\n      a: 2\n  - with:\n      b: 3\n      a: 4\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = KeyOrderingRule;
        let config = LintConfig::default();
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);

        assert_eq!(
            diagnostics.len(),
            2,
            "expected 2 diagnostics, got {}",
            diagnostics.len()
        );
    }
}
