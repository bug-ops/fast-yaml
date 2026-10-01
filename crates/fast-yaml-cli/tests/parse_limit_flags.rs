//! End-to-end tests for the `--max-depth` and `--max-alias-bytes` flags.

#![allow(clippy::missing_docs_in_private_items)]
#![allow(deprecated)] // Command::cargo_bin is deprecated but still works

use assert_cmd::Command;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const SMALL_ALIAS_YAML: &str = "- &a [x, x, x, x, x, x, x, x, x, x]\n- *a\n";

fn write_fixture(dir: &TempDir, name: &str, content: &str) -> PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, content).unwrap();
    path
}

fn run(args: &[&str], path: &Path) -> (Option<i32>, String) {
    let output = Command::cargo_bin("fy")
        .unwrap()
        .args(args)
        .arg(path)
        .output()
        .unwrap();
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn with_flag<'a>(cmd: &[&'a str], flag: &'a str, value: &'a str) -> Vec<&'a str> {
    let mut args = cmd.to_vec();
    args.extend([flag, value]);
    args
}

const SUBCOMMANDS: [&[&str]; 3] = [&["parse"], &["lint"], &["convert", "json"]];

#[test]
fn max_depth_flag_lowers_and_raises_the_limit() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "nested.yaml", "[[[1]]]\n");
    for cmd in SUBCOMMANDS {
        let (code, stderr) = run(&with_flag(cmd, "--max-depth", "2"), &path);
        assert_eq!(code, Some(1), "{cmd:?}: {stderr}");
        assert!(stderr.contains("raise with --max-depth"), "{stderr}");

        let (code, stderr) = run(&with_flag(cmd, "--max-depth", "3"), &path);
        assert!(matches!(code, Some(0 | 2)), "{cmd:?}: {stderr}");
        assert!(!stderr.contains("limit exceeded"), "{stderr}");
    }
}

#[test]
fn default_depth_failure_hints_at_max_depth() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "deep.yaml", &format!("{}x\n", "- ".repeat(20_000)));
    let (code, stderr) = run(&["parse"], &path);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("raise with --max-depth"), "{stderr}");
}

#[test]
fn max_alias_bytes_flag_accepts_suffixes_and_hints() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "alias.yaml", SMALL_ALIAS_YAML);
    for cmd in SUBCOMMANDS {
        let (code, stderr) = run(&with_flag(cmd, "--max-alias-bytes", "256"), &path);
        assert_eq!(code, Some(1), "{cmd:?}: {stderr}");
        assert!(stderr.contains("raise with --max-alias-bytes"), "{stderr}");

        for value in ["4096", "4KiB", "1MiB", "1GiB"] {
            let (code, stderr) = run(&with_flag(cmd, "--max-alias-bytes", value), &path);
            assert!(matches!(code, Some(0 | 2)), "{cmd:?} {value}: {stderr}");
            assert!(!stderr.contains("limit exceeded"), "{stderr}");
        }
    }
}

#[test]
fn lint_batch_failure_hints_at_flag() {
    let dir = TempDir::new().unwrap();
    write_fixture(&dir, "a.yaml", SMALL_ALIAS_YAML);
    write_fixture(&dir, "b.yaml", "k: v\n");
    write_fixture(&dir, "c.yaml", "[[[1]]]\n");
    let output = Command::cargo_bin("fy")
        .unwrap()
        .args(["lint", "--max-alias-bytes", "256", "--max-depth", "2"])
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(output.status.code().is_some_and(|c| c != 0));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("raise with --max-alias-bytes"), "{stderr}");
    assert!(stderr.contains("raise with --max-depth"), "{stderr}");
}

#[test]
fn tag_limit_failure_has_no_raise_hint() {
    let dir = TempDir::new().unwrap();
    let mut yaml = format!("%TAG !e! tag:e.com,{}\n---\n", "a".repeat(100_000));
    for i in 0..1_000 {
        writeln!(yaml, "k{i}: !e!x v").unwrap();
    }
    let path = write_fixture(&dir, "tagprefix.yaml", &yaml);
    let (_, stderr) = run(&["parse"], &path);
    assert!(stderr.contains("limit exceeded"), "{stderr}");
    assert!(!stderr.contains("raise with"), "{stderr}");
}

#[test]
fn out_of_range_limit_values_are_rejected() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "ok.yaml", "a: 1\n");
    for (flag, value) in [
        ("--max-depth", "0"),
        ("--max-depth", "513"),
        ("--max-depth", "-1"),
        ("--max-depth", "abc"),
        ("--max-alias-bytes", "0"),
        ("--max-alias-bytes", "2GiB"),
        ("--max-alias-bytes", "1073741825"),
        ("--max-alias-bytes", "1.5MiB"),
        ("--max-alias-bytes", "99999999999999999999GiB"),
    ] {
        let joined = format!("{flag}={value}");
        let (code, stderr) = run(&["parse", &joined], &path);
        assert_eq!(code, Some(2), "{flag} {value}: {stderr}");
        assert!(stderr.contains(flag), "{flag} {value}: {stderr}");
    }

    let (_, stderr) = run(&["parse", "--max-depth=513"], &path);
    assert!(
        stderr.contains("must be between 1 and 512, got 513"),
        "{stderr}"
    );
    let (_, stderr) = run(&["parse", "--max-alias-bytes=2GiB"], &path);
    assert!(
        stderr.contains("must be between 1 and 1073741824, got 2147483648"),
        "{stderr}"
    );
}

#[test]
fn boundary_limit_values_are_accepted() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "ok.yaml", "a: 1\n");
    for (flag, value) in [
        ("--max-depth", "1"),
        ("--max-depth", "512"),
        ("--max-alias-bytes", "1"),
        ("--max-alias-bytes", "1GiB"),
    ] {
        for cmd in SUBCOMMANDS {
            let (code, stderr) = run(&with_flag(cmd, flag, value), &path);
            assert!(
                matches!(code, Some(0 | 2)),
                "{cmd:?} {flag} {value}: {stderr}"
            );
            assert!(
                !stderr.contains("error"),
                "{cmd:?} {flag} {value}: {stderr}"
            );
        }
    }
}

#[test]
fn convert_to_yaml_ignores_parse_limits() {
    Command::cargo_bin("fy")
        .unwrap()
        .args(["convert", "yaml", "--max-depth", "1"])
        .write_stdin("{\"a\": {\"b\": {\"c\": 1}}}")
        .assert()
        .success();
}

#[test]
fn format_accepts_max_depth_but_not_alias_limits() {
    Command::cargo_bin("fy")
        .unwrap()
        .args(["format", "--max-alias-bytes", "256"])
        .assert()
        .code(2);
    Command::cargo_bin("fy")
        .unwrap()
        .args(["format", "--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("--max-depth"));
}

#[test]
fn limit_flags_are_documented_in_help() {
    for sub in ["parse", "convert", "lint"] {
        Command::cargo_bin("fy")
            .unwrap()
            .args([sub, "--help"])
            .assert()
            .success()
            .stdout(predicates::str::contains("--max-depth"))
            .stdout(predicates::str::contains("--max-alias-bytes"));
    }
}
