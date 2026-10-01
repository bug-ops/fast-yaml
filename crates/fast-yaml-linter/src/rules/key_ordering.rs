//! Rule to check key ordering in mappings.

use serde::{Deserialize, Serialize};

use crate::config::{PatternList, RuleOptions};
use crate::context::KeyIndex;
use crate::{Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Severity};
use fast_yaml_core::Value;

/// Linting rule for key ordering.
///
/// Checks if keys in mappings are alphabetically ordered.
/// This helps maintain consistency and makes it easier to find keys in large YAML files.
///
/// Like yamllint, a key is reported when it sorts before any earlier key of the same mapping that
/// was itself accepted. The keys of a flow mapping are checked only when it is the whole root of
/// a document (`--- {b: 1, a: 2}`); nested flow mappings are not located in the source.
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

impl super::LintRule for KeyOrderingRule {
    fn code(&self) -> &str {
        DiagnosticCode::KEY_ORDERING
    }

    fn name(&self) -> &'static str {
        "Key Ordering"
    }

    fn description(&self) -> &'static str {
        "Checks if keys in mappings are alphabetically ordered"
    }

    fn needs_value(&self) -> bool {
        true
    }

    fn default_severity(&self) -> Severity {
        Severity::Info
    }

    fn check(&self, context: &LintContext, value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        let options = &config.rules.key_ordering.options;
        let mut walk = OrderingWalk {
            context,
            index: context.key_index(),
            case_sensitive: options.case_sensitive,
            ignored: &options.ignored_keys,
            config,
            diagnostics: Vec::new(),
            cursor: context.doc_start_line(),
        };
        walk.visit_root(value);
        walk.diagnostics
    }
}

/// A mapping key found in the source.
struct LocatedKey {
    key: String,
    line: usize,
    /// Byte column and length (quotes included) when the key was not found by its line.
    at: Option<(usize, usize)>,
}

/// Recursive walk that locates keys in the source and emits ordering diagnostics.
///
/// `cursor` is a 1-based source line index that advances after each key is
/// located. Searching forward from the cursor scopes each mapping's key search
/// to its own position in the document, preventing duplicate diagnostics when
/// the same key name appears in multiple mappings (#105).
struct OrderingWalk<'a, 'src> {
    context: &'a LintContext<'src>,
    index: &'a KeyIndex<'src>,
    case_sensitive: bool,
    ignored: &'a PatternList,
    config: &'a LintConfig,
    diagnostics: Vec<Diagnostic>,
    cursor: usize,
}

impl OrderingWalk<'_, '_> {
    /// Walks a document root, checking a root flow mapping from the text of its first line.
    fn visit_root(&mut self, value: &Value) {
        if let Value::Mapping(_) = value
            && let Some(keys) = self.root_flow_keys()
        {
            self.emit_ordering_diagnostics(&keys);
            return;
        }
        self.visit(value);
    }

    /// Keys of a flow mapping that starts the document, e.g. `--- {b: 1, a: 2}`, on its first line.
    fn root_flow_keys(&self) -> Option<Vec<LocatedKey>> {
        let source_context = self.context.source_context();
        // The cursor of a document with a `---` marker is the line after the marker
        let marker_line = self.cursor.checked_sub(1).filter(|&number| {
            source_context
                .get_line(number)
                .is_some_and(|line| strip_document_marker(line).len() != line.len())
        });
        let first = marker_line.unwrap_or(self.cursor);
        let (line_num, line) = (first..=source_context.line_count()).find_map(|number| {
            let line = source_context.get_line(number)?;
            let content = strip_document_marker(line);
            (!content.trim().is_empty() && !content.trim_start().starts_with('#'))
                .then_some((number, line))
        })?;
        let offset = line.len() - strip_document_marker(line).trim_start().len();
        let keys = flow_keys(line.get(offset..)?)?;
        Some(
            keys.into_iter()
                .filter(|(text, ..)| !self.ignored.is_match(text))
                .map(|(key, column, len)| LocatedKey {
                    key,
                    line: line_num,
                    at: Some((offset + column, len)),
                })
                .collect(),
        )
    }

