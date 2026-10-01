//! Integration tests for the `fy` CLI tool.

#![allow(clippy::missing_docs_in_private_items)]

use assert_cmd::cargo_bin_cmd;
use predicates::prelude::*;

#[test]
fn test_version() {
    cargo_bin_cmd!("fy")
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("fy"));
}

#[test]
fn test_help() {
    cargo_bin_cmd!("fy")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Fast YAML"));
}

#[test]
fn test_parse_stdin() {
    cargo_bin_cmd!("fy")
        .arg("--quiet")
        .arg("parse")
        .write_stdin("name: test\nvalue: 123")
        .assert()
        .success();
}

#[test]
fn test_parse_invalid_yaml() {
    cargo_bin_cmd!("fy")
        .arg("parse")
        .write_stdin("invalid: [")
        .assert()
        .failure()
        .code(1);
}

#[test]
fn test_format_stdin() {
    cargo_bin_cmd!("fy")
        .arg("format")
        .write_stdin("name:   test\nvalue:    123")
        .assert()
        .success()
        .stdout(predicate::str::contains("name: test"));
}

#[test]
fn test_convert_yaml_to_json() {
    cargo_bin_cmd!("fy")
        .arg("convert")
        .arg("json")
        .write_stdin("name: test\nvalue: 123")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"name\": \"test\""));
}

#[test]
fn test_convert_json_big_integer_key_and_value() {
    cargo_bin_cmd!("fy")
        .arg("convert")
        .arg("json")
        .write_stdin("9223372036854775808: -99999999999999999999\n!!int 99999999999999999999: x")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"9223372036854775808\": -99999999999999999999",
        ))
        .stdout(predicate::str::contains("\"99999999999999999999\": \"x\""));
}

#[test]
fn test_convert_json_big_integers_are_numbers() {
    let out = cargo_bin_cmd!("fy")
        .arg("convert")
        .arg("json")
        .write_stdin(
            "a: 9223372036854775808\nb: -9223372036854775809\nc: +99999999999999999999\n\
             d: 00000000000000000000123456789012345678901\ne: \"9223372036854775808\"\n",
        )
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(json["a"].to_string(), "9223372036854775808");
    assert_eq!(json["b"].to_string(), "-9223372036854775809");
    assert_eq!(json["c"].to_string(), "99999999999999999999");
    assert_eq!(json["d"].to_string(), "123456789012345678901");
    assert_eq!(json["e"], "9223372036854775808");
}

#[test]
fn test_convert_yaml_big_integers_keep_digits() {
    cargo_bin_cmd!("fy")
        .arg("convert")
        .arg("yaml")
        .write_stdin(
            r#"{"a":9223372036854775808,"b":18446744073709551615,"c":-9223372036854775809,"d":123456789012345678901234567890,"e":[18446744073709551616],"f":9223372036854775807,"g":1.5}"#,
        )
        .assert()
        .success()
        .stdout(predicate::str::contains("a: 9223372036854775808\n"))
        .stdout(predicate::str::contains("b: 18446744073709551615\n"))
        .stdout(predicate::str::contains("c: -9223372036854775809\n"))
        .stdout(predicate::str::contains("d: 123456789012345678901234567890\n"))
        .stdout(predicate::str::contains("- 18446744073709551616\n"))
        .stdout(predicate::str::contains("f: 9223372036854775807\n"))
        .stdout(predicate::str::contains("g: 1.5\n"));
}

#[test]
fn test_convert_big_integers_json_yaml_json_round_trip() {
    let json = r#"{"a":9223372036854775808,"b":-99999999999999999999,"c":[18446744073709551616]}"#;
    let yaml = cargo_bin_cmd!("fy")
        .arg("convert")
        .arg("yaml")
        .write_stdin(json)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let back = cargo_bin_cmd!("fy")
        .arg("convert")
        .arg("json")
        .write_stdin(yaml)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let a: serde_json::Value = serde_json::from_str(json).unwrap();
    let b: serde_json::Value = serde_json::from_slice(&back).unwrap();
    assert_eq!(a.to_string(), b.to_string());
}

