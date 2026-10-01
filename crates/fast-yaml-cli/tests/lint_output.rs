//! `fy lint` output destination, input-error reports and closed pipes (#569, #575).

#![allow(clippy::missing_docs_in_private_items)]

use assert_cmd::Command;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tempfile::TempDir;

const DUPLICATE_KEY: &str = "a: 1\na: 2\n";
const FORMATS: [&str; 5] = ["text", "json", "github", "sarif", "parsable"];

fn fy(args: &[&str]) -> Command {
    let mut cmd = assert_cmd::cargo_bin_cmd!("fy");
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
            "lint",
            "-o",
            path_arg(&target),
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
            "lint",
            "-o",
            path_arg(&target),
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
        "lint",
        "-o",
        path_arg(&target),
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
    let mut child = std::process::Command::new(assert_cmd::cargo_bin!("fy"))
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

/// Runs `fy` with the chosen output streams closed on the reader side; returns the exit code.
fn run_closed(args: &[&str], close_stdout: bool, close_stderr: bool) -> Option<i32> {
    let mut child = std::process::Command::new(assert_cmd::cargo_bin!("fy"))
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if close_stdout {
        drop(child.stdout.take());
    }
    if close_stderr {
        drop(child.stderr.take());
    }
    child.wait_with_output().unwrap().status.code()
}

#[test]
fn closed_stderr_never_panics_and_keeps_the_exit_code() {
    let dir = TempDir::new().unwrap();
    let dup = fixture(&dir, "dup.yaml", DUPLICATE_KEY);
    fixture(&dir, "broken.yaml", "a: [\n");
    for format in FORMATS {
        for (close_stdout, close_stderr) in [(false, true), (true, true)] {
            let single = run_closed(
                &["-v", "lint", "--format", format, path_arg(&dup)],
                close_stdout,
                close_stderr,
            );
            assert_eq!(
                single,
                Some(2),
                "single {format} {close_stdout} {close_stderr}"
            );

            let batch = run_closed(
                &["lint", "--format", format, path_arg(dir.path())],
                close_stdout,
                close_stderr,
            );
            assert_eq!(
                batch,
                Some(2),
                "batch {format} {close_stdout} {close_stderr}"
            );
        }
    }
}

#[test]
fn closed_stderr_does_not_panic_when_reporting_an_error() {
    let dir = TempDir::new().unwrap();
    let missing = dir.path().join("missing.yaml");
    assert_eq!(
        run_closed(&["lint", path_arg(&missing)], false, true),
        Some(1)
    );
    assert_eq!(
        run_closed(&["format", path_arg(&missing)], true, true),
        Some(1)
    );
}

#[test]
fn output_flag_never_overwrites_an_input_file() {
    let dir = TempDir::new().unwrap();
    let input = fixture(&dir, "a.yaml", DUPLICATE_KEY);
    let other = fixture(&dir, "b.yaml", "k: 1\n");
    let path = path_arg(&input);
    for args in [
        vec!["lint", "-o", path, path],
        vec!["lint", "-o", path, path, path],
        vec!["lint", "-o", path, path, path_arg(&other)],
        vec!["lint", "-o", path, path_arg(dir.path())],
    ] {
        let output = fy(&args).output().unwrap();
        assert_eq!(output.status.code(), Some(1), "{args:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("also an input file"), "{args:?}: {stderr}");
        assert_eq!(
            fs::read_to_string(&input).unwrap(),
            DUPLICATE_KEY,
            "{args:?}"
        );
    }
}
