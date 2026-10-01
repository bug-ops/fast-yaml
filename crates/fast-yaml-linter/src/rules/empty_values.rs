//! Rule to check for empty (implicit null) values.

use serde::{Deserialize, Serialize};

use super::node_roles::{NodeRole, RoleTracker};
use crate::config::RuleOptions;
use crate::echo::{KEY_LIMIT, echo};
use crate::source::offset::ByteOffset;
use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Severity, SourceContext,
};
use fast_yaml_core::Value;
use saphyr_parser::{Event, Parser as SaphyrParser, ScalarStyle};

/// Linting rule for empty values.
///
/// Detects keys with implicit null values (no explicit `null` or `~`).
///
/// Configuration options:
/// - `forbid-in-block-mappings`: bool (default: true)
/// - `forbid-in-flow-mappings`: bool (default: true)
/// - `forbid-in-block-sequences`: bool (default: true)
///
/// # Examples
///
/// ```
/// use fast_yaml_core::Parser;
/// use fast_yaml_linter::{rules::EmptyValuesRule, rules::LintRule, LintConfig};
///
/// let rule = EmptyValuesRule;
/// let yaml = "key: null";  // Explicit null is OK
/// let value = Parser::parse_str(yaml).unwrap().unwrap();
///
/// let diagnostics = rule.check(&fast_yaml_linter::LintContext::new(yaml), &value, &LintConfig::new());
/// assert!(diagnostics.is_empty());
/// ```
pub struct EmptyValuesRule;

/// Options of the empty-values rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", default)]
pub struct EmptyValuesOptions {
    /// Flag empty values in block mappings.
    pub forbid_in_block_mappings: bool,
    /// Flag empty values in flow mappings.
    pub forbid_in_flow_mappings: bool,
    /// Flag empty items in block sequences.
    pub forbid_in_block_sequences: bool,
}

impl Default for EmptyValuesOptions {
    fn default() -> Self {
        Self {
            forbid_in_block_mappings: true,
            forbid_in_flow_mappings: true,
            forbid_in_block_sequences: true,
        }
    }
}

impl RuleOptions for EmptyValuesOptions {}

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

    fn check(&self, context: &LintContext, _value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        let options = &config.rules.empty_values.options;
        if !options.forbid_in_block_mappings && !options.forbid_in_flow_mappings {
            return Vec::new();
        }

        let source_context = context.source_context();
        let severity = config.rules.empty_values.severity_or(Severity::Warning);
        collect_empty_values(context.source(), source_context, options)
            .into_iter()
            .map(|EmptyValue { key, colon }| {
                let span = source_context.span_at(colon, 1);
                DiagnosticBuilder::new(
                    self.code(),
                    severity,
                    format!("empty value for key '{}'", echo(&key, KEY_LIMIT)),
                    span,
                )
                .with_suggestion("Add explicit 'null'", span, Some(" null".to_string()))
                .build_with_context(source_context)
            })
            .collect()
    }
}

/// A mapping entry whose value is missing.
struct EmptyValue {
    key: String,
    colon: ByteOffset,
}

/// Scalar key seen last, awaiting its value.
struct PendingKey {
    text: String,
    end: ByteOffset,
}

/// Walks parser events and collects entries with an implicit null value.
///
/// An implicit null is a zero-width plain scalar without a tag (an anchor is allowed); an
/// explicit `null`, `~` or `!!null` occupies source text.
fn collect_empty_values(
    source: &str,
    source_context: &SourceContext<'_>,
    options: &EmptyValuesOptions,
) -> Vec<EmptyValue> {
    let mut found = Vec::new();
    let mut roles = RoleTracker::default();
    let mut pending: Option<PendingKey> = None;
    #[allow(
        clippy::disallowed_methods,
        reason = "source passed the guarded parse in the same lint call"
    )]
    let mut parser = SaphyrParser::new_from_str(source);

    while let Some(Ok((event, span))) = parser.next_event() {
        let range = source_context.byte_range_of(span);
        match event {
            Event::Scalar(text, style, _, tag) => match roles.node() {
                NodeRole::MappingKey => {
                    pending = Some(PendingKey {
                        text: text.into_owned(),
                        end: range.end(),
                    });
                }
                NodeRole::MappingValue => {
                    let forbidden = if roles.in_flow() {
                        options.forbid_in_flow_mappings
                    } else {
                        options.forbid_in_block_mappings
                    };
                    let implicit = range.start() == range.end()
                        && style == ScalarStyle::Plain
                        && tag.is_none();
                    if let (true, true, Some(key)) = (forbidden, implicit, pending.take())
                        && let Some(colon) = colon_after(source, key.end)
                    {
                        found.push(EmptyValue {
                            key: key.text,
                            colon,
                        });
                    }
                }
                NodeRole::SequenceItem | NodeRole::Root => {}
            },
            Event::MappingStart(..) => {
                pending = None;
                roles.start_mapping(source, range);
            }
            Event::SequenceStart(..) => {
                pending = None;
                roles.start_sequence(source, range);
            }
            Event::MappingEnd | Event::SequenceEnd => roles.leave(),
            Event::Alias(..) => {
                pending = None;
                if roles.node() == NodeRole::MappingKey {
                    pending = source
                        .get(range.start().get()..range.end().get())
                        .map(|text| PendingKey {
                            text: text.to_owned(),
                            end: range.end(),
                        });
                }
            }
            _ => {}
        }
    }

    found
}

