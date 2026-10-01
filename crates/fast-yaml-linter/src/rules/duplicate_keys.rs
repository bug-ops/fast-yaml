//! Rule to detect duplicate keys in YAML mappings.

use serde::{Deserialize, Serialize};

use super::core_schema::{is_merge_key, is_set};
use super::node_roles::{NodeRole, RoleTracker};
use crate::config::RuleOptions;
use crate::echo::{KEY_LIMIT, echo};
use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Severity,
    SourceContext, Span,
};
use fast_yaml_core::Value;
use saphyr_parser::{Event, Parser as SaphyrParser, Span as SaphyrSpan};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// Rule to detect duplicate keys in YAML mappings.
///
/// Detects duplicate keys at all nesting levels by processing raw YAML events before
/// the parser deduplicates them. Each duplicate key occurrence produces exactly one
/// diagnostic pointing to the duplicate, with a note referencing the first definition.
///
/// Duplicate keys cause silent data loss — most parsers keep the last value, silently
/// discarding earlier ones. This rule detects them per-mapping-scope at all depths.
pub struct DuplicateKeysRule;

/// Options of the duplicate-key rule (none).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DuplicateKeysOptions {}

impl RuleOptions for DuplicateKeysOptions {
    const YAMLLINT_UNSUPPORTED: &'static [&'static str] = &["forbid-duplicated-merge-keys"];
}

impl super::LintRule for DuplicateKeysRule {
    fn code(&self) -> &str {
        DiagnosticCode::DUPLICATE_KEY
    }

    fn name(&self) -> &'static str {
        "Duplicate Keys"
    }

    fn description(&self) -> &'static str {
        "Detects duplicate keys in YAML mappings at all nesting levels"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, context: &LintContext, _value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        let severity = config
            .rules
            .duplicate_key
            .severity_or(self.default_severity());
        scan_duplicate_keys(context.source(), context.source_context(), severity)
    }
}

/// What a mapping key stands for when two keys of one mapping are compared.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum KeyIdentity {
    /// A `<<` or `!!merge` key, or an alias to one, in a mapping that is not a `!!set`.
    Merge,
    /// An anchored collection key, or an alias to one; collections are only equal through their
    /// anchor.
    Anchor(usize),
    /// A scalar, or an alias to one, by its text.
    Text(Rc<str>),
}

/// A repeated key occurrence.
struct DuplicateKey {
    key: KeyIdentity,
    first_line: usize,
    span: Span,
}

/// Keys seen so far in one open mapping (identity -> 1-indexed line of first occurrence).
#[derive(Default)]
struct MappingKeys {
    set: bool,
    seen: HashMap<KeyIdentity, usize>,
}

/// Anchors defined so far in the current document.
#[derive(Default)]
struct Anchors {
    identities: HashMap<usize, KeyIdentity>,
    merge: HashSet<usize>,
}

impl Anchors {
    fn identity_of_alias(&self, id: usize, set: bool) -> Option<KeyIdentity> {
        if !set && self.merge.contains(&id) {
            return Some(KeyIdentity::Merge);
        }
        self.identities.get(&id).cloned()
    }
}

/// Collects duplicate key occurrences, one mapping scope at a time.
#[derive(Default)]
struct Collector {
    mappings: Vec<MappingKeys>,
    anchors: Anchors,
    duplicates: Vec<DuplicateKey>,
}

impl Collector {
    /// Records `identity` as a key of the innermost mapping, reporting it when already seen.
    fn key(&mut self, identity: KeyIdentity, span: SaphyrSpan, source_context: &SourceContext<'_>) {
        let Some(keys) = self.mappings.last_mut() else {
            return;
        };
        if let Some(&first_line) = keys.seen.get(&identity) {
            self.duplicates.push(DuplicateKey {
                key: identity,
                first_line,
                span: source_context.span_of(span),
            });
        } else {
            keys.seen.insert(identity, span.start.line());
        }
    }

