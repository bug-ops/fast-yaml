//! Regression tests for formatter output that must re-parse: directive hoisting (#430),
//! dropped document end markers (#429) and anchor names (#431).

use fast_yaml_core::streaming::format_streaming;
use fast_yaml_core::{Emitter, EmitterConfig, Parser};

fn formats(input: &str) -> Vec<String> {
    let config = EmitterConfig::default();
    let mut outputs = vec![Emitter::format(input).unwrap()];
    outputs.push(format_streaming(input, &config).unwrap());
    #[cfg(feature = "arena")]
    outputs.push(fast_yaml_core::streaming::format_streaming_arena(input, &config).unwrap());
    outputs
}

/// Output parses, has the same document count as the input, and is idempotent.
#[track_caller]
fn check(input: &str) -> String {
    let docs = Parser::parse_all(input).unwrap().len();
    let outputs = formats(input);
    for out in &outputs {
        let parsed = Parser::parse_all(out)
            .unwrap_or_else(|e| panic!("output {out:?} of {input:?} does not parse: {e}"));
        assert_eq!(
            parsed.len(),
            docs,
            "document count for {input:?} -> {out:?}"
        );
        assert_eq!(
            &Emitter::format(out).unwrap(),
            out,
            "idempotency for {input:?}"
        );
    }
    outputs.into_iter().next().unwrap()
}

#[test]
fn directive_like_continuation_line_is_content() {
    assert_eq!(check("s\n%TAG"), "s %TAG\n");
    assert_eq!(check("s\n%YAML 1.2"), "s %YAML 1.2\n");
    check("%YAML 1.2\n---\ns\n%TAG");
    check("s\n  %TAG x");
    check("s\r\n%TAG\r\n");
}

#[test]
fn percent_lines_inside_block_scalars_are_content() {
    check("k: |\n  %TAG !e! tag:x,2000:\n  %YAML 1.2\n");
    check("---\n- >\n  a\n  %TAG\n");
}

#[test]
fn real_directives_are_still_hoisted() {
    let out = check("# c\n%TAG !e! tag:x,2000:\n---\n!e!a b");
    assert!(out.starts_with("%TAG !e! tag:x,2000:\n---\n"), "{out:?}");
    let out = check("%YAML 1.2\n---\na: 1\n");
    assert!(out.starts_with("%YAML 1.2\n---\n"), "{out:?}");
}

#[test]
fn document_end_marker_keeps_documents_separate() {
    check("[]\n...\nT");
    check("a\n...\nb");
    check("a\n...\n");
    check("a\n---\nb\n...\n");
    check("...\na");
    let out = check("a\n...\nb");
    assert_eq!(out, "a\n---\nb\n");
}

#[test]
fn explicit_start_config_emits_one_marker_per_document() {
    let config = EmitterConfig::new().with_explicit_start(true);
    let out = format_streaming("a\n---\nb\n", &config).unwrap();
    assert_eq!(out, "---\na\n---\nb\n");
}

#[test]
fn anchor_with_colon_re_parses_and_control_chars_are_rejected() {
    check("! &:&");
    check("&a:b x");
    for input in ["&\x01 x", "a: &\x01 x\nb: *\x01"] {
        assert!(Emitter::format(input).is_err(), "{input:?}");
        assert!(
            format_streaming(input, &EmitterConfig::default()).is_err(),
            "{input:?}"
        );
    }
}

#[test]
fn anchor_with_bom_is_idempotent() {
    check("&0\u{feff}");
}

#[test]
fn ampersand_inside_plain_scalar_does_not_shift_anchor_names() {
    let out = check("a&b: 1\n&c d: 2\ne: *c");
    assert!(out.contains("&c d"), "{out:?}");
    assert!(out.contains("*c"), "{out:?}");
}

#[test]
fn tagged_empty_key_keeps_colon_separate() {
    assert_eq!(check("!  :\n? ?#\n"), "! : null\n?#: null\n");
}

#[test]
fn plain_scalar_resembling_document_marker_is_quoted() {
    assert_eq!(check("\t---\t-"), "'---\t-'\n");
    check("  ... x");
    check("a: ---\n");
}

#[test]
fn flow_plain_scalars_starting_with_indicators_are_quoted() {
    assert_eq!(check("{|}"), "'|': null\n");
    check("[>, |x]");
}

#[test]
fn aliases_follow_renamed_anchors() {
    check("&a\u{feff}b [1]\n---\nk: &b [2]\nv: *b\n");
    check("x: &keep [1]\ny: *keep\nz: 'it''s'\n");
}

/// `check` plus equality of the parsed values (aliases resolved).
#[track_caller]
fn check_values(input: &str) -> String {
    let out = check(input);
    assert_eq!(
        Parser::parse_all(input).unwrap(),
        Parser::parse_all(&out).unwrap(),
        "values for {input:?} -> {out:?}"
    );
    out
}

#[test]
fn equal_count_anchor_misalignment_never_rebinds_aliases() {
    let out = check_values("t: Tom &q\nk: [a#b, &p 1]\nb: &q 2\nc: *p\n");
    assert!(out.contains("&p 1") && out.contains("c: *p"), "{out:?}");
    check_values("t: Tom &q\nk: [a 'b, &p 1]\nb: &q 2\nc: 'x'\nd: *p\ne: Tom &q\n");
    check_values("a&b: 1\n&c d: 2\ne: *c\n");
}

#[test]
fn original_anchor_names_survive_apostrophes_and_hashes() {
    let out = check_values("a: it's\nu: http://x/#frag\nk: &first 1\nc: *first\n");
    assert!(
        out.contains("&first 1") && out.contains("*first"),
        "{out:?}"
    );
}

#[test]
fn plain_scalars_are_not_over_quoted() {
    for yaml in [
        "a: -foo\n",
        "a: ?x\n",
        "a: :x\n",
        "a: a%x\n",
        "a: ---x\n",
        "a: ...x\n",
        "[-foo, ?x, :x, a%x, ---x, ...x]\n",
    ] {
        let out = check_values(yaml);
        assert!(!out.contains('\''), "{yaml:?} was quoted: {out:?}");
        assert!(!out.contains('"'), "{yaml:?} was quoted: {out:?}");
    }
}

#[test]
fn document_end_marker_edge_cases() {
    check("a\n... # end\nb");
    check("a\n... # end\n");
    check("---\n---\nx");
    check("---\n---\n");
    check("a\n...\n...\nb");
}

#[test]
fn empty_tagged_keys() {
    check("{! : 2}");
    check("? !!str\n");
    check("? !!str\n: v\n");
}
