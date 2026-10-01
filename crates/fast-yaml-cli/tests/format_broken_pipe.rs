//! `fy format` and `fy convert` end quietly when the reader of stdout goes away.

#![allow(clippy::missing_docs_in_private_items)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use assert_cmd::cargo::cargo_bin;
use tempfile::TempDir;

fn big_yaml(dir: &TempDir) -> PathBuf {
    let path = dir.path().join("big.yaml");
    let mut file = std::fs::File::create(&path).unwrap();
    for i in 0..200_000 {
        writeln!(file, "key{i}: value{i}").unwrap();
    }
    path
}

fn run_with_closed_stdout(args: &[&str], file: &Path) -> Output {
    let mut child = Command::new(cargo_bin("fy"))
        .args(args)
        .arg(file)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    child.wait_with_output().unwrap()
}

#[test]
fn format_with_closed_stdout_is_quiet_and_succeeds() {
    let dir = TempDir::new().unwrap();
    let out = run_with_closed_stdout(&["format"], &big_yaml(&dir));
    assert!(out.status.success(), "{out:?}");
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn convert_with_closed_stdout_is_quiet_and_succeeds() {
    let dir = TempDir::new().unwrap();
    let out = run_with_closed_stdout(&["convert", "json"], &big_yaml(&dir));
    assert!(out.status.success(), "{out:?}");
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn invalid_input_still_fails_with_closed_stdout() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bad.yaml");
    std::fs::write(&path, "a: [\n").unwrap();
    let out = run_with_closed_stdout(&["format"], &path);
    assert!(!out.status.success(), "{out:?}");
}
