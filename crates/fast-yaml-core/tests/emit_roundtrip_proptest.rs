//! Property-based check that `Emitter::emit_*` output parses back to the same `Value`.
//!
//! Trees mix nested lists, complex keys, sets, multiline and Unicode-whitespace strings and floats
//! with source text, and are emitted at every indentation width.

use fast_yaml_core::{Emitter, EmitterConfig, Float, Indent, Mapping, Parser, Set, Value};
use proptest::prelude::*;

fn text() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-z][a-z0-9_]{0,6}",
        "[0-9]{1,4}",
        Just(String::new()),
        Just("true".to_owned()),
        Just("null".to_owned()),
        Just("~".to_owned()),
        Just("- a".to_owned()),
        Just("? a".to_owned()),
        Just(": a".to_owned()),
        Just("a: b".to_owned()),
        Just("a #b".to_owned()),
        Just("---".to_owned()),
        Just("...".to_owned()),
        Just("<<".to_owned()),
        Just("!x".to_owned()),
        Just("&a".to_owned()),
        Just("*a".to_owned()),
        Just("|".to_owned()),
        Just("line1\nline2".to_owned()),
        Just("line1\n\nline3\n".to_owned()),
        Just("\nlead".to_owned()),
        Just("\n\n".to_owned()),
        Just("a\r\nb".to_owned()),
        Just("a\rb".to_owned()),
        Just(" a\nb".to_owned()),
        Just("a\n\n".to_owned()),
        Just("line1\n---x\n...\n%YAML y".to_owned()),
        Just("x>y".to_owned()),
        Just("a|b".to_owned()),
        Just("  indented\n  body".to_owned()),
        Just("trail ".to_owned()),
        Just(" lead".to_owned()),
        Just("\u{FEFF}".to_owned()),
        Just("\u{FEFF}admin".to_owned()),
        Just("a\u{FEFF}b".to_owned()),
        Just("\u{A0}nbsp".to_owned()),
        Just("\u{3000}wide".to_owned()),
        Just("\u{85}nel".to_owned()),
        Just("\u{2028}ls".to_owned()),
        Just("tab\there".to_owned()),
        "[а-яé日本]{1,4}",
    ]
}

fn scalar() -> impl Strategy<Value = Value> {
    prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(Value::Int),
        (-1.0e6f64..1.0e6).prop_map(|f| Value::Float(Float::from(f))),
        Just(Value::Float(Float::from(f64::INFINITY))),
        text().prop_map(Value::String),
    ]
}

fn key() -> impl Strategy<Value = Value> {
    prop_oneof![
        4 => scalar(),
        1 => prop::collection::vec(scalar(), 1..3).prop_map(Value::Sequence),
        1 => prop::collection::vec((scalar(), scalar()), 1..3)
            .prop_map(|pairs| Value::Mapping(pairs.into_iter().collect::<Mapping>())),
    ]
}

fn tree() -> impl Strategy<Value = Value> {
    scalar().prop_recursive(4, 32, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4).prop_map(Value::Sequence),
            prop::collection::vec((key(), inner), 0..4)
                .prop_map(|pairs| Value::Mapping(pairs.into_iter().collect::<Mapping>())),
            prop::collection::vec(scalar(), 0..4)
                .prop_map(|members| Value::Set(members.into_iter().collect::<Set>())),
        ]
    })
}

fn check(doc: &Value, indent: usize, multiline: bool) -> Result<(), TestCaseError> {
    let config = EmitterConfig::new()
        .with_indent(Indent::new(indent).unwrap())
        .with_multiline_strings(multiline);
    let yaml = Emitter::emit_str_with_config(doc, &config)
        .map_err(|e| TestCaseError::fail(format!("emit failed: {e}")))?;
    let back = Parser::parse_str(&yaml)
        .map_err(|e| TestCaseError::fail(format!("reparse failed: {e}\n{yaml}")))?
        .unwrap_or(Value::Null);
    prop_assert_eq!(
        &back,
        doc,
        "indent {} multiline {}:\n{}",
        indent,
        multiline,
        yaml
    );
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    #[test]
    fn block_emit_round_trips_at_every_indent(doc in tree(), indent in 1usize..=9) {
        check(&doc, indent, false)?;
    }

    #[test]
    fn multiline_block_emit_round_trips_at_every_indent(doc in tree(), indent in 1usize..=9) {
        check(&doc, indent, true)?;
    }
}
