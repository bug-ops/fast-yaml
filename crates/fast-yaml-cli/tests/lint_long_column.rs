//! Regression test for #416: a diagnostic column beyond `u16::MAX` must not panic the text formatter.

#![allow(clippy::missing_docs_in_private_items)]
#![allow(deprecated)] // Command::cargo_bin is deprecated but still works

use assert_cmd::Command;
use std::fs;

const LONG: usize = 70_000;

fn assert_lint_reports_without_panic(content: &str) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("long.yaml");
    fs::write(&path, content).unwrap();

    let output = Command::cargo_bin("fy")
        .unwrap()
        .arg("lint")
        .arg(&path)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stderr.contains("panicked"), "{stderr}");
    assert_ne!(output.status.code(), Some(101), "{stderr}");
    assert!(stdout.contains("line-length"), "{stdout:.200}");
    assert!(stdout.contains('^'));
}

#[test]
fn lint_survives_ascii_line_beyond_u16() {
    assert_lint_reports_without_panic(&format!("! {}", "a".repeat(LONG)));
}

#[test]
fn lint_survives_non_ascii_line_beyond_u16() {
    assert_lint_reports_without_panic(&format!("ключ: {}", "я".repeat(LONG)));
}
