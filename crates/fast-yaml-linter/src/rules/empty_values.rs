//! Rule to check for empty (implicit null) values.

use super::RuleId;
use crate::config::RuleName;
use serde::{Deserialize, Serialize};

use super::node_roles::NodeRole;
use crate::config::RuleOptions;
use crate::echo::{KEY_LIMIT, echo};
use crate::nodes::{Node, ScalarNode, TagKind};
use crate::source::offset::ByteOffset;
use crate::{Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Severity};
use fast_yaml_core::ScalarStyle;

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
/// use fast_yaml_linter::{rules::EmptyValuesRule, rules::SourceRule, LintConfig};
///
/// let rule = EmptyValuesRule;
/// let yaml = "key: null";  // Explicit null is OK
///
/// let diagnostics = rule.check(&fast_yaml_linter::LintContext::new(yaml), &LintConfig::new());
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
    fn id(&self) -> RuleId<'_> {
        RuleId::BuiltIn(RuleName::EmptyValues)
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
}

impl super::SourceRule for EmptyValuesRule {
    fn check(&self, context: &LintContext, config: &LintConfig) -> Vec<Diagnostic> {
        let options = &config.rules.empty_values.options;
        if !options.forbid_in_block_mappings
            && !options.forbid_in_flow_mappings
            && !options.forbid_in_block_sequences
        {
            return Vec::new();
        }

        let source_context = context.source_context();
        let severity = config.rules.empty_values.severity_or(Severity::Warning);
        collect_empty_values(context, options)
            .into_iter()
            .map(|EmptyValue { kind, indicator }| {
                let span = source_context.span_at(indicator.add_bytes(1), 0);
                let message = match kind {
                    EmptyKind::Mapping(key) => {
                        format!("empty value for key '{}'", echo(&key, KEY_LIMIT))
                    }
                    EmptyKind::BlockSequence => "empty value in block sequence".to_owned(),
                };
                DiagnosticBuilder::new(DiagnosticCode::EMPTY_VALUES, severity, message, span)
                    .with_suggestion("Add explicit 'null'", span, Some(" null".to_string()))
                    .build()
            })
            .collect()
    }
}

/// Where a value is missing.
enum EmptyKind {
    /// A mapping entry with this key.
    Mapping(String),
    /// A `-` entry of a block sequence.
    BlockSequence,
}

/// A missing value and the `:` or `-` indicator that introduces it.
struct EmptyValue {
    kind: EmptyKind,
    indicator: ByteOffset,
}

/// Scalar key seen last, awaiting its value.
struct PendingKey<'i> {
    text: &'i str,
    end: ByteOffset,
}

/// Walks the node index and collects entries with an implicit null value.
///
/// An implicit null is a zero-width plain scalar without a tag (an anchor is allowed); an
/// explicit `null`, `~` or `!!null` occupies source text.
fn collect_empty_values(
    context: &LintContext<'_>,
    options: &EmptyValuesOptions,
) -> Vec<EmptyValue> {
    let index = context.nodes();
    let source = context.source();
    let mut found = Vec::new();
    let mut pending: Option<PendingKey<'_>> = None;

    for node in index.nodes() {
        match node {
            Node::Scalar(scalar) => match scalar.role {
                NodeRole::MappingKey => {
                    pending = Some(PendingKey {
                        text: index.text(scalar),
                        end: scalar.range.end(),
                    });
                }
                NodeRole::MappingValue => {
                    let forbidden = if scalar.in_flow {
                        options.forbid_in_flow_mappings
                    } else {
                        options.forbid_in_block_mappings
                    };
                    if let (true, true, Some(key)) =
                        (forbidden, is_implicit_null(scalar), pending.take())
                        && let Some(colon) = colon_after(source, key.end)
                    {
                        found.push(EmptyValue {
                            kind: EmptyKind::Mapping(key.text.to_owned()),
                            indicator: colon,
                        });
                    }
                }
                NodeRole::SequenceItem => {
                    if options.forbid_in_block_sequences
                        && !scalar.in_flow
                        && is_implicit_null(scalar)
                        && let Some(dash) = dash_before(source, scalar.range.start())
                    {
                        found.push(EmptyValue {
                            kind: EmptyKind::BlockSequence,
                            indicator: dash,
                        });
                    }
                }
                NodeRole::Root => {}
            },
            Node::Open { .. } => pending = None,
            Node::Close => {}
            Node::Alias { range, role } => {
                pending = (*role == NodeRole::MappingKey)
                    .then(|| {
                        index.source_text(*range).map(|text| PendingKey {
                            text,
                            end: range.end(),
                        })
                    })
                    .flatten();
            }
        }
    }

    found
}

