//! `fy format` and `fy convert` end quietly when the reader of stdout goes away.

#![allow(clippy::missing_docs_in_private_items)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::process::{Command, Output, Stdio};

use assert_cmd::cargo::cargo_bin;
use tempfile::TempDir;

fn big_yaml(dir: &TempDir) -> PathBuf {
    let path = dir.path().join("big.yaml");
    let mut file = std::fs::File::create(&path).unwrap();
    for i in 0..20_000 {
        writeln!(file, "key{i}: value{i}").unwrap();
    }
    path
}

fn run_with_closed_stdout(args: &[&str], file: &Path) -> Output {
    let mut child = Command::new(cargo_bin("fy"))
        .args(args)
        .arg(file)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    child.wait_with_output().unwrap()
}

#[test]
fn format_with_closed_stdout_is_quiet_and_succeeds() {
    let dir = TempDir::new().unwrap();
    let out = run_with_closed_stdout(&["format"], &big_yaml(&dir));
    assert!(out.status.success(), "{out:?}");
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn convert_with_closed_stdout_is_quiet_and_succeeds() {
    let dir = TempDir::new().unwrap();
    let out = run_with_closed_stdout(&["convert", "json"], &big_yaml(&dir));
    assert!(out.status.success(), "{out:?}");
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn invalid_input_still_fails_with_closed_stdout() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bad.yaml");
    std::fs::write(&path, "a: [\n").unwrap();
    let out = run_with_closed_stdout(&["format"], &path);
    assert!(!out.status.success(), "{out:?}");
}

fn trailing_space_yaml(dir: &TempDir) -> PathBuf {
    let path = dir.path().join("noisy.yaml");
    let mut file = std::fs::File::create(&path).unwrap();
    for i in 0..3_000 {
        writeln!(file, "key{i}:   value{i}   ").unwrap();
    }
    path
}

fn run_closing(args: &[&str], stdin: Option<&Path>, close_stderr: bool) -> Output {
    let mut command = Command::new(cargo_bin("fy"));
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match stdin {
        Some(path) => command.stdin(std::fs::File::open(path).unwrap()),
        None => command.stdin(Stdio::null()),
    };
    let mut child: Child = command.spawn().unwrap();
    drop(child.stdout.take());
    if close_stderr {
        drop(child.stderr.take());
    }
    child.wait_with_output().unwrap()
}

fn run_open(args: &[&str], stdin: Option<&Path>) -> Output {
    let mut command = Command::new(cargo_bin("fy"));
    command.args(args);
    match stdin {
        Some(path) => command.stdin(std::fs::File::open(path).unwrap()),
        None => command.stdin(Stdio::null()),
    };
    command.output().unwrap()
}

#[test]
fn lint_formats_with_closed_stdout_keep_exit_code_and_stay_quiet() {
    let dir = TempDir::new().unwrap();
    let path = trailing_space_yaml(&dir);
    let file = path.to_str().unwrap();
    for format in ["json", "sarif", "github"] {
        let args = ["lint", "--format", format, file];
        let expected = run_open(&args, None).status.code();
        let out = run_closing(&args, None, false);
        assert_eq!(out.status.code(), expected, "{format}: {out:?}");
        assert!(
            out.stderr.is_empty(),
            "{format}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn stdin_input_with_closed_stdout_is_quiet_and_succeeds() {
    let dir = TempDir::new().unwrap();
    let path = big_yaml(&dir);
    for args in [&["format"][..], &["convert", "json"][..]] {
        let out = run_closing(args, Some(&path), false);
        assert!(out.status.success(), "{args:?}: {out:?}");
        assert!(out.stderr.is_empty(), "{args:?}");
    }
}

#[test]
fn format_dry_run_keeps_would_change_exit_code_with_closed_stdout_and_stderr() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("loose.yaml");
    std::fs::write(&path, "a:    1\n").unwrap();
    let out = run_closing(&["format", "--dry-run", path.to_str().unwrap()], None, true);
    assert_eq!(out.status.code(), Some(5), "{out:?}");
}

#[cfg(unix)]
fn set_xattr(path: &Path, name: &str, value: &str) -> bool {
    let status = if cfg!(target_os = "linux") {
        Command::new("setfattr")
            .args(["-n", name, "-v", value])
            .arg(path)
            .status()
    } else {
        Command::new("xattr")
            .args(["-w", name, value])
            .arg(path)
            .status()
    };
    status.is_ok_and(|s| s.success())
}

#[cfg(unix)]
fn get_xattr(path: &Path, name: &str) -> Option<String> {
    let output = if cfg!(target_os = "linux") {
        Command::new("getfattr")
            .args(["--only-values", "-n", name])
            .arg(path)
            .output()
    } else {
        Command::new("xattr").args(["-p", name]).arg(path).output()
    }
    .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[cfg(unix)]
#[test]
fn in_place_format_preserves_an_extended_attribute() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("tagged.yaml");
    std::fs::write(&path, "a:    1\n").unwrap();
    let name = if cfg!(target_os = "linux") {
        "user.fy.test"
    } else {
        "com.fy.test"
    };
    if !set_xattr(&path, name, "kept") || get_xattr(&path, name).as_deref() != Some("kept") {
        eprintln!("skipped: no xattr tool or the file system refuses extended attributes");
        return;
    }
    let out = Command::new(cargo_bin("fy"))
        .args(["format", "-i"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "a: 1\n");
    assert_eq!(get_xattr(&path, name).as_deref(), Some("kept"));
}