#[test]
fn test_convert_json_big_integer_key_is_canonical() {
    cargo_bin_cmd!("fy")
        .arg("convert")
        .arg("json")
        .write_stdin("+99999999999999999999: v\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"99999999999999999999\": \"v\""));
}

#[test]
fn test_convert_json_equivalent_big_integer_keys_collapse_last_wins() {
    cargo_bin_cmd!("fy")
        .arg("convert")
        .arg("json")
        .write_stdin("? 99999999999999999999\n: first\n? +99999999999999999999\n: second\n")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"99999999999999999999\": \"second\"",
        ))
        .stdout(predicate::str::contains("first").not());
}

#[test]
fn test_convert_yaml_json_negative_zero_is_integer_zero() {
    cargo_bin_cmd!("fy")
        .arg("convert")
        .arg("yaml")
        .write_stdin(r#"{"z":-0}"#)
        .assert()
        .success()
        .stdout(predicate::str::contains("z: 0\n"));
}

#[test]
fn test_convert_json_to_yaml() {
    cargo_bin_cmd!("fy")
        .arg("convert")
        .arg("yaml")
        .write_stdin(r#"{"name": "test", "value": 123}"#)
        .assert()
        .success()
        .stdout(predicate::str::contains("name:"));
}

#[test]
fn test_default_format_passthrough() {
    cargo_bin_cmd!("fy")
        .write_stdin("test: value")
        .assert()
        .success()
        .stdout(predicate::str::contains("test: value"));
}

#[test]
fn test_no_color_flag() {
    cargo_bin_cmd!("fy")
        .arg("--no-color")
        .arg("--quiet")
        .arg("parse")
        .write_stdin("test: value")
        .assert()
        .success();
}

#[test]
fn test_quiet_mode() {
    cargo_bin_cmd!("fy")
        .arg("--quiet")
        .arg("parse")
        .write_stdin("test: value")
        .assert()
        .success();
}

#[test]
#[cfg(feature = "linter")]
fn test_lint_valid_yaml() {
    cargo_bin_cmd!("fy")
        .arg("--quiet")
        .arg("lint")
        .write_stdin("name: test\nvalue: 123\n")
        .assert()
        .success()
        .code(0);
}

#[test]
#[cfg(feature = "linter")]
fn test_lint_with_warnings() {
    cargo_bin_cmd!("fy")
        .arg("lint")
        .arg("--max-line-length")
        .arg("80")
        .write_stdin("name: this is a very very very very very very very very very very very very very very very very very very long line that exceeds the maximum")
        .assert()
        .success()
        .code(0);
}

#[test]
#[cfg(feature = "linter")]
fn test_lint_invalid_yaml() {
    cargo_bin_cmd!("fy")
        .arg("lint")
        .write_stdin("invalid: [unclosed")
        .assert()
        .failure()
        .code(1);
}

// Regression test for issue #302: non-ASCII text before a block scalar containing `}` panicked
#[test]
#[cfg(feature = "linter")]
fn test_lint_non_ascii_before_block_scalar_no_panic() {
    cargo_bin_cmd!("fy")
        .arg("lint")
        .write_stdin(include_str!(
            "../../fast-yaml-linter/tests/fixtures/edge_cases/non_ascii_block_scalar_braces.yaml"
        ))
        .assert()
        .success()
        .stderr(predicate::str::contains("panicked").not());
}

#[test]
#[cfg(feature = "linter")]
fn test_lint_json_format() {
    cargo_bin_cmd!("fy")
        .arg("lint")
        .arg("--format")
        .arg("json")
        .write_stdin("name: test\nvalue: 123\n")
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::starts_with("["));
}

#[test]
#[cfg(feature = "linter")]
fn test_lint_duplicate_keys_reported_by_default() {
    cargo_bin_cmd!("fy")
        .arg("lint")
        .write_stdin("key: value1\nkey: value2\n")
        .assert()
        .failure()
        .code(2)
        .stdout(predicate::str::contains("duplicate key 'key'"));
}

#[test]
#[cfg(feature = "linter")]
fn test_lint_duplicate_keys_allowed_with_flag() {
    cargo_bin_cmd!("fy")
        .arg("lint")
        .arg("--allow-duplicate-keys")
        .write_stdin("key: value1\nkey: value2\n")
        .assert()
        .success()
        .code(0);
}

#[cfg(unix)]
#[test]
fn test_format_output_dev_stdout() {
    cargo_bin_cmd!("fy")
        .arg("format")
        .arg("-o")
        .arg("/dev/stdout")
        .write_stdin("name:   test\nvalue:    123")
        .assert()
        .success()
        .stdout(predicate::str::contains("name: test"));
}

#[test]
fn test_format_output_dash() {
    cargo_bin_cmd!("fy")
        .arg("format")
        .arg("-o")
        .arg("-")
        .write_stdin("name:   test\nvalue:    123")
        .assert()
        .success()
        .stdout(predicate::str::contains("name: test"));
}

#[test]
fn test_convert_output_dash() {
    cargo_bin_cmd!("fy")
        .arg("convert")
        .arg("json")
        .arg("-o")
        .arg("-")
        .write_stdin("name: test\nvalue: 123\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("name"));
}

#[test]
#[cfg(feature = "linter")]
fn test_lint_verbose_mode() {
    cargo_bin_cmd!("fy")
        .arg("--verbose")
        .arg("lint")
        .write_stdin("name: test\n")
        .assert()
        .success()
        .stderr(predicate::str::contains("Lint time:"));
}

const BOM_YAML: &str = "\u{FEFF}# c\na: 1\n";

#[test]
fn test_bom_parse_stdin() {
    cargo_bin_cmd!("fy")
        .arg("parse")
        .write_stdin(BOM_YAML)
        .assert()
        .success()
        .stdout(predicate::str::contains("valid"));
}

#[test]
fn test_bom_format_keeps_bom() {
    cargo_bin_cmd!("fy")
        .arg("format")
        .write_stdin("\u{FEFF}a: 1\n")
        .assert()
        .success()
        .stdout("\u{FEFF}a: 1\n");
}

#[test]
fn test_bom_file_is_unchanged_by_format_check() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("bom.yaml");
    std::fs::write(&file, "\u{FEFF}a: 1\nb:\n  - x\n").unwrap();
    cargo_bin_cmd!("fy")
        .args(["format", "--dry-run"])
        .arg(&file)
        .assert()
        .success()
        .code(0);
    cargo_bin_cmd!("fy")
        .args(["format", "-i"])
        .arg(&file)
        .assert()
        .success();
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "\u{FEFF}a: 1\nb:\n  - x\n"
    );
}

