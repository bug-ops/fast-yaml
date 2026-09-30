//! End-to-end tests for the `--max-input-size` cap on single-file and stdin input.

#![allow(clippy::missing_docs_in_private_items)]
#![allow(deprecated)] // Command::cargo_bin is deprecated but still works

use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

const YAML: &str = "key: value\n";

fn fy(args: &[&str]) -> Command {
    let mut cmd = Command::cargo_bin("fy").unwrap();
    cmd.args(args);
    cmd
}

#[test]
fn file_over_limit_fails_for_every_subcommand() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("a.yaml");
    fs::write(&path, YAML).unwrap();
    let path = path.to_str().unwrap();

    for cmd in [&["parse"][..], &["format"], &["lint"], &["convert", "json"]] {
        let mut args = cmd.to_vec();
        args.extend(["--max-input-size", "5", path]);
        let output = fy(&args).output().unwrap();
        assert_eq!(output.status.code(), Some(1), "{cmd:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("maximum size of 5 bytes"), "{stderr}");
        assert!(stderr.contains("--max-input-size"), "{stderr}");
    }
}

#[test]
fn file_at_limit_is_accepted() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("a.yaml");
    fs::write(&path, YAML).unwrap();
    let limit = YAML.len().to_string();

    fy(&["parse", "--max-input-size", &limit, path.to_str().unwrap()])
        .assert()
        .success();
}

#[test]
fn bare_stdin_format_honors_limit() {
    fy(&["--max-input-size", "5"])
        .write_stdin(YAML)
        .assert()
        .code(1);
}

#[test]
fn quiet_and_verbose_conflict_in_every_placement() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("a.yaml");
    fs::write(&path, YAML).unwrap();
    let path = path.to_str().unwrap();

    for args in [
        vec!["-q", "-v", "parse", path],
        vec!["parse", "--quiet", "--verbose", path],
        vec!["-q", "parse", "-v", path],
        vec!["-v", "parse", "-q", path],
        vec!["-q", "lint", "-v", path],
        vec!["-v", "format", "-q", path],
    ] {
        let output = fy(&args).output().unwrap();
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("cannot be used with"), "{args:?}: {stderr}");
    }
}

#[test]
fn stdin_at_limit_is_accepted_and_one_over_is_rejected() {
    let limit = YAML.len();
    fy(&["parse", "--max-input-size", &limit.to_string()])
        .write_stdin(YAML)
        .assert()
        .success();
    let output = fy(&["parse", "--max-input-size", &(limit - 1).to_string()])
        .write_stdin(YAML)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("maximum size of 10 bytes"), "{stderr}");
}

#[test]
fn invalid_sizes_are_usage_errors() {
    for bad in [
        "0",
        "2GiB",
        "abc",
        "-1",
        "1.5MiB",
        "1kib",
        "",
        "17179869184GiB",
        "18446744073709551615KiB",
    ] {
        let output = fy(&["parse", "--max-input-size", bad])
            .write_stdin(YAML)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{bad:?}");
    }
}

#[test]
fn batch_format_honors_limit() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.yaml"), YAML).unwrap();
    fs::write(dir.path().join("b.yaml"), YAML).unwrap();
    let root = dir.path().to_str().unwrap();
    let a = dir.path().join("a.yaml");
    let b = dir.path().join("b.yaml");
    let (a, b) = (a.to_str().unwrap(), b.to_str().unwrap());

    for paths in [vec![root], vec![a, b]] {
        let mut args = vec!["format", "--dry-run", "--max-input-size", "5"];
        args.extend(&paths);
        let output = fy(&args).output().unwrap();
        assert_eq!(output.status.code(), Some(1), "{paths:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("exceeds"), "{stderr}");

        let mut args = vec!["format", "--dry-run"];
        args.extend(&paths);
        assert!(!matches!(
            fy(&args).output().unwrap().status.code(),
            Some(1)
        ));
    }
}

#[test]
fn batch_lint_honors_limit() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.yaml"), YAML).unwrap();
    let root = dir.path().to_str().unwrap();

    let output = fy(&["lint", "--max-input-size", "5", root])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("maximum size of 5 bytes"), "{stderr}");

    let output = fy(&["lint", root]).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
}