    /// Walks `value` and emits ordering diagnostics.
    ///
    /// For mappings, each key is located and its value is recursed into immediately
    /// before searching for the next sibling key. This ensures the cursor is at the
    /// correct position when scanning nested keys, fixing false negatives when
    /// a parent mapping has multiple top-level keys (#130).
    fn visit(&mut self, value: &Value) {
        match value {
            Value::Mapping(hash) => {
                let mut located: Vec<LocatedKey> = Vec::new();

                for (key_value, nested_value) in hash {
                    let Value::String(key) = key_value else {
                        continue;
                    };
                    if let Some(line) = self.index.locate(key, &mut self.cursor)
                        && !self.ignored.is_match(key)
                    {
                        located.push(LocatedKey {
                            key: key.clone(),
                            line,
                            at: None,
                        });
                    }
                    // Recurse into the value immediately after finding its key so
                    // the cursor is positioned correctly for nested keys before the
                    // next sibling key is searched.
                    self.visit(nested_value);
                }

                self.emit_ordering_diagnostics(&located);
            }
            Value::Sequence(arr) => {
                for item in arr {
                    self.visit(item);
                }
            }
            Value::Set(set) => {
                let members = set.iter().map(|m| (m.clone(), Value::Null)).collect();
                self.visit(&Value::Mapping(members));
            }
            _ => {}
        }
    }

    fn sorts_before(&self, key: &str, earlier: &str) -> bool {
        if self.case_sensitive {
            key < earlier
        } else {
            key.to_lowercase() < earlier.to_lowercase()
        }
    }

    /// Pushes a diagnostic for each key that sorts before an earlier accepted key.
    fn emit_ordering_diagnostics(&mut self, keys: &[LocatedKey]) {
        let context = self.context;
        let config = self.config;
        let mut accepted: Vec<&LocatedKey> = Vec::new();

        for located in keys {
            let LocatedKey { key, line, at } = located;
            let Some(prev) = accepted
                .iter()
                .find(|earlier| self.sorts_before(key, &earlier.key))
            else {
                accepted.push(located);
                continue;
            };

            let severity = config.rules.key_ordering.severity_or(Severity::Info);
            let source_context = context.source_context();
            let (key_start, key_len) = at.unwrap_or_else(|| {
                let text = source_context.get_line(*line).unwrap_or_default();
                let key_start = text.find(key.as_str()).unwrap_or_default();
                match text.get(..key_start).and_then(|p| p.chars().next_back()) {
                    Some(quote @ ('"' | '\''))
                        if text
                            .get(key_start + key.len()..)
                            .is_some_and(|rest| rest.starts_with(quote)) =>
                    {
                        (key_start - 1, key.len() + 2)
                    }
                    _ => (key_start, key.len()),
                }
            });
            let span = source_context.span_at(
                source_context.line_start(*line).add_bytes(key_start),
                key_len,
            );

            self.diagnostics.push(
                DiagnosticBuilder::new(
                    DiagnosticCode::KEY_ORDERING,
                    severity,
                    format!(
                        "key '{}' should be ordered before '{}' (line {})",
                        key, prev.key, prev.line
                    ),
                    span,
                )
                .build_with_context(context.source_context()),
            );
        }
    }
}

/// Removes a leading `---` marker (followed by a blank or the end of the line) from `line`.
fn strip_document_marker(line: &str) -> &str {
    line.strip_prefix("---")
        .filter(|rest| rest.is_empty() || rest.starts_with([' ', '\t']))
        .unwrap_or(line)
}