#[test]
fn test_bom_lint_stdin_reports_bom_free_offsets() {
    cargo_bin_cmd!("fy")
        .arg("lint")
        .arg("--format")
        .arg("json")
        .write_stdin("\u{FEFF}a: 1   \n")
        .assert()
        .success()
        .stdout(predicate::str::contains("trailing-whitespace"))
        .stdout(predicate::str::contains("\"column\": 5"))
        .stdout(predicate::str::contains("\"offset\": 4"));
}

#[test]
fn test_bom_only_convert_json_yields_null() {
    cargo_bin_cmd!("fy")
        .arg("convert")
        .arg("json")
        .write_stdin("\u{FEFF}")
        .assert()
        .success()
        .stdout(predicate::str::contains("null"));
}

#[test]
fn test_double_bom_strips_only_one() {
    cargo_bin_cmd!("fy")
        .arg("convert")
        .arg("json")
        .write_stdin("\u{FEFF}\u{FEFF}a: 1\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"\u{FEFF}a\": 1"));
}

#[test]
fn test_bom_lint_directory() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(dir.path().join("bom.yaml"), "\u{FEFF}# c\na: 1\n").unwrap();
    cargo_bin_cmd!("fy")
        .arg("lint")
        .arg(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("mapping values are not allowed").not());
}

#[test]
fn test_bom_convert_yaml_to_json_key_has_no_bom() {
    cargo_bin_cmd!("fy")
        .arg("convert")
        .arg("json")
        .write_stdin("\u{FEFF}a: 1\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"a\": 1"))
        .stdout(predicate::str::contains("\u{FEFF}").not());
}

#[test]
fn test_bom_convert_json_to_yaml() {
    cargo_bin_cmd!("fy")
        .arg("convert")
        .arg("yaml")
        .write_stdin("\u{FEFF}{\"a\": 1}")
        .assert()
        .success()
        .stdout(predicate::str::contains("a: 1"));
}

