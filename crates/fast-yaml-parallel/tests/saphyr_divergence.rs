//! Pins known divergences between `Parser::parse_all` (saphyr) and `parse_parallel` (#407).
//!
//! Pinned: `--- |2\n---\n` is an error for saphyr but not for the chunker, and an empty
//! clip/keep block scalar at EOF yields `"\n"` instead of `""` (#456). Tests flip when saphyr
//! fixes either one; where the parallel side is itself an EOF-chomping result it flips on both
//! fixes. Column-0 `---` inside an unindented top-level block scalar is scalar content in both
//! engines (#552). Update deliberately.

use fast_yaml_core::limits::{MaxDocuments, ParseLimits};
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
fn marker_inside_unindented_top_level_block_scalar_is_content() {
    assert_agrees("--- |\nx\n---\nb: 1\n", &[string("x\n---\nb: 1\n")]);
    assert_agrees("--- >\nx\n---\nb: 1\n", &[string("x --- b: 1\n")]);
}

#[test]
fn marker_inside_top_level_block_scalar_without_document_start_is_content() {
    assert_agrees("|\nx\n---\nb: 1\n", &[string("x\n---\nb: 1\n")]);
    assert_agrees(">\nx\n---\nb: 1\n", &[string("x --- b: 1\n")]);
}

#[test]
fn marker_inside_top_level_block_scalar_is_content_for_every_chomping() {
    for (header, value) in [
        ("|-", "x\n---\nb: 1"),
        ("|+", "x\n---\nb: 1\n"),
        (">-", "x --- b: 1"),
        (">+", "x --- b: 1\n"),
    ] {
        assert_agrees(&format!("--- {header}\nx\n---\nb: 1\n"), &[string(value)]);
    }
}

#[test]
fn marker_inside_top_level_block_scalar_is_content_with_crlf() {
    assert_agrees("--- |\r\nx\r\n---\r\nb: 1\r\n", &[string("x\n---\nb: 1\n")]);
}

#[test]
fn marker_directly_after_top_level_block_header_is_content() {
    for (header, value) in [
        ("|", "---\nb: 1\n"),
        (">", "--- b: 1\n"),
        ("|-", "---\nb: 1"),
        (">-", "--- b: 1"),
        ("|+", "---\nb: 1\n"),
        (">+", "--- b: 1\n"),
    ] {
        assert_agrees(&format!("--- {header}\n---\nb: 1\n"), &[string(value)]);
    }
    assert_agrees("--- |\n\n---\nb: 1\n", &[string("\n---\nb: 1\n")]);
}

#[test]
fn block_header_variants_merge_like_saphyr() {
    for input in [
        "--- | # c\nx\n---\nb\n",
        "--- |\t# c\nx\n---\nb\n",
        "--- &a |\nx\n---\nb\n",
        "--- !t |\nx\n---\nb\n",
        "--- &a\n!t\n|\nx\n---\nb\n",
        "---\n|\nx\n---\nb\n",
        "---\n\n# c\n|\nx\n---\nb\n",
        "--- # c\n|\nx\n---\nb\n",
        "---\t|\nx\n---\nb\n",
        "--- |\n# c\n  x\n---\nb\n",
        "--- |\n\t\n---\nb\n",
        "--- |\n\n\tx\n---\nb\n",
        "--- |\n\tx\n---\nb\n",
        "---\n  |\nx\n---\nb\n",
        "  |\nx\n---\nb\n",
        "--- !t\n\t|\nx\n---\nb\n",
        "  |\n---\n",
        "--- |\n%YAML 1.2\n---\nb\n",
        "--- |x\n---\nb\n",
        "--- | foo\n---\nb\n",
        "--- |",
        "--- !!str |\n\n\n",
        "--- !!str |\n\n\n---\nb\n",
        "--- |\nx\n...\n%YAML 1.2\n---\nb\n",
        "--- &a\n  !t\n|\nx\n---\nb\n",
        "--- |\n \n---\nb\n",
        "%YAML 1.2\n--- |\nx\n---\nb\n",
        "a\n...\n|\nx\n---\nb\n",
        "# c\n|\nx\n---\nb\n",
        "--- |\nx\n---\t\nb\n",
        "--- |\nx\n--- # c\nb\n",
        "--- |\nx\n...x\n---\nb\n",
        "--- |\r\rx\r---\rb\r",
    ] {
        let all = Parser::parse_all(input).ok();
        let sequential = Config::new().with_workers(Some(0));
        for result in [
            parse_parallel(input),
            parse_parallel_with_config(input, &sequential),
        ] {
            assert_eq!(result.ok(), all, "{input:?}");
        }
    }
}

#[test]
fn document_end_first_after_block_header_still_ends_document() {
    assert_agrees("--- |\n...\n---\nb\n", &[string(""), string("b")]);
    assert_agrees("--- |\n\n...\n---\nb\n", &[string(""), string("b")]);
}

#[test]
fn indented_top_level_block_scalar_still_splits_at_marker_in_both() {
    assert_agrees("--- |\n  x\n---\nb\n", &[string("x\n"), string("b")]);
    assert_agrees("--- |\n \n  x\n---\nb\n", &[string("\nx\n"), string("b")]);
}

#[test]
fn comment_line_after_block_header_is_scalar_content() {
    assert_agrees("--- |\n# c\n  x\n---\nb\n", &[string("# c\n  x\n---\nb\n")]);
}

#[test]
fn merged_chunk_reports_the_stream_document_index() {
    for input in ["--- |\nx\n---\nb\n...\n---\n[\n", "--- |\nx\n...\n---\n[\n"] {
        let expected = Parser::parse_all(input).unwrap_err();
        for result in [
            parse_parallel(input),
            parse_parallel_with_config(input, &Config::new().with_workers(Some(0))),
        ] {
            let Err(Error::Parse { index, source }) = result else {
                panic!("expected parse error: {input:?}");
            };
            assert_eq!(index, expected.document_index(), "{input:?}");
            assert_eq!(source.position(), expected.position(), "{input:?}");
        }
    }
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
fn max_documents_counts_documents_of_a_merged_chunk() {
    let input = "--- |\nx\n---\nb\n---\nc\n";
    assert_eq!(Parser::parse_all(input).unwrap().len(), 1);
    let config = Config::new().with_parse_limits(ParseLimits {
        max_documents: MaxDocuments::new(1).unwrap(),
        ..ParseLimits::default()
    });
    assert_eq!(parse_parallel_with_config(input, &config).unwrap().len(), 1);
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
