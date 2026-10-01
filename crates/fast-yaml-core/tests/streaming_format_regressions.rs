//! Regression tests for the streaming formatter: custom indentation (#357),
//! folded and keep-chomp block scalars (#356) and empty flow collections (#355).
//!
//! Every case asserts the exact output, idempotency and value preservation.

use std::fmt::Write;

use fast_yaml_core::streaming::format_streaming;
use fast_yaml_core::{
    EmitError, Emitter, EmitterConfig, Indent, LimitKind, MaxDepth, ParseError, ParseLimits,
    Parser, Value,
};

fn fmt(input: &str, indent: usize) -> String {
    let config = EmitterConfig::new().with_indent(Indent::new(indent).unwrap());
    format_streaming(input, &config).unwrap()
}

#[track_caller]
fn check(input: &str, indent: usize, expected: &str) {
    let once = fmt(input, indent);
    assert_eq!(
        once, expected,
        "exact output for {input:?} at indent {indent}"
    );
    assert_eq!(
        fmt(&once, indent),
        once,
        "idempotency for {input:?} at indent {indent}"
    );
    assert_eq!(
        Parser::parse_all(input).unwrap(),
        Parser::parse_all(&once).unwrap(),
        "value preservation for {input:?} at indent {indent}"
    );
}

#[track_caller]
fn check_all_indents(input: &str, expected_for: impl Fn(usize) -> String) {
    for indent in [2, 3, 4, 8] {
        check(input, indent, &expected_for(indent));
    }
}

fn spaces(n: usize) -> String {
    " ".repeat(n)
}

#[test]
fn seq_of_maps_keeps_dash_column() {
    check_all_indents("- a: 1\n  b: 2\n- c: 3\n", |_| {
        "- a: 1\n  b: 2\n- c: 3\n".into()
    });
}

#[test]
fn seq_in_seq_keeps_dash_column() {
    check_all_indents("- - b\n  - c\n- d\n", |_| "- - b\n  - c\n- d\n".into());
    check_all_indents("- - - x\n    - y\n  - z\n", |_| {
        "- - - x\n    - y\n  - z\n".into()
    });
}

#[test]
fn seq_of_seq_of_maps() {
    check_all_indents("- - a: 1\n    b: 2\n", |_| "- - a: 1\n    b: 2\n".into());
}

#[test]
fn map_of_seq_of_maps_uses_configured_indent() {
    check_all_indents("k:\n  - a: 1\n    b: 2\n  - c: 3\n", |i| {
        let s = spaces(i);
        format!("k:\n{s}- a: 1\n{s}  b: 2\n{s}- c: 3\n")
    });
}

#[test]
fn nested_map_in_seq_item_uses_configured_indent() {
    check_all_indents("- k:\n    - a: 1\n      b: 2\n", |i| {
        let dash = spaces(2 + i);
        let key = spaces(4 + i);
        format!("- k:\n{dash}- a: 1\n{key}b: 2\n")
    });
}

#[test]
fn flow_seq_inside_seq() {
    check_all_indents("- [x, y]\n- z\n", |_| "- - x\n  - y\n- z\n".into());
}

#[test]
fn literal_in_seq_of_maps_uses_configured_indent() {
    check_all_indents("- k: |\n    x\n    y\n  m: 1\n", |i| {
        let s = spaces(2 + i);
        format!("- k: |\n{s}x\n{s}y\n  m: 1\n")
    });
}

#[test]
fn anchored_collection_items_in_seq() {
    check_all_indents("- &a\n  k: v\n- &b\n  - x\n", |_| {
        "- &a\n  k: v\n- &b\n  - x\n".into()
    });
}

#[test]
fn alias_key_in_seq_item() {
    check_all_indents("- &a x\n- *a : 1\n  b: 2\n", |_| {
        "- &a x\n- *a : 1\n  b: 2\n".into()
    });
}

#[test]
fn folded_keeps_blank_line_between_paragraphs() {
    check("a: >\n  x\n\n  y\n", 2, "a: >\n  x\n\n  y\n");
    check("a: >\n  x\n\n\n  y z\n", 2, "a: >\n  x\n\n\n  y z\n");
    check("a: >\n  x\n\n\n\n  y\n", 2, "a: >\n  x\n\n\n\n  y\n");
}

