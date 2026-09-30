//! Pins known divergences between `Parser::parse_all` (saphyr) and `parse_parallel` (#407).
//!
//! The literal/folded tests flip when saphyr fixes column-0 `---` handling in block scalars; the
//! empty-scalar test flips when saphyr fixes empty block scalar chomping at EOF. Update deliberately.

use fast_yaml_core::{Parser, ScalarOwned, Value};
use fast_yaml_parallel::{Config, Error, parse_parallel, parse_parallel_with_config};

fn string(s: &str) -> Value {
    Value::Value(ScalarOwned::String(s.into()))
}

fn int_map(key: &str, n: i64) -> Value {
    let mut map = fast_yaml_core::Map::new();
    map.insert(string(key), Value::Value(ScalarOwned::Integer(n)));
    Value::Mapping(map)
}

fn str_map(key: &str, value: &str) -> Value {
    let mut map = fast_yaml_core::Map::new();
    map.insert(string(key), string(value));
    Value::Mapping(map)
}

fn assert_diverges(input: &str, saphyr: &[Value], parallel: &[Value]) {
    let sequential = Config::new().with_workers(Some(0));
    let all = Parser::parse_all(input).unwrap();
    assert_eq!(all, saphyr, "parse_all: {input:?}");
    for docs in [
        parse_parallel(input).unwrap(),
        parse_parallel_with_config(input, &sequential).unwrap(),
    ] {
        assert_eq!(docs, parallel, "parse_parallel: {input:?}");
        assert_ne!(docs, all, "{input:?}");
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
fn empty_block_scalar_before_marker_differs_from_eof() {
    assert_diverges(
        "a: |\n---\nx\n",
        &[str_map("a", ""), string("x")],
        &[str_map("a", "\n"), string("x")],
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
