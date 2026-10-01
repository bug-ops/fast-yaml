//! Bounds and secrecy of `ignore-from-file` (security audit M1, L1).

use std::error::Error;
use std::fs;

use fast_yaml_linter::{ConfigFile, ConfigFileError};
use tempfile::TempDir;

const SECRET: &str = "export API_TOKEN=\"ghp_S3cr3t{1234\"";

fn chain(error: &dyn Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text = format!("{text} | {cause}");
        source = cause.source();
    }
    text
}

fn load(dir: &TempDir, config: &str) -> Result<ConfigFile, ConfigFileError> {
    let path = dir.path().join("cfg.yaml");
    fs::write(&path, config).unwrap();
    ConfigFile::load(&path)
}

fn top_and_rule(files: &str) -> [String; 2] {
    [
        format!("ignore-from-file: {files}\n"),
        format!("rules:\n  braces:\n    ignore-from-file: {files}\n"),
    ]
}

#[test]
fn a_repeated_file_name_is_read_once() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("p.txt"), "vendor/\n".repeat(100)).unwrap();
    let names = vec!["p.txt"; 400].join(", ");
    for config in top_and_rule(&format!("[{names}]")) {
        assert!(load(&dir, &config).is_ok(), "{config:.60}");
    }
}

#[test]
fn a_file_over_the_pattern_cap_stops_at_the_cap() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("big.txt"), "\n".repeat(100_000)).unwrap();
    for config in top_and_rule("big.txt") {
        let error = load(&dir, &config).unwrap_err();
        let text = chain(&error);
        assert!(text.contains("pattern lines"), "{text}");
    }
}

#[test]
fn many_different_files_are_capped() {
    let dir = TempDir::new().unwrap();
    let mut names = Vec::new();
    for index in 0..40 {
        let name = format!("f{index}.txt");
        fs::write(dir.path().join(&name), "x/\n").unwrap();
        names.push(name);
    }
    for config in top_and_rule(&format!("[{}]", names.join(", "))) {
        let text = chain(&load(&dir, &config).unwrap_err());
        assert!(text.contains("different files"), "{text}");
    }
}

#[test]
fn an_invalid_line_is_reported_by_position_without_its_text() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("ig.txt"), format!("ok/\n{SECRET}\n")).unwrap();
    for config in top_and_rule("ig.txt") {
        let error = load(&dir, &config).unwrap_err();
        let text = chain(&error);
        assert!(text.contains("ig.txt"), "{text}");
        assert!(text.contains("line 2"), "{text}");
        assert!(
            !text.contains("ghp_") && !text.contains("API_TOKEN"),
            "{text}"
        );
    }
}

#[test]
fn an_invalid_inline_pattern_still_names_the_pattern() {
    let dir = TempDir::new().unwrap();
    let text = chain(&load(&dir, "ignore: ['a{1']\n").unwrap_err());
    assert!(text.contains("a{1"), "{text}");
}
