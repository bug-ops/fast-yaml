//! Rule to detect duplicate keys in YAML mappings.

use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Severity,
    source::SourceMapper,
};
use fast_yaml_core::Value;

/// Rule to detect duplicate keys in YAML mappings.
///
/// This rule scans the source text directly to find duplicate keys at the top level
/// that would be silently deduplicated by the parser. While YAML 1.2 allows duplicate
/// keys, keeping the last value, this is often a mistake that should be reported.
///
/// **Scope**: Only checks top-level keys to avoid false positives from nested mappings
/// or array elements with the same key names. Nested mappings should use unique keys
/// within their own context naturally.
///
/// Duplicate keys can lead to unexpected behavior where later values
/// silently override earlier ones without warning.
pub struct DuplicateKeysRule;

impl super::LintRule for DuplicateKeysRule {
    fn code(&self) -> &str {
        DiagnosticCode::DUPLICATE_KEY
    }

    fn name(&self) -> &'static str {
        "Duplicate Keys"
    }

    fn description(&self) -> &'static str {
        "Detects duplicate keys in YAML mappings, which violate the YAML 1.2 specification"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, context: &LintContext, value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        let source = context.source();
        if config.allow_duplicate_keys {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        let mut mapper = SourceMapper::new(source);
        // Only check top-level mapping to avoid false positives from nested contexts
        check_top_level(source, value, &mut diagnostics, &mut mapper);
        diagnostics
    }
}

fn check_top_level(
    source: &str,
    value: &Value,
    diagnostics: &mut Vec<Diagnostic>,
    mapper: &mut SourceMapper,
) {
    // Only check top-level mapping for duplicate keys
    if let Value::Mapping(map) = value {
        // Get unique keys from the (deduplicated) AST map
        let unique_keys: Vec<String> = map
            .iter()
            .filter_map(|(k, _)| k.as_str().map(String::from))
            .collect();

        // For each unique key, find ALL occurrences in source and detect duplicates
        for key_str in &unique_keys {
            let all_spans = mapper.find_all_key_spans(key_str);

            // If we find more than one occurrence, all after the first are duplicates
            if all_spans.len() > 1 {
                let first_span = all_spans[0];
                for duplicate_span in &all_spans[1..] {
                    let diagnostic = DiagnosticBuilder::new(
                        DiagnosticCode::DUPLICATE_KEY,
                        Severity::Error,
                        format!(
                            "duplicate key '{}' (first defined at line {})",
                            key_str, first_span.start.line
                        ),
                        *duplicate_span,
                    )
                    .with_suggestion(
                        "remove this duplicate key or rename it",
                        *duplicate_span,
                        None,
                    )
                    .build(source);

                    diagnostics.push(diagnostic);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::LintRule;
    use fast_yaml_core::Parser;

    #[test]
    fn test_no_duplicate_keys() {
        let yaml = "name: John\nage: 30\ncity: NYC";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = DuplicateKeysRule;
        let config = LintConfig::default();
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_duplicate_keys_detected() {
        // The linter now scans source directly to find duplicates
        // even though the parser deduplicates them
        let yaml = "name: John\nage: 30\nname: Jane";

        let value = Parser::parse_str(yaml).unwrap().unwrap();

        // Verify saphyr deduplicates and keeps the last value
        if let Value::Mapping(map) = &value {
            // Only 2 keys should remain (age and name)
            assert_eq!(map.len(), 2);

            let name_val = map
                .iter()
                .find(|(k, _)| k.as_str() == Some("name"))
                .map(|(_, v)| v.as_str());
            assert_eq!(name_val, Some(Some("Jane")));
        } else {
            panic!("Expected mapping");
        }

        // The linter should detect the duplicate by scanning source
        // but rule is disabled by default, so enable it explicitly
        let rule = DuplicateKeysRule;
        let config = LintConfig {
            allow_duplicate_keys: false,
            ..Default::default()
        };
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);

        // Should find one duplicate
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("duplicate key 'name'"));
        assert_eq!(diagnostics[0].span.start.line, 3);
    }

    #[test]
    fn test_allow_duplicate_keys_config() {
        let yaml = "name: John\nage: 30\ncity: NYC";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = DuplicateKeysRule;
        let config = LintConfig {
            allow_duplicate_keys: true,
            ..Default::default()
        };
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_nested_same_keys_are_valid() {
        let yaml = "
parent:
  name: parent_value
child:
  name: child_value
another:
  nested:
    name: nested_value
";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = DuplicateKeysRule;
        let config = LintConfig::default();
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);

        // Same key names in different nested scopes should not trigger errors
        // (rule only checks top-level keys)
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_keys_in_different_mappings() {
        let yaml = "
user1:
  id: 1
  email: user1@example.com
user2:
  id: 2
  email: user2@example.com
";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = DuplicateKeysRule;
        let config = LintConfig::default();
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);

        // Keys 'id' and 'email' appear in different mappings, which is valid
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_array_of_mappings_with_same_keys() {
        let yaml = "
users:
  - name: Alice
    age: 30
  - name: Bob
    age: 25
  - name: Charlie
    age: 35
";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = DuplicateKeysRule;
        let config = LintConfig::default();
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);

        // Same keys in array items are valid
        assert!(diagnostics.is_empty());
    }
}
