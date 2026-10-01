//! End-to-end tests for the `github`, `parsable` and `sarif` lint report formats (#314).

#![allow(clippy::missing_docs_in_private_items)]

use assert_cmd::{Command, cargo_bin_cmd};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const DUPLICATE: &str = "key: 1\nkey: 2\n";
const BROKEN: &str = "a: [\n";

fn fy() -> Command {
    cargo_bin_cmd!("fy")
}

struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn run(format: &str, args: &[&Path], stdin: Option<&str>) -> Run {
    let mut cmd = fy();
    cmd.args(["lint", "--no-config", "--format", format])
        .args(args);
    if let Some(stdin) = stdin {
        cmd.write_stdin(stdin);
    }
    let output = cmd.output().unwrap();
    Run {
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn write(dir: &TempDir, name: &str, content: &str) -> PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, content).unwrap();
    path
}

fn canonical(path: &Path) -> String {
    let canonical = path.canonicalize().unwrap().display().to_string();
    canonical
        .strip_prefix(r"\\?\")
        .map_or_else(|| canonical.clone(), str::to_owned)
}

#[test]
fn parsable_names_the_absolute_path() {
    let dir = TempDir::new().unwrap();
    let path = write(&dir, "a.yaml", DUPLICATE);
    let out = run("parsable", &[&path], None);
    assert_eq!(out.code, Some(2), "{}", out.stderr);
    let first = out.stdout.lines().next().unwrap();
    assert!(
        first.starts_with(&format!("{}:2:1: [error] duplicate key", canonical(&path))),
        "{first}"
    );
    assert!(first.ends_with("(duplicate-key)"), "{first}");
}

#[test]
fn github_emits_annotations_with_file_property() {
    let dir = TempDir::new().unwrap();
    let path = write(&dir, "a.yaml", DUPLICATE);
    let out = run("github", &[&path], None);
    assert_eq!(out.code, Some(2), "{}", out.stderr);
    let first = out.stdout.lines().next().unwrap();
    assert!(first.starts_with("::error file="), "{first}");
    assert!(first.contains(",line=2,col=1,"), "{first}");
    assert!(first.contains(",title=duplicate-key::"), "{first}");
}

#[test]
fn github_stdin_omits_file() {
    let out = run("github", &[], Some(DUPLICATE));
    assert_eq!(out.code, Some(2), "{}", out.stderr);
    assert!(
        out.stdout.starts_with("::error line=2,col=1,"),
        "{}",
        out.stdout
    );
}

#[test]
fn sarif_is_valid_json_with_file_uri() {
    let dir = TempDir::new().unwrap();
    let path = write(&dir, "a b.yaml", DUPLICATE);
    let out = run("sarif", &[&path], None);
    assert_eq!(out.code, Some(2), "{}", out.stderr);
    let json: Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(json["version"], "2.1.0");
    let result = &json["runs"][0]["results"][0];
    assert_eq!(result["ruleId"], "duplicate-key");
    let uri = result["locations"][0]["physicalLocation"]["artifactLocation"]["uri"]
        .as_str()
        .unwrap();
    assert!(uri.starts_with("file:///"), "{uri}");
    assert!(uri.ends_with("/a%20b.yaml"), "{uri}");
}

#[test]
fn clean_file_gives_empty_report_and_success() {
    let dir = TempDir::new().unwrap();
    let path = write(&dir, "ok.yaml", "a: 1\n");
    let sarif = run("sarif", &[&path], None);
    assert_eq!(sarif.code, Some(0), "{}", sarif.stderr);
    let json: Value = serde_json::from_str(&sarif.stdout).unwrap();
    assert_eq!(
        json["runs"][0]["results"].as_array().unwrap().as_slice(),
        [] as [serde_json::Value; 0]
    );
    for format in ["github", "parsable"] {
        let out = run(format, &[&path], None);
        assert_eq!(out.code, Some(0), "{}", out.stderr);
        assert!(out.stdout.is_empty(), "{format}: {}", out.stdout);
    }
}

#[test]
fn syntax_error_is_reported_and_exit_code_unchanged() {
    let dir = TempDir::new().unwrap();
    let path = write(&dir, "bad.yaml", BROKEN);
    let text = run("text", &[&path], None);
    for format in ["github", "parsable", "sarif"] {
        let out = run(format, &[&path], None);
        assert_eq!(out.code, text.code, "{format}");
        assert_eq!(out.stderr, text.stderr, "{format}");
        assert!(out.stdout.contains("syntax"), "{format}: {}", out.stdout);
    }
    let sarif: Value = serde_json::from_str(&run("sarif", &[&path], None).stdout).unwrap();
    assert_eq!(sarif["runs"][0]["results"][0]["ruleId"], "syntax");
}

#[test]
fn unreadable_input_still_prints_a_report() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("latin1.yaml");
    fs::write(&path, b"a: \xff\xfe\xff\n").unwrap();
    let out = run("sarif", &[&path], None);
    assert_ne!(out.code, Some(0));
    let json: Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(json["runs"][0]["results"][0]["ruleId"], "syntax");
}

