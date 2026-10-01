//! Regressions for the yamllint-parity batch (#615-#618, #606), checked against yamllint 1.38
//! on the same inputs.
//!
//! Each expected line was taken from yamllint itself; columns are not compared.

use fast_yaml_linter::{ConfigFile, Linter};

/// Lints `source` with `rules` (a YAML mapping of rule settings) and returns the line of every
/// diagnostic with `code`.
fn lines(source: &str, rules: &str, code: &str) -> Vec<usize> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, format!("rules:\n{rules}")).unwrap();
    let (config, _) = ConfigFile::load(&path).unwrap().into_parts();
    Linter::with_config(config)
        .lint(source)
        .unwrap()
        .iter()
        .filter(|d| d.code.as_str() == code)
        .map(|d| d.span.start.line)
        .collect()
}

#[test]
fn flow_collections_ignore_indentation_before_a_closing_bracket_on_its_own_line() {
    let source = "---\na: [\n  1,\n  2\n  ]\nb: {\n  x: 1\n  }\n";
    assert_eq!(
        lines(source, "  brackets: {max-spaces-inside: 0}\n", "brackets"),
        Vec::<usize>::new()
    );
    assert_eq!(
        lines(source, "  braces: {max-spaces-inside: 0}\n", "braces"),
        Vec::<usize>::new()
    );
}

#[test]
fn flow_collections_ignore_trailing_space_after_an_opening_bracket() {
    let source = "---\na: [ \n  1 ]\nb: { # c\n  x: [1] }\n";
    assert_eq!(
        lines(source, "  brackets: {max-spaces-inside: 0}\n", "brackets"),
        [3]
    );
    assert_eq!(
        lines(source, "  braces: {max-spaces-inside: 0}\n", "braces"),
        [5]
    );
}

#[test]
fn flow_collections_still_flag_same_line_spaces() {
    let source = "---\na: [ 1, 2 ]\nb: {  x: 1  }\n";
    assert_eq!(
        lines(source, "  brackets: {max-spaces-inside: 0}\n", "brackets"),
        [2, 2]
    );
    assert_eq!(
        lines(source, "  braces: {max-spaces-inside: 0}\n", "braces"),
        [3, 3]
    );
}

#[test]
fn colons_ignore_spaces_before_a_trailing_comment_of_an_empty_value() {
    let source = "---\non:\n  push:  # c\n  pull:   # c\n  x: 1  # c\n  y:   2\n";
    assert_eq!(
        lines(source, "  colons: {max-spaces-after: 1}\n", "colons"),
        [6]
    );
}

#[test]
fn hyphens_follow_block_sequence_entries_only() {
    let source = "---\nrun: security import \"x\"\n  -k \"y\"\n  -t cert\nd: [a,\n  -1]\ne:\n  -  1\n  - -   2\n  -  # c\n";
    assert_eq!(
        lines(source, "  hyphens: {max-spaces-after: 1}\n", "hyphens"),
        [8, 9]
    );
}
