//! Positions reported for the comment, key-ordering, anchor and marker rules, checked against
//! yamllint 1.38 on the same inputs.
//!
//! Each expected `(line, column)` was taken from yamllint itself.

use fast_yaml_linter::{ConfigFile, DiagnosticCode, Linter};

/// Lints `source` with `rules` (a YAML mapping of rule settings) and returns the `(line, column)`
/// of every diagnostic with `code`.
fn positions(source: &str, rules: &str, code: &str) -> Vec<(usize, usize)> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, format!("rules:\n{rules}")).unwrap();
    let (config, _) = ConfigFile::load(&path).unwrap().into_parts();
    Linter::with_config(config)
        .lint(source)
        .unwrap()
        .iter()
        .filter(|d| d.code.as_str() == code)
        .map(|d| (d.span.start.line, d.span.start.column))
        .collect()
}

#[test]
fn comments_skip_repeated_hashes() {
    let source = "## double hash\n#### many\n##glued\n# ok\n";
    let found = positions(source, "  comments: enable\n", DiagnosticCode::COMMENTS);
    let lines: Vec<usize> = found.iter().map(|&(line, _)| line).collect();
    assert_eq!(lines, [3]);
}

#[test]
fn comments_indentation_matches_the_previous_or_next_line() {
    let source = "\
a:
  b: 1
  # matches the previous line
c:
  - 1
# c1
  # c2
d: |
  text
# right after the scalar
    # checked
e: 1
";
    assert_eq!(
        positions(
            source,
            "  comments-indentation: enable\n",
            DiagnosticCode::COMMENTS_INDENTATION
        ),
        [(7, 3), (11, 5)]
    );
}

#[test]
fn key_ordering_checks_a_flow_mapping_on_the_marker_line() {
    let code = DiagnosticCode::KEY_ORDERING;
    let rules = "  key-ordering: enable\n";
    assert_eq!(positions("--- {b: 1, a: 2}\n", rules, code), [(1, 12)]);
    assert_eq!(
        positions("--- {b: 1, a: 2}\n--- {d: 1, c: 2}\n", rules, code),
        [(1, 12), (2, 12)]
    );
}

#[test]
fn key_ordering_ignored_keys() {
    let code = DiagnosticCode::KEY_ORDERING;
    let source = "--- {b: 1, a: 2}\n--- {d: 1, c: 2}\n";
    assert_eq!(
        positions(source, "  key-ordering: {ignored-keys: ['^a$']}\n", code),
        [(2, 12)]
    );
    let block = "a:\nb:\nname:\nfirst-name:\nc:\nd:\n";
    assert_eq!(
        positions(block, "  key-ordering: {ignored-keys: ['name']}\n", code),
        []
    );
}

#[test]
fn anchors_report_duplicated_and_unused_anchors() {
    let source = "- &a 1\n- &b 2\n- *a\n- &b 3\n---\n- &c 1\n...\n--- &d [1, 2]\n";
    let rules =
        "  invalid-anchor: {forbid-duplicated-anchors: true, forbid-unused-anchors: true}\n";
    assert_eq!(
        positions(source, rules, DiagnosticCode::INVALID_ANCHOR),
        [(4, 3), (4, 3), (6, 3), (8, 5)]
    );
}

#[test]
fn document_start_forbidden_flags_a_marker_after_a_directive() {
    assert_eq!(
        positions(
            "%YAML 1.2\n---\na: 1\n",
            "  document-start: {present: false}\n",
            DiagnosticCode::DOCUMENT_START
        ),
        [(2, 1)]
    );
}

#[test]
fn markers_are_not_required_in_a_source_without_documents() {
    for source in ["", "# only a comment\n"] {
        assert_eq!(
            positions(
                source,
                "  document-start: {present: true}\n",
                DiagnosticCode::DOCUMENT_START
            ),
            []
        );
        assert_eq!(
            positions(
                source,
                "  document-end: {present: true}\n",
                DiagnosticCode::DOCUMENT_END
            ),
            []
        );
    }
}
