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

const DUPLICATE_MERGE_CASES: [(&str, &str); 2] = [
    (
        "a: &a {x: 1}\nb: &b {y: 2}\nm:\n  <<: *a\n  <<: *b\n",
        "line 5, column 3",
    ),
    ("m: {<<: {x: 1}, <<: {y: 2}}\n", "line 1, column 17"),
];

#[test]
fn duplicate_plain_merge_key_is_rejected_with_its_position_by_every_yaml_command() {
    for args in [&["convert", "json"][..], &["parse"][..], &["format"][..]] {
        for (yaml, position) in DUPLICATE_MERGE_CASES {
            let output = run(args, yaml);
            assert_eq!(output.status.code(), Some(1), "{args:?}: {output:?}");
            assert!(output.stdout.is_empty(), "{args:?}");
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                stderr.contains("duplicate merge key `<<`") && stderr.contains(position),
                "{args:?}: {stderr}"
            );
        }
    }
}

#[test]
fn duplicate_merge_key_stays_a_lint_diagnostic() {
    let output = run(&["lint"], "m: {<<: {x: 1}, <<: {y: 2}}\n");
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("duplicate-key"), "{stdout}");
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("error:"),
        "{output:?}"
    );
    let suppressed = run(
        &["lint"],
        "m: {<<: {x: 1}, <<: {y: 2}} # fy: disable-line\n",
    );
    assert!(
        !String::from_utf8_lossy(&suppressed.stdout).contains("duplicate-key"),
        "{suppressed:?}"
    );
}

#[test]
fn duplicate_merge_key_written_through_an_alias_is_not_seen_by_lint() {
    let output = run(
        &["lint"],
        "a: &a {x: 1}\nb: &b {y: 2}\nc: {&k <<: *a, *k : *b}\n",
    );
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("duplicate-key"),
        "{output:?}"
    );
}

#[test]
fn set_member_value_is_rejected_with_its_position_by_every_yaml_command() {
    for args in [
        &["convert", "json"][..],
        &["parse"][..],
        &["format"][..],
        &["lint"][..],
    ] {
        let output = run(args, "s: !!set {a: 1}\n");
        assert_eq!(output.status.code(), Some(1), "{args:?}: {output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("!!set member has a non-null value")
                && stderr.contains("line 1, column 11"),
            "{args:?}: {stderr}"
        );
    }
    let json = to_json("s: !!set {a, b: }\n");
    assert_eq!(json["s"], serde_json::json!({"a": null, "b": null}));
}

#[test]
fn format_keeps_the_float_spellings_it_was_given() {
    let output = run(&["format"], "a: -.5\nb: 1.0E5\nc: 1e300\n");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "a: -.5\nb: 1.0E5\nc: 1e300\n"
    );
}

