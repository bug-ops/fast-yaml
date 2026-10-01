//! Batch format mode integration tests.
//!
//! These tests verify the new batch processing capabilities:
//! - Directory processing
//! - Multi-file processing
//! - stdin-files mode
//! - Include/exclude patterns
//! - Dry-run mode

use assert_cmd::{Command, cargo_bin_cmd};
use predicates::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Helper to create fy command
fn fy() -> Command {
    cargo_bin_cmd!("fy")
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

    assert_eq!(fs::read_to_string(&file1).unwrap(), "key1: value1\n");
    assert_eq!(fs::read_to_string(&file2).unwrap(), "key2: value2\n");
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

    assert_eq!(fs::read_to_string(&file1).unwrap(), "key1: value1\n");
    assert_eq!(fs::read_to_string(&file2).unwrap(), "key2: value2\n");
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
    assert_eq!(fs::read_to_string(&file1).unwrap(), "key1: value1\n");

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
    assert_eq!(fs::read_to_string(&file1).unwrap(), "key1: value1\n");

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
    assert_eq!(fs::read_to_string(&yml_file).unwrap(), "key2: value2\n");

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
        .code(5);
    fy().args(["format", "-i", "--dry-run", file.to_str().unwrap()])
        .assert()
        .code(5);

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

    assert_eq!(fs::read_to_string(&file1).unwrap(), "key1: value1\n");
    assert_eq!(fs::read_to_string(&file2).unwrap(), "key2: value2\n");
}

#[test]
fn test_batch_empty_directory() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("empty");
    fs::create_dir(&dir).unwrap();

    fy().args(["format", "-i", dir.to_str().unwrap()])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("no YAML files found"));
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
    assert_eq!(fs::read_to_string(&valid).unwrap(), "key: value\n");
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
            format!("key{i}: value{i}\n")
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
    assert_eq!(fs::read_to_string(&tracked).unwrap(), "key1: value1\n");

    // Ignored file should NOT be formatted (respects .gitignore)
    assert_eq!(fs::read_to_string(&ignored).unwrap(), "key2:  value2\n");
}

const COMMENTED: &str = "# important\nkey:   value  # note\n";

#[test]
fn test_batch_with_comments_errors_and_leaves_file_unchanged() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("x.yaml");
    fs::write(&file, COMMENTED).unwrap();

    fy().args(["format", "-i", temp.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--strip-comments"));

    assert_eq!(fs::read_to_string(&file).unwrap(), COMMENTED);
}

#[test]
fn test_batch_with_comments_strip_flag_strips() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("x.yaml");
    fs::write(&file, COMMENTED).unwrap();

    fy().args([
        "format",
        "-i",
        "--strip-comments",
        temp.path().to_str().unwrap(),
    ])
    .assert()
    .success();

    assert_eq!(fs::read_to_string(&file).unwrap(), "key: value\n");
}

#[test]
fn test_batch_mixed_only_formats_files_without_comments() {
    let temp = TempDir::new().unwrap();
    let commented = temp.path().join("a.yaml");
    let clean = temp.path().join("b.yaml");
    fs::write(&commented, COMMENTED).unwrap();
    fs::write(&clean, "key:   value\n").unwrap();

    fy().args(["format", "-i", temp.path().to_str().unwrap()])
        .assert()
        .failure();

    assert_eq!(fs::read_to_string(&commented).unwrap(), COMMENTED);
    assert_eq!(fs::read_to_string(&clean).unwrap(), "key: value\n");
}

#[test]
fn test_dry_run_with_output_is_rejected() {
    let temp = TempDir::new().unwrap();
    let out = temp.path().join("out.yaml");

    fy().args(["format", "--dry-run", "-o", out.to_str().unwrap()])
        .write_stdin("key:    value\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--dry-run"));

    assert!(!out.exists());
}

#[test]
fn test_stdin_with_comments_errors() {
    fy().arg("format")
        .write_stdin(COMMENTED)
        .assert()
        .failure()
        .stderr(predicate::str::contains("--strip-comments"));

    fy().args(["format", "--dry-run"])
        .write_stdin(COMMENTED)
        .assert()
        .failure()
        .stderr(predicate::str::contains("--strip-comments"));
}

