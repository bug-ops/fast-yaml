//! Rule to detect duplicate keys in YAML mappings.

use serde::{Deserialize, Serialize};

use crate::config::RuleOptions;
use crate::echo::{KEY_LIMIT, echo};
use crate::scan::{KeyRepeat, RepeatedKey};
use crate::{Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Severity};
use fast_yaml_core::Value;

/// Rule to detect duplicate keys in YAML mappings.
///
/// Detects duplicate keys at all nesting levels by processing raw YAML events before
/// the parser deduplicates them. Each duplicate key occurrence produces exactly one
/// diagnostic pointing to the duplicate, with a note referencing the first definition.
///
/// Duplicate keys cause silent data loss — most parsers keep the last value, silently
/// discarding earlier ones. This rule detects them per-mapping-scope at all depths.
///
/// Keys are compared by their resolved value, the way the loaders see them, not by spelling:
/// `99` and `+99`, `0x10` and `16`, `~` and `null` are the same key, while `"1"` and `1` are
/// not. This is a deliberate divergence from yamllint, which compares the key text (it flags
/// `"1"` next to `1`, and misses `+99` next to `99` or `true` next to `True`). A plain `<<`
/// (or `!!merge`) merge key never equals a quoted `"<<"` key.
///
/// A collection key (`? [a, b]`) is never compared by its content, which matches yamllint: it
/// reports no repeated collection key at all. fast-yaml reports one more case, a collection
/// key that is repeated through its anchor (`? &x [c]` followed by `? *x`).
pub struct DuplicateKeysRule;

/// Options of the duplicate-key rule.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::rules::DuplicateKeysOptions;
///
/// assert!(DuplicateKeysOptions::default().forbid_duplicated_merge_keys);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", default)]
pub struct DuplicateKeysOptions {
    /// Reports a second `<<` merge key in one mapping.
    ///
    /// On by default, because the loaders keep only the last merge key and silently drop the
    /// others; the yamllint presets turn it off, as yamllint does.
    pub forbid_duplicated_merge_keys: bool,
}

impl Default for DuplicateKeysOptions {
    fn default() -> Self {
        Self {
            forbid_duplicated_merge_keys: true,
        }
    }
}

