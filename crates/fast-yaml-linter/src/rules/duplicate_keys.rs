//! Rule to detect duplicate keys in YAML mappings.

use crate::{Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, Location, Severity, Span};
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

    fn check(&self, source: &str, value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        if config.allow_duplicate_keys {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        check_value(source, value, &mut diagnostics);
        diagnostics
    }
}

fn check_value(source: &str, value: &Value, diagnostics: &mut Vec<Diagnostic>) {
    match value {
        Value::Hash(map) => {
            let mut seen_keys: HashMap<String, Location> = HashMap::new();

            for (key_yaml, val_yaml) in map {
                if let Value::String(key_str) = key_yaml
                    && let Some(_prev_location) =
                        seen_keys.insert(key_str.clone(), Location::start())
                {
                    let span = Span::new(Location::start(), Location::start());

                    let diagnostic = DiagnosticBuilder::new(
                        DiagnosticCode::DUPLICATE_KEY,
                        Severity::Error,
                        format!("duplicate key '{key_str}' found"),
                        span,
                    )
                    .build(source);

                    diagnostics.push(diagnostic);
                }

                check_value(source, val_yaml, diagnostics);
            }
        }
        Value::Array(arr) => {
            for item in arr {
                check_value(source, item, diagnostics);
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
        let diagnostics = rule.check(yaml, &value, &config);

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
        let diagnostics = rule.check(yaml, &value, &config);

        assert!(diagnostics.is_empty());
    }
}
