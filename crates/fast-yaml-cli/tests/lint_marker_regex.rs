//! End-to-end tests for `document-end: {present: false}` (#419) and regex `quoted-strings`
//! patterns (#421) in `fy lint --config`.

#![allow(deprecated)] // Command::cargo_bin is deprecated but still works

use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn config_in(dir: &TempDir, content: &str) -> PathBuf {
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, content).unwrap();
    path
}

fn lint(dir: &TempDir, config: &str) -> Command {
    let mut cmd = Command::cargo_bin("fy").unwrap();
    cmd.args(["lint", "--config"]).arg(config_in(dir, config));
    cmd
}

#[test]
fn document_end_forbidden_flags_marker_with_position() {
    let dir = TempDir::new().unwrap();
    lint(&dir, "rules:\n  document-end:\n    present: false\n")
        .write_stdin("a: 1\n...\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("document-end"))
        .stdout(predicate::str::contains(
            "document end marker '...' is forbidden",
        ))
        .stdout(predicate::str::contains("2:1"));
}

#[test]
fn document_end_forbidden_accepts_unmarked_document() {
    let dir = TempDir::new().unwrap();
    lint(&dir, "rules:\n  document-end:\n    present: false\n")
        .write_stdin("a: 1\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("document-end").not());
}

#[test]
fn document_end_required_is_unchanged() {
    let dir = TempDir::new().unwrap();
    lint(&dir, "rules:\n  document-end:\n    present: true\n")
        .write_stdin("a: 1\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("missing document end marker"));
}

#[test]
fn extra_required_regex_flags_matching_plain_scalar() {
    let dir = TempDir::new().unwrap();
    lint(
        &dir,
        "rules:\n  quoted-strings:\n    extra-required: ['^http://']\n",
    )
    .write_stdin("a: http://x\nb: plain\n")
    .assert()
    .success()
    .stdout(predicate::str::contains("string should be quoted"))
    .stdout(predicate::str::contains("1:4"))
    .stdout(predicate::str::contains("2:4").not());
}

#[test]
fn extra_allowed_regex_keeps_plain_scalar_under_only_when_needed() {
    let dir = TempDir::new().unwrap();
    lint(
        &dir,
        "rules:\n  quoted-strings:\n    extra-allowed: ['^ftp://']\n",
    )
    .write_stdin("a: ftp://x\nb: \"ftp://x\"\nc: \"plain\"\n")
    .assert()
    .success()
    .stdout(predicate::str::contains("string does not need quotes"));
}

#[test]
fn invalid_regex_fails_naming_rule_option_and_index() {
    let dir = TempDir::new().unwrap();
    lint(
        &dir,
        "rules:\n  quoted-strings:\n    extra-required: ['ok', '(?=x)']\n",
    )
    .write_stdin("a: 1\n")
    .assert()
    .code(1)
    .stderr(predicate::str::contains("quoted-strings"))
    .stderr(predicate::str::contains("extra-required"))
    .stderr(predicate::str::contains("pattern 1"))
    .stderr(predicate::str::contains("look-around"));
}

#[test]
fn more_than_64_patterns_are_rejected() {
    let dir = TempDir::new().unwrap();
    let many = vec!["'a'"; 65].join(", ");
    lint(
        &dir,
        &format!("rules:\n  quoted-strings:\n    extra-required: [{many}]\n"),
    )
    .write_stdin("a: 1\n")
    .assert()
    .code(1)
    .stderr(predicate::str::contains("extra-required"))
    .stderr(predicate::str::contains("limit is 64"));
}

#[test]
fn pattern_over_256_bytes_is_rejected() {
    let dir = TempDir::new().unwrap();
    let long = "a".repeat(257);
    lint(
        &dir,
        &format!("rules:\n  quoted-strings:\n    extra-required: ['{long}']\n"),
    )
    .write_stdin("a: 1\n")
    .assert()
    .code(1)
    .stderr(predicate::str::contains("pattern 0"))
    .stderr(predicate::str::contains("the limit is 256"));
}

#[test]
fn oversized_compiled_pattern_reports_size_not_syntax() {
    let dir = TempDir::new().unwrap();
    lint(
        &dir,
        "rules:\n  quoted-strings:\n    extra-required: ['(a{1000}){1000}']\n",
    )
    .write_stdin("a: 1\n")
    .assert()
    .code(1)
    .stderr(predicate::str::contains("compiles to more than"))
    .stderr(predicate::str::contains("look-around").not());
}

#[test]
fn unicode_class_pattern_is_accepted() {
    let dir = TempDir::new().unwrap();
    lint(
        &dir,
        "rules:\n  quoted-strings:\n    extra-required: ['\\w{1,40}']\n",
    )
    .write_stdin("a: word\n")
    .assert()
    .success()
    .stdout(predicate::str::contains("string should be quoted"));
}

#[test]
fn always_with_extra_allowed_is_rejected() {
    let dir = TempDir::new().unwrap();
    lint(
        &dir,
        "rules:\n  quoted-strings:\n    required: always\n    extra-allowed: ['a']\n",
    )
    .write_stdin("a: 1\n")
    .assert()
    .code(1)
    .stderr(predicate::str::contains("extra-allowed"))
    .stderr(predicate::str::contains("only-when-needed"));
}
