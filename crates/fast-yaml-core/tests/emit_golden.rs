//! Golden outputs of the event-driven emitter where it differs from the `saphyr` one it replaced.
//!
//! Each case names the divergence it pins; every output also reads back as the same value.

use fast_yaml_core::{Emitter, EmitterConfig, Indent, Mapping, Parser, Set, Value};

fn text(s: &str) -> Value {
    Value::String(s.into())
}

fn map(entries: impl IntoIterator<Item = (Value, Value)>) -> Value {
    Value::Mapping(entries.into_iter().collect::<Mapping>())
}

fn emit(value: &Value, config: &EmitterConfig) -> String {
    let out = Emitter::emit_str_with_config(value, config).unwrap();
    let back = Parser::parse_str(&out).unwrap().unwrap_or(Value::Null);
    assert_eq!(&back, value, "{out:?}");
    out
}

fn block() -> EmitterConfig {
    EmitterConfig::new()
}

fn flow() -> EmitterConfig {
    EmitterConfig::new().with_default_flow_style(Some(true))
}

#[test]
fn null_keys_are_a_tilde_in_flow_style() {
    let doc = map([(Value::Null, text("b"))]);
    assert_eq!(emit(&doc, &flow()), "{~: b}\n");
    assert_eq!(emit(&doc, &block()), "~: b\n");
}

#[test]
fn collection_keys_use_the_formatter_layout_at_every_indent() {
    let key = Value::Sequence(vec![text("a"), text("b")]);
    let doc = map([(key, Value::Sequence(vec![Value::Int(1)]))]);
    assert_eq!(emit(&doc, &block()), "?\n  - a\n  - b\n:\n  - 1\n");
    let four = EmitterConfig::new().with_indent(Indent::new(4).unwrap());
    assert_eq!(emit(&doc, &four), "?\n    - a\n    - b\n:\n    - 1\n");
}

#[test]
fn root_literal_blocks_are_indented_under_their_header() {
    let config = EmitterConfig::new().with_multiline_strings(true);
    assert_eq!(emit(&text("one\ntwo\n"), &config), "|\n  one\n  two\n");
}

#[test]
fn literal_blocks_work_at_any_indent_and_blank_lines_stay_empty() {
    let doc = map([(text("k"), text("a\n\nb\n"))]);
    for indent in 1..=9 {
        let config = EmitterConfig::new()
            .with_indent(Indent::new(indent).unwrap())
            .with_multiline_strings(true);
        let pad = " ".repeat(indent);
        assert_eq!(
            emit(&doc, &config),
            format!("k: |\n{pad}a\n\n{pad}b\n"),
            "indent {indent}"
        );
    }
}

#[test]
fn yaml_11_words_and_lookalikes_are_quoted_in_every_position() {
    let doc = map([
        (text("yes"), text("on")),
        (text("-foo"), text("infinity")),
        (text("a"), text("1_000")),
        (text("b"), text("2001-12-14")),
    ]);
    assert_eq!(
        emit(&doc, &block()),
        "\"yes\": \"on\"\n\"-foo\": \"infinity\"\na: \"1_000\"\nb: \"2001-12-14\"\n"
    );
    assert_eq!(
        emit(&doc, &flow()),
        "{\"yes\": \"on\", \"-foo\": \"infinity\", a: \"1_000\", b: \"2001-12-14\"}\n"
    );
}

#[test]
fn line_separators_are_escaped() {
    let doc = map([(text("k"), text("a\u{2028}b\u{2029}"))]);
    assert_eq!(emit(&doc, &block()), "k: \"a\\u2028b\\u2029\"\n");
    let config = EmitterConfig::new().with_multiline_strings(true);
    assert_eq!(
        emit(&text("x\u{2028}\ny\n"), &config),
        "\"x\\u2028\\ny\\n\"\n"
    );
}

#[test]
fn flow_strings_ending_in_a_dash_and_binary_lookalikes_are_quoted() {
    let doc = map([(
        text("k"),
        Value::Sequence(vec![text("a -"), text("0b1010")]),
    )]);
    assert_eq!(emit(&doc, &flow()), "{k: [\"a -\", \"0b1010\"]}\n");
    assert_eq!(emit(&doc, &block()), "k:\n  - a -\n  - \"0b1010\"\n");
}

#[test]
fn next_line_characters_are_escaped_and_never_plain_or_literal() {
    let doc = map([(text("k"), text("line\nb\u{85}evil: 1"))]);
    let multiline = EmitterConfig::new().with_multiline_strings(true);
    assert_eq!(emit(&doc, &multiline), "k: \"line\\nb\\x85evil: 1\"\n");
    assert_eq!(emit(&text("a\u{85}- x"), &flow()), "\"a\\x85- x\"\n");
}

#[test]
fn format_writes_literal_blocks_with_line_separators_double_quoted() {
    let out = Emitter::format("c: |\n  l\u{2028}m\n  n\u{85}o\n").unwrap();
    assert_eq!(out, "c: \"l\\u2028m\\nn\\x85o\\n\"\n");
}

#[test]
fn sets_round_trip_in_block_and_flow() {
    let set: Set = [text("a"), text("b")].into_iter().collect();
    let doc = map([(text("s"), Value::Set(set))]);
    assert_eq!(emit(&doc, &block()), "s: !!set\n  a: ~\n  b: ~\n");
    assert_eq!(emit(&doc, &flow()), "{s: !!set {a, b}}\n");
}
