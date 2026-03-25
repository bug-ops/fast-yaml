//! Rule to detect duplicate keys in YAML mappings.

use crate::{Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Severity};
use fast_yaml_core::Value;
use saphyr_parser::{BufferedInput, Event, Parser as SaphyrParser};
use std::collections::HashMap;

/// Rule to detect duplicate keys in YAML mappings.
///
/// Detects duplicate keys at all nesting levels by processing raw YAML events before
/// the parser deduplicates them. Each duplicate key occurrence produces exactly one
/// diagnostic pointing to the duplicate, with a note referencing the first definition.
///
/// Duplicate keys cause silent data loss — most parsers keep the last value, silently
/// discarding earlier ones. This rule detects them per-mapping-scope at all depths.
pub struct DuplicateKeysRule;

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
        if config.allow_duplicate_keys {
            return Vec::new();
        }
        scan_duplicate_keys(context.source())
    }
}

/// Scope stack entry to track mapping vs sequence nesting.
enum ScopeKind {
    /// A YAML mapping. Tracks seen keys and whether the next scalar is a key.
    Mapping {
        seen: HashMap<String, (usize, usize)>, // key → (1-indexed line, 1-indexed col)
        expecting_key: bool,
    },
    /// A YAML sequence. No key tracking needed.
    Sequence,
}

/// Parses raw YAML events and collects duplicate key occurrences.
///
/// Returns a list of `(key, first_line, dup_line, dup_col)` (all 1-indexed).
fn collect_duplicates(source: &str) -> Vec<(String, usize, usize, usize)> {
    let mut duplicates: Vec<(String, usize, usize, usize)> = Vec::new();
    let mut scopes: Vec<ScopeKind> = Vec::new();

    let input = BufferedInput::new(source.chars());
    let mut parser = SaphyrParser::new(input);

    while let Some(Ok(ev)) = parser.next_event() {
        let (event, span) = ev;

        match event {
            Event::MappingStart(..) => {
                scopes.push(ScopeKind::Mapping {
                    seen: HashMap::new(),
                    expecting_key: true,
                });
            }

            Event::MappingEnd => {
                scopes.pop();
                // The mapping was a value in the parent scope; parent now expects the next key.
                advance_parent_to_key(&mut scopes);
            }

            Event::SequenceStart(..) => {
                scopes.push(ScopeKind::Sequence);
            }

            Event::SequenceEnd => {
                scopes.pop();
                // The sequence was a value; parent now expects the next key.
                advance_parent_to_key(&mut scopes);
            }

            Event::Scalar(ref value, ..) => {
                // saphyr Marker: line() is 1-indexed, col() is 0-indexed.
                let scalar_line = span.start.line();
                let scalar_col = span.start.col() + 1; // convert to 1-indexed

                match scopes.last_mut() {
                    Some(ScopeKind::Mapping {
                        seen,
                        expecting_key,
                    }) => {
                        if *expecting_key {
                            let key = value.as_ref().to_owned();
                            if let Some(&(first_line, _)) = seen.get(&key) {
                                duplicates.push((key, first_line, scalar_line, scalar_col));
                            } else {
                                seen.insert(key, (scalar_line, scalar_col));
                            }
                            *expecting_key = false; // next scalar in this mapping is a value
                        } else {
                            *expecting_key = true; // value consumed, next is a key
                        }
                    }
                    Some(ScopeKind::Sequence) | None => {
                        // No key/value alternation for sequences or top-level scalars.
                    }
                }
            }

            _ => {}
        }
    }

    duplicates
}

/// After a nested mapping or sequence ends, the parent mapping (if any) should
/// advance to expecting its next key.
const fn advance_parent_to_key(scopes: &mut [ScopeKind]) {
    if let Some(ScopeKind::Mapping { expecting_key, .. }) = scopes.last_mut() {
        *expecting_key = true;
    }
}

fn scan_duplicate_keys(source: &str) -> Vec<Diagnostic> {
    collect_duplicates(source)
        .into_iter()
        .map(|(key, first_line, dup_line, dup_col)| {
            let span = span_for_key(source, dup_line, dup_col, key.len());
            DiagnosticBuilder::new(
                DiagnosticCode::DUPLICATE_KEY,
                Severity::Error,
                format!("duplicate key '{key}' (first defined at line {first_line})"),
                span,
            )
            .with_suggestion("remove this duplicate key or rename it", span, None)
            .build(source)
        })
        .collect()
}

/// Constructs a [`crate::Span`] for a key at the given 1-indexed line and column.
fn span_for_key(source: &str, line: usize, col: usize, key_len: usize) -> crate::Span {
    use crate::{Location, Span};

    let line_start_offset: usize = source
        .lines()
        .take(line.saturating_sub(1))
        .map(|l| l.len() + 1) // +1 for the newline byte
        .sum();

    let col_offset = col.saturating_sub(1);
    let start_offset = line_start_offset + col_offset;

    Span::new(
        Location::new(line, col, start_offset),
        Location::new(line, col + key_len, start_offset + key_len),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::LintRule;
    use fast_yaml_core::Parser;

    fn run(yaml: &str) -> Vec<Diagnostic> {
        let value = Parser::parse_str(yaml)
            .unwrap()
            .unwrap_or(Value::Value(fast_yaml_core::ScalarOwned::Null));
        let rule = DuplicateKeysRule;
        let config = LintConfig {
            allow_duplicate_keys: false,
            ..Default::default()
        };
        rule.check(&LintContext::new(yaml), &value, &config)
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
    fn test_allow_duplicate_keys_config() {
        let value = Parser::parse_str("key: first\nkey: second\n")
            .unwrap()
            .unwrap();
        let config = LintConfig {
            allow_duplicate_keys: true,
            ..Default::default()
        };
        assert!(
            DuplicateKeysRule
                .check(
                    &LintContext::new("key: first\nkey: second\n"),
                    &value,
                    &config
                )
                .is_empty()
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
}