#[test]
fn test_multiple_paths_with_comments_guard() {
    let temp = TempDir::new().unwrap();
    let commented = temp.path().join("a.yaml");
    let clean = temp.path().join("b.yaml");
    fs::write(&commented, COMMENTED).unwrap();
    fs::write(&clean, "key:   value\n").unwrap();

    fy().args([
        "format",
        "-i",
        commented.to_str().unwrap(),
        clean.to_str().unwrap(),
    ])
    .assert()
    .code(1);

    assert_eq!(fs::read_to_string(&commented).unwrap(), COMMENTED);
    assert_eq!(fs::read_to_string(&clean).unwrap(), "key: value\n");
}

#[test]
fn test_glob_with_comments_guard() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("x.yaml");
    fs::write(&file, COMMENTED).unwrap();
    let pattern = temp.path().join("*.yaml");

    fy().args(["format", "-i", pattern.to_str().unwrap()])
        .assert()
        .failure();

    assert_eq!(fs::read_to_string(&file).unwrap(), COMMENTED);
}

#[test]
fn test_single_file_dry_run_with_comments() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("x.yaml");
    fs::write(&file, COMMENTED).unwrap();
    let path = file.to_str().unwrap();

    fy().args(["format", "-i", "--dry-run", path])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--strip-comments"));
    assert_eq!(fs::read_to_string(&file).unwrap(), COMMENTED);

    fy().args(["format", "-i", "--dry-run", "--strip-comments", path])
        .assert()
        .code(5);
    assert_eq!(fs::read_to_string(&file).unwrap(), COMMENTED);
}

