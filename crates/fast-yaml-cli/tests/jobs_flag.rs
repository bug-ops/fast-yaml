//! `-j/--jobs` is a bounded worker setting: 0 is auto, 1..=128 a fixed pool, anything else a
//! usage error (#610).

#![allow(clippy::missing_docs_in_private_items)]

use assert_cmd::{Command, cargo_bin_cmd};
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn fy(args: &[&str]) -> Command {
    let mut cmd = cargo_bin_cmd!("fy");
    cmd.args(args);
    cmd
}

fn dir_with_files() -> TempDir {
    let dir = TempDir::new().unwrap();
    for i in 0..6 {
        fs::write(dir.path().join(format!("f{i}.yaml")), "a:   1\n").unwrap();
    }
    dir
}

#[test]
fn jobs_above_the_cap_is_a_usage_error() {
    let dir = dir_with_files();
    let path = dir.path().to_str().unwrap();
    for sub in ["format", "lint"] {
        fy(&[sub, "-j", "129", path])
            .assert()
            .code(2)
            .stderr(predicate::str::contains("between 0 and 128, got 129"));
    }
    fy(&["format", "-j", "-1", path]).assert().code(2);
    fy(&["format", "-j", "x", path]).assert().code(2);
}

#[test]
fn jobs_at_the_cap_runs() {
    let dir = dir_with_files();
    let path = dir.path().to_str().unwrap();
    fy(&["format", "-n", "-j", "128", path]).assert().code(5);
    fy(&["lint", "--no-config", "-j", "128", path])
        .assert()
        .success();
}

#[test]
fn jobs_zero_is_auto_and_does_not_force_batch() {
    let dir = dir_with_files();
    let file = dir.path().join("f0.yaml");

    fy(&["format", "-j", "0", file.to_str().unwrap()])
        .assert()
        .success()
        .stdout("a: 1\n");
    fy(&["format", "-j", "0"])
        .write_stdin("a:   1\n")
        .assert()
        .success()
        .stdout("a: 1\n");
}

#[test]
fn explicit_jobs_still_selects_batch_mode() {
    let dir = dir_with_files();
    let file = dir.path().join("f0.yaml");
    fy(&["format", "-j", "2", file.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--dry-run"));
}

#[test]
fn jobs_help_documents_the_range() {
    fy(&["format", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("0 = auto, 1-128"));
}
