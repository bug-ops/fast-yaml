//! Batch lint mode discovery integration tests (#513, #514).

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

const NO_FILES: &str = "no YAML files found";
const BROKEN: &str = "a: [1\n";
const CLEAN: &str = "---\nb: 2\n";

#[allow(deprecated)]
fn fy() -> Command {
    Command::cargo_bin("fy").unwrap()
}

#[test]
fn test_lint_explicit_uppercase_extension_is_linted() {
    let temp = TempDir::new().unwrap();
    let upper = temp.path().join("UP.YAML");
    let lower = temp.path().join("m1.yaml");
    fs::write(&upper, BROKEN).unwrap();
    fs::write(&lower, CLEAN).unwrap();

    fy().args(["lint", upper.to_str().unwrap(), lower.to_str().unwrap()])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("UP.YAML"));
}

#[test]
fn test_lint_empty_directory_fails() {
    let temp = TempDir::new().unwrap();

    fy().args(["lint", temp.path().to_str().unwrap()])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(NO_FILES));
}

#[test]
fn test_lint_non_yaml_glob_fails() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("notes.txt"), "a: 1\n").unwrap();
    let pattern = temp.path().join("notes*");

    fy().args(["lint", pattern.to_str().unwrap()])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(NO_FILES));
}

#[test]
fn test_lint_exclude_all_fails() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("a.yaml"), CLEAN).unwrap();

    fy().args(["lint", "--exclude", "*.yaml", temp.path().to_str().unwrap()])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(NO_FILES));
}

#[test]
fn test_lint_no_files_fails_even_when_quiet() {
    let temp = TempDir::new().unwrap();

    fy().args(["-q", "lint", temp.path().to_str().unwrap()])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(NO_FILES));
}

#[test]
fn test_lint_uppercase_extension_found_in_directory() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("UP.YAML"), BROKEN).unwrap();

    fy().args(["lint", temp.path().to_str().unwrap()])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("UP.YAML"));
}

#[test]
fn test_lint_user_include_is_case_insensitive() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("a.YML"), BROKEN).unwrap();

    fy().args(["lint", "--include", "*.yml", temp.path().to_str().unwrap()])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("a.YML"));
}

#[test]
fn test_lint_exclude_is_case_insensitive() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("KEY.YAML"), BROKEN).unwrap();
    fs::write(temp.path().join("ok.yaml"), CLEAN).unwrap();

    fy().args([
        "lint",
        "--exclude",
        "**/key.yaml",
        temp.path().to_str().unwrap(),
    ])
    .assert()
    .success();
}

#[test]
fn test_lint_mixed_empty_glob_and_file_succeeds() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("notes.txt"), "a: 1\n").unwrap();
    let ok = temp.path().join("ok.yaml");
    fs::write(&ok, CLEAN).unwrap();
    let pattern = temp.path().join("notes*");

    fy().args(["lint", pattern.to_str().unwrap(), ok.to_str().unwrap()])
        .assert()
        .success();
}

#[test]
fn test_lint_unmatched_glob_with_file_still_fails() {
    let temp = TempDir::new().unwrap();
    let ok = temp.path().join("ok.yaml");
    fs::write(&ok, CLEAN).unwrap();
    let pattern = temp.path().join("nomatch*");

    fy().args(["lint", pattern.to_str().unwrap(), ok.to_str().unwrap()])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("glob pattern matched no files"));
}

#[test]
fn test_lint_exclude_drops_explicit_file() {
    let temp = TempDir::new().unwrap();
    let skipped = temp.path().join("skip.yaml");
    let ok = temp.path().join("ok.yaml");
    fs::write(&skipped, BROKEN).unwrap();
    fs::write(&ok, CLEAN).unwrap();

    fy().args([
        "lint",
        "--exclude",
        "**/skip.yaml",
        skipped.to_str().unwrap(),
        ok.to_str().unwrap(),
    ])
    .assert()
    .success();
}

#[test]
fn test_lint_stdin_files_lints_each_listed_file() {
    let temp = TempDir::new().unwrap();
    let broken = temp.path().join("broken.yaml");
    let clean = temp.path().join("clean.yaml");
    fs::write(&broken, BROKEN).unwrap();
    fs::write(&clean, CLEAN).unwrap();

    fy().args(["lint", "--stdin-files"])
        .write_stdin(format!("{}\n{}\n", broken.display(), clean.display()))
        .assert()
        .code(2)
        .stderr(predicate::str::contains("broken.yaml"));
}

