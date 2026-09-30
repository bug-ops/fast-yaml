//! Pins known divergences between `Parser::parse_all` (saphyr) and `parse_parallel` (#407).
//!
//! Two saphyr behaviors are pinned. Column-0 `---` inside a top-level block scalar is scalar
//! content for saphyr but a document marker for the chunker, and `--- |2\n---\n` is an error for
//! saphyr but not for the chunker. An empty clip/keep block scalar at EOF yields `"\n"` instead
//! of `""` (#456). Tests flip when saphyr fixes either one; where the parallel side is itself an
//! EOF-chomping result it flips on both fixes. Update deliberately.

use fast_yaml_core::{Parser, Value};
use fast_yaml_parallel::{Config, Error, parse_parallel, parse_parallel_with_config};

fn string(s: &str) -> Value {
    Value::String(s.into())
}

fn int_map(key: &str, n: i64) -> Value {
    let mut map = fast_yaml_core::Mapping::new();
    map.insert(string(key), Value::Int(n));
    Value::Mapping(map)
}

fn str_map(key: &str, value: &str) -> Value {
    let mut map = fast_yaml_core::Mapping::new();
    map.insert(string(key), string(value));
    Value::Mapping(map)
}

fn parallel_results(input: &str) -> [Vec<Value>; 2] {
    let sequential = Config::new().with_workers(Some(0));
    [
        parse_parallel(input).unwrap(),
        parse_parallel_with_config(input, &sequential).unwrap(),
    ]
}

fn assert_diverges(input: &str, saphyr: &[Value], parallel: &[Value]) {
    let all = Parser::parse_all(input).unwrap();
    assert_eq!(all, saphyr, "parse_all: {input:?}");
    for docs in parallel_results(input) {
        assert_eq!(docs, parallel, "parse_parallel: {input:?}");
        assert_ne!(docs, all, "{input:?}");
    }
}

fn assert_agrees(input: &str, expected: &[Value]) {
    assert_eq!(Parser::parse_all(input).unwrap(), expected, "{input:?}");
    for docs in parallel_results(input) {
        assert_eq!(docs, expected, "parse_parallel: {input:?}");
    }
}

#[test]
fn marker_inside_top_level_literal_splits_document() {
    assert_diverges(
        "--- |\nx\n---\nb: 1\n",
        &[string("x\n---\nb: 1\n")],
        &[string("x\n"), int_map("b", 1)],
    );
}

#[test]
fn marker_inside_top_level_folded_splits_document() {
    assert_diverges(
        "--- >\nx\n---\nb: 1\n",
        &[string("x --- b: 1\n")],
        &[string("x\n"), int_map("b", 1)],
    );
}

#[test]
fn marker_inside_top_level_block_scalar_without_document_start_splits_document() {
    assert_diverges(
        "|\nx\n---\nb: 1\n",
        &[string("x\n---\nb: 1\n")],
        &[string("x\n"), int_map("b", 1)],
    );
    assert_diverges(
        ">\nx\n---\nb: 1\n",
        &[string("x --- b: 1\n")],
        &[string("x\n"), int_map("b", 1)],
    );
}

#[test]
fn marker_inside_top_level_block_scalar_splits_document_for_every_chomping() {
    for (header, saphyr, parallel) in [
        ("|-", "x\n---\nb: 1", "x"),
        ("|+", "x\n---\nb: 1\n", "x\n"),
        (">-", "x --- b: 1", "x"),
        (">+", "x --- b: 1\n", "x\n"),
    ] {
        assert_diverges(
            &format!("--- {header}\nx\n---\nb: 1\n"),
            &[string(saphyr)],
            &[string(parallel), int_map("b", 1)],
        );
    }
}

#[test]
fn marker_inside_top_level_block_scalar_splits_document_with_crlf() {
    assert_diverges(
        "--- |\r\nx\r\n---\r\nb: 1\r\n",
        &[string("x\n---\nb: 1\n")],
        &[string("x\n"), int_map("b", 1)],
    );
}