#[test]
fn batch_results_are_sorted_by_path() {
    let dir = TempDir::new().unwrap();
    for name in ["c.yaml", "a.yaml", "b.yaml"] {
        write(&dir, name, DUPLICATE);
    }
    let out = run("parsable", &[dir.path()], None);
    assert_eq!(out.code, Some(2), "{}", out.stderr);
    let names: Vec<&str> = out
        .stdout
        .lines()
        .map(|l| {
            l.split(".yaml:")
                .next()
                .unwrap()
                .rsplit(['/', '\\'])
                .next()
                .unwrap()
        })
        .collect();
    assert_eq!(names, ["a", "b", "c"]);
}

#[test]
fn batch_syntax_error_contributes_a_diagnostic() {
    let dir = TempDir::new().unwrap();
    write(&dir, "ok.yaml", "a: 1\n");
    write(&dir, "bad.yaml", BROKEN);
    let out = run("parsable", &[dir.path()], None);
    assert_eq!(out.code, Some(2), "{}", out.stderr);
    assert!(out.stdout.contains("bad.yaml:"), "{}", out.stdout);
    assert!(out.stdout.contains("(syntax)"), "{}", out.stdout);
}

#[test]
fn ignored_file_prints_empty_sarif() {
    let dir = TempDir::new().unwrap();
    let config = write(&dir, "cfg.yaml", "ignore:\n  - skipped.yaml\n");
    let path = write(&dir, "skipped.yaml", DUPLICATE);
    let output = fy()
        .args(["lint", "--format", "sarif", "--config"])
        .arg(&config)
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        json["runs"][0]["results"].as_array().unwrap().as_slice(),
        [] as [serde_json::Value; 0]
    );
}

#[test]
fn batch_unreadable_file_is_reported() {
    let dir = TempDir::new().unwrap();
    write(&dir, "big.yaml", "key: 0123456789abc\n");
    let output = fy()
        .args([
            "--max-input-bytes",
            "8",
            "lint",
            "--no-config",
            "--format",
            "parsable",
        ])
        .arg(dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(2));
    assert!(stdout.contains("big.yaml:1:1: [error]"), "{stdout}");
    assert!(stdout.contains("(syntax)"), "{stdout}");
}

#[test]
fn bom_does_not_shift_columns() {
    let dir = TempDir::new().unwrap();
    let path = write(&dir, "bom.yaml", "\u{FEFF}key: 1\nkey: 2\n");
    let out = run("parsable", &[&path], None);
    assert_eq!(out.code, Some(2), "{}", out.stderr);
    assert!(
        out.stdout.contains(":2:1: [error] duplicate key"),
        "{}",
        out.stdout
    );
}

#[test]
fn set_member_value_is_a_lint_error_not_a_parse_failure() {
    let dir = TempDir::new().unwrap();
    let path = write(&dir, "set.yaml", "s: !!set\n  ? a\n  b: 1\n");
    let out = run("parsable", &[&path], None);
    assert_eq!(out.code, Some(2), "{}", out.stderr);
    assert!(out.stdout.contains("(set-values)"), "{}", out.stdout);
}

#[cfg(unix)]
#[test]
fn newline_in_file_name_cannot_inject_a_line() {
    let dir = TempDir::new().unwrap();
    let path = write(&dir, "a\n::error::x.yaml", DUPLICATE);
    let out = run("parsable", &[&path], None);
    assert!(
        out.stdout.lines().all(|l| !l.starts_with("::")),
        "{}",
        out.stdout
    );
}

#[test]
fn help_lists_report_formats() {
    let output = fy().args(["lint", "--help"]).output().unwrap();
    let help = String::from_utf8_lossy(&output.stdout);
    for format in ["github", "sarif", "parsable"] {
        assert!(help.contains(format), "{help}");
    }
}