fn fy_stdout(args: &[&str], stdin: &str) -> String {
    let out = cargo_bin_cmd!("fy")
        .args(args)
        .write_stdin(stdin.to_owned())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    String::from_utf8(out).unwrap()
}

fn format_stdin(input: &str) -> String {
    fy_stdout(&["format"], input)
}

#[test]
fn test_format_preserves_explicit_tags() {
    let out = format_stdin("a: !!str 123\nb: !custom x\n");
    assert_eq!(out, "a: !!str 123\nb: !custom x\n");
}

#[test]
fn test_format_keep_chomp_is_idempotent() {
    let input = "a: |+\n  x\n\n";
    assert_eq!(format_stdin(input), input);
}

#[test]
fn test_format_complex_keys_stay_valid() {
    let out = format_stdin("? [a, b]\n: c\n");
    assert_eq!(out, "?\n  - a\n  - b\n: c\n");
}

// Regression tests for #318 (block scalar indentation indicator) and #319 (empty null values)
#[test]
fn test_format_block_scalar_leading_space_round_trips() {
    let input = "a: |2\n   x\nb: >1\n  y\nc: |2+\n   z\n\nd:\n  - |1-\n   w\n";
    for indent in ["2", "3", "4", "8"] {
        let once = fy_stdout(&["format", "--indent", indent], input);
        let twice = fy_stdout(&["format", "--indent", indent], &once);
        assert_eq!(once, twice, "not idempotent at indent {indent}");
        assert_eq!(
            fy_stdout(&["convert", "json"], input),
            fy_stdout(&["convert", "json"], &once),
            "value changed at indent {indent}: {once:?}"
        );
    }
}

#[test]
fn test_format_block_scalar_compact_and_root_round_trips() {
    for input in ["- a: |2\n     x\n", "|2\n   root\n"] {
        for indent in ["2", "3", "4", "8"] {
            let once = fy_stdout(&["format", "--indent", indent], input);
            assert_eq!(once, fy_stdout(&["format", "--indent", indent], &once));
            assert_eq!(
                fy_stdout(&["convert", "json"], input),
                fy_stdout(&["convert", "json"], &once),
                "{input:?} at indent {indent}: {once:?}"
            );
        }
    }
}

#[test]
fn test_format_null_values_emit_null_and_lint_clean() {
    let input = "a:\nb: 1\nc:\n  - \n  - !!set {x, y}\n";
    let once = fy_stdout(&["format"], input);
    assert!(!once.lines().any(|l| l.ends_with(' ')), "got {once:?}");
    assert!(once.contains("a: null\n"), "got {once:?}");
    assert_eq!(once, fy_stdout(&["format"], &once));
    assert_eq!(
        fy_stdout(&["convert", "json"], input),
        fy_stdout(&["convert", "json"], &once)
    );
    #[cfg(feature = "linter")]
    {
        let lint = fy_stdout(&["lint"], &once);
        assert!(!lint.contains("empty-values"), "got {lint}");
        assert!(!lint.contains("trailing-whitespace"), "got {lint}");
    }
}

fn convert_json_stdin(input: &str) -> String {
    let out = cargo_bin_cmd!("fy")
        .args(["convert", "json"])
        .write_stdin(input.to_owned())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    String::from_utf8(out).unwrap()
}

fn assert_format_roundtrips(input: &str) -> String {
    let once = format_stdin(input);
    assert_eq!(format_stdin(&once), once, "not idempotent for {input:?}");
    assert_eq!(
        convert_json_stdin(&once),
        convert_json_stdin(input),
        "value changed: {input:?} -> {once:?}"
    );
    once
}

#[test]
fn test_format_alias_key_keeps_space_before_colon() {
    assert_eq!(
        assert_format_roundtrips("&k a: 1\n? *k\n: 2\n"),
        "&k a: 1\n*k : 2\n"
    );
    assert_eq!(
        assert_format_roundtrips("- &k a\n- *k : 1\n"),
        "- &k a\n- *k : 1\n"
    );
    assert_format_roundtrips("x:\n  y:\n    &k a: 1\n    *k : 2\n");
    assert_format_roundtrips("x: {&k a: 1, *k : 2}\n");
}

