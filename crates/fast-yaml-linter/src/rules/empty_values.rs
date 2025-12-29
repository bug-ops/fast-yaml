//! Rule to check for empty (implicit null) values.

use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Location, Severity,
    Span, source::SourceMapper,
};
use fast_yaml_core::Value;

/// Linting rule for empty values.
///
/// Detects keys with implicit null values (no explicit `null` or `~`).
///
/// Configuration options:
/// - `forbid_in_block_mappings`: bool (default: true)
/// - `forbid_in_flow_mappings`: bool (default: true)
/// - `forbid_in_block_sequences`: bool (default: true)
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{rules::EmptyValuesRule, rules::LintRule, LintConfig};
///
/// let rule = EmptyValuesRule;
/// let yaml = "key: null";  // Explicit null is OK
/// let value = Parser::parse_str(yaml).unwrap().unwrap();
///
/// let diagnostics = rule.check(yaml, &value, &LintConfig::new());
/// assert!(diagnostics.is_empty());
/// ```
pub struct EmptyValuesRule;

impl super::LintRule for EmptyValuesRule {
    fn code(&self) -> &str {
        DiagnosticCode::EMPTY_VALUES
    }

    fn name(&self) -> &'static str {
        "Empty Values"
    }

    fn description(&self) -> &'static str {
        "Forbids keys with implicit null values (missing explicit 'null' or '~')"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, context: &LintContext, value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        let source = context.source();
        let forbid_block = config
            .get_rule_config(self.code())
            .and_then(|rc| rc.options.get_bool("forbid_in_block_mappings"))
            .unwrap_or(true);

        let forbid_flow = config
            .get_rule_config(self.code())
            .and_then(|rc| rc.options.get_bool("forbid_in_flow_mappings"))
            .unwrap_or(true);

        let forbid_block_sequences = config
            .get_rule_config(self.code())
            .and_then(|rc| rc.options.get_bool("forbid_in_block_sequences"))
            .unwrap_or(true);

        if !forbid_block && !forbid_flow && !forbid_block_sequences {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        let mapper = SourceMapper::new(source);

        check_value_for_empty(
            value,
            source,
            &mapper,
            &mut diagnostics,
            config,
            self.code(),
            forbid_block,
            forbid_flow,
            forbid_block_sequences,
        );

        diagnostics
    }
}

#[allow(clippy::too_many_arguments)]
fn check_value_for_empty(
    value: &Value,
    source: &str,
    mapper: &SourceMapper<'_>,
    diagnostics: &mut Vec<Diagnostic>,
    config: &LintConfig,
    code: &str,
    forbid_block: bool,
    forbid_flow: bool,
    forbid_block_sequences: bool,
) {
    match value {
        Value::Mapping(hash) => {
            for (key, val) in hash {
                // Check if value is null and has no explicit null in source
                if val.is_null()
                    && let Some(key_str) = key.as_str()
                    && !has_explicit_null_value(source, key_str, mapper)
                {
                    // Determine if it's in a flow or block mapping
                    let is_flow = is_in_flow_mapping(source, key_str);

                    if ((is_flow && forbid_flow) || (!is_flow && forbid_block))
                        && let Some(span) = find_empty_value_span(source, key_str, mapper)
                    {
                        let severity = config.get_effective_severity(code, Severity::Warning);
                        diagnostics.push(
                            DiagnosticBuilder::new(
                                code,
                                severity,
                                format!("empty value for key '{key_str}'"),
                                span,
                            )
                            .with_suggestion("Add explicit 'null'", span, Some(" null".to_string()))
                            .build(source),
                        );
                    }
                }

                // Recurse into nested structures
                check_value_for_empty(
                    val,
                    source,
                    mapper,
                    diagnostics,
                    config,
                    code,
                    forbid_block,
                    forbid_flow,
                    forbid_block_sequences,
                );
            }
        }
        Value::Sequence(arr) => {
            for (idx, item) in arr.iter().enumerate() {
                // Check for null items in sequences
                if item.is_null() && forbid_block_sequences {
                    // Check if this is an implicit null in a block sequence
                    if is_in_block_sequence_with_implicit_null(source, idx) {
                        let severity = config.get_effective_severity(code, Severity::Warning);
                        // Create a diagnostic for the null item in sequence
                        // For simplicity, we'll mark the whole source
                        // A more precise implementation would find the exact list item location
                        let loc = Location::new(1, 1, 0);
                        let span = Span::new(loc, loc);

                        diagnostics.push(
                            DiagnosticBuilder::new(
                                code,
                                severity,
                                format!("empty value in sequence at index {idx}"),
                                span,
                            )
                            .build(source),
                        );
                    }
                }

                check_value_for_empty(
                    item,
                    source,
                    mapper,
                    diagnostics,
                    config,
                    code,
                    forbid_block,
                    forbid_flow,
                    forbid_block_sequences,
                );
            }
        }
        _ => {}
    }
}