fn is_implicit_null(scalar: &ScalarNode) -> bool {
    scalar.range.start() == scalar.range.end()
        && scalar.style == ScalarStyle::Plain
        && scalar.tag == TagKind::None
        && !scalar.anchored
}

/// Offset of the `-` that ends the code before `from`, skipping blanks, line breaks, comments.
fn dash_before(source: &str, from: ByteOffset) -> Option<ByteOffset> {
    let mut end = from.get();
    loop {
        let head = source.get(..end)?.trim_end_matches([' ', '\t', '\r', '\n']);
        let line_start = head.rfind('\n').map_or(0, |at| at + 1);
        let line = head.get(line_start..)?;
        let comment = line
            .char_indices()
            .find(|&(at, c)| {
                c == '#'
                    && line
                        .get(..at)
                        .is_none_or(|before| before.ends_with([' ', '\t']))
            })
            .map(|(at, _)| at);
        let code = comment
            .map_or(line, |at| line.get(..at).unwrap_or(line))
            .trim_end();
        if !code.is_empty() {
            return code
                .ends_with('-')
                .then(|| ByteOffset::new(line_start + code.len() - 1));
        }
        if line_start == 0 {
            return None;
        }
        end = line_start;
    }
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
        rules::SourceRule,
    };
    use fast_yaml_core::Parser;

    #[test]
    fn test_empty_value_block_mapping() {
        let yaml = "key:";
        let _value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = EmptyValuesRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &LintConfig::new());

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("empty value"));
    }

    #[test]
    fn test_explicit_null_ok() {
        let yaml = "key: null";

        let rule = EmptyValuesRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &LintConfig::new());

        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_explicit_tilde_ok() {
        let yaml = "key: ~";

        let rule = EmptyValuesRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &LintConfig::new());

        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_empty_value_with_config() {
        let yaml = "key:";

        let rule = EmptyValuesRule;
        let config = config_with_rule(RuleName::EmptyValues, "{forbid-in-block-mappings: false}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_nested_empty_values() {
        let yaml = "parent:\n  child:";
        let _value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = EmptyValuesRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &LintConfig::new());

        // Should detect empty value for 'child'
        assert_ne!(diagnostics, []);
    }

    #[test]
    fn test_value_with_content() {
        let yaml = "key: value";

        let rule = EmptyValuesRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &LintConfig::new());

        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_empty_value_flow_mapping() {
        let yaml = "{key:}";
        let _value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = EmptyValuesRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &LintConfig::new());

        // Should detect empty value in flow mapping
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("empty value"));
    }

    #[test]
    fn test_empty_value_flow_mapping_config() {
        let yaml = "{key:}";

        let rule = EmptyValuesRule;
        let config = config_with_rule(RuleName::EmptyValues, "{forbid-in-flow-mappings: false}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &config);
        // Should not detect when forbid_in_flow_mappings is false
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_empty_value_block_sequence() {
        let yaml = "-\n-";

        let rule = EmptyValuesRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &LintConfig::new());

        let positions: Vec<_> = diagnostics
            .iter()
            .map(|d| (d.span.start.line, d.span.start.column))
            .collect();
        assert_eq!(positions, [(1, 2), (2, 2)]);
    }

    #[test]
    fn test_explicit_tag_null_ok() {
        let yaml = "key: !!null null";

        let rule = EmptyValuesRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &LintConfig::new());

        assert!(
            diagnostics.is_empty(),
            "!!null null should not trigger empty-values"
        );
    }

    #[test]
    fn test_explicit_tag_str_ok() {
        let yaml = "key: !!str value";
        let _value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = EmptyValuesRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &LintConfig::new());

        assert!(
            diagnostics.is_empty(),
            "!!str value should not trigger empty-values"
        );
    }

    #[test]
    fn test_explicit_tag_int_ok() {
        let yaml = "key: !!int 42";

        let rule = EmptyValuesRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &LintConfig::new());

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

        let rule = EmptyValuesRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &LintConfig::new());

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].span.start.line, 2,
            "diagnostic must be on line 2"
        );
        assert_eq!(
            diagnostics[0].span.start.column, 5,
            "diagnostic must point right after the colon"
        );
    }

    #[test]
    fn test_empty_value_position_prefix_key() {
        // Key "pa" must not match inside "parent" on line 1.
        let yaml = "parent:\n  pa:\n  b: 1\n";

        let rule = EmptyValuesRule;
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &LintConfig::new());

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].span.start.line, 2,
            "diagnostic must be on line 2"
        );
    }

    fn empty_positions(yaml: &str) -> Vec<(usize, usize)> {
        EmptyValuesRule
            .check(&LintContext::new(yaml), &LintConfig::new())
            .iter()
            .map(|d| (d.span.start.line, d.span.start.column))
            .collect()
    }

    #[test]
    fn test_empty_key_reported_at_its_own_line() {
        assert_eq!(empty_positions("a:\n  b:\nc:\n  b: 1\n"), [(2, 5)]);
        assert_eq!(empty_positions("a:\n  b: 1\nc:\n  b:\n"), [(4, 5)]);
        assert_eq!(
            empty_positions("a:\n  b: 1\nc:\n  b: 2\nd: {b: 1}\ne:\n  b:\n"),
            [(7, 5)]
        );
    }

    #[test]
    fn test_quoted_key_with_colon() {
        assert_eq!(empty_positions("\"a:b\":\nc: 1\n"), [(1, 7)]);
        assert_eq!(empty_positions("'a b':\n"), [(1, 7)]);
    }

    #[test]
    fn test_explicit_forms_are_not_empty() {
        assert_eq!(empty_positions("a: !!null\nc: ~\nd: ''\ne: null\n"), []);
    }

    #[test]
    fn test_flow_empty_values() {
        assert_eq!(empty_positions("m: {a: 1, b:, c: 2}\n"), [(1, 13)]);
    }

    #[test]
    fn test_key_without_colon_is_not_reported() {
        assert_eq!(empty_positions("? a\n"), []);
        assert_eq!(empty_positions("{a, b: 1}\n"), []);
    }

    #[test]
    fn test_flow_pair_in_flow_sequence_is_flow() {
        assert_eq!(empty_positions("k: [a: ]\n"), [(1, 7)]);
        let only_block =
            config_with_rule(RuleName::EmptyValues, "{forbid-in-flow-mappings: false}");
        let yaml = "k: [a: ]\nm:\n";
        let diags = EmptyValuesRule.check(&LintContext::new(yaml), &only_block);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].span.start.line, 2);
    }

    #[test]
    fn test_anchored_empty_value_is_not_reported() {
        assert_eq!(empty_positions("k: &x\n"), []);
    }

    #[test]
    fn test_explicit_key_without_value_is_not_reported() {
        assert_eq!(empty_positions("? a\n? b\n"), []);
    }

    #[test]
    fn test_explicit_key_with_empty_value_is_reported_at_colon() {
        assert_eq!(empty_positions("? a\n:\n"), [(2, 2)]);
        assert_eq!(empty_positions("? a\n# note\n: \n? b\n"), [(3, 2)]);
        assert_eq!(empty_positions("k:\n  ? a\n  :\n"), [(3, 4)]);
    }

    #[test]
    fn test_alias_key_with_empty_value_is_reported() {
        assert_eq!(empty_positions("x: &v k\n*v :\n"), [(2, 5)]);
    }

    #[test]
    fn test_tagged_empty_values_are_skipped() {
        assert_eq!(empty_positions("a: !!null\nb: !!str\n? c\n: !!null\n"), []);
    }

    #[test]
    fn test_crlf_and_multibyte_keys() {
        assert_eq!(
            empty_positions("ключ:\r\nдва: 1\r\nтри:\r\n"),
            [(1, 6), (3, 5)]
        );
    }

    #[test]
    fn test_long_key_is_truncated_in_message() {
        let yaml = format!("{}:\n", "k".repeat(300));
        let diags = EmptyValuesRule.check(&LintContext::new(&yaml), &LintConfig::new());
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

        let rule = EmptyValuesRule;
        let config = config_with_rule(RuleName::EmptyValues, "{forbid-in-block-sequences: true}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &config);
        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn test_colon_prefixed_key_is_not_a_value_indicator() {
        assert_eq!(empty_positions("? a\n:x: 1\n"), []);
    }

    #[test]
    fn test_explicit_key_layout_variants() {
        assert_eq!(empty_positions("? a\n: # c\n"), [(2, 2)]);
        assert_eq!(empty_positions("? a\n\n\n\n:\n"), [(5, 2)]);
        assert_eq!(empty_positions("? a\r\n:\r\n"), [(2, 2)]);
        assert_eq!(empty_positions("? a\n# : x\n: v\n"), []);
        assert_eq!(empty_positions("? a\n# : x\n:\n"), [(3, 2)]);
    }
}
