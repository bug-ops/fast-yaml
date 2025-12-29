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
fn test_lint_duplicate_keys_disabled_by_default() {
    // Duplicate key detection is disabled by default to avoid false positives
    // from nested keys with the same name in different contexts
    Command::cargo_bin("fy")
        .unwrap()
        .arg("lint")
        .write_stdin("key: value1\nkey: value2\n")
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains("0 errors, 0 warnings"));
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
