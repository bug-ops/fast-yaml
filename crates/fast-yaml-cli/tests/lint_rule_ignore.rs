//! Per-rule `ignore` / `ignore-from-file` through `fy lint` (#585) and the JSON report of input
//! errors (#591).

#![allow(clippy::missing_docs_in_private_items)]

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use tempfile::TempDir;

const SOURCE: &str = "a: 1 \nb: 2\nb: 3\n";
const CONFIG: &str = "rules:\n  trailing-spaces:\n    ignore: |\n      generated/\n  key-duplicates:\n    ignore-from-file: dup-ignores\n";

fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    for name in ["src/a.yaml", "generated/a.yaml", "other/a.yaml"] {
        let path = dir.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, SOURCE).unwrap();
    }
    fs::write(dir.path().join("dup-ignores"), "other/\n").unwrap();
    fs::write(dir.path().join(".fast-yaml.yaml"), CONFIG).unwrap();
    dir
}

fn fy(dir: &Path, args: &[&str]) -> Command {
    let mut cmd = assert_cmd::cargo_bin_cmd!("fy");
    cmd.current_dir(dir).args(args);
    cmd
}

fn normalize(text: &str) -> String {
    text.replace('\\', "/").replace("//?/", "")
}

fn lines(stdout: &[u8], root: &Path) -> Vec<String> {
    let root = normalize(&format!("{}/", root.canonicalize().unwrap().display()));
    normalize(&String::from_utf8_lossy(stdout))
        .lines()
        .map(|line| line.replace(&root, "").trim_start_matches("./").to_owned())
        .collect()
}

#[test]
fn single_files_skip_only_the_ignored_rules() {
    let dir = project();
    let report = |file: &str| {
        let out = fy(dir.path(), &["lint", "--format", "parsable", file])
            .output()
            .unwrap();
        lines(&out.stdout, dir.path())
    };

    let src = report("src/a.yaml");
    assert_eq!(src.len(), 2, "{src:?}");
    let generated = report("generated/a.yaml");
    assert_eq!(generated.len(), 1, "{generated:?}");
    assert!(generated[0].contains("(duplicate-key)"), "{generated:?}");
    let other = report("other/a.yaml");
    assert_eq!(other.len(), 1, "{other:?}");
    assert!(other[0].contains("(trailing-whitespace)"), "{other:?}");
}

#[test]
fn batch_runs_apply_the_same_ignores() {
    let dir = project();
    let out = fy(dir.path(), &["lint", "--format", "parsable", "."])
        .output()
        .unwrap();
    let found = lines(&out.stdout, dir.path());
    let has = |file: &str, code: &str| {
        found
            .iter()
            .any(|line| line.starts_with(file) && line.contains(&format!("({code})")))
    };
    assert!(has("src/a.yaml", "trailing-whitespace"));
    assert!(has("src/a.yaml", "duplicate-key"));
    assert!(!has("generated/a.yaml", "trailing-whitespace"));
    assert!(has("generated/a.yaml", "duplicate-key"));
    assert!(has("other/a.yaml", "trailing-whitespace"));
    assert!(!has("other/a.yaml", "duplicate-key"));
}

#[test]
fn standard_input_is_never_ignored() {
    let dir = project();
    let out = fy(dir.path(), &["lint", "--format", "parsable"])
        .write_stdin(SOURCE)
        .output()
        .unwrap();
    let found = lines(&out.stdout, dir.path());
    assert!(found.iter().any(|l| l.contains("(trailing-whitespace)")));
    assert!(found.iter().any(|l| l.contains("(duplicate-key)")));
}

#[test]
fn a_relative_spelling_of_the_path_is_matched_through_its_canonical_form() {
    let dir = project();
    let out = fy(
        dir.path(),
        &["lint", "--format", "parsable", "./src/../generated/a.yaml"],
    )
    .output()
    .unwrap();
    let found = lines(&out.stdout, dir.path());
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("(duplicate-key)"), "{found:?}");
}

#[test]
fn a_rule_with_both_ignore_keys_is_a_config_error() {
    let dir = project();
    fs::write(
        dir.path().join("bad.yaml"),
        "rules:\n  braces: {ignore: [x/], ignore-from-file: dup-ignores}\n",
    )
    .unwrap();
    let out = fy(dir.path(), &["lint", "--config", "bad.yaml", "src/a.yaml"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("cannot be used together"), "{stderr}");
}

#[test]
fn json_format_prints_a_diagnostic_for_a_missing_path() {
    let dir = TempDir::new().unwrap();
    let out = fy(
        dir.path(),
        &["lint", "--no-config", "--format", "json", "missing.yaml"],
    )
    .output()
    .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let items = json.as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["code"], "syntax");
    assert!(
        items[0]["message"]
            .as_str()
            .unwrap()
            .contains("missing.yaml")
    );
}

#[test]
fn json_format_prints_a_diagnostic_for_a_syntax_error() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("broken.yaml"), "a: [\n").unwrap();
    let out = fy(
        dir.path(),
        &["lint", "--no-config", "--format", "json", "broken.yaml"],
    )
    .output()
    .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json.as_array().unwrap()[0]["code"], "syntax");
}

#[test]
fn text_format_keeps_input_errors_on_stderr_only() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("broken.yaml"), "a: [\n").unwrap();
    for file in ["missing.yaml", "broken.yaml"] {
        let out = fy(dir.path(), &["lint", "--no-config", file])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(1), "{file}");
        assert!(out.stdout.is_empty(), "{file}");
        assert!(!out.stderr.is_empty(), "{file}");
    }
}
