//! Regressions for #630, #631 and #620, checked against yamllint 1.38 on the same inputs.
//!
//! Each expected line was taken from yamllint itself; columns are not compared.

use fast_yaml_linter::{ConfigFile, Linter};

/// Lints `source` with `rules` (a YAML mapping of rule settings, empty for the defaults) and
/// returns the line of every diagnostic with `code`.
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
        .map(|d| d.span.start.line())
        .collect()
}

const TIGHT: &str = "  braces: {max-spaces-inside: 0}\n  brackets: {max-spaces-inside: 0}\n";

#[test]
fn plain_scalars_in_block_sequence_entries_are_not_flow_collections() {
    for source in [
        "- a [ b ]\n",
        "- a { b }\n",
        "k:\n  - cargo-{{ arch }}-{{ x }}\n",
        "- if [ -n \"$X\" ]; then\n",
        "- - a [ b ]\n",
        "- ? a [ b ]\n",
    ] {
        for code in ["braces", "brackets", "colons"] {
            assert_eq!(
                lines(source, TIGHT, code),
                Vec::<usize>::new(),
                "{code}: {source:?}"
            );
        }
    }
}

#[test]
fn flow_collections_in_block_sequence_entries_are_still_checked() {
    let source =
        "---\nk: &b [ v ]\nl:\n  - key: [ x ]\n  - &a [ y ]\n  - !!map { z }\n  - - [ w ]\n";
    assert_eq!(lines(source, TIGHT, "brackets"), [2, 2, 4, 4, 5, 5, 7, 7]);
    assert_eq!(lines(source, TIGHT, "braces"), [6, 6]);
}

#[test]
fn a_value_in_a_block_sequence_entry_stays_plain() {
    let source = "---\n- k: v [ y ]\n- k: v { y }\n";
    assert_eq!(lines(source, TIGHT, "brackets"), Vec::<usize>::new());
    assert_eq!(lines(source, TIGHT, "braces"), Vec::<usize>::new());
}

#[test]
fn a_colon_before_a_non_blank_is_not_a_value_indicator_in_block_context() {
    for source in ["- :year\n", "k: :year\n", "- :a:b\n"] {
        assert_eq!(
            lines(source, "", "colons"),
            Vec::<usize>::new(),
            "{source:?}"
        );
    }
    assert_eq!(lines("k:  v\n", "", "colons"), [1]);
    assert_eq!(lines("k :  v\n", "", "colons"), [1, 1]);
}

#[test]
fn a_colon_stays_a_value_indicator_in_flow_collections() {
    assert_eq!(lines("k: [\"a\"  :1]\n", "", "colons"), [1]);
}

#[test]
fn commas_ignore_the_gap_before_a_trailing_comment() {
    let source = "---\na: [1,  # c\n  2]\nb: {x: 1,  # c\n  y: 2}\n";
    assert_eq!(lines(source, "", "commas"), Vec::<usize>::new());
    assert_eq!(lines(source, "", "comments"), Vec::<usize>::new());
}

#[test]
fn commas_still_check_the_gap_before_the_next_token() {
    assert_eq!(lines("---\na: [1,  2]\n", "", "commas"), [2]);
    assert_eq!(lines("---\na: [1,2]\n", "", "commas"), [2]);
}

#[test]
fn a_comment_one_space_after_a_comma_is_reported_by_comments_only() {
    let source = "---\na: [1, # c\n  2]\n";
    assert_eq!(lines(source, "", "commas"), Vec::<usize>::new());
    assert_eq!(lines(source, "", "comments"), [2]);
}

#[test]
fn braces_see_spaces_before_a_closing_brace_after_a_plain_scalar_on_a_continuation_line() {
    let loose = "  braces: {min-spaces-inside: 1, max-spaces-inside: 1}\n";
    assert_eq!(lines("---\nb: {x: 1,\n  y: 2 }\n", loose, "braces"), [2]);
    assert_eq!(lines("---\nb: {x: 1,\n  y: z   }\n", TIGHT, "braces"), [3]);
}

#[test]
fn nested_collections_keep_their_depth_across_continuation_lines() {
    assert_eq!(
        lines("---\nk: {a: {b: 1,\n  c: z  }, d: 1  }\n", TIGHT, "braces"),
        [3, 3]
    );
    assert_eq!(
        lines("---\nk: [a, [b,\n  c  ], d  ]\n", TIGHT, "brackets"),
        [3, 3]
    );
}

#[test]
fn a_flow_collection_opened_at_the_start_of_a_line_is_not_a_continuation() {
    assert_eq!(lines("---\n- {a: 1,\n  b: 2 }\n", TIGHT, "braces"), [3]);
}

#[test]
fn plain_scalars_at_the_root_and_in_keys_are_not_flow_collections() {
    for source in [
        "if [ -n \"$X\" ]; then\n",
        "--- plain {{ x }}\n",
        "plain {{ x }} text\n",
        "%YAML 1.2\n--- plain [ x ]\n",
        "a [ b ]: 1\n",
        "plain [ x ]\n---\nother { y }\n",
        "? a [ b ]\n: v { c }\n",
    ] {
        for code in ["braces", "brackets"] {
            assert_eq!(
                lines(source, TIGHT, code),
                Vec::<usize>::new(),
                "{code}: {source:?}"
            );
        }
    }
}

#[test]
fn a_plain_scalar_continued_on_the_next_line_stays_text() {
    for source in ["k: foo\n  bar { x }\n", "- a\n  b [ c ]\n"] {
        for code in ["braces", "brackets"] {
            assert_eq!(
                lines(source, TIGHT, code),
                Vec::<usize>::new(),
                "{code}: {source:?}"
            );
        }
    }
}

#[test]
fn flow_collections_with_empty_values_are_still_checked() {
    assert_eq!(lines("k: { y, z }\n", TIGHT, "braces"), [1, 1]);
    assert_eq!(lines("- !!map { y }\n", TIGHT, "braces"), [1, 1]);
}

#[test]
fn a_tag_or_anchor_before_a_flow_collection_keeps_it_checked() {
    let source = "---\n- !<tag:example.com,2000:app/foo> [ x ]\n- !<a,b> { y }\n- &anc [ z ]\n";
    assert_eq!(lines(source, TIGHT, "brackets"), [2, 2, 4, 4]);
    assert_eq!(lines(source, TIGHT, "braces"), [3, 3]);
}

#[test]
fn a_trailing_comma_before_the_closing_indicator_is_checked_in_a_mapping_too() {
    for source in [
        "---\nk: {a: b,}\n",
        "---\nk: {a,}\n",
        "---\nk: {a: \"b\",}\n",
        "---\nk: [a,]\n",
    ] {
        assert_eq!(lines(source, "", "commas"), [2], "{source:?}");
    }
    assert_eq!(
        lines("---\nk: {a: b, # c\n}\n", "", "commas"),
        Vec::<usize>::new()
    );
    assert_eq!(
        lines("---\nk: {a: b,\n}\n", "", "commas"),
        Vec::<usize>::new()
    );
}

#[test]
fn an_empty_flow_collection_with_spaces_is_reported_once() {
    let source = "---\nk: [ ]\nj: { }\nl: [  ]\n";
    assert_eq!(lines(source, "", "brackets"), [2, 4]);
    assert_eq!(lines(source, "", "braces"), [3]);
}
