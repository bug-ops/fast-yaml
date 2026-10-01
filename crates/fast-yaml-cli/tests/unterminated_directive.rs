//! Regression tests for #403: input ending in an unterminated `%` directive must fail, not hang.

#![allow(clippy::missing_docs_in_private_items)]
#![allow(deprecated)] // Command::cargo_bin is deprecated but still works

use assert_cmd::Command;
use std::time::Duration;

fn assert_fails_without_hanging(args: &[&str]) {
    let output = Command::cargo_bin("fy")
        .unwrap()
        .args(args)
        .write_stdin("%")
        .timeout(Duration::from_secs(10))
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(1),
        "{args:?}: {:?}",
        output.status
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("while scanning a directive"),
        "{args:?}: {stderr}"
    );
}

#[test]
fn parse_rejects_unterminated_directive() {
    assert_fails_without_hanging(&["parse"]);
}

#[test]
fn lint_rejects_unterminated_directive() {
    assert_fails_without_hanging(&["lint"]);
}

#[test]
fn convert_rejects_unterminated_directive() {
    assert_fails_without_hanging(&["convert", "json"]);
}

#[test]
fn format_keeps_comment_after_non_ascii_directive() {
    let output = Command::cargo_bin("fy")
        .unwrap()
        .arg("format")
        .write_stdin("%FOO ééééé\n---\na: b # c\n")
        .timeout(Duration::from_secs(10))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{:?}", output.status);
    assert_eq!(output.stdout, []);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("would strip"), "{stderr}");
}