// The parallel "\n" for clip/keep is the EOF-chomping divergence (spec value is ""), so these
// flip on either upstream fix.
#[test]
fn marker_directly_after_top_level_block_header_splits_document() {
    for (header, saphyr, parallel) in [
        ("|", "---\nb: 1\n", "\n"),
        (">", "--- b: 1\n", "\n"),
        ("|-", "---\nb: 1", ""),
        (">-", "--- b: 1", ""),
        ("|+", "---\nb: 1\n", "\n"),
        (">+", "--- b: 1\n", "\n"),
    ] {
        assert_diverges(
            &format!("--- {header}\n---\nb: 1\n"),
            &[string(saphyr)],
            &[string(parallel), int_map("b", 1)],
        );
    }
    assert_diverges(
        "--- |\n\n---\nb: 1\n",
        &[string("\n---\nb: 1\n")],
        &[string("\n"), int_map("b", 1)],
    );
}

#[test]
fn marker_after_block_scalar_with_explicit_indent_indicator_errors_only_in_saphyr() {
    for input in [
        "--- |2\n---\nb: 1\n",
        "--- |1\n---\nb: 1\n",
        "--- |1\n\n---\nb: 1\n",
    ] {
        assert!(Parser::parse_all(input).is_err(), "{input:?}");
        for docs in parallel_results(input) {
            assert_eq!(docs, [string("\n"), int_map("b", 1)], "{input:?}");
        }
    }
}

#[test]
fn marker_after_indented_top_level_block_scalar_agrees() {
    assert_agrees("--- |\n x\n---\nb: 1\n", &[string("x\n"), int_map("b", 1)]);
}

#[test]
fn document_end_marker_terminates_top_level_block_scalar_in_both() {
    assert_agrees(
        "--- |\nx\n...\n---\nb: 1\n",
        &[string("x\n"), int_map("b", 1)],
    );
}

// Parallel is the non-spec side here: the chunk ends at EOF, where saphyr yields "\n" (spec: "").
#[test]
fn empty_block_scalar_before_marker_differs_from_eof() {
    for (header, gap) in [
        ("|", ""),
        (">", ""),
        ("|+", ""),
        (">+", ""),
        ("|2", ""),
        ("|", "\n"),
        (">", "\n"),
        ("|2", "\n"),
    ] {
        assert_diverges(
            &format!("a: {header}\n{gap}---\nx\n"),
            &[str_map("a", ""), string("x")],
            &[str_map("a", "\n"), string("x")],
        );
    }
    assert_diverges(
        "a: |\r\n---\r\nx\r\n",
        &[str_map("a", ""), string("x")],
        &[str_map("a", "\n"), string("x")],
    );
}

// Spec-correct value for the "\n" rows is "" (empty clip/keep); flip once saphyr fixes it.
#[test]
fn empty_block_scalar_at_eof_keeps_header_newline_in_both_engines() {
    for (input, value) in [
        ("a: |\n", "\n"),
        ("a: >\n", "\n"),
        ("a: |+\n", "\n"),
        ("a: >+\n", "\n"),
        ("a: |2\n", "\n"),
        ("a: |+2\n", "\n"),
        ("a: |2+\n", "\n"),
        ("a: |\n\n", "\n"),
        ("a: >\n\n", "\n"),
        ("a: |-\n", ""),
        ("a: >-\n", ""),
        ("a: |", ""),
    ] {
        assert_agrees(input, &[str_map("a", value)]);
    }
    for input in ["--- |\n", "|\n"] {
        assert_agrees(input, &[string("\n")]);
    }
}

#[test]
fn empty_block_scalar_followed_by_blank_line_keeps_one_newline_when_kept() {
    let mut map = fast_yaml_core::Mapping::new();
    map.insert(string("strip"), string(""));
    map.insert(string("clip"), string(""));
    map.insert(string("keep"), string("\n"));
    assert_agrees(
        "strip: >-\n\nclip: >\n\nkeep: |+\n\n",
        &[Value::Mapping(map)],
    );
}

#[test]
fn quoted_scalar_broken_by_marker_errors_in_both_with_different_messages() {
    let input = "a: 'x\n---\nb: 1\n";
    let all = Parser::parse_all(input).unwrap_err().to_string();
    let Err(Error::Parse { source, .. }) = parse_parallel(input) else {
        panic!("expected parse error");
    };
    let parallel = source.to_string();
    assert!(all.contains("document indicator"), "{all}");
    assert!(parallel.contains("end of stream"), "{parallel}");
}
