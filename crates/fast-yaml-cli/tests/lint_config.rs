//! End-to-end tests for typed rule configuration in `fy lint --config` (#324, #327).

use std::path::{Path, PathBuf};

use assert_cmd::{Command, cargo_bin_cmd};
use predicates::prelude::*;
use tempfile::TempDir;

fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../fast-yaml-linter/tests/fixtures/config")
        .join(relative)
}

fn lint(config: &Path) -> Command {
    let mut cmd = cargo_bin_cmd!("fy");
    cmd.args(["lint", "--config"]).arg(config);
    cmd
}

#[test]
fn issue_324_document_start_present_true_is_enforced() {
    lint(&fixture("valid/bool-forms.yaml"))
        .write_stdin("a: 1\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("document-start"))
        .stdout(predicate::str::contains("missing document start marker"));
}

#[test]
fn document_start_present_true_accepts_marker() {
    lint(&fixture("valid/bool-forms.yaml"))
        .write_stdin("---\na: 1\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("document-start").not());
}

#[test]
fn issue_324_quote_type_typo_fails_with_rule_and_key() {
    lint(&fixture("invalid/quote-type-typo.yaml"))
        .write_stdin("a: 1\n")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("quoted-strings"))
        .stderr(predicate::str::contains("quote-type"))
        .stderr(predicate::str::contains("singel"));
}

#[test]
fn invalid_configs_fail_with_actionable_messages() {
    for (file, needles) in [
        ("unknown-rule.yaml", &["no-such-rule"][..]),
        ("wrong-type.yaml", &["line-length", "max"][..]),
        ("unknown-option.yaml", &["line-length", "maxx"][..]),
        (
            "unsupported-yamllint-option.yaml",
            &["indentation", "spaces", "yamllint"][..],
        ),
        (
            "always-extra-allowed.yaml",
            &["quoted-strings", "extra-allowed"][..],
        ),
        ("bad-severity.yaml", &["braces", "loud"][..]),
        ("null-option.yaml", &["quoted-strings", "quote-type"][..]),
    ] {
        let mut assertion = lint(&fixture(&format!("invalid/{file}")))
            .write_stdin("a: 1\n")
            .assert()
            .code(1);
        for needle in needles {
            assertion = assertion.stderr(predicate::str::contains(*needle));
        }
    }
}

#[test]
fn config_with_invalid_yaml_exits_one() {
    let dir = TempDir::new().unwrap();
    let config = config_in(&dir, "rules: [broken yaml: {");
    lint(&config)
        .write_stdin("a: 1\n")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("failed to parse config file"));
}

#[test]
fn config_rules_must_be_a_mapping() {
    let dir = TempDir::new().unwrap();
    let config = config_in(&dir, "rules: 5\n");
    lint(&config)
        .write_stdin("a: 1\n")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("mapping of rule names"));
}

#[test]
fn top_level_typo_in_config_exits_one() {
    lint(&fixture("invalid/top-level-typo.yaml"))
        .write_stdin("a: 1\n")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("rulez"))
        .stderr(predicate::str::contains("'rules'"));
}

#[test]
fn extending_a_missing_config_file_is_reported() {
    lint(&fixture("invalid/yamllint-extends.yaml"))
        .write_stdin("a: 1\n")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("extends"))
        .stderr(predicate::str::contains("base.yaml"));
}

#[test]
fn inert_extra_required_is_rejected() {
    lint(&fixture("invalid/inert-extra-required.yaml"))
        .write_stdin("a: 1\n")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("extra-required"))
        .stderr(predicate::str::contains("cannot be combined"));
}

#[test]
fn bad_severity_message_appears_once() {
    let output = lint(&fixture("invalid/bad-severity.yaml"))
        .write_stdin("a: 1\n")
        .assert()
        .code(1)
        .get_output()
        .stderr
        .clone();
    let text = String::from_utf8(output).unwrap();
    assert_eq!(text.matches("unknown severity 'loud'").count(), 1, "{text}");
}

#[test]
fn every_valid_fixture_config_is_accepted() {
    for name in [
        "full.yaml",
        "bool-forms.yaml",
        "shorthands.yaml",
        "quoted-regex.yaml",
        "document-end-forbidden.yaml",
    ] {
        lint(&fixture(&format!("valid/{name}")))
            .write_stdin("a: 1\n")
            .assert()
            .code(predicate::in_iter([0, 2]));
    }
}

fn config_in(dir: &TempDir, content: &str) -> PathBuf {
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, content).unwrap();
    path
}

#[test]
fn line_length_max_from_config_applies() {
    let dir = TempDir::new().unwrap();
    let config = config_in(&dir, "rules:\n  line-length:\n    max: 20\n");
    lint(&config)
        .write_stdin("key: this line is longer than twenty characters\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("line-length"));
}

#[test]
fn line_length_null_max_disables_the_limit() {
    let dir = TempDir::new().unwrap();
    let config = config_in(&dir, "rules:\n  line-length:\n    max: ~\n");
    lint(&config)
        .write_stdin(format!("key: {}\n", "x".repeat(300)))
        .assert()
        .success()
        .stdout(predicate::str::contains("line-length").not());
}

#[test]
fn indent_size_from_config_applies() {
    let dir = TempDir::new().unwrap();
    let config = config_in(&dir, "rules:\n  indentation:\n    indent-size: 4\n");
    lint(&config)
        .write_stdin("a:\n  b: 1\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("indentation"));
}

#[test]
fn cli_max_line_length_overrides_config() {
    let dir = TempDir::new().unwrap();
    let config = config_in(&dir, "rules:\n  line-length:\n    max: 20\n");
    lint(&config)
        .args(["--max-line-length", "200"])
        .write_stdin("key: this line is longer than twenty characters\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("line-length").not());
}

#[test]
fn cli_rejects_invalid_numeric_overrides() {
    for args in [
        ["--max-line-length", "0"],
        ["--indent-size", "0"],
        ["--indent-size", "17"],
    ] {
        cargo_bin_cmd!("fy")
            .arg("lint")
            .args(args)
            .write_stdin("a: 1\n")
            .assert()
            .code(2);
    }
}

#[test]
fn disabled_rules_suppress_diagnostics() {
    let dir = TempDir::new().unwrap();
    let config = config_in(
        &dir,
        "rules:\n  document-start:\n    present: true\n    enabled: false\n  trailing-whitespace: disable\n",
    );
    lint(&config)
        .write_stdin("a: 1 \n")
        .assert()
        .success()
        .stdout(predicate::str::contains("document-start").not())
        .stdout(predicate::str::contains("trailing-whitespace").not());
}

#[test]
fn deeply_nested_config_fails_fast() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("deep.yaml");
    let nested = format!("rules: {}1{}\n", "[".repeat(100_000), "]".repeat(100_000));
    std::fs::write(&config, nested).unwrap();

    lint(&config)
        .write_stdin("a: 1\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("flow collection nesting exceeds"));
}