#[test]
fn folded_chomping_variants() {
    check("a: >-\n  x\n\n  y\n", 2, "a: >-\n  x\n\n  y\n");
    check(
        "a: >+\n  x\n\n  y\n\n\nb: 1\n",
        2,
        "a: >+\n  x\n\n  y\n\n\nb: 1\n",
    );
    check("a: >\n  x\n  y\n", 2, "a: >\n  x y\n");
}

#[test]
fn folded_with_more_indented_line_falls_back_to_literal() {
    check(
        "a: >\n  x\n\n    indented\n  y\n",
        2,
        "a: |\n  x\n\n    indented\n  y\n",
    );
}

#[test]
fn folded_at_configured_indent() {
    check_all_indents("- k: >\n    x\n\n    y\n", |i| {
        let s = spaces(2 + i);
        format!("- k: >\n{s}x\n\n{s}y\n")
    });
}

#[test]
fn keep_chomp_is_stable() {
    check("b: |+\n  p\n\n\nc: 1\n", 2, "b: |+\n  p\n\n\nc: 1\n");
    check("b: |+\n  p\n\nc: 1\n", 2, "b: |+\n  p\n\nc: 1\n");
    check("- |+\n  p\n\n\n- q\n", 2, "- |+\n  p\n\n\n- q\n");
}

#[test]
fn chomp_variants_have_no_extra_blank_line() {
    check(
        "a: |\n  x\nb: |-\n  y\nc: 1\n",
        2,
        "a: |\n  x\nb: |-\n  y\nc: 1\n",
    );
    check("a: |-\n  x\n\n  y\nb: 1\n", 2, "a: |-\n  x\n\n  y\nb: 1\n");
}

#[test]
fn block_scalar_leading_space_gets_indentation_indicator() {
    check("e: |2\n    lead\n  x\n", 2, "e: |2\n    lead\n  x\n");
    check("e: >2\n    lead\n  x\n", 2, "e: |2\n    lead\n  x\n");
    check("e: |1\n  lead\n x\n", 4, "e: |4\n     lead\n    x\n");
}

#[test]
fn block_scalar_degenerate_values() {
    check("a: |+\n\nb: 1\n", 2, "a: |+\n\nb: 1\n");
    check("a: |-\nb: 1\n", 2, "a: |-\nb: 1\n");
    check("a: |\n  x\n", 2, "a: |\n  x\n");
}

#[test]
fn block_scalar_in_seq_and_root() {
    check("- |\n  x\n- >-\n  y\n", 2, "- |\n  x\n- >-\n  y\n");
    check("|\n  x\n  y\n", 2, "|\n  x\n  y\n");
}

#[test]
fn empty_flow_as_mapping_value() {
    check_all_indents("a: []\nb: {}\n", |_| "a: []\nb: {}\n".into());
    check(
        "a:\n  b: []\n  c: {}\nd: 1\n",
        2,
        "a:\n  b: []\n  c: {}\nd: 1\n",
    );
    check_all_indents("k:\n  - a: []\n    b: {}\n", |i| {
        let s = spaces(i);
        format!("k:\n{s}- a: []\n{s}  b: {{}}\n")
    });
}

#[test]
fn empty_flow_as_seq_item() {
    check("- []\n- x\n", 2, "- []\n- x\n");
    check("- x\n- {}\n- y\n", 4, "- x\n- {}\n- y\n");
    check("k:\n  - []\n  - x\n", 2, "k:\n  - []\n  - x\n");
    check("- {}\n- []\n", 2, "- {}\n- []\n");
}

#[test]
fn empty_flow_followed_by_anchored_collection() {
    check("- {}\n- &b [1]\n", 2, "- {}\n- &b\n  - 1\n");
    check(
        "- []\n- &b {k: v}\n- &c x\n- *c\n",
        4,
        "- []\n- &b\n  k: v\n- &c x\n- *c\n",
    );
}

#[test]
fn empty_flow_as_key() {
    check("? []\n: 1\n? {}\n: 2\n", 2, "[]: 1\n{}: 2\n");
}

#[test]
fn empty_flow_at_root() {
    check("[]\n", 2, "[]\n");
    check("{}\n", 2, "{}\n");
}

#[test]
fn empty_flow_with_anchor_and_alias() {
    check("a: &x []\nb: *x\n", 2, "a: &x []\nb: *x\n");
    check("- &x {}\n- *x\n", 2, "- &x {}\n- *x\n");
}

