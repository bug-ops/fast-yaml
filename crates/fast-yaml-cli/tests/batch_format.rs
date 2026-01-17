//! Batch format mode integration tests.
//!
//! These tests verify the new batch processing capabilities:
//! - Directory processing
//! - Multi-file processing
//! - stdin-files mode
//! - Include/exclude patterns
//! - Dry-run mode

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Helper to create fy command
#[allow(deprecated)]
fn fy() -> Command {
    Command::cargo_bin("fy").unwrap()
}

#[test]
fn test_batch_multiple_files() {
    let temp = TempDir::new().unwrap();
    let file1 = temp.path().join("file1.yaml");
    let file2 = temp.path().join("file2.yaml");

    fs::write(&file1, "key1:  value1\n").unwrap();
    fs::write(&file2, "key2:  value2\n").unwrap();

    fy().args([
        "format",
        "-i",
        file1.to_str().unwrap(),
        file2.to_str().unwrap(),
    ])
    .assert()
    .success();

    assert_eq!(fs::read_to_string(&file1).unwrap(), "key1: value1");
    assert_eq!(fs::read_to_string(&file2).unwrap(), "key2: value2");
}

#[test]
fn test_batch_directory_recursive() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("yaml");
    let subdir = dir.join("subdir");
    fs::create_dir_all(&subdir).unwrap();

    let file1 = dir.join("test1.yaml");
    let file2 = subdir.join("test2.yaml");

    fs::write(&file1, "key1:  value1\n").unwrap();
    fs::write(&file2, "key2:  value2\n").unwrap();

    fy().args(["format", "-i", dir.to_str().unwrap()])
        .assert()
        .success();

    assert_eq!(fs::read_to_string(&file1).unwrap(), "key1: value1");
    assert_eq!(fs::read_to_string(&file2).unwrap(), "key2: value2");
}

#[test]
fn test_batch_directory_no_recursive() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("yaml");
    let subdir = dir.join("subdir");
    fs::create_dir_all(&subdir).unwrap();

    let file1 = dir.join("test1.yaml");
    let file2 = subdir.join("test2.yaml");

    fs::write(&file1, "key1:  value1\n").unwrap();
    fs::write(&file2, "key2:  value2\n").unwrap();

    fy().args(["format", "-i", "--no-recursive", dir.to_str().unwrap()])
        .assert()
        .success();

    // Top-level file should be formatted
    assert_eq!(fs::read_to_string(&file1).unwrap(), "key1: value1");

    // Subdirectory file should NOT be formatted
    assert_eq!(fs::read_to_string(&file2).unwrap(), "key2:  value2\n");
}

#[test]
fn test_batch_exclude_pattern() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("yaml");
    let vendor = dir.join("vendor");
    fs::create_dir_all(&vendor).unwrap();

    let file1 = dir.join("config.yaml");
    let file2 = vendor.join("lib.yaml");

    fs::write(&file1, "key1:  value1\n").unwrap();
    fs::write(&file2, "key2:  value2\n").unwrap();

    fy().args([
        "format",
        "-i",
        "--exclude",
        "**/vendor/**",
        dir.to_str().unwrap(),
    ])
    .assert()
    .success();

    // Main file should be formatted
    assert_eq!(fs::read_to_string(&file1).unwrap(), "key1: value1");

    // Vendor file should NOT be formatted
    assert_eq!(fs::read_to_string(&file2).unwrap(), "key2:  value2\n");
}

#[test]
#[allow(clippy::similar_names)]
fn test_batch_include_pattern() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("yaml");
    fs::create_dir(&dir).unwrap();

    let yaml_file = dir.join("test.yaml");
    let yml_file = dir.join("test.yml");
    let txt_file = dir.join("test.txt");

    fs::write(&yaml_file, "key1:  value1\n").unwrap();
    fs::write(&yml_file, "key2:  value2\n").unwrap();
    fs::write(&txt_file, "key3:  value3\n").unwrap();

    fy().args(["format", "-i", "--include", "*.yml", dir.to_str().unwrap()])
        .assert()
        .success();

    // .yaml file should NOT be formatted (only .yml included)
    assert_eq!(fs::read_to_string(&yaml_file).unwrap(), "key1:  value1\n");

    // .yml file should be formatted
    assert_eq!(fs::read_to_string(&yml_file).unwrap(), "key2: value2");

    // .txt file should NOT be touched
    assert_eq!(fs::read_to_string(&txt_file).unwrap(), "key3:  value3\n");
}