#[test]
fn test_single_file_dry_run_missing_file_fails() {
    let temp = TempDir::new().unwrap();
    let missing = temp.path().join("nonexist.yaml");

    fy().args(["format", "--dry-run", missing.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("path does not exist"));
}

#[test]
fn test_dry_run_missing_and_clean_path_fails() {
    let temp = TempDir::new().unwrap();
    let missing = temp.path().join("nonexist.yaml");
    let clean = temp.path().join("clean.yaml");
    fs::write(&clean, "key: value\n").unwrap();

    fy().args([
        "format",
        "--dry-run",
        missing.to_str().unwrap(),
        clean.to_str().unwrap(),
    ])
    .assert()
    .code(1)
    .stderr(predicate::str::contains("path does not exist"));

    assert_eq!(fs::read_to_string(&clean).unwrap(), "key: value\n");
}

#[test]
fn test_dry_run_two_missing_paths_fails() {
    let temp = TempDir::new().unwrap();
    let missing = temp.path().join("nonexist.yaml");
    let other = temp.path().join("other.yaml");

    fy().args([
        "format",
        "--dry-run",
        missing.to_str().unwrap(),
        other.to_str().unwrap(),
    ])
    .assert()
    .code(1)
    .stderr(predicate::str::contains("path does not exist"))
    .stderr(predicate::str::contains("no YAML files found").not());
}

#[test]
fn test_in_place_missing_path_writes_nothing() {
    let temp = TempDir::new().unwrap();
    let missing = temp.path().join("nonexist.yaml");
    let messy = temp.path().join("messy.yaml");
    fs::write(&messy, "key:   value\n").unwrap();

    fy().args([
        "format",
        "-i",
        missing.to_str().unwrap(),
        messy.to_str().unwrap(),
    ])
    .assert()
    .code(1)
    .stderr(predicate::str::contains("path does not exist"));

    assert_eq!(fs::read_to_string(&messy).unwrap(), "key:   value\n");
}

#[test]
fn test_zero_match_glob_fails() {
    let temp = TempDir::new().unwrap();
    let pattern = temp.path().join("*.nomatch");

    fy().args(["format", "--dry-run", pattern.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("glob pattern matched no files"));
}

#[test]
fn test_zero_match_glob_with_clean_file_fails() {
    let temp = TempDir::new().unwrap();
    let clean = temp.path().join("clean.yaml");
    fs::write(&clean, "key: value\n").unwrap();
    let pattern = temp.path().join("nomatch*.yaml");

    fy().args([
        "format",
        "--dry-run",
        pattern.to_str().unwrap(),
        clean.to_str().unwrap(),
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains("glob pattern matched no files"));
}

#[test]
fn test_bracket_shaped_missing_path_fails() {
    let temp = TempDir::new().unwrap();
    let clean = temp.path().join("clean.yaml");
    fs::write(&clean, "key: value\n").unwrap();
    let missing = temp.path().join("missing[1].yaml");

    fy().args([
        "format",
        "--dry-run",
        missing.to_str().unwrap(),
        clean.to_str().unwrap(),
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains("path does not exist"));
}

#[test]
fn test_stdin_files_missing_line_fails() {
    let temp = TempDir::new().unwrap();
    let clean = temp.path().join("clean.yaml");
    fs::write(&clean, "key: value\n").unwrap();
    let missing = temp.path().join("missing.yaml");

    fy().args(["format", "--dry-run", "--stdin-files"])
        .write_stdin(format!("{}\n{}\n", missing.display(), clean.display()))
        .assert()
        .failure()
        .stderr(predicate::str::contains("path does not exist"));
}

#[test]
fn test_batch_flags_without_input_fail() {
    fy().args(["format", "--dry-run", "-j", "2"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "batch options (--jobs, --include, --exclude) need input",
        ));
}

#[test]
fn test_single_file_dry_run_summary_is_singular() {
    let temp = TempDir::new().unwrap();
    let clean = temp.path().join("clean.yaml");
    fs::write(&clean, "key: value\n").unwrap();

    fy().args(["format", "--dry-run", clean.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("Completed: 1 file "))
        .stderr(predicate::str::contains("1 files").not());
}

#[test]
fn test_bom_with_comment_is_guarded() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("bom.yaml");
    let content = "\u{FEFF}# header\nkey:   v\n";
    fs::write(&file, content).unwrap();

    fy().args(["format", "-i", file.to_str().unwrap()])
        .assert()
        .failure();
    assert_eq!(fs::read_to_string(&file).unwrap(), content);

    fy().args(["format", "-i", temp.path().to_str().unwrap()])
        .assert()
        .failure();
    assert_eq!(fs::read_to_string(&file).unwrap(), content);
}

#[test]
fn test_bom_without_comment_still_formats() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("bom.yaml");
    fs::write(&file, "\u{FEFF}key:   v\n").unwrap();

    fy().args(["format", "-i", file.to_str().unwrap()])
        .assert()
        .success();
    assert_eq!(fs::read_to_string(&file).unwrap(), "\u{FEFF}key: v\n");
}

#[test]
fn test_compact_flow_with_hash_in_string_formats() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("j.yaml");
    fs::write(&file, "{\"a\":\"x # y\"}\n").unwrap();

    fy().args(["format", "-i", file.to_str().unwrap()])
        .assert()
        .success();
}

#[test]
fn test_crlf_and_tab_comments_are_guarded() {
    let temp = TempDir::new().unwrap();
    for (name, content) in [
        ("crlf.yaml", "key: v\r\n# c\r\n"),
        ("tab.yaml", "key: v\t# c\n"),
    ] {
        let file = temp.path().join(name);
        fs::write(&file, content).unwrap();
        fy().args(["format", "-i", file.to_str().unwrap()])
            .assert()
            .failure();
        assert_eq!(fs::read_to_string(&file).unwrap(), content);
    }
}

#[test]
fn test_guard_exit_code_is_consistent_across_modes() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("x.yaml");
    fs::write(&file, COMMENTED).unwrap();
    let path = file.to_str().unwrap();

    let single = fy().args(["format", "-i", path]).assert().failure();
    let batch = fy()
        .args(["format", "-i", temp.path().to_str().unwrap()])
        .assert()
        .failure();
    let stdin = fy().arg("format").write_stdin(COMMENTED).assert().failure();

    let code = |a: &assert_cmd::assert::Assert| a.get_output().status.code();
    assert_eq!(code(&single), code(&batch));
    assert_eq!(code(&single), code(&stdin));
}

#[test]
fn test_apostrophe_comment_is_guarded() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("a.yaml");
    let content = "msg: don't # note\nb: 1\n";
    fs::write(&file, content).unwrap();

    fy().args(["format", "-i", file.to_str().unwrap()])
        .assert()
        .failure();

    assert_eq!(fs::read_to_string(&file).unwrap(), content);
}

