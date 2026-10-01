//! Rule to check key ordering in mappings.

use super::RuleId;
use crate::config::RuleName;
use serde::{Deserialize, Serialize};

use super::node_roles::NodeRole;
use crate::config::{PatternList, RuleOptions};
use crate::nodes::{CollectionKind, Node, TagKind};
use crate::source::offset::ByteRange;
use crate::{Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Severity};
use fast_yaml_core::Value;

/// Linting rule for key ordering.
///
/// Checks if keys in mappings are alphabetically ordered.
/// This helps maintain consistency and makes it easier to find keys in large YAML files.
///
/// Like yamllint, a key is reported when it sorts before any earlier key of the same mapping that
/// was itself accepted. Block and flow mappings are checked alike, keys compare by code point,
/// and keys that are not plain or quoted scalars, or carry an anchor or a tag, are skipped.
///
/// Configuration options:
/// - `case-sensitive`: boolean (default: true)
/// - `ignored-keys`: list of regular expressions (default: empty); a key matching any of them
///   (`re.search` semantics) is neither checked nor compared against
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{rules::KeyOrderingRule, rules::LintRule, LintConfig};
/// use fast_yaml_core::Parser;
///
/// let rule = KeyOrderingRule;
/// let yaml = "name: John\nage: 30";
/// let value = Parser::parse_str(yaml).unwrap().unwrap();
///
/// let config = LintConfig::default();
/// let context = fast_yaml_linter::LintContext::new(yaml);
/// let diagnostics = rule.check(&context, &value, &config);
/// assert!(!diagnostics.is_empty());  // Keys are not in alphabetical order
/// ```
pub struct KeyOrderingRule;

/// Options of the key-ordering rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", default)]
pub struct KeyOrderingOptions {
    /// Compare keys case-sensitively.
    pub case_sensitive: bool,
    /// Regular expressions for keys that are skipped by the ordering check.
    pub ignored_keys: PatternList,
}

impl Default for KeyOrderingOptions {
    fn default() -> Self {
        Self {
            case_sensitive: true,
            ignored_keys: PatternList::default(),
        }
    }
}

impl RuleOptions for KeyOrderingOptions {}

/// A key that sorts after every key accepted before it in its mapping.
struct Accepted<'i> {
    text: &'i str,
    range: ByteRange,
}

fn compare(key: &str, earlier: &str, case_sensitive: bool) -> std::cmp::Ordering {
    if case_sensitive {
        key.cmp(earlier)
    } else {
        key.chars()
            .flat_map(char::to_lowercase)
            .cmp(earlier.chars().flat_map(char::to_lowercase))
    }
}

impl super::LintRule for KeyOrderingRule {
    fn id(&self) -> RuleId<'_> {
        RuleId::BuiltIn(RuleName::KeyOrdering)
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

    fn check(&self, context: &LintContext, _value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        let options = &config.rules.key_ordering.options;
        let index = context.nodes();
        let source_context = context.source_context();
        let severity = config.rules.key_ordering.severity_or(Severity::Info);
        let mut diagnostics = Vec::new();
        // One entry per open collection; `None` for sequences.
        let mut stack: Vec<Option<Vec<Accepted<'_>>>> = Vec::new();

        for node in index.nodes() {
            match node {
                Node::Open { kind, .. } => {
                    stack.push((*kind == CollectionKind::Mapping).then(Vec::new));
                }
                Node::Close => {
                    stack.pop();
                }
                Node::Scalar(scalar)
                    if scalar.role == NodeRole::MappingKey
                        && !scalar.anchored
                        && scalar.tag == TagKind::None
                        && scalar.range.start() != scalar.range.end() =>
                {
                    let Some(Some(accepted)) = stack.last_mut() else {
                        continue;
                    };
                    let key = index.text(scalar);
                    if options.ignored_keys.is_match(key) {
                        continue;
                    }
                    let sorts_before = |earlier: &Accepted<'_>| {
                        compare(key, earlier.text, options.case_sensitive).is_lt()
                    };
                    if !accepted.last().is_some_and(sorts_before) {
                        accepted.push(Accepted {
                            text: key,
                            range: scalar.range,
                        });
                        continue;
                    }
                    let at = accepted.partition_point(|earlier| !sorts_before(earlier));
                    let Some(prev) = accepted.get(at) else {
                        continue;
                    };
                    let line = source_context.span_of_bytes(prev.range).start.line;
                    diagnostics.push(
                        DiagnosticBuilder::new(
                            DiagnosticCode::KEY_ORDERING,
                            severity,
                            format!(
                                "key '{key}' should be ordered before '{}' (line {line})",
                                prev.text
                            ),
                            source_context.span_of_bytes(scalar.range),
                        )
                        .build(),
                    );
                }
                Node::Scalar(_) | Node::Alias { .. } => {}
            }
        }
        diagnostics
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