#[test]
fn test_batch_dry_run() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("test.yaml");
    let original = "key:  value\n";

    fs::write(&file, original).unwrap();

    fy().args(["format", "-n", file.to_str().unwrap()])
        .assert()
        .success();

    // File should NOT be modified
    assert_eq!(fs::read_to_string(&file).unwrap(), original);
}

#[test]
fn test_batch_stdin_files() {
    let temp = TempDir::new().unwrap();
    let file1 = temp.path().join("file1.yaml");
    let file2 = temp.path().join("file2.yaml");

    fs::write(&file1, "key1:  value1\n").unwrap();
    fs::write(&file2, "key2:  value2\n").unwrap();

    let stdin_input = format!("{}\n{}\n", file1.to_str().unwrap(), file2.to_str().unwrap());

    fy().args(["format", "-i", "--stdin-files"])
        .write_stdin(stdin_input)
        .assert()
        .success();

    assert_eq!(fs::read_to_string(&file1).unwrap(), "key1: value1");
    assert_eq!(fs::read_to_string(&file2).unwrap(), "key2: value2");
}

#[test]
fn test_batch_empty_directory() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("empty");
    fs::create_dir(&dir).unwrap();

    fy().args(["format", "-i", dir.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("No YAML files found"));
}

#[test]
fn test_batch_mixed_success_failure() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("yaml");
    fs::create_dir(&dir).unwrap();

    let valid = dir.join("valid.yaml");
    let invalid = dir.join("invalid.yaml");

    fs::write(&valid, "key:  value\n").unwrap();
    fs::write(&invalid, "invalid: [\n").unwrap();

    fy().args(["format", "-i", dir.to_str().unwrap()])
        .assert()
        .failure()
        .code(1);

    // Valid file should still be formatted
    assert_eq!(fs::read_to_string(&valid).unwrap(), "key: value");
}

#[test]
fn test_batch_jobs_parallel() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("yaml");
    fs::create_dir(&dir).unwrap();

    for i in 0..10 {
        let file = dir.join(format!("file{i}.yaml"));
        fs::write(&file, format!("key{i}:  value{i}\n")).unwrap();
    }

    fy().args(["format", "-i", "-j", "4", dir.to_str().unwrap()])
        .assert()
        .success();

    // Verify all files formatted
    for i in 0..10 {
        let file = dir.join(format!("file{i}.yaml"));
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            format!("key{i}: value{i}")
        );
    }
}

#[test]
fn test_batch_quiet_mode() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("test.yaml");
    fs::write(&file, "key:  value\n").unwrap();

    fy().args(["format", "-i", "-q", file.to_str().unwrap()])
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn test_batch_verbose_mode() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("test.yaml");
    fs::write(&file, "key:  value\n").unwrap();

    fy().args(["format", "-i", "-v", file.to_str().unwrap()])
        .assert()
        .success();
}

#[test]
fn test_batch_custom_indent() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("yaml");
    fs::create_dir(&dir).unwrap();

    let file = dir.join("test.yaml");
    fs::write(&file, "parent:\n  child: value\n").unwrap();

    fy().args(["format", "-i", "--indent", "4", dir.to_str().unwrap()])
        .assert()
        .success();

    let output = fs::read_to_string(&file).unwrap();
    assert!(output.contains("parent:"));
}

#[test]
fn test_batch_respects_gitignore() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("project");
    fs::create_dir(&dir).unwrap();

    // Initialize git repo (required for .gitignore to work)
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(&dir)
        .output()
        .unwrap();

    let gitignore = dir.join(".gitignore");
    fs::write(&gitignore, "ignored.yaml\n").unwrap();

    let tracked = dir.join("tracked.yaml");
    let ignored = dir.join("ignored.yaml");

    fs::write(&tracked, "key1:  value1\n").unwrap();
    fs::write(&ignored, "key2:  value2\n").unwrap();

    fy().args(["format", "-i", dir.to_str().unwrap()])
        .assert()
        .success();

    // Tracked file should be formatted
    assert_eq!(fs::read_to_string(&tracked).unwrap(), "key1: value1");

    // Ignored file should NOT be formatted (respects .gitignore)
    assert_eq!(fs::read_to_string(&ignored).unwrap(), "key2:  value2\n");
}
