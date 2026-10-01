//! `fy lint --max-diagnostics` caps the output per file and never the exit code (#603).

#![allow(clippy::missing_docs_in_private_items)]

use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::process::Output;

use assert_cmd::Command;
use tempfile::TempDir;

fn fy() -> Command {
    assert_cmd::cargo_bin_cmd!("fy")
}

/// Nine warnings (trailing whitespace) on lines 2-10 and one error (duplicate key) on line 11.
fn noisy() -> String {
    let mut text = String::from("---\n");
    for i in 0..9 {
        writeln!(text, "k{i}: 1 ").unwrap();
    }
    text.push_str("k0: 2\n");
    text
}

fn write(dir: &Path, name: &str, content: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    fs::write(&path, content).unwrap();
    path
}

fn json(output: &Output) -> Vec<serde_json::Value> {
    serde_json::from_slice(&output.stdout).unwrap()
}

fn lint(args: &[&str], path: &Path) -> Output {
    fy().arg("lint")
        .args(["--no-config"])
        .args(args)
        .arg(path)
        .output()
        .unwrap()
}

#[test]
fn without_a_cap_every_diagnostic_is_shown() {
    let dir = TempDir::new().unwrap();
    let file = write(dir.path(), "a.yaml", &noisy());
    let output = lint(&["--format", "json"], &file);
    let found = json(&output);
    assert!(found.len() >= 10, "{found:?}");
    assert!(found.iter().all(|d| d["code"] != "diagnostic-limit"));
}

#[test]
fn the_cap_keeps_the_first_diagnostics_and_adds_one_summary() {
    let dir = TempDir::new().unwrap();
    let file = write(dir.path(), "a.yaml", &noisy());
    let output = lint(&["--format", "json", "--max-diagnostics", "3"], &file);
    let found = json(&output);
    assert_eq!(found.len(), 4, "{found:?}");
    let summary = &found[3];
    assert_eq!(summary["code"], "diagnostic-limit");
    assert_eq!(summary["severity"], "error");
    assert!(
        summary["message"]
            .as_str()
            .unwrap()
            .contains("limit is 3 per file")
    );
    let lines: Vec<_> = found[..3]
        .iter()
        .map(|d| d["span"]["start"]["line"].as_u64().unwrap())
        .collect();
    assert!(lines.windows(2).all(|pair| pair[0] <= pair[1]), "{lines:?}");
}

#[test]
fn the_exit_code_ignores_the_cap() {
    let dir = TempDir::new().unwrap();
    let file = write(dir.path(), "a.yaml", &noisy());
    for format in ["text", "json", "sarif", "github", "parsable"] {
        let output = lint(&["--format", format, "--max-diagnostics", "1"], &file);
        assert_eq!(output.status.code(), Some(2), "{format}");
    }
}

#[test]
fn an_omitted_error_stays_visible_as_an_error_in_every_format() {
    let dir = TempDir::new().unwrap();
    let file = write(dir.path(), "a.yaml", &noisy());

    let sarif = lint(&["--format", "sarif", "--max-diagnostics", "2"], &file);
    let report: serde_json::Value = serde_json::from_slice(&sarif.stdout).unwrap();
    let results = report["runs"][0]["results"].as_array().unwrap();
    let summary = results
        .iter()
        .find(|r| r["ruleId"] == "diagnostic-limit")
        .unwrap();
    assert_eq!(summary["level"], "error");

    let github = lint(&["--format", "github", "--max-diagnostics", "2"], &file);
    let text = String::from_utf8_lossy(&github.stdout);
    assert!(
        text.lines()
            .any(|l| l.starts_with("::error ") && l.contains("title=diagnostic-limit")),
        "{text}"
    );

    let parsable = lint(&["--format", "parsable", "--max-diagnostics", "2"], &file);
    let text = String::from_utf8_lossy(&parsable.stdout);
    assert!(
        text.contains("[error]") && text.contains("(diagnostic-limit)"),
        "{text}"
    );

    let plain = lint(&["--max-diagnostics", "2"], &file);
    assert!(String::from_utf8_lossy(&plain.stdout).contains("output truncated: "));
}

