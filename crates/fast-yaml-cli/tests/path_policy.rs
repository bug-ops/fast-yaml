//! Regression tests for #497/#498: literal bracket paths and explicit non-YAML input are errors.

#![allow(clippy::missing_docs_in_private_items)]
#![allow(deprecated)] // Command::cargo_bin is deprecated but still works

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn fy() -> Command {
    Command::cargo_bin("fy").unwrap()
}

fn messy_dir() -> TempDir {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("m1.yaml"), "key:   value\n").unwrap();
    temp
}

#[test]
fn format_missing_bracket_path_does_not_rewrite_glob_match() {
    let temp = messy_dir();
    let missing = temp.path().join("m[1].yaml");

    fy().args(["format", "-i", missing.to_str().unwrap()])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("path does not exist"));

    assert_eq!(
        fs::read_to_string(temp.path().join("m1.yaml")).unwrap(),
        "key:   value\n"
    );
}

#[test]
fn format_existing_bracket_path_is_literal_even_with_glob_sibling() {
    let temp = messy_dir();
    let literal = temp.path().join("m[1].yaml");
    fs::write(&literal, "key:   value\n").unwrap();

    fy().args(["format", "-i", literal.to_str().unwrap()])
        .assert()
        .success();

    assert_eq!(fs::read_to_string(&literal).unwrap(), "key: value\n");
    assert_eq!(
        fs::read_to_string(temp.path().join("m1.yaml")).unwrap(),
        "key:   value\n"
    );
}

#[test]
fn format_stdin_files_overlong_line_fails() {
    let temp = messy_dir();
    let good = temp.path().join("m1.yaml");

    fy().args(["format", "--dry-run", "--stdin-files"])
        .write_stdin(format!("{}\n{}\n", "x".repeat(4097), good.display()))
        .assert()
        .code(1)
        .stderr(predicate::str::contains("exceeds 4096 bytes"));
}

#[test]
fn format_stdin_files_invalid_line_leaves_valid_file_unmodified() {
    let temp = messy_dir();
    let good = temp.path().join("m1.yaml");
    let text = temp.path().join("notes.txt");
    fs::write(&text, "hello\n").unwrap();

    fy().args(["format", "-i", "--stdin-files"])
        .write_stdin(format!("{}\n{}\n", good.display(), text.display()))
        .assert()
        .code(1)
        .stderr(predicate::str::contains("--stdin-files line 2"))
        .stderr(predicate::str::contains(
            "not matched by the include patterns",
        ));

    assert_eq!(fs::read_to_string(&good).unwrap(), "key:   value\n");
}

#[test]
fn format_stdin_files_accepts_blank_comment_and_crlf_lines() {
    let temp = messy_dir();
    let good = temp.path().join("m1.yaml");

    fy().args(["format", "-i", "--stdin-files"])
        .write_stdin(format!(
            "# {}\r\n\r\n{}\r\n",
            "c".repeat(5000),
            good.display()
        ))
        .assert()
        .success();

    assert_eq!(fs::read_to_string(&good).unwrap(), "key: value\n");
}

#[test]
fn format_include_filter_rejection_names_the_include_patterns() {
    let temp = messy_dir();

    fy().args([
        "format",
        "--dry-run",
        "--include",
        "*.txt",
        temp.path().join("m1.yaml").to_str().unwrap(),
    ])
    .assert()
    .code(1)
    .stderr(predicate::str::contains(
        "not matched by the include patterns",
    ));
}

#[test]
fn format_explicit_non_yaml_file_in_batch_fails() {
    let temp = messy_dir();
    let text = temp.path().join("notes.txt");
    fs::write(&text, "hello\n").unwrap();

    fy().args([
        "format",
        "--dry-run",
        temp.path().join("m1.yaml").to_str().unwrap(),
        text.to_str().unwrap(),
    ])
    .assert()
    .code(1)
    .stderr(predicate::str::contains(
        "not matched by the include patterns",
    ));
}

#[test]
fn format_directory_argument_skips_non_yaml_files() {
    let temp = messy_dir();
    fs::write(temp.path().join("notes.txt"), "hello\n").unwrap();

    fy().args(["format", "--dry-run", temp.path().to_str().unwrap()])
        .assert()
        .code(5);
}

#[test]
fn format_single_non_yaml_file_still_formats() {
    let temp = TempDir::new().unwrap();
    let text = temp.path().join("config.txt");
    fs::write(&text, "key:   value\n").unwrap();

    fy().args(["format", text.to_str().unwrap()])
        .assert()
        .success()
        .stdout("key: value\n");
}

#[cfg(feature = "linter")]
mod lint {
    use super::*;

    #[test]
    fn missing_bracket_path_fails_instead_of_linting_glob_match() {
        let temp = messy_dir();
        let missing = temp.path().join("m[1].yaml");

        fy().args(["lint", missing.to_str().unwrap(), "-j", "1"])
            .assert()
            .code(1)
            .stderr(predicate::str::contains("path does not exist"));
    }

    #[test]
    fn existing_bracket_path_is_linted_as_file() {
        let temp = messy_dir();
        let literal = temp.path().join("m[1].yaml");
        fs::write(&literal, "key: value\n").unwrap();

        fy().args(["--quiet", "lint", literal.to_str().unwrap()])
            .assert()
            .success();
    }

    #[test]
    fn explicit_non_yaml_file_in_batch_fails() {
        let temp = messy_dir();
        let text = temp.path().join("notes.txt");
        fs::write(&text, "hello\n").unwrap();

        fy().args([
            "lint",
            temp.path().join("m1.yaml").to_str().unwrap(),
            text.to_str().unwrap(),
        ])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "not matched by the include patterns",
        ));
    }

    #[test]
    fn directory_argument_skips_non_yaml_files() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("a.yaml"), "key: value\n").unwrap();
        fs::write(temp.path().join("notes.txt"), "hello\n").unwrap();

        fy().args(["--quiet", "lint", temp.path().to_str().unwrap()])
            .assert()
            .success();
    }
}