#[test]
fn test_dry_run_already_formatted_is_unchanged() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("clean.yaml");
    fs::write(&file, "key: value\n").unwrap();

    fy().args(["format", "--dry-run", file.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("1 unchanged"))
        .stderr(predicate::str::contains("would change").not());
}

#[test]
fn test_dry_run_batch_mixed_reports_accurately() {
    let temp = TempDir::new().unwrap();
    let clean = temp.path().join("clean.yaml");
    let dirty = temp.path().join("dirty.yaml");
    fs::write(&clean, "key: value\n").unwrap();
    fs::write(&dirty, "key:    value\n").unwrap();

    fy().args([
        "format",
        "--dry-run",
        clean.to_str().unwrap(),
        dirty.to_str().unwrap(),
    ])
    .assert()
    .code(5)
    .stderr(predicate::str::contains("1 unchanged"))
    .stderr(predicate::str::contains("1 would change"));

    assert_eq!(fs::read_to_string(&dirty).unwrap(), "key:    value\n");
}

#[test]
fn test_dry_run_batch_all_clean_succeeds() {
    let temp = TempDir::new().unwrap();
    let a = temp.path().join("a.yaml");
    let b = temp.path().join("b.yaml");
    fs::write(&a, "a: 1\n").unwrap();
    fs::write(&b, "b: 2\n").unwrap();

    fy().args([
        "format",
        "--dry-run",
        a.to_str().unwrap(),
        b.to_str().unwrap(),
    ])
    .assert()
    .success()
    .stderr(predicate::str::contains("2 unchanged"));
}

#[test]
fn test_dry_run_failure_takes_precedence_over_would_change() {
    let temp = TempDir::new().unwrap();
    let dirty = temp.path().join("dirty.yaml");
    let commented = temp.path().join("c.yaml");
    fs::write(&dirty, "key:    value\n").unwrap();
    fs::write(&commented, COMMENTED).unwrap();

    fy().args([
        "format",
        "--dry-run",
        dirty.to_str().unwrap(),
        commented.to_str().unwrap(),
    ])
    .assert()
    .code(1)
    .stderr(predicate::str::contains("--strip-comments"));
}

#[test]
fn test_stdin_dry_run_reports_summary_and_exit_code() {
    fy().args(["format", "--dry-run"])
        .write_stdin("key: value\n")
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("1 unchanged"));

    fy().args(["format", "--dry-run"])
        .write_stdin("key:    value\n")
        .assert()
        .code(5)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("1 would change"));
}

