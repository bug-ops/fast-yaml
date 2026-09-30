//! Integration tests for the `fy` CLI tool.

#![allow(clippy::missing_docs_in_private_items)]
#![allow(deprecated)] // Command::cargo_bin is deprecated but still works

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn test_version() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("fy"));
}

#[test]
fn test_help() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Fast YAML"));
}

#[test]
fn test_parse_stdin() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("--quiet")
        .arg("parse")
        .write_stdin("name: test\nvalue: 123")
        .assert()
        .success();
}

#[test]
fn test_parse_invalid_yaml() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("parse")
        .write_stdin("invalid: [")
        .assert()
        .failure()
        .code(1);
}

#[test]
fn test_format_stdin() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("format")
        .write_stdin("name:   test\nvalue:    123")
        .assert()
        .success()
        .stdout(predicate::str::contains("name: test"));
}

#[test]
fn test_convert_yaml_to_json() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("convert")
        .arg("json")
        .write_stdin("name: test\nvalue: 123")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"name\": \"test\""));
}

#[test]
fn test_convert_json_to_yaml() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("convert")
        .arg("yaml")
        .write_stdin(r#"{"name": "test", "value": 123}"#)
        .assert()
        .success()
        .stdout(predicate::str::contains("name:"));
}

#[test]
fn test_default_format_passthrough() {
    Command::cargo_bin("fy")
        .unwrap()
        .write_stdin("test: value")
        .assert()
        .success()
        .stdout(predicate::str::contains("test: value"));
}

#[test]
fn test_no_color_flag() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("--no-color")
        .arg("--quiet")
        .arg("parse")
        .write_stdin("test: value")
        .assert()
        .success();
}

#[test]
fn test_quiet_mode() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("--quiet")
        .arg("parse")
        .write_stdin("test: value")
        .assert()
        .success();
}

#[test]
#[cfg(feature = "linter")]
fn test_lint_valid_yaml() {
    Command::cargo_bin("fy")
        .unwrap()
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
    Command::cargo_bin("fy")
        .unwrap()
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
    Command::cargo_bin("fy")
        .unwrap()
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
    Command::cargo_bin("fy")
        .unwrap()
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
    Command::cargo_bin("fy")
        .unwrap()
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
    Command::cargo_bin("fy")
        .unwrap()
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
    Command::cargo_bin("fy")
        .unwrap()
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
    Command::cargo_bin("fy")
        .unwrap()
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
    Command::cargo_bin("fy")
        .unwrap()
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
    Command::cargo_bin("fy")
        .unwrap()
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
    Command::cargo_bin("fy")
        .unwrap()
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
    Command::cargo_bin("fy")
        .unwrap()
        .arg("parse")
        .write_stdin(BOM_YAML)
        .assert()
        .success()
        .stdout(predicate::str::contains("valid"));
}

#[test]
fn test_bom_format_drops_bom() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("format")
        .write_stdin("\u{FEFF}a: 1\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("a: 1"))
        .stdout(predicate::str::contains("\u{FEFF}").not());
}

#[test]
fn test_bom_lint_stdin_reports_bom_relative_offsets() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("lint")
        .arg("--format")
        .arg("json")
        .write_stdin("\u{FEFF}a: 1   \n")
        .assert()
        .success()
        .stdout(predicate::str::contains("trailing-whitespace"))
        .stdout(predicate::str::contains("\"column\": 5"))
        .stdout(predicate::str::contains("\"offset\": 7"));
}

#[test]
fn test_bom_only_convert_json_yields_null() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("convert")
        .arg("json")
        .write_stdin("\u{FEFF}")
        .assert()
        .success()
        .stdout(predicate::str::contains("null"));
}

#[test]
fn test_double_bom_strips_only_one() {
    Command::cargo_bin("fy")
        .unwrap()
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
    Command::cargo_bin("fy")
        .unwrap()
        .arg("lint")
        .arg(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("mapping values are not allowed").not());
}

#[test]
fn test_bom_convert_yaml_to_json_key_has_no_bom() {
    Command::cargo_bin("fy")
        .unwrap()
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
    Command::cargo_bin("fy")
        .unwrap()
        .arg("convert")
        .arg("yaml")
        .write_stdin("\u{FEFF}{\"a\": 1}")
        .assert()
        .success()
        .stdout(predicate::str::contains("a: 1"));
}

fn fy_stdout(args: &[&str], stdin: &str) -> String {
    let out = Command::cargo_bin("fy")
        .unwrap()
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
