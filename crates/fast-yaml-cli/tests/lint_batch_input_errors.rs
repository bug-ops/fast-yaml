//! Batch lint reports every file with a syntax or encoding error in every format (tester item 1).

#![allow(clippy::missing_docs_in_private_items)]

use std::fs;

use tempfile::TempDir;

const BAD_ESCAPE: &str = "a: \"\\xZZ\"\n";
const UNCLOSED: &str = "a: [1\n";
const DUPLICATE: &str = "a: 1\na: 2\n";

fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("bad_escape.yaml"), BAD_ESCAPE).unwrap();
    fs::write(dir.path().join("unclosed.yaml"), UNCLOSED).unwrap();
    fs::write(dir.path().join("utf16.yaml"), b"\xFF\xFEa\0:\0 \x001\0").unwrap();
    fs::write(dir.path().join("dup.yaml"), DUPLICATE).unwrap();
    dir
}

fn run(dir: &TempDir, format: &str) -> String {
    let out = assert_cmd::cargo_bin_cmd!("fy")
        .args(["lint", "--no-config", "--format", format])
        .arg(dir.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{format}");
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn json_lists_every_failing_file_with_the_file_key_last() {
    let dir = project();
    let json: serde_json::Value = serde_json::from_str(&run(&dir, "json")).unwrap();
    let records = json.as_array().unwrap();
    for name in ["bad_escape.yaml", "unclosed.yaml", "utf16.yaml", "dup.yaml"] {
        let present = records
            .iter()
            .any(|r| r["file"].as_str().is_some_and(|f| f.ends_with(name)));
        assert!(present, "{name} is missing: {records:?}");
    }
    for record in records {
        let last = record.as_object().unwrap().keys().next_back();
        assert_eq!(last.map(String::as_str), Some("file"), "{record:?}");
    }
    let syntax = records.iter().filter(|r| r["code"] == "syntax").count();
    assert_eq!(syntax, 3);
}

#[test]
fn every_format_reports_the_same_number_of_records() {
    let dir = project();
    let json: serde_json::Value = serde_json::from_str(&run(&dir, "json")).unwrap();
    let json_count = json.as_array().unwrap().len();

    let parsable = run(&dir, "parsable").lines().count();
    let github = run(&dir, "github").lines().count();
    let sarif: serde_json::Value = serde_json::from_str(&run(&dir, "sarif")).unwrap();
    let sarif_count = sarif["runs"][0]["results"].as_array().unwrap().len();

    assert_eq!(parsable, json_count, "parsable");
    assert_eq!(github, json_count, "github");
    assert_eq!(sarif_count, json_count, "sarif");
}
