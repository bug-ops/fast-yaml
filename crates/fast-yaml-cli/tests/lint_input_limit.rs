//! End-to-end tests for `--max-input-bytes`, the `max-input-bytes` config key and the
//! single-file lint error message (#509, #519, #508).

#![allow(clippy::missing_docs_in_private_items)]

use assert_cmd::{Command, cargo_bin_cmd};
use predicates::prelude::*;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

const TWENTY_BYTES: &str = "key: 0123456789abc\n";

fn fy() -> Command {
    cargo_bin_cmd!("fy")
}

fn run(args: &[&str], path: &Path) -> (Option<i32>, String) {
    let output = fy().args(args).arg(path).output().unwrap();
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn max_input_bytes_flag_rejects_with_hint() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("big.yaml");
    fs::write(&path, TWENTY_BYTES).unwrap();

    let (code, stderr) = run(&["lint", "--no-config", "--max-input-bytes", "10"], &path);
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.contains("raise with --max-input-bytes"), "{stderr}");
    assert!(stderr.contains("max-input-bytes config key"), "{stderr}");

    let (code, stderr) = run(&["lint", "--no-config", "--max-input-bytes", "1KiB"], &path);
    assert_eq!(code, Some(0), "{stderr}");
}

#[test]
fn limit_boundary_is_inclusive() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("exact.yaml");
    fs::write(&path, TWENTY_BYTES).unwrap();
    let size = TWENTY_BYTES.len();

    let exact = size.to_string();
    let (code, stderr) = run(&["lint", "--no-config", "--max-input-bytes", &exact], &path);
    assert_eq!(code, Some(0), "{stderr}");
    let below = (size - 1).to_string();
    let (code, stderr) = run(&["lint", "--no-config", "--max-input-bytes", &below], &path);
    assert_eq!(code, Some(1), "{stderr}");
}

#[test]
fn stdin_over_the_limit_is_rejected_and_at_the_limit_passes() {
    let size = TWENTY_BYTES.len();
    fy().args([
        "lint",
        "--no-config",
        "--max-input-bytes",
        &(size - 1).to_string(),
    ])
    .write_stdin(TWENTY_BYTES)
    .assert()
    .code(1)
    .stderr(predicate::str::contains("raise with --max-input-bytes"));
    fy().args([
        "lint",
        "--no-config",
        "--max-input-bytes",
        &size.to_string(),
    ])
    .write_stdin(TWENTY_BYTES)
    .assert()
    .code(0);
}

#[test]
fn large_file_over_the_limit_is_rejected_from_metadata() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("sparse.yaml");
    let file = fs::File::create(&path).unwrap();
    file.set_len(64 << 20).unwrap();
    let (code, stderr) = run(&["lint", "--no-config", "--max-input-bytes", "1KiB"], &path);
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.contains("raise with --max-input-bytes"), "{stderr}");
}

#[test]
fn limit_at_the_maximum_shows_no_hint() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("a.yaml");
    fs::write(&path, "a: [1\n").unwrap();
    let (code, stderr) = run(&["lint", "--no-config", "--max-input-bytes", "1GiB"], &path);
    assert_eq!(code, Some(1), "{stderr}");
    assert!(!stderr.contains("max-input-bytes"), "{stderr}");
}

#[test]
fn default_lint_has_no_input_limit_hint() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("ok.yaml");
    fs::write(&path, TWENTY_BYTES).unwrap();
    let (code, stderr) = run(&["lint", "--no-config"], &path);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(!stderr.contains("max-input-bytes"), "{stderr}");
}

#[test]
fn max_input_bytes_rejects_invalid_flag_values() {
    for value in ["0", "2GiB", "abc"] {
        fy().args(["lint", "--max-input-bytes", value])
            .assert()
            .code(2)
            .stderr(predicate::str::contains("--max-input-bytes"));
    }
}

#[test]
fn config_key_sets_limit_and_flag_overrides_it() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("big.yaml");
    fs::write(&path, TWENTY_BYTES).unwrap();
    let config = dir.path().join("cfg.yaml");
    fs::write(&config, "max-input-bytes: 10\n").unwrap();
    let config = config.to_str().unwrap();

    let (code, stderr) = run(&["lint", "--config", config], &path);
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.contains("raise with --max-input-bytes"), "{stderr}");

    let (code, stderr) = run(
        &["lint", "--config", config, "--max-input-bytes", "1KiB"],
        &path,
    );
    assert_eq!(code, Some(0), "{stderr}");
}

#[test]
fn config_key_rejects_invalid_values_without_hint() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("a.yaml");
    fs::write(&path, "a: 1\n").unwrap();
    for text in ["max-input-bytes: 1MiB\n", "max-input-bytes: 0\n"] {
        let config = dir.path().join("cfg.yaml");
        fs::write(&config, text).unwrap();
        let (code, stderr) = run(&["lint", "--config", config.to_str().unwrap()], &path);
        assert_eq!(code, Some(1), "{text}: {stderr}");
        assert!(stderr.contains("max-input-bytes"), "{stderr}");
        assert!(!stderr.contains("hint:"), "{stderr}");
    }
}

#[test]
fn batch_lint_rejects_oversized_file_with_path_and_hint() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("big.yaml"), TWENTY_BYTES).unwrap();
    fs::write(dir.path().join("small.yaml"), "---\na: 1\n").unwrap();

    let output = fy()
        .args(["lint", "--no-config", "--max-input-bytes", "12"])
        .arg(dir.path())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert_eq!(stderr.matches("big.yaml").count(), 1, "{stderr}");
    assert!(stderr.contains("raise with --max-input-bytes"), "{stderr}");
    assert!(!stderr.contains("small.yaml"), "{stderr}");
}

#[test]
fn single_file_lint_prints_parse_error_once() {
    let dir = TempDir::new().unwrap();
    let merge = dir.path().join("merge.yaml");
    fs::write(&merge, "m:\n  <<: 1\n  k: 0\n").unwrap();
    let scanner = dir.path().join("scanner.yaml");
    fs::write(&scanner, "a: [1\n").unwrap();

    for path in [&merge, &scanner] {
        let (code, stderr) = run(&["lint", "--no-config"], path);
        assert_eq!(code, Some(1), "{stderr}");
        assert!(!stderr.contains("failed to parse YAML"), "{stderr}");
        let causes = stderr.lines().filter(|l| l.contains("caused by")).count();
        assert_eq!(causes, 1, "{stderr}");
    }
}

#[test]
fn parse_stats_count_keys_of_a_set() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("set.yaml");
    fs::write(&path, "s: !!set {a, b}\n").unwrap();
    fy().args(["parse", "--stats"])
        .arg(&path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Keys: 3"));
}