#[test]
fn convert_yaml_writes_floats_that_yaml_11_readers_read_as_floats() {
    let output = run(
        &["convert", "yaml"],
        r#"{"a": 1e300, "b": 1.23e10, "c": 1.0E5, "d": 2.50, "e": 1.5e-7}"#,
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "a: 1.0e+300\nb: 1.23e+10\nc: 1.0e+5\nd: 2.50\ne: 1.5e-7\n"
    );
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
fn invalid_merge_value_reports_key_position_on_every_command() {
    let yaml = "a: 1\nm:\n  k: 0\n  <<: 1\n";
    for args in [&["convert", "json"][..], &["lint"], &["parse"]] {
        let output = run(args, yaml);
        assert_eq!(output.status.code(), Some(1), "{args:?}: {output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("at line 4, column 3"), "{args:?}: {stderr}");
    }
}

#[test]
fn invalid_merge_value_position_is_absolute_in_later_documents() {
    for args in [&["convert", "json"][..], &["parse"], &["lint"]] {
        let output = run(args, "a: 1\n---\nb: 2\n---\nm:\n  <<: [5]\n");
        assert_eq!(output.status.code(), Some(1), "{args:?}: {output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("at line 6, column 3"), "{args:?}: {stderr}");
    }
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

const INVALID_MERGES: &[&str] = &[
    "m:\n  <<: 1\n",
    "m:\n  <<:\n  k: 0\n",
    "m:\n  <<: [1]\n",
    "m:\n  <<: [[{x: 1}]]\n",
    "m:\n  <<: [{x: 1}, 5]\n",
    "x: 1\n---\nm:\n  <<: [1]\n",
    "x: 1\n---\ny: 2\n---\nm: {<<: 1}\n",
    "a: &a [1]\nm:\n  <<: *a\n",
    "a: &a text\nm:\n  <<: *a\n",
    "s: &s !!set {x, y}\nm:\n  <<: *s\n",
    "s: &s !!set {x, y}\nm:\n  <<: [*s]\n",
];

#[test]
fn format_rejects_invalid_merge_values() {
    for yaml in INVALID_MERGES {
        let output = run(&["format"], yaml);
        assert_eq!(output.status.code(), Some(1), "{yaml}: {output:?}");
        assert!(output.stdout.is_empty(), "{yaml}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("merge key"), "{yaml}: {stderr}");
        assert_eq!(run(&["parse"], yaml).status.code(), Some(1), "{yaml}");
    }
}

#[test]
fn format_accepts_valid_merge_values() {
    for yaml in [
        "b: &b {x: 1}\nm:\n  <<: *b\n  k: 0\n",
        "a: &a {x: 1}\nb: &b {y: 2}\nm:\n  <<: [*a, *b]\n",
        "m:\n  <<: !!map {x: 1}\n",
        "m:\n  '<<': 1\n  \"<<\": 2\n",
        "s: !!set {k, <<}\n",
    ] {
        let output = run(&["format"], yaml);
        assert_eq!(output.status.code(), Some(0), "{yaml}: {output:?}");
        assert_eq!(run(&["parse"], yaml).status.code(), Some(0), "{yaml}");
    }
}

#[test]
fn format_of_valid_merge_keys_is_idempotent() {
    for yaml in [
        "a: &a {x: 1}\nm:\n  <<: *a\n",
        "a: &a {x: 1}\nb: &b {y: 2}\nm:\n  <<: [*a, *b]\n",
        "a: &a {x: 1}\nm: {<<: *a, k: 0}\n",
        "a: &a {x: 1}\nm: {<<: [*a], k: 0}\n",
    ] {
        let once = run(&["format"], yaml);
        assert_eq!(once.status.code(), Some(0), "{yaml}: {once:?}");
        let once = String::from_utf8(once.stdout).unwrap();
        let twice = run(&["format"], &once);
        assert_eq!(twice.status.code(), Some(0), "{once}: {twice:?}");
        assert_eq!(String::from_utf8(twice.stdout).unwrap(), once, "{yaml}");
    }
}

#[test]
fn format_rejects_self_referencing_merge_value() {
    for yaml in ["m: &a {<<: *a}\n", "m: &a [{<<: *a}]\n"] {
        let output = run(&["format"], yaml);
        assert_eq!(output.status.code(), Some(1), "{yaml}: {output:?}");
        assert_eq!(run(&["parse"], yaml).status.code(), Some(1), "{yaml}");
    }
}

#[test]
fn merge_error_reports_document_line_and_column() {
    let yaml = "x: 1\n---\ny: 2\n---\nm:\n  <<: 1\n";
    for command in ["format", "parse", "lint"] {
        let output = run(&[command], yaml);
        assert_eq!(output.status.code(), Some(1), "{command}: {output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("line 6, column 3 (document 3)"),
            "{command}: {stderr}"
        );
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

#[test]
fn parse_and_format_agree_on_hidden_and_ordered_merge_errors() {
    const NOT_MAPPING: &str = "requires a mapping or a sequence of mappings";
    const SET_SOURCE: &str = "cannot merge a `!!set`";
    for (yaml, kind, position) in [
        ("x: {<<: 1}\nx: 2\n", NOT_MAPPING, "at line 1, column 5"),
        (
            "m: {<<: 1, x: {<<: !!set {a}}}\n",
            NOT_MAPPING,
            "at line 1, column 5",
        ),
        (
            "m: {x: {<<: !!set {a}}, <<: 1}\n",
            SET_SOURCE,
            "at line 1, column 9",
        ),
    ] {
        for command in ["parse", "format", "lint"] {
            let output = run(&[command], yaml);
            assert_eq!(
                output.status.code(),
                Some(1),
                "{command} {yaml:?}: {output:?}"
            );
            let stderr = String::from_utf8_lossy(&output.stderr);
            let expected = format!("merge key `<<` {kind} {position}");
            assert!(stderr.contains(&expected), "{command} {yaml:?}: {stderr}");
        }
    }
}