#[test]
fn test_format_multiline_quoted_scalars_stay_valid() {
    assert_eq!(
        assert_format_roundtrips("? 'a\n\n  b'\n: 1\n"),
        "\"a\\nb\": 1\n"
    );
    assert_eq!(assert_format_roundtrips("- 'a\n\n  b'\n"), "- \"a\\nb\"\n");
    assert_format_roundtrips("k: 'a\n\n  b'\n");
    assert_format_roundtrips("k: a\n\n  b\n");
    assert_format_roundtrips("? &k 'a\n\n  b'\n: 1\n");
    assert_format_roundtrips("? !!str 'a\n\n  b'\n: 1\n");
    assert_format_roundtrips("'a\n\n  b'\n");
}

#[test]
fn test_format_escapes_del_c1_and_non_characters() {
    assert_eq!(
        assert_format_roundtrips("k: \"a\\x7Fb\\x85c\\uFFFEd\"\n"),
        "k: \"a\\x7Fb\\x85c\\uFFFEd\"\n"
    );
}

#[test]
fn test_format_block_scalar_under_alias_key_roundtrips() {
    assert_format_roundtrips("&k a: 1\nb:\n  c:\n    *k : |\n      text\n");
    assert_format_roundtrips("- &k a\n- *k : |\n    text\n");
}

#[test]
fn test_format_indent_4_alias_key_in_nested_mapping() {
    let input = "&k a: 1\nx:\n  y:\n    *k : \"p\\nq\"\n    z: 2\n";
    let once = cargo_bin_cmd!("fy")
        .args(["format", "--indent", "4"])
        .write_stdin(input.to_owned())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let once = String::from_utf8(once).unwrap();
    assert_eq!(convert_json_stdin(&once), convert_json_stdin(input));
}

fn nested_maps(depth: usize) -> String {
    let mut yaml = String::new();
    for level in 0..depth {
        yaml.push_str(&"  ".repeat(level));
        yaml.push_str("a:\n");
    }
    yaml.push_str(&"  ".repeat(depth));
    yaml.push_str("v\n");
    yaml
}

#[test]
fn test_format_depth_limit_roundtrips_at_256() {
    assert_format_roundtrips(&nested_maps(256));
}

#[test]
fn test_format_in_place_depth_limit_leaves_file_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("deep.yaml");
    let input = nested_maps(257);
    std::fs::write(&file, &input).unwrap();

    cargo_bin_cmd!("fy")
        .args(["format", "-i", file.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("nesting depth"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), input);
}

#[test]
fn test_format_stdin_depth_limit_is_an_error() {
    cargo_bin_cmd!("fy")
        .arg("format")
        .write_stdin(nested_maps(257))
        .assert()
        .failure()
        .stderr(predicate::str::contains("nesting depth"));
}

#[test]
fn test_format_anchor_limit_is_per_document() {
    use std::fmt::Write as _;
    let mut input = String::new();
    for _ in 0..3 {
        input.push_str("---\n");
        for i in 1..=3000 {
            writeln!(input, "- &a{i} v").unwrap();
        }
        input.push_str("- *a3000\n");
    }
    fy_stdout(&["format"], &input);
}

#[test]
fn test_format_anchor_limit_is_an_error() {
    use std::fmt::Write as _;
    let mut input = String::new();
    for i in 1..=4097 {
        writeln!(input, "- &a{i} v").unwrap();
    }
    input.push_str("- *a4097\n");
    cargo_bin_cmd!("fy")
        .arg("format")
        .write_stdin(input)
        .assert()
        .failure()
        .stderr(predicate::str::contains("anchor definitions"));
}

#[test]
fn test_format_long_keys_use_explicit_form() {
    let long = "k".repeat(1025);
    for input in [
        format!("? {long}\n: v\n"),
        format!("? \"{long}\"\n: v\n"),
        format!("{{{long}: v}}\n"),
    ] {
        let out = assert_format_roundtrips(&input);
        assert!(out.starts_with("? "), "{out:.20?}");
    }
    let edge = format!("{}: v\n", "k".repeat(1024));
    assert_eq!(assert_format_roundtrips(&edge), edge);
}

