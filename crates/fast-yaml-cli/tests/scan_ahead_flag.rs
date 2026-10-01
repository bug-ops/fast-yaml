//! End-to-end tests for the `--max-scan-ahead` flag and the `max-scan-ahead` config key (#563).

#![allow(clippy::missing_docs_in_private_items)]

use assert_cmd::{Command, cargo_bin_cmd};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const HINT: &str = "raise with --max-scan-ahead";

/// A root flow sequence: tokenized whole before its first event, so it is what the limit bounds.
fn root_flow() -> String {
    format!("[{}1]\n", "1, ".repeat(100))
}

fn fy(args: &[&str]) -> Command {
    let mut cmd = cargo_bin_cmd!("fy");
    cmd.args(args);
    cmd
}

fn fixture(dir: &TempDir, name: &str, content: &str) -> PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, content).unwrap();
    path
}

fn run(args: &[&str], path: &Path) -> (Option<i32>, String) {
    let output = fy(args).arg(path).output().unwrap();
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn parse_convert_and_lint_reject_over_the_limit_and_hint_at_the_flag() {
    let dir = TempDir::new().unwrap();
    let path = fixture(&dir, "flow.yaml", &root_flow());
    for cmd in [&["parse"][..], &["lint"], &["convert", "json"]] {
        let mut low = cmd.to_vec();
        low.extend(["--max-scan-ahead", "64"]);
        let (code, stderr) = run(&low, &path);
        assert_eq!(code, Some(1), "{cmd:?}: {stderr}");
        assert!(stderr.contains(HINT), "{cmd:?}: {stderr}");
        assert!(stderr.contains("64 characters"), "{cmd:?}: {stderr}");

        let mut high = cmd.to_vec();
        high.extend(["--max-scan-ahead", "64KiB"]);
        let (code, stderr) = run(&high, &path);
        assert_eq!(code, Some(0), "{cmd:?}: {stderr}");
        assert!(!stderr.contains("lookahead"), "{cmd:?}: {stderr}");
    }
}

#[test]
fn default_limit_rejects_a_five_mib_root_flow_collection() {
    let dir = TempDir::new().unwrap();
    let path = fixture(&dir, "big.json", &format!("[{}1]", "1,".repeat(2_600_000)));
    let (code, stderr) = run(&["parse"], &path);
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.contains(HINT), "{stderr}");
    let (code, stderr) = run(&["parse", "--max-scan-ahead", "8MiB"], &path);
    assert_eq!(code, Some(0), "{stderr}");
}

#[test]
fn streaming_flow_values_are_never_rejected() {
    let dir = TempDir::new().unwrap();
    let path = fixture(&dir, "stream.yaml", &format!("a: {}", root_flow()));
    let (code, stderr) = run(&["parse", "--max-scan-ahead", "64"], &path);
    assert_eq!(code, Some(0), "{stderr}");
}

#[test]
fn format_applies_the_limit_to_stdin() {
    let rejected = fy(&["format", "--max-scan-ahead", "64"])
        .write_stdin(root_flow())
        .output()
        .unwrap();
    assert_eq!(rejected.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&rejected.stderr).contains(HINT));

    let accepted = fy(&["format", "--max-scan-ahead", "64KiB"])
        .write_stdin(root_flow())
        .output()
        .unwrap();
    assert_eq!(accepted.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&accepted.stdout).contains("- 1"));
}

#[test]
fn format_applies_the_limit_to_a_single_file() {
    let dir = TempDir::new().unwrap();
    let path = fixture(&dir, "flow.yaml", &root_flow());
    let (code, stderr) = run(&["format", "--max-scan-ahead", "64"], &path);
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.contains(HINT), "{stderr}");
    let (code, stderr) = run(&["format", "--max-scan-ahead", "64KiB"], &path);
    assert_eq!(code, Some(0), "{stderr}");
}

#[test]
fn format_applies_the_limit_to_batch_runs() {
    let dir = TempDir::new().unwrap();
    fixture(&dir, "flow.yaml", &root_flow());
    fixture(&dir, "block.yaml", "a: 1\n");
    let target = dir.path().to_str().unwrap();

    let rejected = fy(&["format", "--dry-run", "--max-scan-ahead", "64", target])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&rejected.stderr);
    assert_eq!(rejected.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("lookahead exceeds 64 characters"),
        "{stderr}"
    );

    let accepted = fy(&["format", "--dry-run", "--max-scan-ahead", "64KiB", target])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&accepted.stderr);
    assert!(accepted.status.code() == Some(5), "{stderr}");
    assert!(!stderr.contains("lookahead"), "{stderr}");
}

#[test]
fn lint_reads_the_limit_from_the_config_file_and_the_flag_overrides_it() {
    let dir = TempDir::new().unwrap();
    let config = fixture(&dir, ".fast-yaml.yaml", "max-scan-ahead: 64\n");
    let path = fixture(&dir, "flow.yaml", &root_flow());
    let config = config.to_str().unwrap();

    let (code, stderr) = run(&["lint", "--config", config], &path);
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.contains("max-scan-ahead config key"), "{stderr}");

    let (code, stderr) = run(
        &["lint", "--config", config, "--max-scan-ahead", "64KiB"],
        &path,
    );
    assert_eq!(code, Some(0), "{stderr}");
    assert!(!stderr.contains("lookahead"), "{stderr}");
}

#[test]
fn lint_report_formats_and_batch_honor_the_limit() {
    let dir = TempDir::new().unwrap();
    let path = fixture(&dir, "flow.yaml", &root_flow());
    for format in ["github", "sarif", "parsable"] {
        let output = fy(&["lint", "--format", format, "--max-scan-ahead", "64"])
            .arg(&path)
            .output()
            .unwrap();
        let all = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(1), "{format}: {all}");
        assert!(all.contains("lookahead exceeds 64"), "{format}: {all}");

        let raised = fy(&["lint", "--format", format, "--max-scan-ahead", "64KiB"])
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(raised.status.code(), Some(0), "{format}");
    }
    let target = dir.path().to_str().unwrap();
    let batch = fy(&[
        "lint",
        "--format",
        "github",
        "--max-scan-ahead",
        "64",
        target,
    ])
    .output()
    .unwrap();
    assert_eq!(batch.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&batch.stdout).contains("lookahead exceeds 64"));
}

#[test]
fn out_of_range_values_are_usage_errors() {
    let dir = TempDir::new().unwrap();
    let path = fixture(&dir, "a.yaml", "a: 1\n");
    for value in ["0", "2GiB", "1.5MiB", "-1", "MiB"] {
        let (code, stderr) = run(&["parse", "--max-scan-ahead", value], &path);
        assert_eq!(code, Some(2), "{value}: {stderr}");
    }
}

#[test]
fn help_documents_the_flag() {
    for cmd in ["parse", "format", "convert", "lint"] {
        fy(&[cmd, "--help"])
            .assert()
            .success()
            .stdout(predicates::str::contains("--max-scan-ahead"));
    }
}
