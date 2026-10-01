//! File names with terminal control sequences are escaped in every human-readable output (#607).

#![allow(clippy::missing_docs_in_private_items)]
#![cfg(unix)]

use std::fs;
use std::path::Path;
use std::process::Output;

use assert_cmd::cargo_bin_cmd;
use tempfile::TempDir;

const HOSTILE: &str = "x\u{1b}]0;PWNED\u{7}\u{1b}[2Jy";
const ESCAPED: &str = "x\\u{1b}]0;PWNED\\u{7}\\u{1b}[2Jy";

fn tree() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join(format!("{HOSTILE}.yaml")), "a: 1\na: 2\n").unwrap();
    fs::write(dir.path().join("b\u{1b}[31mz.yaml"), "a: [1\n").unwrap();
    dir
}

fn has_raw_control(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .any(|b| matches!(b, 0x00..=0x09 | 0x0b..=0x1f | 0x7f))
}

fn assert_no_raw_control(output: &Output, what: &str) {
    assert!(!has_raw_control(&output.stdout), "{what}: stdout");
    assert!(!has_raw_control(&output.stderr), "{what}: stderr");
}

fn run(args: &[&str], path: &Path) -> Output {
    cargo_bin_cmd!("fy").args(args).arg(path).output().unwrap()
}

#[test]
fn lint_text_names_the_file_with_escapes() {
    let dir = tree();
    let output = run(&["lint"], dir.path());
    assert_no_raw_control(&output, "text");
    assert!(String::from_utf8_lossy(&output.stdout).contains(ESCAPED));
}

#[test]
fn lint_parsable_names_the_file_with_escapes() {
    let dir = tree();
    let output = run(&["lint", "--format", "parsable"], dir.path());
    assert_no_raw_control(&output, "parsable");
    assert!(String::from_utf8_lossy(&output.stdout).contains(ESCAPED));
}

#[test]
fn lint_github_names_the_file_with_escapes() {
    let dir = tree();
    let output = run(&["lint", "--format", "github"], dir.path());
    assert_no_raw_control(&output, "github");
    assert!(String::from_utf8_lossy(&output.stdout).contains("file="));
}

#[test]
fn lint_json_and_sarif_keep_the_real_name_json_escaped() {
    let dir = tree();
    for format in ["json", "sarif"] {
        let output = run(&["lint", "--format", format], dir.path());
        assert_no_raw_control(&output, format);
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(report.to_string().contains("PWNED"), "{format}");
    }
}

#[test]
fn lint_single_file_verbose_header_is_escaped() {
    let dir = tree();
    let file = dir.path().join(format!("{HOSTILE}.yaml"));
    let output = run(&["lint", "--verbose"], &file);
    assert_no_raw_control(&output, "verbose");
    assert!(String::from_utf8_lossy(&output.stderr).contains(ESCAPED));
}

#[test]
fn format_batch_failure_line_is_escaped() {
    let dir = tree();
    let output = run(&["format", "-n"], dir.path());
    assert_no_raw_control(&output, "format");
    assert!(String::from_utf8_lossy(&output.stderr).contains("b\\u{1b}[31mz.yaml"));
}

#[test]
fn a_missing_path_is_escaped() {
    let dir = tree();
    let missing = dir.path().join("gone\u{1b}[2J.yaml");
    for command in ["lint", "format"] {
        let output = run(&[command], &missing);
        assert_no_raw_control(&output, command);
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("gone\\u{1b}[2J.yaml"),
            "{command}"
        );
    }
}

#[test]
fn unreadable_directory_warnings_are_escaped() {
    use std::os::unix::fs::PermissionsExt as _;

    let dir = TempDir::new().unwrap();
    let hostile = dir.path().join("d\u{1b}[2J\u{7}\u{202e}x");
    fs::create_dir(&hostile).unwrap();
    fs::write(hostile.join("a.yaml"), "a: 1\n").unwrap();
    fs::set_permissions(&hostile, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read_dir(&hostile).is_ok() {
        // running as root: the directory stays readable
        fs::set_permissions(&hostile, fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }

    let walk = run(&["lint"], dir.path());
    let pattern = dir.path().join("d*/*.yaml");
    let glob = cargo_bin_cmd!("fy")
        .arg("lint")
        .arg(&pattern)
        .output()
        .unwrap();
    fs::set_permissions(&hostile, fs::Permissions::from_mode(0o755)).unwrap();

    for (what, output) in [("walk", walk), ("glob", glob)] {
        assert_no_raw_control(&output, what);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!stderr.contains('\u{202e}'), "{what}");
        assert!(stderr.contains("d\\u{1b}[2J"), "{what}: {stderr}");
    }
}
