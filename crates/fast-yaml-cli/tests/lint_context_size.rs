//! Regression test for #454: diagnostic context must not copy the whole source line per diagnostic.

#![cfg(feature = "linter")]
#![allow(clippy::missing_docs_in_private_items)]

use assert_cmd::cargo_bin_cmd;
use fast_yaml_linter::MAX_CONTEXT_COLUMNS;
use std::fs;

const COMMAS: usize = 1_500;

#[test]
fn json_context_of_many_diagnostics_on_one_line_stays_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("long.yaml");
    fs::write(&path, format!("k: [{}]\n", "1 ,".repeat(COMMAS))).unwrap();

    let output = cargo_bin_cmd!("fy")
        .args(["lint", "--format", "json"])
        .arg(&path)
        .output()
        .unwrap();

    let diagnostics: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let diagnostics = diagnostics.as_array().unwrap();
    assert!(diagnostics.len() >= COMMAS);

    for line in diagnostics
        .iter()
        .flat_map(|d| d["context"]["lines"].as_array().unwrap())
    {
        let content = line["content"].as_str().unwrap();
        assert!(content.chars().count() <= MAX_CONTEXT_COLUMNS);
    }
    assert!(
        output.stdout.len() < 5_000_000,
        "json output is {} bytes",
        output.stdout.len()
    );
}