/// Keys of the flow mapping that `text` starts with, as (unquoted key, byte column, byte length
/// including quotes). Returns `None` when `text` does not start with `{`.
fn flow_keys(text: &str) -> Option<Vec<(String, usize, usize)>> {
    let body = text.strip_prefix('{')?;
    let mut keys = Vec::new();
    let mut depth = 1usize;
    let mut expect_key = true;
    let mut chars = body.char_indices().peekable();
    while let Some((idx, c)) = chars.next() {
        let column = idx + 1;
        match c {
            '#' if body
                .get(..idx)
                .is_some_and(|before| before.is_empty() || before.ends_with([' ', '\t'])) =>
            {
                break;
            }
            '{' | '[' => {
                depth += 1;
                expect_key = false;
            }
            '}' | ']' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            ',' if depth == 1 => expect_key = true,
            ' ' | '\t' => {}
            '"' | '\'' => {
                let mut end = None;
                while let Some((at, next)) = chars.next() {
                    if c == '"' && next == '\\' {
                        chars.next();
                    } else if next == c {
                        if c == '\'' && chars.peek().is_some_and(|&(_, after)| after == '\'') {
                            chars.next();
                        } else {
                            end = Some(at);
                            break;
                        }
                    }
                }
                let end = end?;
                if depth == 1 && expect_key {
                    let inner = body.get(idx + 1..end)?;
                    keys.push((inner.to_owned(), column, end + 1 - idx));
                }
                expect_key = false;
            }
            _ if depth == 1 && expect_key => {
                let rest = body.get(idx..)?;
                let len = rest.find([',', '}', ':']).unwrap_or(rest.len());
                let key = rest.get(..len)?.trim_end();
                if !key.is_empty() {
                    keys.push((key.to_owned(), column, key.len()));
                }
                while chars.peek().is_some_and(|&(at, _)| at < idx + len) {
                    chars.next();
                }
                expect_key = false;
            }
            _ => expect_key = false,
        }
    }
    Some(keys)
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
    fn test_key_ordering_sorted() {
        let yaml = "age: 30\nname: John\nzip: 12345";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = KeyOrderingRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_key_ordering_checks_inside_tagged_set() {
        let yaml = "s: !!set\n  b:\n  a:\nt: 1";
        let value = Parser::parse_str(yaml).unwrap().unwrap();
        let context = LintContext::new(yaml);
        let diagnostics = KeyOrderingRule.check(&context, &value, &LintConfig::default());
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    }

    #[test]
    fn test_key_ordering_unsorted() {
        let yaml = "name: John\nage: 30";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = KeyOrderingRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_ne!(diagnostics, []);
        assert!(diagnostics[0].message.contains("should be ordered before"));
    }

    #[test]
    fn test_key_ordering_case_insensitive() {
        let yaml = "Name: John\nage: 30";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = KeyOrderingRule;
        let config = config_with_rule(RuleName::KeyOrdering, "{case-sensitive: false}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_ne!(diagnostics, []);
    }

    #[test]
    fn test_key_ordering_case_sensitive() {
        let yaml = "Name: John\nage: 30";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = KeyOrderingRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // 'N' < 'a' in ASCII, so this is sorted
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_key_ordering_nested() {
        let yaml = "person:\n  name: John\n  age: 30";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = KeyOrderingRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_ne!(diagnostics, []);
    }

    #[test]
    fn test_key_ordering_multiple_violations() {
        let yaml = "z: 1\ny: 2\nx: 3";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = KeyOrderingRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn test_key_ordering_single_key() {
        let yaml = "name: John";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = KeyOrderingRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_key_ordering_array_not_checked() {
        let yaml = "items:\n  - name: John\n  - age: 30";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = KeyOrderingRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics, []);
    }

    /// Regression test for #105: same key names across multiple mappings must
    /// each produce exactly one diagnostic, not N times.
    #[test]
    fn test_key_ordering_no_duplicate_diagnostics_across_mappings() {
        let yaml = "b: 1\na: 2\n---\nb: 3\na: 4\n";
        let values = fast_yaml_core::Parser::parse_all(yaml).unwrap();

        let rule = KeyOrderingRule;
        let config = LintConfig::default();
        let context = LintContext::new(yaml);

        let total: usize = values
            .iter()
            .map(|v| rule.check(&context, v, &config).len())
            .sum();
        // Each document contributes exactly 1 violation (a < b).
        assert_eq!(total, 2, "expected 2 diagnostics, got {total}");
    }

    /// Regression test for #130: nested mapping keys must be checked even when
    /// the parent mapping has more than one top-level key.
    #[test]
    fn test_key_ordering_nested_with_multiple_top_level_keys() {
        let yaml = "parent:\n  z_key: 1\n  a_key: 2\nother:\n  b_key: x\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = KeyOrderingRule;
        let config = LintConfig::default();
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);

        // "other" violates top-level order (o < p), and "a_key" violates nested order (a < z).
        assert_eq!(
            diagnostics.len(),
            2,
            "expected 2 diagnostics, got {}: {:?}",
            diagnostics.len(),
            diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    /// Regression test for #156: key-ordering must report correct line numbers
    /// for each document in a multi-doc stream. The second document's cursor
    /// must start at its own first line, not at line 1.
    #[test]
    fn test_key_ordering_multi_doc_correct_line_numbers() {
        let yaml = "z: 1\na: 2\n---\nz: 3\na: 4\n";
        let values = fast_yaml_core::Parser::parse_all(yaml).unwrap();

        let rule = KeyOrderingRule;
        let config = LintConfig::default();

        // doc 0 starts at line 1, doc 1 starts at line 4 (line after `---`)
        let doc_start_lines = [1usize, 4usize];
        let mut all_diagnostics: Vec<Diagnostic> = Vec::new();
        for (idx, value) in values.iter().enumerate() {
            let context = LintContext::new(yaml).with_doc_start_line(doc_start_lines[idx]);
            all_diagnostics.extend(rule.check(&context, value, &config));
        }

        assert_eq!(all_diagnostics.len(), 2, "expected exactly 2 diagnostics");

        // First diagnostic: `a` at line 2 (second line of doc 1)
        assert_eq!(
            all_diagnostics[0].span.start.line, 2,
            "first diagnostic should point to line 2, got {}",
            all_diagnostics[0].span.start.line
        );

        // Second diagnostic: `a` at line 5 (fifth line of the full stream)
        assert_eq!(
            all_diagnostics[1].span.start.line, 5,
            "second diagnostic should point to line 5, got {}",
            all_diagnostics[1].span.start.line
        );
    }

    /// Regression test for #105: repetitive nested structure (CI `with:` blocks)
    /// must not inflate diagnostic count.
    #[test]
    fn test_key_ordering_repetitive_nested_structure() {
        let yaml = "steps:\n  - with:\n      b: 1\n      a: 2\n  - with:\n      b: 3\n      a: 4\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = KeyOrderingRule;
        let config = LintConfig::default();
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);

        assert_eq!(
            diagnostics.len(),
            2,
            "expected 2 diagnostics, got {}",
            diagnostics.len()
        );
    }

    fn check_yaml(yaml: &str) -> Vec<Diagnostic> {
        let value = Parser::parse_str(yaml).unwrap().unwrap();
        let context = LintContext::new(yaml);
        KeyOrderingRule.check(&context, &value, &LintConfig::default())
    }

    #[test]
    fn test_line_key_strips_only_complete_quotes() {
        assert_eq!(crate::context::line_key("  \"a\": 1"), Some("a"));
        assert_eq!(crate::context::line_key("'a': 1"), Some("a"));
        assert_eq!(crate::context::line_key("\"\": 1"), Some(""));
        assert_eq!(crate::context::line_key("\": 1"), Some("\""));
        assert_eq!(crate::context::line_key("' : 1"), Some("'"));
        assert_eq!(crate::context::line_key("\"a': 1"), Some("\"a'"));
        assert_eq!(crate::context::line_key("no colon"), None);
    }

    /// Regression test for #351: a lone quote before a colon must not panic.
    #[test]
    fn test_key_ordering_lone_quote_in_key_does_not_panic() {
        let diagnostics = check_yaml("b: \"a\n ':\"\na: 1\n");
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn test_key_ordering_flow_map_keys_are_not_located() {
        let diagnostics = check_yaml("m: {z: 1, a: 2}\nn: 1\nk: 2\n");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("key 'k'"));
    }

    #[test]
    fn test_key_ordering_sequence_item_keys() {
        let diagnostics =
            check_yaml("items:\n  - b: 1\n    a: 2\n  - b: 3\n    a: 4\nz: 1\ny: 2\n");
        assert_eq!(diagnostics.len(), 3);
        assert!(diagnostics[0].message.contains("key 'a'"));
        assert!(diagnostics[1].message.contains("key 'a'"));
        assert!(diagnostics[2].message.contains("key 'y'"));
    }

    #[test]
    fn test_key_ordering_duplicate_key_names_use_cursor() {
        let diagnostics = check_yaml("a:\n  x: 1\n  y: 2\nb:\n  x: 1\n  y: 2\n");
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_key_ordering_order_violation_reports_line() {
        let diagnostics = check_yaml("c: 1\n# note\n\nb: 2\n");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].span.start.line, 4);
    }

    #[test]
    fn test_key_index_locate_advances_cursor() {
        let yaml = "a: 1\nb: 2\na: 3\n";
        let context = LintContext::new(yaml);
        let index = context.key_index();
        let mut cursor = 1;
        assert_eq!(index.locate("a", &mut cursor), Some(1));
        assert_eq!(index.locate("a", &mut cursor), Some(3));
        assert_eq!(index.locate("a", &mut cursor), None);
        assert_eq!(cursor, 4);
        assert_eq!(index.locate("missing", &mut cursor), None);
    }

    #[test]
    fn test_key_ordering_lone_quote_in_block_scalar() {
        let diagnostics = check_yaml("b: |\n  \":\n  ':\na: 1\n");
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn test_key_ordering_empty_key() {
        let diagnostics = check_yaml("\"\": 1\nb: 2\na: 3\n");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].span.start.line, 3);
    }

    #[test]
    fn test_key_ordering_multi_document_lines() {
        let yaml = "b: 1\na: 2\n---\nd: 1\nc: 2\n";
        let mut ctx = LintContext::new(yaml);
        let lines: Vec<usize> = [(1, "b: 1\na: 2\n"), (4, "d: 1\nc: 2\n")]
            .into_iter()
            .flat_map(|(start, doc)| {
                let value = Parser::parse_str(doc).unwrap().unwrap();
                ctx.set_doc_start_line(start);
                KeyOrderingRule.check(&ctx, &value, &LintConfig::default())
            })
            .map(|d| d.span.start.line)
            .collect();
        assert_eq!(lines, [2, 5]);
    }

    fn check_with(yaml: &str, options: &str) -> Vec<Diagnostic> {
        let value = Parser::parse_str(yaml).unwrap().unwrap();
        let config = config_with_rule(RuleName::KeyOrdering, options);
        KeyOrderingRule.check(&LintContext::new(yaml), &value, &config)
    }

    fn positions(found: &[Diagnostic]) -> Vec<(usize, usize)> {
        found
            .iter()
            .map(|d| (d.span.start.line, d.span.start.column))
            .collect()
    }

    #[test]
    fn test_ignored_keys_are_skipped_and_not_compared() {
        let yaml = "a:\nb:\nname:\nfirst-name:\nc:\nd:\n";
        assert_eq!(check_yaml(yaml).len(), 3);
        assert_eq!(check_with(yaml, "{ignored-keys: ['name']}"), []);
        assert_eq!(
            check_with(yaml, "{ignored-keys: ['^first-', '^name$']}"),
            []
        );
        assert_eq!(check_with("b: 1\na: 2\n", "{ignored-keys: ['^a$']}"), []);
    }

    #[test]
    fn test_ignored_keys_rejects_a_bad_pattern() {
        let mut rules = crate::config::RulesConfig::default();
        let result = rules.apply_rule(
            RuleName::KeyOrdering,
            serde_norway::Deserializer::from_str("{ignored-keys: ['(']}"),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_key_is_compared_with_every_earlier_accepted_key() {
        let found = check_yaml("c: 1\nd: 2\nb: 3\na: 4\n");
        assert_eq!(positions(&found), [(3, 1), (4, 1)]);
        let found = check_yaml("c: 1\na: 2\nb: 3\n");
        assert_eq!(positions(&found), [(2, 1), (3, 1)]);
    }

    #[test]
    fn test_flow_mapping_on_the_document_marker_line() {
        assert_eq!(positions(&check_yaml("--- {b: 1, a: 2}\n")), [(1, 12)]);
        assert_eq!(positions(&check_yaml("{b: 1, a: 2}\n")), [(1, 8)]);
        assert_eq!(positions(&check_yaml("---\n{b: 1, a: 2}\n")), [(2, 8)]);
        assert_eq!(
            positions(&check_yaml("--- {\"b\": 1, 'a': 2}\n")),
            [(1, 14)]
        );
        assert_eq!(check_yaml("--- {a: 1, b: 2}\n"), []);
        assert_eq!(
            positions(&check_yaml("--- {b: {z: 1, y: 2}, a: 2}\n")),
            [(1, 23)]
        );
    }

    #[test]
    fn test_flow_mapping_documents_are_reported_once_each() {
        let yaml = "--- {b: 1, a: 2}\n--- {d: 1, c: 2}\n";
        let values = fast_yaml_core::Parser::parse_all(yaml).unwrap();
        let mut context = LintContext::new(yaml);
        let mut found = Vec::new();
        for (value, start) in values.iter().zip([1, 3]) {
            context.set_doc_start_line(start);
            found.extend(KeyOrderingRule.check(&context, value, &LintConfig::default()));
        }
        assert_eq!(positions(&found), [(1, 12), (2, 12)]);
    }

    #[test]
    fn test_flow_root_honors_ignored_keys() {
        assert_eq!(
            check_with("--- {b: 1, a: 2}\n", "{ignored-keys: ['^a$']}"),
            []
        );
    }

    #[test]
    fn test_key_ordering_unicode_keys() {
        let diagnostics = check_yaml("\"日本\": 1\nключ: 2\nabc: 3\n");
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics[0].message.contains("key 'ключ'"));
        assert!(diagnostics[1].message.contains("key 'abc'"));
        assert_eq!(diagnostics[1].span.start.line, 3);
    }
}
