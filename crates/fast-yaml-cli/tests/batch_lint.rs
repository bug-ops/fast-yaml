//! Batch lint mode discovery integration tests (#513, #514).

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

const NO_FILES: &str = "no YAML files found";
const BROKEN: &str = "a: [1\n";
const CLEAN: &str = "---\nb: 2\n";

#[allow(deprecated)]
fn fy() -> Command {
    Command::cargo_bin("fy").unwrap()
}

#[test]
fn test_lint_explicit_uppercase_extension_is_linted() {
    let temp = TempDir::new().unwrap();
    let upper = temp.path().join("UP.YAML");
    let lower = temp.path().join("m1.yaml");
    fs::write(&upper, BROKEN).unwrap();
    fs::write(&lower, CLEAN).unwrap();

    fy().args(["lint", upper.to_str().unwrap(), lower.to_str().unwrap()])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("UP.YAML"));
}

#[test]
fn test_lint_empty_directory_fails() {
    let temp = TempDir::new().unwrap();

    fy().args(["lint", temp.path().to_str().unwrap()])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(NO_FILES));
}

#[test]
fn test_lint_non_yaml_glob_fails() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("notes.txt"), "a: 1\n").unwrap();
    let pattern = temp.path().join("notes*");

    fy().args(["lint", pattern.to_str().unwrap()])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(NO_FILES));
}

#[test]
fn test_lint_exclude_all_fails() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("a.yaml"), CLEAN).unwrap();

    fy().args(["lint", "--exclude", "*.yaml", temp.path().to_str().unwrap()])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(NO_FILES));
}

#[test]
fn test_lint_no_files_fails_even_when_quiet() {
    let temp = TempDir::new().unwrap();

    fy().args(["-q", "lint", temp.path().to_str().unwrap()])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(NO_FILES));
}

#[test]
fn test_lint_uppercase_extension_found_in_directory() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("UP.YAML"), BROKEN).unwrap();

    fy().args(["lint", temp.path().to_str().unwrap()])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("UP.YAML"));
}

#[test]
fn test_lint_user_include_is_case_insensitive() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("a.YML"), BROKEN).unwrap();

    fy().args(["lint", "--include", "*.yml", temp.path().to_str().unwrap()])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("a.YML"));
}

#[test]
fn test_lint_exclude_is_case_insensitive() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("KEY.YAML"), BROKEN).unwrap();
    fs::write(temp.path().join("ok.yaml"), CLEAN).unwrap();

    fy().args([
        "lint",
        "--exclude",
        "**/key.yaml",
        temp.path().to_str().unwrap(),
    ])
    .assert()
    .success();
}

#[test]
fn test_lint_mixed_empty_glob_and_file_succeeds() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("notes.txt"), "a: 1\n").unwrap();
    let ok = temp.path().join("ok.yaml");
    fs::write(&ok, CLEAN).unwrap();
    let pattern = temp.path().join("notes*");

    fy().args(["lint", pattern.to_str().unwrap(), ok.to_str().unwrap()])
        .assert()
        .success();
}

#[test]
fn test_lint_unmatched_glob_with_file_still_fails() {
    let temp = TempDir::new().unwrap();
    let ok = temp.path().join("ok.yaml");
    fs::write(&ok, CLEAN).unwrap();
    let pattern = temp.path().join("nomatch*");

    fy().args(["lint", pattern.to_str().unwrap(), ok.to_str().unwrap()])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("glob pattern matched no files"));
}

#[test]
fn test_lint_exclude_drops_explicit_file() {
    let temp = TempDir::new().unwrap();
    let skipped = temp.path().join("skip.yaml");
    let ok = temp.path().join("ok.yaml");
    fs::write(&skipped, BROKEN).unwrap();
    fs::write(&ok, CLEAN).unwrap();

    fy().args([
        "lint",
        "--exclude",
        "**/skip.yaml",
        skipped.to_str().unwrap(),
        ok.to_str().unwrap(),
    ])
    .assert()
    .success();
}

#[test]
fn test_lint_stdin_files_lints_each_listed_file() {
    let temp = TempDir::new().unwrap();
    let broken = temp.path().join("broken.yaml");
    let clean = temp.path().join("clean.yaml");
    fs::write(&broken, BROKEN).unwrap();
    fs::write(&clean, CLEAN).unwrap();

    fy().args(["lint", "--stdin-files"])
        .write_stdin(format!("{}\n{}\n", broken.display(), clean.display()))
        .assert()
        .code(2)
        .stderr(predicate::str::contains("broken.yaml"));
}

#[test]
fn test_lint_stdin_files_accepts_blank_and_comment_lines() {
    let temp = TempDir::new().unwrap();
    let clean = temp.path().join("clean.yaml");
    fs::write(&clean, CLEAN).unwrap();

    fy().args(["lint", "--stdin-files"])
        .write_stdin(format!("\n# list\n  {}  \r\n", clean.display()))
        .assert()
        .success();
}

#[test]
fn test_lint_stdin_files_empty_list_succeeds() {
    fy().args(["lint", "--stdin-files"])
        .write_stdin("")
        .assert()
        .success();
}

#[test]
fn test_lint_stdin_files_rejects_non_yaml_line() {
    let temp = TempDir::new().unwrap();
    let clean = temp.path().join("clean.yaml");
    let text = temp.path().join("notes.txt");
    fs::write(&clean, CLEAN).unwrap();
    fs::write(&text, "hello\n").unwrap();

    fy().args(["lint", "--stdin-files"])
        .write_stdin(format!("{}\n{}\n", clean.display(), text.display()))
        .assert()
        .code(1)
        .stderr(predicate::str::contains("--stdin-files line 2"));
}

#[test]
fn test_lint_stdin_files_conflicts_with_paths() {
    fy().args(["lint", "--stdin-files", "a.yaml"])
        .assert()
        .failure();
}