fn has_explicit_null_value(_source: &str, key: &str, mapper: &SourceMapper<'_>) -> bool {
    // Find the key in source
    for line_num in 1..=mapper.context().line_count() {
        if let Some(line) = mapper.context().get_line(line_num)
            && let Some(key_pos) = line.find(key)
        {
            // Check what comes after the key and colon
            let after_key = &line[key_pos + key.len()..];
            if let Some(colon_pos) = after_key.find(':') {
                let after_colon = after_key[colon_pos + 1..].trim();
                // Check for explicit null values
                if after_colon.starts_with("null")
                    || after_colon.starts_with('~')
                    || after_colon.starts_with("Null")
                    || after_colon.starts_with("NULL")
                {
                    return true;
                }
            }
        }
    }

    false
}

fn is_in_flow_mapping(source: &str, key: &str) -> bool {
    for line in source.lines() {
        if let Some(key_pos) = line.find(key) {
            // Check if there's a '{' before the key on the same line
            let before = &line[..key_pos];
            if before.contains('{') {
                return true;
            }
        }
    }

    false
}

fn find_empty_value_span(_source: &str, key: &str, mapper: &SourceMapper<'_>) -> Option<Span> {
    for line_num in 1..=mapper.context().line_count() {
        if let Some(line) = mapper.context().get_line(line_num)
            && let Some(key_pos) = line.find(key)
            && let Some(colon_pos) = line[key_pos..].find(':')
        {
            let abs_colon_pos = key_pos + colon_pos;
            let line_offset: usize = (1..line_num)
                .filter_map(|ln| mapper.context().get_line(ln))
                .map(|l| l.len() + 1)
                .sum();

            return Some(Span::new(
                Location::new(line_num, abs_colon_pos + 1, line_offset + abs_colon_pos),
                Location::new(line_num, abs_colon_pos + 2, line_offset + abs_colon_pos + 1),
            ));
        }
    }

    None
}

const fn is_in_block_sequence_with_implicit_null(_source: &str, _idx: usize) -> bool {
    // For now, we'll return false to avoid false positives
    // A full implementation would need to track which array items
    // are from block sequences vs flow sequences and check for implicit nulls
    // This is complex and would require source position tracking during parsing
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::RuleConfig, rules::LintRule};
    use fast_yaml_core::Parser;

    #[test]
    fn test_empty_value_block_mapping() {
        let yaml = "key:";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = EmptyValuesRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &LintConfig::new());

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("empty value"));
    }

    #[test]
    fn test_explicit_null_ok() {
        let yaml = "key: null";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = EmptyValuesRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &LintConfig::new());

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_explicit_tilde_ok() {
        let yaml = "key: ~";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = EmptyValuesRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &LintConfig::new());

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_empty_value_with_config() {
        let yaml = "key:";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = EmptyValuesRule;
        let config = LintConfig::new().with_rule_config(
            "empty-values",
            RuleConfig::new().with_option("forbid_in_block_mappings", false),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_nested_empty_values() {
        let yaml = "parent:\n  child:";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = EmptyValuesRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &LintConfig::new());

        // Should detect empty value for 'child'
        assert!(!diagnostics.is_empty());
    }

    #[test]
    fn test_value_with_content() {
        let yaml = "key: value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = EmptyValuesRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &LintConfig::new());

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_empty_value_flow_mapping() {
        let yaml = "{key:}";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = EmptyValuesRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &LintConfig::new());

        // Should detect empty value in flow mapping
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("empty value"));
    }

    #[test]
    fn test_empty_value_flow_mapping_config() {
        let yaml = "{key:}";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = EmptyValuesRule;
        let config = LintConfig::new().with_rule_config(
            "empty-values",
            RuleConfig::new().with_option("forbid_in_flow_mappings", false),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // Should not detect when forbid_in_flow_mappings is false
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_empty_value_block_sequence() {
        let yaml = "-\n-";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = EmptyValuesRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &LintConfig::new());

        // Block sequences with implicit nulls are allowed by default
        // (is_in_block_sequence_with_implicit_null returns false)
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_config_forbid_in_block_sequences() {
        let yaml = "-\n-";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = EmptyValuesRule;
        let config = LintConfig::new().with_rule_config(
            "empty-values",
            RuleConfig::new().with_option("forbid_in_block_sequences", true),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // Currently no detection due to is_in_block_sequence_with_implicit_null
        // returning false (implementation limitation noted in code)
        assert!(diagnostics.is_empty());
    }
}