#[test]
fn test_comment_after_multiline_quote_is_guarded() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("h2.yaml");
    let content = "a: foo\n  \"bar\nb: 1 # real comment\n";
    fs::write(&file, content).unwrap();

    fy().args(["format", "-i", file.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--strip-comments"));

    assert_eq!(fs::read_to_string(&file).unwrap(), content);
}

#[test]
fn test_quiet_dry_run_keeps_exit_codes() {
    let temp = TempDir::new().unwrap();
    let clean = temp.path().join("clean.yaml");
    let dirty = temp.path().join("dirty.yaml");
    fs::write(&clean, "key: value\n").unwrap();
    fs::write(&dirty, "key:    value\n").unwrap();

    fy().args(["-q", "format", "--dry-run", clean.to_str().unwrap()])
        .assert()
        .code(0);
    fy().args(["-q", "format", "--dry-run", dirty.to_str().unwrap()])
        .assert()
        .code(5);
}

#[test]
fn test_stdin_dry_run_with_comments() {
    fy().args(["format", "--dry-run"])
        .write_stdin(COMMENTED)
        .assert()
        .code(1);
    fy().args(["format", "--dry-run", "--strip-comments"])
        .write_stdin(COMMENTED)
        .assert()
        .code(5);
}

#[test]
fn test_batch_dry_run_strip_comments() {
    let temp = TempDir::new().unwrap();
    let commented = temp.path().join("c.yaml");
    let clean = temp.path().join("clean.yaml");
    fs::write(&commented, COMMENTED).unwrap();
    fs::write(&clean, "key: value\n").unwrap();

    fy().args([
        "format",
        "--dry-run",
        "--strip-comments",
        commented.to_str().unwrap(),
        clean.to_str().unwrap(),
    ])
    .assert()
    .code(5)
    .stderr(predicate::str::contains("1 would change"))
    .stderr(predicate::str::contains("1 unchanged"));

    assert_eq!(fs::read_to_string(&commented).unwrap(), COMMENTED);
}

#[test]
fn test_in_place_changed_and_failed_exits_with_failure_code() {
    let temp = TempDir::new().unwrap();
    let dirty = temp.path().join("dirty.yaml");
    let broken = temp.path().join("broken.yaml");
    fs::write(&dirty, "key:    value\n").unwrap();
    fs::write(&broken, "key: [\n").unwrap();

    fy().args([
        "format",
        "-i",
        dirty.to_str().unwrap(),
        broken.to_str().unwrap(),
    ])
    .assert()
    .code(1);

    assert_eq!(fs::read_to_string(&dirty).unwrap(), "key: value\n");
}

#[test]
fn test_empty_input_dry_run_is_unchanged() {
    let temp = TempDir::new().unwrap();
    let empty = temp.path().join("empty.yaml");
    fs::write(&empty, "").unwrap();

    fy().args(["format", "--dry-run", empty.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("1 unchanged"));

    fy().args(["format", "--dry-run"])
        .write_stdin("")
        .assert()
        .success()
        .stderr(predicate::str::contains("1 unchanged"));
}

#[test]
fn test_single_file_in_place_skips_write_when_already_formatted() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("clean.yaml");
    fs::write(&file, "key: value\n").unwrap();
    let before = fs::metadata(&file).unwrap().modified().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(50));

    fy().args(["format", "-i", file.to_str().unwrap()])
        .assert()
        .success();

    assert_eq!(fs::metadata(&file).unwrap().modified().unwrap(), before);
    assert_eq!(fs::read_to_string(&file).unwrap(), "key: value\n");
}

#[test]
fn test_empty_dir_without_mode_flag_fails_with_hint() {
    let temp = TempDir::new().unwrap();

    fy().args(["format", temp.path().to_str().unwrap()])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("--dry-run"));
}

#[test]
fn test_in_place_without_file_fails() {
    fy().args(["format", "-i"])
        .write_stdin("")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("requires a file argument"));
}

#[test]
fn test_in_place_without_subcommand_is_a_usage_error() {
    fy().arg("-i")
        .write_stdin("")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unexpected argument"));
}

#[test]
fn test_literal_bracket_filename_formats_in_place() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("a[1].yaml");
    fs::write(&file, "key:   value\n").unwrap();

    fy().args(["format", "-i", file.to_str().unwrap()])
        .assert()
        .success();

    assert_eq!(fs::read_to_string(&file).unwrap(), "key: value\n");
}

#[cfg(unix)]
#[test]
fn test_broken_symlink_path_fails() {
    let temp = TempDir::new().unwrap();
    let link = temp.path().join("link.yaml");
    std::os::unix::fs::symlink(temp.path().join("gone.yaml"), &link).unwrap();

    fy().args(["format", "--dry-run", link.to_str().unwrap()])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("broken symbolic link"));
}

#[test]
fn test_in_place_with_dry_run_leaves_file_untouched() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("messy.yaml");
    fs::write(&file, "key:   value\n").unwrap();

    fy().args(["format", "-i", "--dry-run", file.to_str().unwrap()])
        .assert()
        .code(5);

    assert_eq!(fs::read_to_string(&file).unwrap(), "key:   value\n");
}