    /// Notes an anchored collection; as a key it is compared through its anchor.
    fn collection(
        &mut self,
        role: NodeRole,
        anchor: usize,
        span: SaphyrSpan,
        source_context: &SourceContext<'_>,
    ) {
        if anchor == 0 {
            return;
        }
        let identity = KeyIdentity::Anchor(anchor);
        self.anchors.identities.insert(anchor, identity.clone());
        if role == NodeRole::MappingKey {
            self.key(identity, span, source_context);
        }
    }
}

/// Parses raw YAML events and collects duplicate key occurrences.
fn collect_duplicates(source: &str, source_context: &SourceContext<'_>) -> Vec<DuplicateKey> {
    let mut collector = Collector::default();
    let mut roles = RoleTracker::default();
    let mut parser = SaphyrParser::new_from_str(source);

    while let Some(Ok((event, span))) = parser.next_event() {
        match event {
            Event::DocumentStart(_) => collector.anchors = Anchors::default(),
            Event::MappingStart(anchor, ref tag) => {
                let role = roles.start_mapping(source, source_context.byte_range_of(span));
                collector.collection(role, anchor, span, source_context);
                collector.mappings.push(MappingKeys {
                    set: is_set(tag.as_deref()),
                    ..MappingKeys::default()
                });
            }
            Event::SequenceStart(anchor, _) => {
                let role = roles.start_sequence(source, source_context.byte_range_of(span));
                collector.collection(role, anchor, span, source_context);
            }
            Event::MappingEnd => {
                roles.leave();
                collector.mappings.pop();
            }
            Event::SequenceEnd => roles.leave(),
            Event::Scalar(ref value, style, anchor, ref tag) => {
                let is_key = roles.node() == NodeRole::MappingKey;
                if anchor == 0 && !is_key {
                    continue;
                }
                let merge = is_merge_key(value, style, tag.as_deref());
                let text: Rc<str> = Rc::from(value.as_ref());
                if anchor > 0 {
                    collector
                        .anchors
                        .identities
                        .insert(anchor, KeyIdentity::Text(Rc::clone(&text)));
                    if merge {
                        collector.anchors.merge.insert(anchor);
                    }
                }
                if !is_key {
                    continue;
                }
                let Some(keys) = collector.mappings.last() else {
                    continue;
                };
                let identity = if merge && !keys.set {
                    KeyIdentity::Merge
                } else {
                    KeyIdentity::Text(text)
                };
                collector.key(identity, span, source_context);
            }
            Event::Alias(id) => {
                if roles.node() != NodeRole::MappingKey {
                    continue;
                }
                let Some(keys) = collector.mappings.last() else {
                    continue;
                };
                if let Some(identity) = collector.anchors.identity_of_alias(id, keys.set) {
                    collector.key(identity, span, source_context);
                }
            }
            _ => {}
        }
    }

    collector.duplicates
}

