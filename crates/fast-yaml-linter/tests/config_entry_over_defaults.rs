//! A rule entry in a config file starts from yamllint's rule defaults, not from fast-yaml's
//! (critic M1), while a binding patch keeps the current values.

use std::fs;

use fast_yaml_linter::config::RulesConfig;
use fast_yaml_linter::{ConfigFile, Linter, Severity};
use tempfile::TempDir;

fn load(content: &str) -> ConfigFile {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("cfg.yaml");
    fs::write(&path, content).unwrap();
    ConfigFile::load(&path).unwrap()
}

#[test]
fn a_mapping_entry_without_extends_reports_at_error_like_yamllint() {
    let rules = load("rules:\n  line-length: {max: 60}\n").rules;
    assert_eq!(rules.line_length.severity, Some(Severity::Error));

    let (config, _) = load("rules:\n  line-length: {max: 60}\n").into_parts();
    let long = format!("a: {}\n", "x".repeat(70));
    let found = Linter::with_config(config).lint(&long).unwrap();
    let line_length = found.iter().find(|d| d.code.as_str() == "line-length");
    assert_eq!(line_length.map(|d| d.severity), Some(Severity::Error));
}

#[test]
fn a_severity_entry_and_key_ordering_follow_the_same_rule() {
    let rules = load("rules:\n  key-ordering: {ignored-keys: ['^x']}\n  comments: warning\n").rules;
    assert_eq!(rules.key_ordering.severity, Some(Severity::Error));
    assert_eq!(rules.comments.severity, Some(Severity::Warning));
    assert!(rules.comments.options.require_starting_space);
}

#[test]
fn enabled_false_in_a_mapping_still_disables_the_rule() {
    let rules = load("rules:\n  line-length: {enabled: false}\n").rules;
    assert!(!rules.line_length.enabled);
}

#[test]
fn an_entry_over_a_parent_without_an_entry_starts_from_yamllint_too() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("p.yaml"), "rules:\n  braces: disable\n").unwrap();
    let child = dir.path().join("c.yaml");
    fs::write(
        &child,
        "extends: p.yaml\nrules:\n  line-length: {max: 60}\n",
    )
    .unwrap();
    let rules = ConfigFile::load(&child).unwrap().rules;
    assert_eq!(rules.line_length.severity, Some(Severity::Error));
}

#[test]
fn a_binding_patch_keeps_the_current_values() {
    let mut rules = RulesConfig::default();
    rules
        .apply(serde_norway::Deserializer::from_str(
            "line-length: {max: 60}",
        ))
        .unwrap();
    assert_eq!(rules.line_length.severity, None);
}
