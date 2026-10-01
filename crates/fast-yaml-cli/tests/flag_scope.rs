//! Write flags (`-o`, `-i`) exist only on the subcommands that write, and `-f` is gone (#611).

#![allow(clippy::missing_docs_in_private_items)]

use assert_cmd::{Command, cargo_bin_cmd};
use std::fs;
use tempfile::TempDir;

fn fy(args: &[&str]) -> Command {
    let mut cmd = cargo_bin_cmd!("fy");
    cmd.args(args);
    cmd
}

fn usage_error(args: &[&str]) {
    fy(args).write_stdin("a: 1\n").assert().code(2);
}

#[test]
fn format_flag_is_removed() {
    usage_error(&["-f", "json"]);
    usage_error(&["--format", "json", "parse"]);
}

#[test]
fn parse_rejects_write_flags_without_touching_files() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("a.yaml");
    fs::write(&input, "a:    1\n").unwrap();
    let out = dir.path().join("out.yaml");
    let (input, out_arg) = (input.to_str().unwrap(), out.to_str().unwrap());

    usage_error(&["parse", "-o", out_arg, input]);
    usage_error(&["parse", "-i", input]);
    assert!(!out.exists());
    assert_eq!(fs::read_to_string(input).unwrap(), "a:    1\n");
}

#[test]
fn write_flags_before_the_subcommand_are_rejected() {
    let dir = TempDir::new().unwrap();
    let out = dir.path().join("out.yaml");
    let out_arg = out.to_str().unwrap();

    usage_error(&["-o", out_arg, "format"]);
    usage_error(&["-o", out_arg]);
    usage_error(&["-i"]);
    #[cfg(feature = "linter")]
    usage_error(&["-o", out_arg, "lint"]);
    assert!(!out.exists());
}

#[test]
fn convert_rejects_in_place_with_output() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("a.yaml");
    fs::write(&input, "a: 1\n").unwrap();
    let out = dir.path().join("out.json");

    usage_error(&[
        "convert",
        "json",
        "-i",
        "-o",
        out.to_str().unwrap(),
        input.to_str().unwrap(),
    ]);
    assert!(!out.exists());
    assert_eq!(fs::read_to_string(&input).unwrap(), "a: 1\n");
}

#[test]
fn convert_and_format_still_write_output_files() {
    let dir = TempDir::new().unwrap();
    let json = dir.path().join("out.json");
    let yaml = dir.path().join("out.yaml");

    fy(&["convert", "json", "-o", json.to_str().unwrap()])
        .write_stdin("a: 1\n")
        .assert()
        .success();
    fy(&["format", "-o", yaml.to_str().unwrap()])
        .write_stdin("a:   1\n")
        .assert()
        .success();
    assert!(fs::read_to_string(json).unwrap().contains("\"a\""));
    assert_eq!(fs::read_to_string(yaml).unwrap(), "a: 1\n");
}

#[test]
fn bare_fy_formats_stdin_to_stdout() {
    fy(&[])
        .write_stdin("a:   1\n")
        .assert()
        .success()
        .stdout("a: 1\n");
}

#[test]
fn dry_run_wins_over_in_place() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("a.yaml");
    fs::write(&input, "a:    1\n").unwrap();

    fy(&["format", "-n", "-i", input.to_str().unwrap()])
        .assert()
        .code(5);
    assert_eq!(fs::read_to_string(&input).unwrap(), "a:    1\n");
}

#[cfg(feature = "linter")]
#[test]
fn lint_has_output_but_no_in_place() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("a.yaml");
    fs::write(&input, "a: 1\n").unwrap();
    let report = dir.path().join("report.txt");

    usage_error(&["lint", "-i", input.to_str().unwrap()]);
    fy(&[
        "lint",
        "--no-config",
        "-o",
        report.to_str().unwrap(),
        input.to_str().unwrap(),
    ])
    .assert()
    .success();
    assert!(report.exists());
}
