//! `-o /dev/null` discards the output instead of failing the atomic write, and the
//! `--max-line-length` range error names its bounds (#634).

#![allow(clippy::missing_docs_in_private_items)]
#![cfg(unix)]

use assert_cmd::{Command, cargo_bin_cmd};
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn fy(args: &[&str]) -> Command {
    let mut cmd = cargo_bin_cmd!("fy");
    cmd.args(args);
    cmd
}

fn input(content: &str) -> (TempDir, String) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("in.yaml");
    fs::write(&path, content).unwrap();
    let path = path.to_str().unwrap().to_owned();
    (dir, path)
}

#[test]
fn format_and_convert_write_nothing_to_the_null_device() {
    let (_dir, path) = input("a:    1\n");
    fy(&["format", "-o", "/dev/null", &path])
        .assert()
        .success()
        .stdout("")
        .stderr("");
    fy(&["convert", "json", "-o", "/dev/null", &path])
        .assert()
        .success()
        .stdout("");
    assert_eq!(fs::read_to_string(&path).unwrap(), "a:    1\n");
}

#[cfg(feature = "linter")]
#[test]
fn lint_keeps_its_exit_code_when_the_report_is_discarded() {
    let (_dir, clean) = input("---\na: 1\n");
    fy(&["lint", "-o", "/dev/null", &clean])
        .assert()
        .success()
        .stdout("");
    let (_dir, broken) = input("a: 1\na: 2\n");
    fy(&["lint", "-o", "/dev/null", &broken])
        .assert()
        .code(2)
        .stdout("");
}

#[test]
fn the_null_device_stays_a_device() {
    let (_dir, path) = input("a: 1\n");
    fy(&["format", "-o", "/dev/null", &path]).assert().success();
    let kind = fs::symlink_metadata("/dev/null").unwrap().file_type();
    assert!(!kind.is_file(), "/dev/null was replaced by a regular file");
}

#[cfg(feature = "linter")]
#[test]
fn max_line_length_names_its_range() {
    for value in ["0", "4294967296"] {
        fy(&["lint", "--max-line-length", value])
            .write_stdin("a: 1\n")
            .assert()
            .code(2)
            .stderr(predicate::str::contains(format!(
                "must be between 1 and 4294967295, got {value}"
            )));
    }
    fy(&["lint", "--max-line-length", "4294967295"])
        .write_stdin("---\na: 1\n")
        .assert()
        .success();
}