#[test]
fn nested_empty_flow() {
    check("[[]]\n", 2, "- []\n");
    check("{a: {}}\n", 2, "a: {}\n");
}

#[test]
fn empty_flow_multi_document() {
    check("a: []\n---\nb: {}\n", 2, "a: []\n---\nb: {}\n");
}

#[test]
fn empty_flow_keeps_tags_and_anchors() {
    check(
        "a: !!seq []\nb: !!map {}\n",
        2,
        "a: !!seq []\nb: !!map {}\n",
    );
    check("a: &x !!set {}\nb: *x\n", 2, "a: &x !!set {}\nb: *x\n");
    check("- !!seq []\n- &y !t {}\n", 4, "- !!seq []\n- &y !t {}\n");
}

#[test]
fn empty_flow_with_explicit_keys() {
    check("? [a]\n: []\n", 2, "?\n  - a\n: []\n");
    check("? [a]\n: {}\nk: 1\n", 4, "?\n    - a\n: {}\nk: 1\n");
    check("? []\n: [a]\n", 2, "[]:\n  - a\n");
    check("- ? [a]\n  : {}\n", 4, "- ?\n      - a\n  : {}\n");
}

#[test]
fn complex_keys_at_configured_indent() {
    check_all_indents("? [a, b]\n: c\n", |i| {
        format!("?\n{}- a\n{}- b\n: c\n", spaces(i), spaces(i))
    });
}

#[test]
fn empty_flow_next_to_omitted_nulls() {
    check(
        "a: []\nb:\nc: {}\nd: !!null\ne: !!seq []\n",
        2,
        "a: []\nb: null\nc: {}\nd: !!null\ne: !!seq []\n",
    );
    check(
        "- {}\n-\n- []\n- !!null\n",
        4,
        "- {}\n- null\n- []\n- !!null\n",
    );
    check("? []\n:\n? {}\n: x\n", 2, "[]: null\n{}: x\n");
}

#[test]
fn empty_flow_with_explicit_key_in_seq_and_anchored_key() {
    check("- ? []\n  : 1\n", 4, "- []: 1\n");
    check("&k [] : v\n", 2, "&k []: v\n");
    check("? &k {}\n: v\nw: *k\n", 2, "&k {}: v\nw: *k\n");
}

#[test]
fn tagged_empty_flow_at_indent_4() {
    check(
        "k:\n  - a: !!seq []\n    b: !!map {}\n",
        4,
        "k:\n    - a: !!seq []\n      b: !!map {}\n",
    );
}

#[test]
fn keep_chomp_at_configured_indent_root_and_seq_of_maps() {
    check("k: |+\n  x\n\n\nz: 1\n", 4, "k: |+\n    x\n\n\nz: 1\n");
    check("|+\n  x\n\n\n", 4, "|+\n    x\n\n\n");
    check(
        "- k: |+\n    x\n\n  m: 1\n",
        4,
        "- k: |+\n      x\n\n  m: 1\n",
    );
}

#[test]
fn block_scalars_in_multi_doc_and_root_at_indent_4() {
    check(
        "|\n  a\n---\n>\n  b\n\n  c\n",
        4,
        "|\n    a\n---\n>\n    b\n\n    c\n",
    );
    check(
        "a: |\n  x\n---\nb: >-\n  y\n",
        4,
        "a: |\n    x\n---\nb: >-\n    y\n",
    );
}

#[test]
fn folded_with_whitespace_only_line() {
    check("a: >\n  x\n   \n  y\n", 2, "a: |\n  x\n   \n  y\n");
}

#[test]
fn crlf_input_block_scalars_and_empty_flow() {
    let folded = fmt("a: >\r\n  x\r\n\r\n  y\r\n", 2);
    assert_eq!(folded, "a: >\n  x\n\n  y\n");
    assert_eq!(fmt("a: []\r\nb: {}\r\n", 2), "a: []\nb: {}\n");
}

#[test]
fn alias_key_at_mapping_root() {
    check("a: &x 1\n*x : 2\n", 2, "a: &x 1\n*x : 2\n");
    check_all_indents("k:\n  - &x 1\n  - *x : 2\n", |i| {
        let s = spaces(i);
        format!("k:\n{s}- &x 1\n{s}- *x : 2\n")
    });
}

