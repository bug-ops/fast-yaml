//! Regression tests for #704: diagnostic context has no phantom line after the final newline,
//! except when a diagnostic is located on that line.

#![cfg(feature = "linter")]
use assert_cmd::cargo_bin_cmd;
use std::fs;
use std::path::Path;

fn lint(path: &Path, extra: &[&str]) -> String {
    let output = cargo_bin_cmd!("fy")
        .arg("lint")
        .args(extra)
        .arg(path)
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap()
}

fn has_row(text: &str, number: usize) -> bool {
    text.lines()
        .any(|line| line.trim_start().starts_with(&format!("{number} |")))
}

#[test]
fn trailing_newline_adds_no_context_row_in_text_and_json() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("one.yaml");
    fs::write(&path, "a:   1\n").unwrap();

    let text = lint(&path, &["--no-config"]);
    assert!(has_row(&text, 1), "{text}");
    assert!(!has_row(&text, 2), "{text}");

    let json = lint(&path, &["--no-config", "--format", "json"]);
    let diagnostics: serde_json::Value = serde_json::from_str(&json).unwrap();
    let lines = diagnostics[0]["context"]["lines"].as_array().unwrap();
    assert_eq!(lines.len(), 1, "{json}");
    assert_eq!(lines[0]["line_number"], 1);
}

#[test]
fn document_end_at_eof_keeps_its_own_line_and_caret() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("eof.yaml");
    let config = dir.path().join("config.yaml");
    fs::write(&path, "a: 1\nb: 2\n").unwrap();
    fs::write(&config, "rules:\n  document-end: enable\n").unwrap();

    let text = lint(&path, &["--config", config.to_str().unwrap()]);
    assert!(text.contains("input:3:1"), "{text}");
    assert!(has_row(&text, 3), "{text}");
    assert!(!has_row(&text, 4), "{text}");
    assert!(text.lines().any(|l| l.trim() == "| ^"), "{text}");
}