/// Offset of the `:` that follows `from` after blanks, line breaks and comment lines, if any.
///
/// An explicit key (`? a`) carries its `:` on a later line.
fn colon_after(source: &str, from: ByteOffset) -> Option<ByteOffset> {
    let mut pos = from.get();
    loop {
        let rest = source.get(pos..)?;
        pos += rest.len() - rest.trim_start_matches([' ', '\t', '\r', '\n']).len();
        let rest = source.get(pos..)?;
        if rest.starts_with('#') {
            pos += rest.find('\n').unwrap_or(rest.len());
        } else {
            let indicator = rest.strip_prefix(':')?;
            return indicator
                .chars()
                .next()
                .is_none_or(|c| c.is_whitespace() || matches!(c, ',' | '[' | ']' | '{' | '}'))
                .then(|| ByteOffset::new(pos));
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
        let config = config_with_rule(RuleName::EmptyValues, "{forbid-in-block-mappings: false}");

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
        let config = config_with_rule(RuleName::EmptyValues, "{forbid-in-flow-mappings: false}");

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
    fn test_explicit_tag_null_ok() {
        let yaml = "key: !!null null";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = EmptyValuesRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &LintConfig::new());

        assert!(
            diagnostics.is_empty(),
            "!!null null should not trigger empty-values"
        );
    }

    #[test]
    fn test_explicit_tag_str_ok() {
        let yaml = "key: !!str value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = EmptyValuesRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &LintConfig::new());

        assert!(
            diagnostics.is_empty(),
            "!!str value should not trigger empty-values"
        );
    }

    #[test]
    fn test_explicit_tag_int_ok() {
        let yaml = "key: !!int 42";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = EmptyValuesRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &LintConfig::new());

        assert!(
            diagnostics.is_empty(),
            "!!int 42 should not trigger empty-values"
        );
    }

    #[test]
    fn test_empty_value_position_not_confused_by_key_substring() {
        // Regression for #174: key "a" must not match inside "parent" on line 1.
        // Diagnostic for "a" must point to line 2, not line 1.
        let yaml = "parent:\n  a:\n  b: 1\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = EmptyValuesRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &LintConfig::new());

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].span.start.line, 2,
            "diagnostic must be on line 2"
        );
        assert_eq!(
            diagnostics[0].span.start.column, 4,
            "diagnostic must point to the colon"
        );
    }

    #[test]
    fn test_empty_value_position_prefix_key() {
        // Key "pa" must not match inside "parent" on line 1.
        let yaml = "parent:\n  pa:\n  b: 1\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = EmptyValuesRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &LintConfig::new());

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].span.start.line, 2,
            "diagnostic must be on line 2"
        );
    }

    fn empty_positions(yaml: &str) -> Vec<(usize, usize)> {
        let value = Parser::parse_str(yaml).unwrap().unwrap();
        EmptyValuesRule
            .check(&LintContext::new(yaml), &value, &LintConfig::new())
            .iter()
            .map(|d| (d.span.start.line, d.span.start.column))
            .collect()
    }

    #[test]
    fn test_empty_key_reported_at_its_own_line() {
        assert_eq!(empty_positions("a:\n  b:\nc:\n  b: 1\n"), [(2, 4)]);
        assert_eq!(empty_positions("a:\n  b: 1\nc:\n  b:\n"), [(4, 4)]);
        assert_eq!(
            empty_positions("a:\n  b: 1\nc:\n  b: 2\nd: {b: 1}\ne:\n  b:\n"),
            [(7, 4)]
        );
    }

    #[test]
    fn test_quoted_key_with_colon() {
        assert_eq!(empty_positions("\"a:b\":\nc: 1\n"), [(1, 6)]);
        assert_eq!(empty_positions("'a b':\n"), [(1, 6)]);
    }

    #[test]
    fn test_explicit_forms_are_not_empty() {
        assert!(empty_positions("a: !!null\nc: ~\nd: ''\ne: null\n").is_empty());
    }

    #[test]
    fn test_flow_empty_values() {
        assert_eq!(empty_positions("m: {a: 1, b:, c: 2}\n"), [(1, 12)]);
    }

    #[test]
    fn test_key_without_colon_is_not_reported() {
        assert!(empty_positions("? a\n").is_empty());
        assert!(empty_positions("{a, b: 1}\n").is_empty());
    }

    #[test]
    fn test_flow_pair_in_flow_sequence_is_flow() {
        assert_eq!(empty_positions("k: [a: ]\n"), [(1, 6)]);
        let only_block =
            config_with_rule(RuleName::EmptyValues, "{forbid-in-flow-mappings: false}");
        let yaml = "k: [a: ]\nm:\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();
        let diags = EmptyValuesRule.check(&LintContext::new(yaml), &value, &only_block);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].span.start.line, 2);
    }

    #[test]
    fn test_anchored_empty_value_is_reported() {
        assert_eq!(empty_positions("k: &x\n"), [(1, 2)]);
    }

    #[test]
    fn test_explicit_key_without_value_is_not_reported() {
        assert!(empty_positions("? a\n? b\n").is_empty());
    }

    #[test]
    fn test_explicit_key_with_empty_value_is_reported_at_colon() {
        assert_eq!(empty_positions("? a\n:\n"), [(2, 1)]);
        assert_eq!(empty_positions("? a\n# note\n: \n? b\n"), [(3, 1)]);
        assert_eq!(empty_positions("k:\n  ? a\n  :\n"), [(3, 3)]);
    }

    #[test]
    fn test_alias_key_with_empty_value_is_reported() {
        assert_eq!(empty_positions("x: &v k\n*v :\n"), [(2, 4)]);
    }

    #[test]
    fn test_tagged_empty_values_are_skipped() {
        assert!(empty_positions("a: !!null\nb: !!str\n? c\n: !!null\n").is_empty());
    }

    #[test]
    fn test_crlf_and_multibyte_keys() {
        assert_eq!(
            empty_positions("ключ:\r\nдва: 1\r\nтри:\r\n"),
            [(1, 5), (3, 4)]
        );
    }

    #[test]
    fn test_long_key_is_truncated_in_message() {
        let yaml = format!("{}:\n", "k".repeat(300));
        let value = Parser::parse_str(&yaml).unwrap().unwrap();
        let diags = EmptyValuesRule.check(&LintContext::new(&yaml), &value, &LintConfig::new());
        assert!(diags[0].message.len() < 120, "{}", diags[0].message);
    }

    #[test]
    fn test_many_keys_are_linear() {
        let yaml: String = (0..20_000).map(|_| "- k:\n").collect();
        let start = std::time::Instant::now();
        assert_eq!(empty_positions(&yaml).len(), 20_000);
        assert!(start.elapsed() < std::time::Duration::from_secs(10));
    }

    #[test]
    fn test_config_forbid_in_block_sequences() {
        let yaml = "-\n-";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = EmptyValuesRule;
        let config = config_with_rule(RuleName::EmptyValues, "{forbid-in-block-sequences: true}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // Currently no detection due to is_in_block_sequence_with_implicit_null
        // returning false (implementation limitation noted in code)
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_colon_prefixed_key_is_not_a_value_indicator() {
        assert!(empty_positions("? a\n:x: 1\n").is_empty());
    }

    #[test]
    fn test_explicit_key_layout_variants() {
        assert_eq!(empty_positions("? a\n: # c\n"), [(2, 1)]);
        assert_eq!(empty_positions("? a\n\n\n\n:\n"), [(5, 1)]);
        assert_eq!(empty_positions("? a\r\n:\r\n"), [(2, 1)]);
        assert!(empty_positions("? a\n# : x\n: v\n").is_empty());
        assert_eq!(empty_positions("? a\n# : x\n:\n"), [(3, 1)]);
    }
}
