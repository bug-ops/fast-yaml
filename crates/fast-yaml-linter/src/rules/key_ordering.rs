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

    fn needs_value(&self) -> bool {
        true
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
        let mut cursor = context.doc_start_line();

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
///
/// For mappings, each key is located and its value is recursed into immediately
/// before searching for the next sibling key. This ensures the cursor is at the
/// correct position when scanning nested keys, fixing false negatives when
/// a parent mapping has multiple top-level keys (#130).
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
            let mut key_positions: Vec<(String, usize)> = Vec::new();

            for (key_value, nested_value) in hash {
                let Some(key) = key_value.as_str() else {
                    continue;
                };
                if let Some(line_num) = locate_key(key, context, cursor) {
                    key_positions.push((key.to_string(), line_num));
                }
                // Recurse into the value immediately after finding its key so
                // the cursor is positioned correctly for nested keys before the
                // next sibling key is searched.
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

            emit_ordering_diagnostics(
                &key_positions,
                context,
                source,
                case_sensitive,
                config,
                diagnostics,
            );
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

/// Locates a single `key` in the source, scanning forward from `*cursor`.
///
/// Returns the 1-based line number if found, and advances `*cursor` past it.
fn locate_key(key: &str, context: &LintContext<'_>, cursor: &mut usize) -> Option<usize> {
    let lines = context.lines();
    let line_metadata = context.line_metadata();

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
            *cursor = line_num + 1;
            return Some(line_num);
        }
    }

    None
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

    /// Regression test for #130: nested mapping keys must be checked even when
    /// the parent mapping has more than one top-level key.
    #[test]
    fn test_key_ordering_nested_with_multiple_top_level_keys() {
        let yaml = "parent:\n  z_key: 1\n  a_key: 2\nother:\n  b_key: x\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = KeyOrderingRule;
        let config = LintConfig::default();
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);

        // "other" violates top-level order (o < p), and "a_key" violates nested order (a < z).
        assert_eq!(
            diagnostics.len(),
            2,
            "expected 2 diagnostics, got {}: {:?}",
            diagnostics.len(),
            diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    /// Regression test for #156: key-ordering must report correct line numbers
    /// for each document in a multi-doc stream. The second document's cursor
    /// must start at its own first line, not at line 1.
    #[test]
    fn test_key_ordering_multi_doc_correct_line_numbers() {
        let yaml = "z: 1\na: 2\n---\nz: 3\na: 4\n";
        let values = fast_yaml_core::Parser::parse_all(yaml).unwrap();

        let rule = KeyOrderingRule;
        let config = LintConfig::default();

        // doc 0 starts at line 1, doc 1 starts at line 4 (line after `---`)
        let doc_start_lines = [1usize, 4usize];
        let mut all_diagnostics: Vec<Diagnostic> = Vec::new();
        for (idx, value) in values.iter().enumerate() {
            let context = LintContext::new(yaml).with_doc_start_line(doc_start_lines[idx]);
            all_diagnostics.extend(rule.check(&context, value, &config));
        }

        assert_eq!(all_diagnostics.len(), 2, "expected exactly 2 diagnostics");

        // First diagnostic: `a` at line 2 (second line of doc 1)
        assert_eq!(
            all_diagnostics[0].span.start.line, 2,
            "first diagnostic should point to line 2, got {}",
            all_diagnostics[0].span.start.line
        );

        // Second diagnostic: `a` at line 5 (fifth line of the full stream)
        assert_eq!(
            all_diagnostics[1].span.start.line, 5,
            "second diagnostic should point to line 5, got {}",
            all_diagnostics[1].span.start.line
        );
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
