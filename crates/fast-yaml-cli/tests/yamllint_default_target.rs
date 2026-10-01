//! `.yamllint` is a default target of `fy lint` only (#571).

#![allow(clippy::missing_docs_in_private_items)]
#![allow(deprecated)] // Command::cargo_bin is deprecated but still works

use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

const CONFIG: &str = "# lint config\nrules:\n  line-length: {max: 5}\n";

fn repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.yaml"), "a: 1\n").unwrap();
    fs::write(dir.path().join(".yamllint"), CONFIG).unwrap();
    dir
}

fn fy(args: &[&str], dir: &TempDir) -> std::process::Output {
    Command::cargo_bin("fy")
        .unwrap()
        .args(args)
        .arg(dir.path())
        .output()
        .unwrap()
}

#[test]
fn lint_visits_the_yamllint_file() {
    let dir = repo();
    fs::write(dir.path().join(".yamllint"), "a: 1\na: 2\n").unwrap();
    let output = fy(&["lint", "--no-config", "--format", "parsable"], &dir);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(2), "{stdout}");
    assert!(stdout.contains(".yamllint:2:1"), "{stdout}");
}

#[test]
fn format_ignores_the_yamllint_file() {
    let dir = repo();
    let output = fy(&["format", "-i"], &dir);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        fs::read_to_string(dir.path().join(".yamllint")).unwrap(),
        CONFIG
    );
    let dry = fy(&["format", "--dry-run"], &dir);
    assert_eq!(dry.status.code(), Some(0), "{dry:?}");
}
