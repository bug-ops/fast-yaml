//! `fy lint` output destination, input-error reports and closed pipes (#569, #575).

#![allow(clippy::missing_docs_in_private_items)]
#![allow(deprecated)] // cargo_bin is deprecated but still works

use assert_cmd::Command;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tempfile::TempDir;

const DUPLICATE_KEY: &str = "a: 1\na: 2\n";
const FORMATS: [&str; 5] = ["text", "json", "github", "sarif", "parsable"];

fn fy(args: &[&str]) -> Command {
    let mut cmd = Command::cargo_bin("fy").unwrap();
    cmd.args(args);
    cmd
}

fn fixture(dir: &TempDir, name: &str, content: &str) -> PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, content).unwrap();
    path
}

fn path_arg(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn output_flag_receives_the_report_in_every_format() {
    let dir = TempDir::new().unwrap();
    let input = fixture(&dir, "dup.yaml", DUPLICATE_KEY);
    for format in FORMATS {
        let args = ["lint", "--format", format, path_arg(&input)];
        let printed = fy(&args).output().unwrap();
        assert_eq!(printed.status.code(), Some(2), "{format}");

        let target = dir.path().join(format!("report.{format}"));
        let redirected = fy(&[
            "-o",
            path_arg(&target),
            "lint",
            "--format",
            format,
            path_arg(&input),
        ])
        .output()
        .unwrap();
        assert_eq!(redirected.status.code(), Some(2), "{format}");
        assert!(redirected.stdout.is_empty(), "{format}");
        assert_eq!(fs::read(&target).unwrap(), printed.stdout, "{format}");
    }
}

#[test]
fn output_flag_receives_batch_reports() {
    let dir = TempDir::new().unwrap();
    fixture(&dir, "a.yaml", DUPLICATE_KEY);
    fixture(&dir, "b.yaml", "k: 1\n");
    for format in FORMATS {
        let printed = fy(&["lint", "--format", format, path_arg(dir.path())])
            .output()
            .unwrap();
        assert_eq!(printed.status.code(), Some(2), "{format}");

        let out = TempDir::new().unwrap();
        let target = out.path().join("report");
        let redirected = fy(&[
            "-o",
            path_arg(&target),
            "lint",
            "--format",
            format,
            path_arg(dir.path()),
        ])
        .output()
        .unwrap();
        assert_eq!(redirected.status.code(), Some(2), "{format}");
        assert!(redirected.stdout.is_empty(), "{format}");
        assert_eq!(fs::read(&target).unwrap(), printed.stdout, "{format}");
    }
}

#[test]
fn missing_path_still_yields_a_report_in_report_formats() {
    let dir = TempDir::new().unwrap();
    let missing = dir.path().join("missing.yaml");
    for format in ["github", "sarif", "parsable"] {
        let output = fy(&["lint", "--format", format, path_arg(&missing)])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{format}");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("path does not exist"), "{format}: {stdout}");
        assert!(stdout.contains("missing.yaml"), "{format}: {stdout}");
    }
    let text = fy(&["lint", path_arg(&missing)]).output().unwrap();
    assert_eq!(text.status.code(), Some(1));
    assert_eq!(text.stdout, Vec::<u8>::new());
}

#[test]
fn missing_path_report_goes_to_the_output_file() {
    let dir = TempDir::new().unwrap();
    let missing = dir.path().join("missing.yaml");
    let target = dir.path().join("report.sarif");
    let output = fy(&[
        "-o",
        path_arg(&target),
        "lint",
        "--format",
        "sarif",
        path_arg(&missing),
    ])
    .output()
    .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, Vec::<u8>::new());
    let report: serde_json::Value = serde_json::from_slice(&fs::read(&target).unwrap()).unwrap();
    assert_eq!(report["version"], "2.1.0");
}

/// Runs `fy` with stdout already closed on the reader side and returns its exit code and stderr.
fn run_with_closed_stdout(args: &[&str]) -> (Option<i32>, String) {
    let mut child = std::process::Command::new(assert_cmd::cargo::cargo_bin("fy"))
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let output = child.wait_with_output().unwrap();
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn closed_stdout_is_silent_and_keeps_the_diagnostic_exit_code() {
    let dir = TempDir::new().unwrap();
    let input = fixture(&dir, "dup.yaml", DUPLICATE_KEY);
    let clean = fixture(&dir, "clean.yaml", "k: 1\n");
    for format in FORMATS {
        let (code, stderr) =
            run_with_closed_stdout(&["lint", "--format", format, path_arg(&input)]);
        assert_eq!(code, Some(2), "{format}: {stderr}");
        assert!(stderr.is_empty(), "{format}: {stderr}");

        let (code, stderr) =
            run_with_closed_stdout(&["lint", "--format", format, path_arg(&clean)]);
        assert_eq!(code, Some(0), "{format}: {stderr}");
        assert!(stderr.is_empty(), "{format}: {stderr}");
    }
}

#[test]
fn closed_stdout_is_silent_in_batch_runs() {
    let dir = TempDir::new().unwrap();
    fixture(&dir, "a.yaml", DUPLICATE_KEY);
    fixture(&dir, "b.yaml", DUPLICATE_KEY);
    for format in FORMATS {
        let (code, stderr) =
            run_with_closed_stdout(&["lint", "--format", format, path_arg(dir.path())]);
        assert_eq!(code, Some(2), "{format}: {stderr}");
        assert!(stderr.is_empty(), "{format}: {stderr}");
    }
}