#[test]
fn quiet_output_never_gains_a_non_error_line() {
    let dir = TempDir::new().unwrap();
    let mut content = noisy();
    content.push_str("k1: 3\nk2: 4\n");
    let file = write(dir.path(), "a.yaml", &content);
    let output = fy()
        .args(["--quiet", "lint", "--no-config", "--format", "json"])
        .args(["--max-diagnostics", "1"])
        .arg(&file)
        .output()
        .unwrap();
    let found = json(&output);
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found.iter().all(|d| d["severity"] == "error"), "{found:?}");
    assert_eq!(found[1]["code"], "diagnostic-limit");
}

#[test]
fn a_file_within_the_cap_gets_no_summary() {
    let dir = TempDir::new().unwrap();
    let file = write(dir.path(), "a.yaml", "---\na: 1\n");
    let output = lint(&["--format", "json", "--max-diagnostics", "5"], &file);
    assert_eq!(json(&output), Vec::<serde_json::Value>::new());
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn the_config_key_applies_and_the_flag_overrides_it() {
    let dir = TempDir::new().unwrap();
    let file = write(dir.path(), "a.yaml", &noisy());
    let config = write(dir.path(), "cfg.yaml", "max-diagnostics: 2\n");
    let config = config.to_str().unwrap();

    let output = fy()
        .args(["lint", "--config", config, "--format", "json"])
        .arg(&file)
        .output()
        .unwrap();
    assert_eq!(json(&output).len(), 3);

    let output = fy()
        .args(["lint", "--config", config, "--format", "json"])
        .args(["--max-diagnostics", "4"])
        .arg(&file)
        .output()
        .unwrap();
    assert_eq!(json(&output).len(), 5);
}

#[test]
fn a_zero_or_non_numeric_cap_is_rejected() {
    let dir = TempDir::new().unwrap();
    let file = write(dir.path(), "a.yaml", &noisy());
    for bad in ["0", "many", "1.5"] {
        let output = lint(&["--max-diagnostics", bad], &file);
        assert!(!output.status.success(), "{bad}");
        assert!(output.stdout.is_empty(), "{bad}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("--max-diagnostics"), "{bad}: {stderr}");
    }
}

#[test]
fn every_file_of_a_batch_is_capped_on_its_own() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "a.yaml", &noisy());
    write(dir.path(), "b.yaml", &noisy());
    write(dir.path(), "c.yaml", "---\nx: 1\n");
    let output = fy()
        .args([
            "lint",
            "--no-config",
            "--format",
            "json",
            "--max-diagnostics",
            "2",
        ])
        .arg(dir.path())
        .output()
        .unwrap();
    let found = json(&output);
    let summaries = found
        .iter()
        .filter(|d| d["code"] == "diagnostic-limit")
        .count();
    assert_eq!(summaries, 2, "{found:?}");
    assert_eq!(found.len(), 6, "{found:?}");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn the_text_footer_does_not_count_the_summary() {
    let dir = TempDir::new().unwrap();
    let file = write(dir.path(), "a.yaml", &noisy());
    let output = lint(&["--max-diagnostics", "1"], &file);
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(!text.contains("1 errors"), "{text}");
}

#[test]
fn exactly_the_cap_and_one_more() {
    let dir = TempDir::new().unwrap();
    let file = write(dir.path(), "a.yaml", &noisy());
    let total = json(&lint(&["--format", "json"], &file)).len();
    let at = |cap: usize| {
        json(&lint(
            &["--format", "json", "--max-diagnostics", &cap.to_string()],
            &file,
        ))
    };
    assert_eq!(at(total).len(), total);
    assert!(at(total).iter().all(|d| d["code"] != "diagnostic-limit"));
    let past = at(total - 1);
    assert_eq!(past.len(), total);
    assert_eq!(past[total - 1]["code"], "diagnostic-limit");
}
