//! `RUST_LOG` debug events go to stderr only, and stay off without it (#614).

#![allow(clippy::missing_docs_in_private_items)]

use assert_cmd::{Command, cargo_bin_cmd};
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn fy(dir: &TempDir, args: &[&str]) -> Command {
    let mut cmd = cargo_bin_cmd!("fy");
    cmd.current_dir(dir.path())
        .env_remove("RUST_LOG")
        .args(args);
    cmd
}

fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    for i in 0..5 {
        fs::write(dir.path().join(format!("f{i}.yaml")), "a: 1\n").unwrap();
    }
    fs::write(dir.path().join("notes.txt"), "not yaml\n").unwrap();
    fs::write(dir.path().join(".fast-yaml.yaml"), "rules: {}\n").unwrap();
    dir
}

#[test]
fn no_rust_log_prints_no_events() {
    let dir = project();
    fy(&dir, &["lint", "--format", "json", "-j", "2", "."])
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
}

#[cfg(feature = "linter")]
#[test]
fn rust_log_adds_stderr_events_and_keeps_stdout_valid_json() {
    let dir = project();
    let out = fy(&dir, &["lint", "--format", "json", "-j", "2", "."])
        .env("RUST_LOG", "debug")
        .output()
        .unwrap();
    assert!(out.status.success());

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("DEBUG"), "{stderr}");
    assert!(
        stderr.contains("discovered files for a lint batch"),
        "{stderr}"
    );
    assert!(
        stderr.contains("does not match the include patterns"),
        "{stderr}"
    );
    assert!(
        stderr.contains("building a shared pool of 2 threads"),
        "{stderr}"
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(json.is_array());
}

#[test]
fn rust_log_reports_format_batch_decisions() {
    let dir = project();
    fy(&dir, &["format", "-n", "-j", "2", "."])
        .env("RUST_LOG", "fast_yaml_parallel=debug")
        .assert()
        .success()
        .stderr(predicate::str::contains("running sequentially"));
}

#[test]
fn an_invalid_rust_log_is_reported_and_ignored() {
    let dir = project();
    fy(&dir, &["parse", "f0.yaml"])
        .env("RUST_LOG", "bogus[")
        .assert()
        .success()
        .stderr(predicate::str::contains("ignoring invalid RUST_LOG"));
}

#[cfg(feature = "linter")]
#[test]
fn the_config_file_notice_needs_verbose() {
    let dir = project();
    fy(&dir, &["lint", "f0.yaml"])
        .assert()
        .success()
        .stderr(predicate::str::contains("using config file").not());
    fy(&dir, &["lint", "-v", "f0.yaml"])
        .assert()
        .success()
        .stderr(predicate::str::contains("using config file:"));
    fy(&dir, &["lint", "f0.yaml"])
        .env("RUST_LOG", "debug")
        .assert()
        .success()
        .stderr(predicate::str::contains("discovered config file"));
}

#[cfg(unix)]
#[test]
fn event_paths_never_carry_raw_escape_characters() {
    let dir = project();
    fs::write(dir.path().join("x\u{1b}[31mred.txt"), "not yaml\n").unwrap();
    let out = fy(&dir, &["format", "-n", "-j", "2", "."])
        .env("RUST_LOG", "debug")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("red.txt"), "{stderr}");
    assert!(!stderr.contains('\u{1b}'), "{stderr:?}");
}