fn scan_duplicate_keys(
    source: &str,
    source_context: &SourceContext<'_>,
    severity: Severity,
) -> Vec<Diagnostic> {
    collect_duplicates(source, source_context)
        .into_iter()
        .map(
            |DuplicateKey {
                 key,
                 first_line,
                 span,
             }| {
                let what = match key {
                    KeyIdentity::Merge => "merge key '<<'".to_owned(),
                    KeyIdentity::Anchor(_) => "alias key".to_owned(),
                    KeyIdentity::Text(text) => format!("key '{}'", echo(&text, KEY_LIMIT)),
                };
                DiagnosticBuilder::new(
                    DiagnosticCode::DUPLICATE_KEY,
                    severity,
                    format!("duplicate {what} (first defined at line {first_line})"),
                    span,
                )
                .with_suggestion("remove this duplicate key or rename it", span, None)
                .build_with_context(source_context)
            },
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::LintRule;

    fn run(yaml: &str) -> Vec<Diagnostic> {
        DuplicateKeysRule.check(
            &LintContext::new(yaml),
            &Value::Null,
            &LintConfig::default(),
        )
    }

    #[test]
    fn test_no_duplicate_keys() {
        assert!(run("name: John\nage: 30\ncity: NYC").is_empty());
    }

    #[test]
    fn test_top_level_duplicate_emits_exactly_one_diagnostic() {
        let diags = run("key: first\nkey: second\n");
        assert_eq!(diags.len(), 1, "expected 1 diagnostic, got {}", diags.len());
        assert!(diags[0].message.contains("duplicate key 'key'"));
        assert_eq!(diags[0].span.start.line, 2);
    }

    #[test]
    fn test_triple_duplicate_emits_two_diagnostics() {
        assert_eq!(run("key: a\nkey: b\nkey: c\n").len(), 2);
    }

    #[test]
    fn test_nested_duplicate_detected() {
        let diags = run("top:\n  key: 1\n  key: 2\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("duplicate key 'key'"));
    }

    #[test]
    fn test_deeply_nested_duplicate_detected() {
        let diags = run("parent:\n  child:\n    nested: a\n    nested: b\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("duplicate key 'nested'"));
    }

    #[test]
    fn test_same_key_in_different_scopes_is_valid() {
        assert!(run("parent:\n  name: parent_value\nchild:\n  name: child_value\n").is_empty());
    }

    #[test]
    fn test_top_and_nested_duplicates() {
        let diags = run("key: first\nkey: second\ntop:\n  dup: 1\n  dup: 2\n");
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn test_disabled_rule_is_skipped_by_linter() {
        use crate::{Linter, config::RuleName};
        let config = LintConfig::new().with_disabled_rule(RuleName::DuplicateKey);
        let diagnostics = Linter::with_config(config)
            .lint("key: first\nkey: second\n")
            .unwrap();
        assert!(
            !diagnostics
                .iter()
                .any(|d| d.code.as_str() == "duplicate-key")
        );
    }

    #[test]
    fn test_array_of_mappings_same_keys_valid() {
        let yaml = "users:\n  - name: Alice\n    age: 30\n  - name: Bob\n    age: 25\n";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn test_keys_in_different_mappings_valid() {
        let yaml = "user1:\n  id: 1\n  email: a@b.com\nuser2:\n  id: 2\n  email: c@d.com\n";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn test_first_defined_line_in_message() {
        let diags = run("name: John\nage: 30\nname: Jane\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("first defined at line 1"));
        assert_eq!(diags[0].span.start.line, 3);
    }

    /// Regression test for #131: duplicate-key column must be 1-indexed.
    #[test]
    fn test_duplicate_key_column_is_1_indexed() {
        // "dup" starts at column 1 (first character of the line).
        let diags = run("dup: 1\ndup: 2\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(
            diags[0].span.start.column, 1,
            "column should be 1-indexed; got {}",
            diags[0].span.start.column
        );
    }

    /// Regression test for #131: indented duplicate key column must reflect indent.
    #[test]
    fn test_duplicate_key_indented_column_is_1_indexed() {
        // "key" starts at column 3 (2 spaces + 'k').
        let diags = run("parent:\n  key: 1\n  key: 2\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(
            diags[0].span.start.column, 3,
            "column should be 1-indexed at indent 2; got {}",
            diags[0].span.start.column
        );
    }

    /// Regression tests for #308: spans of non-ASCII keys and CRLF sources.
    #[test]
    fn test_non_ascii_duplicate_key_span() {
        let diags = run("ключ: 1\nключ: 2\n");
        assert_eq!(diags.len(), 1);
        let span = diags[0].span;
        assert_eq!(
            (span.start.line, span.start.column, span.start.offset),
            (2, 1, 12)
        );
        assert_eq!((span.end.column, span.end.offset), (5, 20));
    }

    #[test]
    fn test_non_ascii_flow_duplicate_key_span() {
        let diags = run("{é: 1, é: 2}");
        assert_eq!(diags.len(), 1);
        let span = diags[0].span;
        assert_eq!((span.start.column, span.start.offset), (8, 8));
        assert_eq!((span.end.column, span.end.offset), (9, 10));
    }

    #[test]
    fn test_crlf_duplicate_key_offset() {
        let diags = run("a: 1\r\na: 2\r\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].span.start.offset, 6);
    }

    /// Regression test for #188: duplicate keys after `<<: *anchor` must be detected.
    #[test]
    fn test_duplicate_key_after_merge_alias() {
        let yaml = "base: &base\n  x: 1\n\nchild:\n  <<: *base\n  key: first\n  key: second\n";
        let diags = run(yaml);
        assert_eq!(diags.len(), 1, "expected 1 diagnostic, got {}", diags.len());
        assert!(diags[0].message.contains("duplicate key 'key'"));
    }

    #[test]
    fn test_collection_key_does_not_shift_key_value_roles() {
        let diags = run("? [a, b]\n: 1\nk: 1\nk: 2\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("duplicate key 'k'"));
    }

    #[test]
    fn test_mapping_key_keys_are_scoped_to_the_inner_mapping() {
        assert!(run("? {a: 1}\n: {a: 2}\na: 3\n").is_empty());
        assert_eq!(run("? {a: 1, a: 2}\n: x\n").len(), 1);
    }

    #[test]
    fn test_alias_key_keeps_roles_aligned() {
        let diags = run("x: &v k\n*v : 1\nk2: 1\nk2: 2\n");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn test_alias_values_keep_roles_aligned() {
        let diags = run("a: &v 1\nb: *v\nb: *v\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("'b'"));
    }

    #[test]
    fn test_nested_flow_duplicate() {
        assert_eq!(run("{a: {b: 1, b: 2}}").len(), 1);
    }

    #[test]
    fn test_repeated_collection_key_is_not_reported() {
        assert!(run("? [a, b]\n: 1\n? [a, b]\n: 2\n").is_empty());
    }

    #[test]
    fn test_duplicate_after_collection_key() {
        let diags = run("? {x: 1}\n: v\nk: 1\nk: 2\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].span.start.line, 4);
    }

    #[test]
    fn test_alias_written_duplicate_merge_key_is_reported() {
        let diags = run("a: &a {x: 1}\nb: &b {y: 2}\nc: {&k <<: *a, *k : *b}\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("duplicate merge key '<<'"));
    }

    #[test]
    fn test_merge_tag_and_plain_merge_key_collide() {
        let diags = run("a: &a {x: 1}\nb: &b {y: 2}\nc: {<<: *a, !!merge x: *b}\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("duplicate merge key '<<'"));
    }

    #[test]
    fn test_plain_duplicate_merge_key_message() {
        let diags = run("a: &a {x: 1}\nc:\n  <<: *a\n  <<: *a\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("duplicate merge key '<<'"));
        assert!(diags[0].message.contains("first defined at line 3"));
    }

    #[test]
    fn test_quoted_merge_text_is_not_a_merge_key() {
        assert!(run("a: &a {x: 1}\nc: {\"<<\": 1, <<: *a}\n").is_empty());
    }

    #[test]
    fn test_alias_key_to_scalar_collides_with_its_text() {
        let diags = run("x: &k a\nm:\n  *k : 1\n  a: 2\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("duplicate key 'a'"));
    }

    #[test]
    fn test_anchored_scalar_key_collides_with_its_alias() {
        let diags = run("{&k a: 1, *k : 2}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("duplicate key 'a'"));
    }

    #[test]
    fn test_repeated_alias_to_collection_is_reported() {
        let diags = run("c: &c [1]\nm:\n  *c : 1\n  *c : 2\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("duplicate alias key"));
    }

    #[test]
    fn test_merge_key_inside_set_is_an_ordinary_key() {
        assert!(run("!!set {<<, a}\n").is_empty());
        let diags = run("!!set {<<, <<}\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("duplicate key '<<'"));
    }

    #[test]
    fn test_anchored_collection_key_collides_with_its_alias() {
        assert_eq!(run("{&c [1]: a, *c : b}").len(), 1);
        assert_eq!(run("{&c {x: 1}: a, *c : b}").len(), 1);
        assert!(run("{&c [1]: a, &d [1]: b}").is_empty());
    }

    #[test]
    fn test_long_alias_key_is_truncated_in_the_message() {
        let yaml = format!("{{&k {}: 1, *k : 2}}", "a".repeat(10_000));
        let diags = run(&yaml);
        assert_eq!(diags.len(), 1);
        assert!(
            diags[0].message.chars().count() < 200,
            "{}",
            diags[0].message
        );
    }
}