#[test]
fn test_malformed_glob_fails() {
    fy().args(["format", "--dry-run", "a*["])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid glob pattern 'a*['"));
}

#[test]
fn test_include_exclude_without_paths_fail() {
    for flag in ["--include", "--exclude"] {
        fy().args(["format", "--dry-run", flag, "*.yaml"])
            .assert()
            .code(1)
            .stderr(predicate::str::contains(
                "batch options (--jobs, --include, --exclude) need input",
            ));
    }
}

const NO_FILES: &str = "no YAML files found";

fn write(dir: &Path, name: &str, content: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, content).unwrap();
    path
}

#[test]
fn test_batch_uppercase_extension_found_in_directory() {
    let temp = TempDir::new().unwrap();
    let upper = write(temp.path(), "UP.YAML", "a:  1\n");

    fy().args(["format", "-i", temp.path().to_str().unwrap()])
        .assert()
        .success();

    assert_eq!(fs::read_to_string(&upper).unwrap(), "a: 1\n");
}

#[test]
fn test_batch_user_include_is_case_insensitive() {
    let temp = TempDir::new().unwrap();
    let file = write(temp.path(), "a.YML", "a:  1\n");

    fy().args([
        "format",
        "-i",
        "--include",
        "*.yml",
        temp.path().to_str().unwrap(),
    ])
    .assert()
    .success();

    assert_eq!(fs::read_to_string(&file).unwrap(), "a: 1\n");
}

#[test]
fn test_batch_exclude_is_case_insensitive() {
    let temp = TempDir::new().unwrap();
    let secret = write(temp.path(), "KEY.YAML", "a:  1\n");
    let other = write(temp.path(), "ok.yaml", "b:  2\n");

    fy().args([
        "format",
        "-i",
        "--exclude",
        "**/key.yaml",
        temp.path().to_str().unwrap(),
    ])
    .assert()
    .success();

    assert_eq!(fs::read_to_string(&secret).unwrap(), "a:  1\n");
    assert_eq!(fs::read_to_string(&other).unwrap(), "b: 2\n");
}

#[test]
fn test_batch_exclude_matches_dot_slash_path() {
    let temp = TempDir::new().unwrap();
    let notes = write(temp.path(), "notes.txt", "a:  1\n");
    write(temp.path(), "ok.yaml", "b:  2\n");

    fy().current_dir(temp.path())
        .args([
            "format",
            "-i",
            "--exclude",
            "notes.txt",
            "./notes.txt",
            "ok.yaml",
        ])
        .assert()
        .success();

    assert_eq!(fs::read_to_string(&notes).unwrap(), "a:  1\n");
}

#[test]
fn test_batch_exclude_drops_explicit_file() {
    let temp = TempDir::new().unwrap();
    let skipped = write(temp.path(), "skip.yaml", "a:  1\n");
    let other = write(temp.path(), "ok.yaml", "b:  2\n");

    fy().args([
        "format",
        "-i",
        "--exclude",
        "**/skip.yaml",
        skipped.to_str().unwrap(),
        other.to_str().unwrap(),
    ])
    .assert()
    .success();

    assert_eq!(fs::read_to_string(&skipped).unwrap(), "a:  1\n");
    assert_eq!(fs::read_to_string(&other).unwrap(), "b: 2\n");
}

#[test]
fn test_batch_non_yaml_glob_fails() {
    let temp = TempDir::new().unwrap();
    write(temp.path(), "notes.txt", "a: 1\n");
    let pattern = temp.path().join("notes*");

    fy().args(["format", "--dry-run", pattern.to_str().unwrap()])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(NO_FILES));
}

#[test]
fn test_batch_mixed_empty_glob_and_file_succeeds() {
    let temp = TempDir::new().unwrap();
    write(temp.path(), "notes.txt", "a: 1\n");
    let ok = write(temp.path(), "ok.yaml", "b: 2\n");
    let pattern = temp.path().join("notes*");

    fy().args([
        "format",
        "--dry-run",
        pattern.to_str().unwrap(),
        ok.to_str().unwrap(),
    ])
    .assert()
    .success()
    .stderr(predicate::str::contains("Completed: 1 file"));
}