#[test]
fn test_lint_stdin_files_accepts_blank_and_comment_lines() {
    let temp = TempDir::new().unwrap();
    let clean = temp.path().join("clean.yaml");
    fs::write(&clean, CLEAN).unwrap();

    fy().args(["lint", "--stdin-files"])
        .write_stdin(format!("\n# list\n  {}  \r\n", clean.display()))
        .assert()
        .success();
}

#[test]
fn test_lint_stdin_files_empty_list_succeeds() {
    fy().args(["lint", "--stdin-files"])
        .write_stdin("")
        .assert()
        .success();
}

#[test]
fn test_lint_stdin_files_rejects_non_yaml_line() {
    let temp = TempDir::new().unwrap();
    let clean = temp.path().join("clean.yaml");
    let text = temp.path().join("notes.txt");
    fs::write(&clean, CLEAN).unwrap();
    fs::write(&text, "hello\n").unwrap();

    fy().args(["lint", "--stdin-files"])
        .write_stdin(format!("{}\n{}\n", clean.display(), text.display()))
        .assert()
        .code(1)
        .stderr(predicate::str::contains("--stdin-files line 2"));
}

#[test]
fn test_lint_stdin_files_conflicts_with_paths() {
    fy().args(["lint", "--stdin-files", "a.yaml"])
        .assert()
        .failure();
}

/// Lints `files` as JSON and returns stdout, requiring the pretty layout of one buffered array.
fn json_output(dir: &TempDir, files: &[&str]) -> String {
    let paths: Vec<String> = files
        .iter()
        .map(|f| dir.path().join(f).to_str().unwrap().to_owned())
        .collect();
    let output = fy()
        .args(["lint", "--format", "json", "-j", "4"])
        .args(&paths)
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let value: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        stdout,
        format!("{}\n", serde_json::to_string_pretty(&value).unwrap())
    );
    stdout
}

#[test]
fn test_lint_json_batch_keeps_the_buffered_layout_for_zero_one_and_many() {
    let temp = TempDir::new().unwrap();
    for (name, content) in [
        ("clean1.yaml", "---\na: 1\n"),
        ("clean2.yaml", "---\nb: 2\n"),
        ("one.yaml", "---\na: 1 \n"),
        ("two.yaml", "---\nk: 1\nk: 2\nz: 1 \n"),
    ] {
        fs::write(temp.path().join(name), content).unwrap();
    }

    assert_eq!(json_output(&temp, &["clean1.yaml", "clean2.yaml"]), "[]\n");

    let one = json_output(&temp, &["one.yaml", "clean1.yaml"]);
    let value: serde_json::Value = serde_json::from_str(&one).unwrap();
    assert_eq!(value.as_array().unwrap().len(), 1);
    assert!(value[0]["file"].as_str().unwrap().ends_with("one.yaml"));

    let many = json_output(&temp, &["two.yaml", "clean1.yaml", "one.yaml"]);
    let value: serde_json::Value = serde_json::from_str(&many).unwrap();
    let files: Vec<&str> = value
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["file"].as_str().unwrap())
        .collect();
    assert!(files.len() >= 3);
    assert!(files.first().unwrap().ends_with("two.yaml"));
    assert!(files.last().unwrap().ends_with("one.yaml"));
}

/// Writes `count` files named `{prefix}NN.yaml` and returns their paths in a fixed shuffled order.
fn shuffled_files(temp: &TempDir, prefix: &str, count: usize, content: &str) -> Vec<String> {
    let mut paths: Vec<String> = (0..count)
        .map(|i| {
            let path = temp.path().join(format!("{prefix}{i:02}.yaml"));
            fs::write(&path, content).unwrap();
            path.to_str().unwrap().to_owned()
        })
        .collect();
    paths.sort_by_key(|p| {
        p.bytes()
            .fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)))
    });
    paths
}

fn positions_in(output: &str, paths: &[String]) -> Vec<usize> {
    paths
        .iter()
        .map(|p| output.find(p.as_str()).unwrap())
        .collect()
}

#[test]
fn test_lint_batch_reports_failures_on_stderr_in_file_order() {
    let temp = TempDir::new().unwrap();
    let paths = shuffled_files(&temp, "f", 60, BROKEN);

    for _ in 0..3 {
        let output = fy()
            .args(["lint", "-j", "8"])
            .args(&paths)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(positions_in(&stderr, &paths).is_sorted(), "{stderr}");
    }
}

#[test]
fn test_lint_batch_text_output_follows_file_order() {
    let temp = TempDir::new().unwrap();
    let paths = shuffled_files(&temp, "t", 30, "---\na: 1 \n");

    let output = fy()
        .args(["lint", "-j", "8"])
        .args(&paths)
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(positions_in(&stdout, &paths).is_sorted(), "{stdout}");
}
