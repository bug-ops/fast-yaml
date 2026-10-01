//! Batch runs scale the default scan-ahead limit per worker and retry rejected files at the full
//! limit, so the result never depends on `-j` or the machine (#577).

#![allow(clippy::missing_docs_in_private_items)]
#![allow(deprecated)] // Command::cargo_bin is deprecated but still works

use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

/// A root flow sequence of about 1.8 Mi characters: over the 1 Mi scaled limit of a wide batch,
/// under the 4 Mi default.
fn between_limits() -> String {
    format!("[{}1]\n", "1, ".repeat(600_000))
}

fn batch_dir(files: usize) -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("adversarial.yaml"), between_limits()).unwrap();
    for i in 0..files {
        fs::write(
            dir.path().join(format!("small{i}.yaml")),
            format!("k: {i}\n"),
        )
        .unwrap();
    }
    dir
}

fn run(args: &[&str], dir: &TempDir) -> (Option<i32>, String, String) {
    let output = Command::cargo_bin("fy")
        .unwrap()
        .args(args)
        .arg(dir.path())
        .output()
        .unwrap();
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn lint_batch_result_is_the_same_for_every_worker_count() {
    let dir = batch_dir(6);
    let single = run(&["lint", "-j", "1"], &dir);
    assert_eq!(single.0, Some(0), "{}", single.2);
    for jobs in ["2", "4", "16"] {
        let result = run(&["lint", "-j", jobs], &dir);
        assert_eq!(result, single, "-j {jobs}");
    }
}

#[test]
fn format_batch_result_is_the_same_for_every_worker_count() {
    let dir = batch_dir(6);
    let single = run(&["format", "--dry-run", "-j", "1"], &dir);
    assert!(!single.2.contains("lookahead"), "{}", single.2);
    for jobs in ["4", "16"] {
        let (code, _, stderr) = run(&["format", "--dry-run", "-j", jobs], &dir);
        assert_eq!(code, single.0, "-j {jobs}: {stderr}");
        assert!(!stderr.contains("lookahead"), "-j {jobs}: {stderr}");
    }
}

#[test]
fn explicit_limit_is_neither_scaled_nor_retried() {
    let dir = batch_dir(6);
    let (code, _, stderr) = run(&["lint", "-j", "4", "--max-scan-ahead", "1MiB"], &dir);
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stderr.contains("lookahead exceeds 1048576"), "{stderr}");

    let (code, _, stderr) = run(&["lint", "-j", "4", "--max-scan-ahead", "4MiB"], &dir);
    assert_eq!(code, Some(0), "{stderr}");
}

#[test]
fn a_file_over_the_full_limit_still_fails_in_a_scaled_batch() {
    let dir = TempDir::new().unwrap();
    let over = format!("[{}1]\n", "1, ".repeat(1_500_000));
    fs::write(dir.path().join("over.yaml"), over).unwrap();
    for i in 0..4 {
        fs::write(dir.path().join(format!("s{i}.yaml")), "k: 1\n").unwrap();
    }
    let (code, _, stderr) = run(&["lint", "-j", "8"], &dir);
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stderr.contains("lookahead exceeds 4194304"), "{stderr}");
}
