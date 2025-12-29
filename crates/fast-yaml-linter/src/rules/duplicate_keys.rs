//! Rule to detect duplicate keys in YAML mappings.

use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Severity, Span,
    source::SourceMapper,
};
use fast_yaml_core::Value;
use std::collections::HashMap;

/// Rule to detect duplicate keys in YAML mappings.
///
/// Duplicate keys violate the YAML 1.2 specification and can lead
/// to unexpected behavior where later values silently override earlier ones.
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
        check_value(source, value, &mut diagnostics, &mut mapper);
        diagnostics
    }
}

fn check_value(
    source: &str,
    value: &Value,
    diagnostics: &mut Vec<Diagnostic>,
    mapper: &mut SourceMapper,
) {
    match value {
        Value::Hash(map) => {
            // First pass: find all key positions in source
            let mut key_spans: Vec<(String, Span)> = Vec::new();
            for (key_yaml, _) in map {
                if let Value::String(key_str) = key_yaml {
                    // Search for this key in the source, starting from the last position we found
                    let line_hint = key_spans
                        .iter()
                        .rfind(|(k, _)| k == key_str)
                        .map_or(1, |(_, s)| s.end.line + 1);

                    if let Some(span) = mapper.find_key_span(key_str, line_hint) {
                        key_spans.push((key_str.clone(), span));
                    }
                }
            }

            // Second pass: detect duplicates
            let mut seen_keys: HashMap<String, Span> = HashMap::new();
            for (key_str, key_span) in key_spans {
                if let Some(prev_span) = seen_keys.insert(key_str.clone(), key_span) {
                    let diagnostic = DiagnosticBuilder::new(
                        DiagnosticCode::DUPLICATE_KEY,
                        Severity::Error,
                        format!(
                            "duplicate key '{}' (first defined at line {})",
                            key_str, prev_span.start.line
                        ),
                        key_span,
                    )
                    .with_suggestion("remove this duplicate key or rename it", key_span, None)
                    .build(source);

                    diagnostics.push(diagnostic);
                }
            }

            // Recursively check nested values
            for (_, val_yaml) in map {
                check_value(source, val_yaml, diagnostics, mapper);
            }
        }
        Value::Array(arr) => {
            for item in arr {
                check_value(source, item, diagnostics, mapper);
            }
        }
        _ => {}
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
        let yaml = "name: John\nage: 30\nname: Jane";

        let result = Parser::parse_str(yaml);
        assert!(result.is_err());
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

        // Same key names in different scopes should not trigger errors
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
