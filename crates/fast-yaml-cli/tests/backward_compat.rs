//! Backward compatibility tests for Phase 4 CLI integration.
//!
//! These tests ensure that existing single-file and stdin modes
//! continue to work exactly as before batch mode was added.

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
fn test_stdin_mode_unchanged() {
    let input = "key:  value\n";
    let expected = "key: value";

    fy().arg("format")
        .write_stdin(input)
        .assert()
        .success()
        .stdout(expected);
}

#[test]
fn test_stdin_mode_with_custom_indent() {
    let input = "parent:\n  child: value\n";

    fy().args(["format", "--indent", "4"])
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::contains("parent:"));
}

#[test]
fn test_stdin_mode_with_custom_width() {
    let input = "key: value\n";

    fy().args(["format", "--width", "120"])
        .write_stdin(input)
        .assert()
        .success();
}

#[test]
fn test_single_file_stdout_unchanged() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("test.yaml");
    let input = "key:  value\n";
    let expected = "key: value";

    fs::write(&file, input).unwrap();

    fy().args(["format", file.to_str().unwrap()])
        .assert()
        .success()
        .stdout(expected);

    // File should NOT be modified
    assert_eq!(fs::read_to_string(&file).unwrap(), input);
}

#[test]
fn test_single_file_in_place_unchanged() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("test.yaml");
    let input = "key:  value\n";
    let expected = "key: value";

    fs::write(&file, input).unwrap();

    fy().args(["format", "-i", file.to_str().unwrap()])
        .assert()
        .success()
        .stdout("");

    // File SHOULD be modified
    assert_eq!(fs::read_to_string(&file).unwrap(), expected);
}

#[test]
fn test_single_file_with_indent() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("test.yaml");
    let input = "parent:\n  child: value\n";

    fs::write(&file, input).unwrap();

    fy().args(["format", "--indent", "4", file.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("parent:"));

    // File should NOT be modified when not using -i
    assert_eq!(fs::read_to_string(&file).unwrap(), input);
}

#[test]
fn test_single_file_in_place_with_indent() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("test.yaml");
    let input = "parent:\n  child: value\n";

    fs::write(&file, input).unwrap();

    fy().args(["format", "-i", "--indent", "4", file.to_str().unwrap()])
        .assert()
        .success();

    // File should be modified
    let output = fs::read_to_string(&file).unwrap();
    assert!(output.contains("parent:"));
}

#[test]
fn test_format_invalid_yaml_error() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("invalid.yaml");
    fs::write(&file, "invalid: [\n").unwrap();

    fy().args(["format", file.to_str().unwrap()])
        .assert()
        .failure()
        .code(1);
}

#[test]
fn test_format_nonexistent_file_error() {
    fy().args(["format", "/nonexistent/file.yaml"])
        .assert()
        .failure()
        .code(1);
}

#[test]
fn test_default_command_stdin() {
    let input = "key:  value\n";
    let expected = "key: value";

    fy().write_stdin(input).assert().success().stdout(expected);
}

#[test]
fn test_format_preserves_multiline_strings() {
    let input = "description: |\n  Line 1\n  Line 2\n";

    fy().arg("format")
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::contains("description:"));
}

#[test]
fn test_format_preserves_block_scalars() {
    let input = "text: >\n  folded\n  line\n";

    fy().arg("format")
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::contains("text:"));
}

#[test]
fn test_single_file_with_output_flag() {
    let temp = TempDir::new().unwrap();
    let input_file = temp.path().join("input.yaml");
    let output_file = temp.path().join("output.yaml");
    let input = "key:  value\n";
    let expected = "key: value";

    fs::write(&input_file, input).unwrap();

    fy().args([
        "format",
        "-o",
        output_file.to_str().unwrap(),
        input_file.to_str().unwrap(),
    ])
    .assert()
    .success();

    // Input file should NOT be modified
    assert_eq!(fs::read_to_string(&input_file).unwrap(), input);

    // Output file should contain formatted content
    assert_eq!(fs::read_to_string(&output_file).unwrap(), expected);
}