#[test]
fn widest_indent_keeps_block_scalar_indicator_valid() {
    let input = "e: |1\n  lead\n x\n";
    let config = EmitterConfig::new().with_indent(Indent::MAX);
    let out = format_streaming(input, &config).unwrap();
    assert_eq!(out, "e: |9\n          lead\n         x\n");
    assert_eq!(format_streaming(&out, &config).unwrap(), out);
    assert_eq!(
        Parser::parse_all(input).unwrap(),
        Parser::parse_all(&out).unwrap()
    );
}

#[test]
fn narrowest_indent_is_one_space() {
    let config = EmitterConfig::new().with_indent(Indent::MIN);
    let out = format_streaming("k:\n  - a\n", &config).unwrap();
    assert_eq!(out, "k:\n - a\n");
}

#[test]
fn documents_separated_by_document_end_keep_separator() {
    check("a: 1\n...\nb: 2\n", 2, "a: 1\n---\nb: 2\n");
    check("a\n...\nb\n", 2, "a\n---\nb\n");
    check("!\n...\r$", 2, "!\n---\n$\n");
}

#[test]
fn deep_nesting_keeps_column_stack_balanced() {
    let depth = 200;
    let mut input = String::new();
    for i in 0..depth {
        writeln!(input, "{}k{i}:", "  ".repeat(i)).unwrap();
    }
    writeln!(input, "{}leaf: 1\nafter: 2", "  ".repeat(depth)).unwrap();
    for indent in [2, 4] {
        let once = fmt(&input, indent);
        assert_eq!(fmt(&once, indent), once);
        assert!(once.ends_with("after: 2\n"));
        assert_eq!(
            Parser::parse_all(&input).unwrap(),
            Parser::parse_all(&once).unwrap()
        );
    }
}

fn on_stack<T: Send + 'static>(mib: usize, f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(mib * 1024 * 1024)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn max_depth_bounds_the_formatter_and_defaults_to_256() {
    let nested = |depth: usize| format!("{}v\n", "- ".repeat(depth));
    let config = EmitterConfig::new();
    assert!(format_streaming(&nested(256), &config).is_ok());
    let err = format_streaming(&nested(257), &config).unwrap_err();
    let EmitError::Parse(ParseError::LimitExceeded { kind, line, .. }) = err else {
        panic!("depth error expected, got {err:?}");
    };
    assert_eq!(kind, LimitKind::Depth(MaxDepth::DEFAULT));
    assert_eq!(line, 1);

    let shallow = EmitterConfig::new().with_parse_limits(ParseLimits {
        max_depth: MaxDepth::new(2).unwrap(),
        ..ParseLimits::default()
    });
    assert!(format_streaming(&nested(2), &shallow).is_ok());
    assert!(format_streaming(&nested(3), &shallow).is_err());
}

#[test]
fn formatter_at_max_depth_fits_a_2_mib_stack() {
    on_stack(2, || {
        let config = EmitterConfig::new().with_parse_limits(ParseLimits {
            max_depth: MaxDepth::MAX,
            ..ParseLimits::default()
        });
        let seqs = format!("{}v\n", "- ".repeat(512));
        let out = format_streaming(&seqs, &config).unwrap();
        assert_eq!(format_streaming(&out, &config).unwrap(), out);
        let mut maps = String::new();
        for i in 0..511 {
            writeln!(maps, "{}k:", "  ".repeat(i)).unwrap();
        }
        writeln!(maps, "{}v: 1", "  ".repeat(511)).unwrap();
        let out = format_streaming(&maps, &config).unwrap();
        assert_eq!(format_streaming(&out, &config).unwrap(), out);
    });
}

#[test]
fn emitting_at_max_depth_fits_a_small_stack() {
    // saphyr's recursive block emitter needs about 2 MiB at depth 512 in release, more in debug
    let mib = if cfg!(debug_assertions) { 8 } else { 2 };
    on_stack(mib, || {
        let mut doc = Value::Int(1);
        for _ in 0..512 {
            doc = Value::Sequence(vec![doc]);
        }
        let block = EmitterConfig::new().with_max_emit_depth(MaxDepth::MAX);
        let flow = block.clone().with_default_flow_style(Some(true));
        for config in [&block, &flow] {
            let out = Emitter::emit_str_with_config(&doc, config).unwrap();
            assert!(out.contains('1'));
        }
        // dropping a 512-deep value recurses; leak it to keep the test about emission
        std::mem::forget(doc);
    });
}