#[test]
fn test_format_multiline_plain_and_single_quoted_roundtrip() {
    for input in ["a: x\n  y\n\n  z\n", "a: 'x\n\n  y'\n", "- x\n  y\n\n  z\n"] {
        assert_format_roundtrips(input);
    }
}

#[test]
#[cfg(feature = "linter")]
fn test_lint_missing_path_with_clean_file_fails() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nonexist.yaml");
    let clean = dir.path().join("clean.yaml");
    std::fs::write(&clean, "key: value\n").unwrap();

    cargo_bin_cmd!("fy")
        .args(["lint", "--no-config"])
        .arg(&missing)
        .arg(&clean)
        .assert()
        .code(1)
        .stderr(predicate::str::contains("path does not exist"));
}

#[test]
#[cfg(feature = "linter")]
fn test_lint_zero_match_glob_fails() {
    let dir = tempfile::tempdir().unwrap();
    let pattern = dir.path().join("*.nomatch");

    cargo_bin_cmd!("fy")
        .args(["lint", "--no-config"])
        .arg(&pattern)
        .assert()
        .failure()
        .stderr(predicate::str::contains("glob pattern matched no files"));
}

#[test]
#[cfg(feature = "linter")]
fn test_lint_batch_flags_without_input_fail() {
    cargo_bin_cmd!("fy")
        .args(["lint", "--no-config"])
        .arg("-j2")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "batch options (--jobs, --include, --exclude) need input",
        ));
}

#[test]
#[cfg(feature = "linter")]
fn test_lint_directory_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("clean.yaml"), "key: value\n").unwrap();

    cargo_bin_cmd!("fy")
        .args(["lint", "--no-config"])
        .arg(dir.path())
        .assert()
        .success();
}

#[test]
#[cfg(feature = "linter")]
fn test_lint_rejects_unknown_rule_in_config() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("lint.yaml");
    let file = dir.path().join("clean.yaml");
    std::fs::write(&config, "rules:\n  no-such-rule:\n    enabled: true\n").unwrap();
    std::fs::write(&file, "key: value\n").unwrap();

    cargo_bin_cmd!("fy")
        .args(["lint", "--config"])
        .arg(&config)
        .arg(&file)
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown rule 'no-such-rule'"));
}

#[test]
#[cfg(feature = "linter")]
fn test_lint_directive_suppresses_duplicate_key() {
    cargo_bin_cmd!("fy")
        .args(["lint", "--no-config"])
        .write_stdin("key: value1\nkey: value2  # fy: disable-line duplicate-key\n")
        .assert()
        .success();
}

#[test]
#[cfg(feature = "linter")]
fn test_lint_directive_in_file_and_batch_dir() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.yaml");
    std::fs::write(&file, "# fy: disable-file\nk: 1\nk: 2\n").unwrap();
    std::fs::write(
        dir.path().join("b.yaml"),
        "# yamllint disable rule:key-duplicates\nk: 1\nk: 2\n",
    )
    .unwrap();

    cargo_bin_cmd!("fy")
        .args(["lint", "--no-config"])
        .arg(&file)
        .assert()
        .success();
    cargo_bin_cmd!("fy")
        .args(["lint", "--no-config"])
        .arg(dir.path())
        .assert()
        .success();
}

#[test]
#[cfg(feature = "linter")]
fn test_lint_directive_severity_from_config_file() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("fy.yaml");
    std::fs::write(&config, "rules:\n  lint-directive:\n    severity: error\n").unwrap();

    cargo_bin_cmd!("fy")
        .arg("lint")
        .arg("--config")
        .arg(&config)
        .write_stdin("# fy: disable no-such-rule\nk: 1\n")
        .assert()
        .failure()
        .stdout(predicate::str::contains("error[lint-directive]"));
}

#[test]
#[cfg(feature = "linter")]
fn test_lint_directive_unknown_rule_reported_in_json() {
    cargo_bin_cmd!("fy")
        .args(["lint", "--no-config", "--format", "json"])
        .write_stdin("# fy: disable no-such-rule\nk: 1\n")
        .assert()
        .stdout(predicate::str::contains("\"lint-directive\""))
        .stdout(predicate::str::contains("no-such-rule"));
}