    fn check_config(yaml: &str, config: &LintConfig) -> Vec<Diagnostic> {
        let value = Parser::parse_str(yaml).unwrap().unwrap_or(Value::Null);
        KeyOrderingRule.check(&LintContext::new(yaml), &value, config)
    }

    fn check_yaml(yaml: &str) -> Vec<Diagnostic> {
        check_config(yaml, &LintConfig::default())
    }

    fn check_with(yaml: &str, options: &str) -> Vec<Diagnostic> {
        check_config(yaml, &config_with_rule(RuleName::KeyOrdering, options))
    }

    fn positions(found: &[Diagnostic]) -> Vec<(usize, usize)> {
        found
            .iter()
            .map(|d| (d.span.start.line, d.span.start.column))
            .collect()
    }

    #[test]
    fn sorted_keys_are_accepted() {
        assert_eq!(check_yaml("age: 30\nname: John\nzip: 12345"), []);
        assert_eq!(check_yaml("name: John"), []);
    }

    #[test]
    fn unsorted_keys_are_reported_with_the_earlier_key() {
        let found = check_yaml("name: John\nage: 30");
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].message,
            "key 'age' should be ordered before 'name' (line 1)"
        );
        assert_eq!(positions(&found), [(2, 1)]);
    }

    #[test]
    fn case_sensitivity_option() {
        assert_eq!(check_yaml("Name: John\nage: 30"), []);
        let found = check_with("Name: John\nage: 30", "{case-sensitive: false}");
        assert_eq!(positions(&found), [(2, 1)]);
    }

    #[test]
    fn nested_mappings_and_sequence_items_are_checked() {
        assert_eq!(check_yaml("person:\n  name: John\n  age: 30").len(), 1);
        assert_eq!(check_yaml("items:\n  - name: John\n  - age: 30"), []);
        let found = check_yaml("items:\n  - b: 1\n    a: 2\n  - b: 3\n    a: 4\nz: 1\ny: 2\n");
        assert_eq!(positions(&found), [(3, 5), (5, 5), (7, 1)]);
    }

    #[test]
    fn every_key_is_compared_with_the_accepted_ones() {
        assert_eq!(positions(&check_yaml("z: 1\ny: 2\nx: 3")), [(2, 1), (3, 1)]);
        let found = check_yaml("c: 1\nd: 2\nb: 3\na: 4\n");
        assert_eq!(positions(&found), [(3, 1), (4, 1)]);
        assert_eq!(
            found[0].message,
            "key 'b' should be ordered before 'c' (line 1)"
        );
    }

    #[test]
    fn same_key_names_in_other_mappings_are_independent() {
        assert_eq!(check_yaml("a:\n  x: 1\n  y: 2\nb:\n  x: 1\n  y: 2\n"), []);
        let yaml = "steps:\n  - with:\n      b: 1\n      a: 2\n  - with:\n      b: 3\n      a: 4\n";
        assert_eq!(check_yaml(yaml).len(), 2);
    }

    #[test]
    fn nested_keys_are_checked_when_the_parent_has_many_keys() {
        let yaml = "parent:\n  z_key: 1\n  a_key: 2\nother:\n  b_key: x\n";
        assert_eq!(positions(&check_yaml(yaml)), [(3, 3), (4, 1)]);
    }

    #[test]
    fn multi_document_streams_report_each_document_once() {
        let found = check_yaml("z: 1\na: 2\n---\nz: 3\na: 4\n");
        assert_eq!(positions(&found), [(2, 1), (5, 1)]);
    }

    #[test]
    fn flow_mappings_are_checked_wherever_they_are() {
        assert_eq!(positions(&check_yaml("{b: 1, a: 2}\n")), [(1, 8)]);
        assert_eq!(positions(&check_yaml("--- {b: 1, a: 2}\n")), [(1, 12)]);
        assert_eq!(positions(&check_yaml("m: {z: 1, a: 2}\nn: 1\n")), [(1, 11)]);
        assert_eq!(positions(&check_yaml("- {y: 1, x: 2}\n")), [(1, 10)]);
        assert_eq!(positions(&check_yaml("a: {d: 1, c: 2}\n")), [(1, 11)]);
        assert_eq!(
            positions(&check_yaml("--- {b: {z: 1, y: 2}, a: 2}\n")),
            [(1, 16), (1, 23)]
        );
        assert_eq!(
            positions(&check_yaml("--- {\"b\": 1, 'a': 2}\n")),
            [(1, 14)]
        );
        assert_eq!(check_yaml("--- {a: 1, b: 2}\n"), []);
    }

    #[test]
    fn keys_of_a_flow_sequence_pair_are_not_ordered() {
        assert_eq!(check_yaml("[b: 1, a: 2]\n"), []);
    }

    #[test]
    fn explicit_and_special_keys_are_compared() {
        let yaml = "z: 1\nnull: 2\nbooleans: 3\n~: 4\n<<: {a: 1}\n? y\n: 6\nm: 7\n";
        assert_eq!(
            positions(&check_yaml(yaml)),
            [(2, 1), (3, 1), (5, 1), (6, 3), (8, 1)]
        );
    }

    #[test]
    fn anchored_tagged_and_collection_keys_are_skipped() {
        assert_eq!(check_yaml("b: 1\n&x a: 1\n!!str a2: 1\n"), []);
        assert_eq!(check_yaml("b: 1\n? [a, b]\n: 1\n"), []);
        assert_eq!(check_yaml("b: 1\nz: &y v\n*y : 1\n"), []);
    }

    #[test]
    fn quoted_key_is_reported_at_the_opening_quote() {
        assert_eq!(
            positions(&check_yaml("b: 1\n\"a\": 2\n'A': 3\n")),
            [(2, 1), (3, 1)]
        );
        assert_eq!(positions(&check_yaml("b: 1\n  \n'a': 2\n")), [(3, 1)]);
    }

    #[test]
    fn keys_compare_by_code_point() {
        let found = check_yaml("\"日本\": 1\nключ: 2\nabc: 3\n");
        assert_eq!(found.len(), 2);
        assert!(found[0].message.contains("key 'ключ'"));
        assert!(found[1].message.contains("key 'abc'"));
        assert_eq!(found[1].span.start.line, 3);
    }

    #[test]
    fn empty_quoted_key() {
        let found = check_yaml("\"\": 1\nb: 2\na: 3\n");
        assert_eq!(positions(&found), [(3, 1)]);
    }

    #[test]
    fn lone_quote_in_values_does_not_confuse_the_rule() {
        assert_eq!(check_yaml("b: \"a\n ':\"\na: 1\n").len(), 1);
        assert_eq!(check_yaml("b: |\n  \":\n  ':\na: 1\n").len(), 1);
    }

    #[test]
    fn violation_is_reported_on_its_own_line() {
        let found = check_yaml("c: 1\n# note\n\nb: 2\n");
        assert_eq!(positions(&found), [(4, 1)]);
    }

    #[test]
    fn keys_inside_a_tagged_set_are_checked() {
        assert_eq!(check_yaml("s: !!set\n  b:\n  a:\nt: 1").len(), 1);
    }

    #[test]
    fn ignored_keys_are_skipped_and_not_compared() {
        let yaml = "a:\nb:\nname:\nfirst-name:\nc:\nd:\n";
        assert_eq!(check_yaml(yaml).len(), 3);
        assert_eq!(check_with(yaml, "{ignored-keys: ['name']}"), []);
        assert_eq!(
            check_with(yaml, "{ignored-keys: ['^first-', '^name$']}"),
            []
        );
        assert_eq!(check_with("b: 1\na: 2\n", "{ignored-keys: ['^a$']}"), []);
        assert_eq!(
            check_with("--- {b: 1, a: 2}\n", "{ignored-keys: ['^a$']}"),
            []
        );
    }

    #[test]
    fn ignored_keys_rejects_a_bad_pattern() {
        let mut rules = crate::config::RulesConfig::default();
        let result = rules.apply_rule(
            RuleName::KeyOrdering,
            serde_norway::Deserializer::from_str("{ignored-keys: ['(']}"),
        );
        assert!(result.is_err());
    }
}