impl RuleOptions for DuplicateKeysOptions {}

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
        let forbid_merge_repeats = config
            .rules
            .duplicate_key
            .options
            .forbid_duplicated_merge_keys;

        context
            .key_repeats()
            .iter()
            .filter(|repeat| repeat.kind != RepeatedKey::Merge || forbid_merge_repeats)
            .map(
                |KeyRepeat {
                     key,
                     first_line,
                     span,
                     kind,
                 }| {
                    let what = match kind {
                        RepeatedKey::Merge => "merge key '<<'".to_owned(),
                        RepeatedKey::Alias => "alias key".to_owned(),
                        RepeatedKey::Ordinary => format!("key '{}'", echo(key, KEY_LIMIT)),
                    };
                    DiagnosticBuilder::new(
                        DiagnosticCode::DUPLICATE_KEY,
                        severity,
                        format!("duplicate {what} (first defined at line {first_line})"),
                        *span,
                    )
                    .with_suggestion("remove this duplicate key or rename it", *span, None)
                    .build_with_context(context.source_context())
                },
            )
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::LintRule;
    use fast_yaml_core::Parser;

    fn run(yaml: &str) -> Vec<Diagnostic> {
        DuplicateKeysRule.check(
            &LintContext::new(yaml),
            &Value::Null,
            &LintConfig::default(),
        )
    }

    #[test]
    fn test_no_duplicate_keys() {
        assert_eq!(run("name: John\nage: 30\ncity: NYC"), []);
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
        assert_eq!(
            run("parent:\n  name: parent_value\nchild:\n  name: child_value\n"),
            []
        );
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
        assert_eq!(run(yaml), []);
    }

    #[test]
    fn test_keys_in_different_mappings_valid() {
        let yaml = "user1:\n  id: 1\n  email: a@b.com\nuser2:\n  id: 2\n  email: c@d.com\n";
        assert_eq!(run(yaml), []);
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
        assert_eq!(run("? {a: 1}\n: {a: 2}\na: 3\n"), []);
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
        assert_eq!(run("? [a, b]\n: 1\n? [a, b]\n: 2\n"), []);
    }

    #[test]
    fn test_duplicate_after_collection_key() {
        let diags = run("? {x: 1}\n: v\nk: 1\nk: 2\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].span.start.line, 4);
    }

    #[test]
    fn test_spelling_variants_of_one_value_are_duplicates() {
        for yaml in [
            "99: a\n+99: b\n",
            "0x10: a\n16: b\n",
            "~: a\nnull: b\n",
            "1.0: a\n1.00: b\n",
        ] {
            assert_eq!(run(yaml).len(), 1, "{yaml:?}");
        }
    }

    #[test]
    fn test_keys_of_different_types_are_not_duplicates() {
        for yaml in [
            "\"1\": a\n1: b\n",
            "!!str 1: a\n1: b\n",
            "'null': a\nnull: b\n",
        ] {
            assert!(run(yaml).is_empty(), "{yaml:?}");
        }
    }

    #[test]
    fn test_message_keeps_the_source_spelling() {
        let diags = run("99: a\n+99: b\n");
        assert!(diags[0].message.contains("duplicate key '+99'"));
        assert!(diags[0].message.contains("first defined at line 1"));
    }

    #[test]
    fn test_loaders_agree_with_the_rule() {
        for (yaml, duplicated) in [
            ("99: a\n+99: b\n", true),
            ("0x10: a\n16: b\n", true),
            ("\"1\": a\n1: b\n", false),
        ] {
            let Some(Value::Mapping(map)) = Parser::parse_str(yaml).unwrap() else {
                panic!("not a mapping: {yaml:?}");
            };
            assert_eq!(map.len() == 1, duplicated, "{yaml:?}");
            assert_eq!(!run(yaml).is_empty(), duplicated, "{yaml:?}");
        }
    }

    #[test]
    fn test_plain_merge_key_is_not_the_quoted_one() {
        let yaml = "base: &a\n  x: 1\nchild:\n  <<: *a\n  \"<<\": 1\n";
        assert_eq!(run(yaml), []);
    }

    #[test]
    fn test_repeated_quoted_merge_key_is_a_duplicate() {
        let diags = run("\"<<\": 1\n\"<<\": 2\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("duplicate key '<<'"));
    }

    #[test]
    fn test_repeated_merge_key_is_a_duplicate() {
        let yaml = "a: &a\n  x: 1\nb: &b\n  y: 2\nc:\n  <<: *a\n  <<: *b\n";
        assert_eq!(run(yaml).len(), 1);
        assert_eq!(run("{<<: {a: 1}, <<: {b: 2}}").len(), 1);
    }

    #[test]
    fn test_tagged_merge_key_counts_as_merge_key() {
        let yaml = "a: &a\n  x: 1\nc:\n  <<: *a\n  !!merge \"<<\": *a\n";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn test_invalid_merge_value_stops_the_scan_without_panicking() {
        let yaml = "a: 1\na: 2\nb: {<<: 1}\nc: 1\nc: 2\n";
        let diags = DuplicateKeysRule.check(
            &LintContext::new(yaml),
            &Value::Null,
            &LintConfig::default(),
        );
        assert_eq!(diags.len(), 1);
    }

    fn run_with(yaml: &str, options: &str) -> Vec<Diagnostic> {
        use crate::config::{RuleName, test_support::config_with_rule};
        let config = config_with_rule(RuleName::DuplicateKey, options);
        DuplicateKeysRule.check(&LintContext::new(yaml), &Value::Null, &config)
    }

    #[test]
    fn test_forbid_duplicated_merge_keys_is_on_by_default() {
        let yaml = "a: &a {x: 1}\nb: &b {y: 2}\nc:\n  <<: *a\n  <<: *b\n";
        assert_eq!(run(yaml).len(), 1);
        assert_eq!(
            run_with(yaml, "{forbid-duplicated-merge-keys: true}").len(),
            1
        );
    }

    #[test]
    fn test_repeated_merge_key_is_allowed_when_not_forbidden() {
        let yaml = "a: &a {x: 1}\nb: &b {y: 2}\nc:\n  <<: *a\n  <<: *b\n";
        assert_eq!(run_with(yaml, "{forbid-duplicated-merge-keys: false}"), []);
    }

    #[test]
    fn test_option_does_not_hide_repeated_ordinary_or_quoted_keys() {
        let off = "{forbid-duplicated-merge-keys: false}";
        assert_eq!(run_with("\"<<\": 1\n\"<<\": 2\n", off).len(), 1);
        assert_eq!(run_with("a: 1\na: 2\n", off).len(), 1);
    }

    #[test]
    fn test_presets_turn_the_option_off() {
        use crate::config::Preset;
        for preset in [Preset::Default, Preset::Relaxed] {
            assert!(
                !preset
                    .rules()
                    .duplicate_key
                    .options
                    .forbid_duplicated_merge_keys
            );
        }
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
        assert_eq!(run("a: &a {x: 1}\nc: {\"<<\": 1, <<: *a}\n"), []);
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
        assert_eq!(run("!!set {<<, a}\n"), []);
        let diags = run("!!set {<<, <<}\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("duplicate key '<<'"));
    }

    #[test]
    fn test_anchored_collection_key_collides_with_its_alias() {
        assert_eq!(run("{&c [1]: a, *c : b}").len(), 1);
        assert_eq!(run("{&c {x: 1}: a, *c : b}").len(), 1);
        assert_eq!(run("{&c [1]: a, &d [1]: b}"), []);
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