#[test]
fn test_batch_unmatched_glob_with_file_still_fails() {
    let temp = TempDir::new().unwrap();
    let ok = write(temp.path(), "ok.yaml", "b: 2\n");
    let pattern = temp.path().join("nomatch*");

    fy().args([
        "format",
        "--dry-run",
        pattern.to_str().unwrap(),
        ok.to_str().unwrap(),
    ])
    .assert()
    .code(1)
    .stderr(predicate::str::contains("glob pattern matched no files"));
}

#[test]
fn test_batch_exclude_all_fails() {
    let temp = TempDir::new().unwrap();
    write(temp.path(), "a.yaml", "a: 1\n");

    fy().args([
        "format",
        "--dry-run",
        "--exclude",
        "*.yaml",
        temp.path().to_str().unwrap(),
    ])
    .assert()
    .code(1)
    .stderr(predicate::str::contains(NO_FILES));
}

#[test]
fn test_batch_no_files_fails_even_when_quiet() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("empty");
    fs::create_dir(&dir).unwrap();

    fy().args(["-q", "format", "--dry-run", dir.to_str().unwrap()])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(NO_FILES));
}

#[cfg(unix)]
#[test]
fn test_batch_symlink_and_target_processed_once() {
    let temp = TempDir::new().unwrap();
    let target = write(temp.path(), "a.yaml", "a: 1\n");
    let link = temp.path().join("link.yaml");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    fy().args([
        "format",
        "--dry-run",
        target.to_str().unwrap(),
        link.to_str().unwrap(),
    ])
    .assert()
    .success()
    .stderr(predicate::str::contains("Completed: 1 file"));
}

#[test]
fn test_stdin_files_empty_list_succeeds() {
    fy().args(["format", "--dry-run", "--stdin-files"])
        .write_stdin("")
        .assert()
        .success();
}

#[test]
fn test_stdin_files_blank_and_comment_lines_succeed() {
    fy().args(["format", "--dry-run", "--stdin-files"])
        .write_stdin("\n   \r\n# comment\n")
        .assert()
        .success();
}

#[test]
fn test_stdin_files_crlf_and_padding() {
    let temp = TempDir::new().unwrap();
    let first = write(temp.path(), "a.yaml", "a:  1\n");
    let second = write(temp.path(), "b.yaml", "b:  2\n");

    fy().args(["format", "-i", "--stdin-files"])
        .write_stdin(format!(
            "\r\n  {}  \r\n\n{}\r\n",
            first.display(),
            second.display()
        ))
        .assert()
        .success();

    assert_eq!(fs::read_to_string(&first).unwrap(), "a: 1\n");
    assert_eq!(fs::read_to_string(&second).unwrap(), "b: 2\n");
}

#[test]
fn test_batch_explicit_uppercase_extension_is_accepted() {
    let temp = TempDir::new().unwrap();
    let upper = write(temp.path(), "UP.YAML", "a:  1\n");
    let lower = write(temp.path(), "m1.yaml", "b:  2\n");

    fy().args([
        "format",
        "-i",
        upper.to_str().unwrap(),
        lower.to_str().unwrap(),
    ])
    .assert()
    .success();

    assert_eq!(fs::read_to_string(&upper).unwrap(), "a: 1\n");
    assert_eq!(fs::read_to_string(&lower).unwrap(), "b: 2\n");
}

#[test]
fn test_stdin_files_exclude_all_fails() {
    let temp = TempDir::new().unwrap();
    let yaml = write(temp.path(), "a.yaml", "a: 1\n");

    fy().args([
        "format",
        "--dry-run",
        "--stdin-files",
        "--exclude",
        "*.yaml",
    ])
    .write_stdin(format!("{}\n", yaml.display()))
    .assert()
    .code(1)
    .stderr(predicate::str::contains(NO_FILES));
}
