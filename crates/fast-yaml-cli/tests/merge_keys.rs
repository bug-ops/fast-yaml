//! Regression tests for #478 and #481: only a plain `<<` merges, and its value must be a mapping.

#![allow(clippy::missing_docs_in_private_items)]
#![allow(deprecated)] // Command::cargo_bin is deprecated but still works

use assert_cmd::Command;
use std::time::Duration;

fn run(args: &[&str], stdin: &str) -> std::process::Output {
    Command::cargo_bin("fy")
        .unwrap()
        .args(args)
        .write_stdin(stdin)
        .timeout(Duration::from_secs(10))
        .output()
        .unwrap()
}

fn to_json(yaml: &str) -> serde_json::Value {
    let output = run(&["convert", "json"], yaml);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn quoted_and_tagged_merge_keys_are_ordinary_keys() {
    for key in ["'<<'", "\"<<\"", "!!str <<"] {
        let json = to_json(&format!("b: &b {{x: 1}}\nm:\n  {key}: *b\n  k: 0\n"));
        assert_eq!(
            json["m"],
            serde_json::json!({"<<": {"x": 1}, "k": 0}),
            "{key}"
        );
    }
}

#[test]
fn plain_merge_key_still_merges() {
    let json = to_json("b: &b {x: 1}\nm:\n  <<: *b\n  k: 0\n");
    assert_eq!(json["m"], serde_json::json!({"x": 1, "k": 0}));
}

#[test]
fn json_merge_key_survives_a_yaml_round_trip() {
    let yaml = run(
        &["convert", "yaml"],
        r#"{"m": {"<<": {"admin": true}, "k": 0}}"#,
    );
    assert_eq!(yaml.status.code(), Some(0), "{yaml:?}");
    let json = to_json(&String::from_utf8(yaml.stdout).unwrap());
    assert_eq!(
        json["m"],
        serde_json::json!({"<<": {"admin": true}, "k": 0})
    );
}

#[test]
fn invalid_merge_values_are_rejected() {
    for merge in ["1", "null", "text", "[1]", "[[{x: 1}]]", "[{x: 1}, 5]"] {
        let output = run(
            &["convert", "json"],
            &format!("m:\n  <<: {merge}\n  k: 0\n"),
        );
        assert_eq!(output.status.code(), Some(1), "{merge}");
        assert!(output.stdout.is_empty(), "{merge}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("merge key"), "{merge}: {stderr}");
    }
}

#[test]
fn set_merge_source_is_rejected() {
    let output = run(&["convert", "json"], "s: &s !!set {x, y}\nm:\n  <<: *s\n");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("!!set"));
}

#[test]
fn merge_key_is_an_ordinary_set_element() {
    let json = to_json("s: !!set {k, <<}\n");
    assert_eq!(json["s"], serde_json::json!({"k": null, "<<": null}));
}

#[test]
fn aliased_set_keeps_merge_element() {
    let json = to_json("a: &a !!set {k, <<}\nb: *a\n");
    assert_eq!(json["b"], serde_json::json!({"k": null, "<<": null}));
}

#[test]
fn explicitly_tagged_merge_values_are_mappings() {
    let json = to_json("m:\n  <<: !!seq [!!map {x: 1}, {y: 2}]\n  k: 0\n");
    assert_eq!(json["m"], serde_json::json!({"x": 1, "y": 2, "k": 0}));
}

#[test]
fn nested_invalid_merge_values_are_rejected() {
    for yaml in [
        "m:\n  <<: {<<: 1}\n",
        "a: &a {<<: 1}\nm:\n  <<: *a\n",
        "m:\n  <<: [{x: 1}, {<<: [2]}]\n",
    ] {
        let output = run(&["convert", "json"], yaml);
        assert_eq!(output.status.code(), Some(1), "{yaml}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("merge key"),
            "{yaml}"
        );
    }
}

#[test]
fn duplicate_plain_merge_key_keeps_the_last() {
    let json = to_json("a: &a {x: 1}\nb: &b {y: 2}\nm:\n  <<: *a\n  <<: *b\n");
    assert_eq!(json["m"], serde_json::json!({"y": 2}));
}

#[test]
fn duplicate_merge_key_rejects_invalid_earlier_value() {
    let output = run(&["convert", "json"], "m:\n  <<: 1\n  <<: {a: 1}\n");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("merge key"));
}

const ANCHORED_KEY: &str = "a: {&k <<: {x: 1}}\n";

#[test]
fn alias_to_anchored_merge_key_in_value_is_the_plain_string() {
    let json = to_json(&format!("{ANCHORED_KEY}b: *k\n"));
    assert_eq!(json["b"], "<<");
}

#[test]
fn alias_to_anchored_merge_key_in_sequence_is_the_plain_string() {
    let json = to_json(&format!("{ANCHORED_KEY}n: [*k]\n"));
    assert_eq!(json["n"], serde_json::json!(["<<"]));
}

#[test]
fn alias_to_anchored_merge_key_as_later_key_merges() {
    let json = to_json(&format!("{ANCHORED_KEY}n: {{*k : {{y: 2}}}}\n"));
    assert_eq!(json["n"], serde_json::json!({"y": 2}));
}

#[test]
fn alias_to_anchored_merge_key_in_set_is_the_plain_string() {
    let json = to_json(&format!("{ANCHORED_KEY}s: !!set {{*k, z}}\n"));
    assert_eq!(json["s"], serde_json::json!({"<<": null, "z": null}));
}

#[test]
fn duplicate_merge_key_through_alias_rejects_invalid_earlier_value() {
    let output = run(
        &["convert", "json"],
        "m:\n  ? &k <<\n  : 1\n  *k : {y: 2}\n",
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
}

#[test]
fn lint_fails_on_invalid_merge_value() {
    let output = run(&["lint"], "m:\n  <<: 1\n  k: 0\n");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("merge key"));
}

#[test]
fn flow_dump_of_json_merge_key_is_quoted() {
    let output = run(&["convert", "yaml"], r#"{"m": {"<<": 1}}"#);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let json = to_json(&String::from_utf8(output.stdout).unwrap());
    assert_eq!(json["m"], serde_json::json!({"<<": 1}));
}

#[test]
fn parse_rejects_invalid_merge_value_in_any_document() {
    for yaml in [
        "x: 1\n---\nm:\n  <<: [1]\n",
        "x: 1\n---\ny: 2\n---\nm:\n  <<: 1\n",
    ] {
        let parse = run(&["parse"], yaml);
        assert_eq!(parse.status.code(), Some(1), "{yaml}: {parse:?}");
        assert!(
            String::from_utf8_lossy(&parse.stderr).contains("merge key"),
            "{yaml}"
        );
        assert_eq!(run(&["lint"], yaml).status.code(), Some(1), "{yaml}");
    }
}

#[test]
fn parse_accepts_valid_merge_keys_in_later_documents() {
    let output = run(
        &["parse"],
        "x: 1\n---\nb: &b {y: 2}\nm:\n  <<: *b\n---\nz: 3\n",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}
