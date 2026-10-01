//! yamllint rule names, `enable` semantics over `extends` (#589) and the `locale` key (#585).

use std::fs;
use std::path::PathBuf;

use fast_yaml_linter::config::{
    ConfigFileError, EntryOrigin, RuleConfigError, RuleName, RulesConfig, TopLevelKey,
};
use fast_yaml_linter::rules::MarkerPresence;
use fast_yaml_linter::{ConfigFile, Linter, Severity};
use tempfile::TempDir;

fn write(dir: &TempDir, name: &str, content: &str) -> PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, content).unwrap();
    path
}

fn load(content: &str) -> Result<ConfigFile, ConfigFileError> {
    let dir = TempDir::new().unwrap();
    ConfigFile::load(&write(&dir, "cfg.yaml", content))
}

fn rules_error(content: &str) -> RuleConfigError {
    let Err(ConfigFileError::InvalidRules { source, .. }) = load(content) else {
        panic!("expected InvalidRules for {content:?}");
    };
    source
}

#[test]
fn yamllint_rule_names_are_accepted_as_keys() {
    let config = load(
        "rules:\n  trailing-spaces: warning\n  key-duplicates: disable\n  anchors: {forbid-duplicated-anchors: false}\n",
    )
    .unwrap();
    assert_eq!(
        config.rules.trailing_whitespace.severity,
        Some(Severity::Warning)
    );
    assert!(!config.rules.duplicate_key.enabled);
    assert!(
        !config
            .rules
            .invalid_anchor
            .options
            .forbid_duplicated_anchors
    );
}

#[test]
fn a_yamllint_name_and_the_fast_yaml_code_cannot_both_be_set() {
    let error = rules_error("rules:\n  trailing-spaces: error\n  trailing-whitespace: warning\n");
    assert!(
        matches!(
            error,
            RuleConfigError::InvalidEntry {
                rule: RuleName::TrailingWhitespace,
                ..
            }
        ),
        "{error:?}"
    );
}

#[test]
fn only_the_config_parser_accepts_yamllint_names() {
    assert_eq!(
        RuleName::from_config_key("anchors"),
        Ok(RuleName::InvalidAnchor)
    );
    assert!("anchors".parse::<RuleName>().is_err());
    assert!(RuleName::from_config_key("key-duplicate").is_err());
}

fn document_start_after(content: &str) -> (MarkerPresence, Option<Severity>, bool) {
    let settings = load(content).unwrap().rules.document_start;
    (
        settings.options.present,
        settings.severity,
        settings.enabled,
    )
}

#[test]
fn enable_without_extends_resets_to_yamllint_defaults() {
    assert_eq!(
        document_start_after("rules:\n  document-start: enable\n"),
        (MarkerPresence::Required, Some(Severity::Error), true)
    );
}

#[test]
fn enable_over_default_preset_keeps_the_presets_warning() {
    assert_eq!(
        document_start_after("extends: default\nrules:\n  document-start: enable\n"),
        (MarkerPresence::Required, Some(Severity::Warning), true)
    );
}

#[test]
fn enable_over_a_preset_disabled_rule_gives_yamllint_defaults_at_error() {
    assert_eq!(
        document_start_after("extends: relaxed\nrules:\n  document-start: enable\n"),
        (MarkerPresence::Required, Some(Severity::Error), true)
    );
    let comments = load("extends: relaxed\nrules:\n  comments: enable\n")
        .unwrap()
        .rules
        .comments;
    assert_eq!(comments.severity, Some(Severity::Error));
    assert!(comments.enabled);
}

#[test]
fn enable_over_a_parent_without_an_entry_resets() {
    let dir = TempDir::new().unwrap();
    write(&dir, "p.yaml", "rules:\n  line-length: {max: 100}\n");
    let child = write(
        &dir,
        "c.yaml",
        "extends: p.yaml\nrules:\n  document-start: enable\n",
    );
    let rules = ConfigFile::load(&child).unwrap().rules;
    assert_eq!(
        rules.document_start.options.present,
        MarkerPresence::Required
    );
    assert_eq!(rules.document_start.severity, Some(Severity::Error));

    let (config, _) = ConfigFile::load(&child).unwrap().into_parts();
    let diagnostics = Linter::with_config(config).lint("a: 1\n").unwrap();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code.as_str() == "document-start")
    );
}

#[test]
fn enable_over_a_configured_parent_keeps_its_options_level_and_ignore() {
    let dir = TempDir::new().unwrap();
    write(
        &dir,
        "p.yaml",
        "rules:\n  line-length: {max: 120, level: warning, ignore: [gen/]}\n",
    );
    let child = write(
        &dir,
        "c.yaml",
        "extends: p.yaml\nrules:\n  line-length: enable\n",
    );
    let line_length = ConfigFile::load(&child).unwrap().rules.line_length;
    assert_eq!(
        line_length.options.max.map(std::num::NonZero::get),
        Some(120)
    );
    assert_eq!(line_length.severity, Some(Severity::Warning));
    assert!(line_length.ignore.is_some());
    assert_eq!(line_length.origin, EntryOrigin::Configured);
}

#[test]
fn enable_over_a_disabled_parent_entry_resets() {
    let dir = TempDir::new().unwrap();
    write(
        &dir,
        "p.yaml",
        "rules:\n  line-length: {max: 120, level: warning}\n",
    );
    write(
        &dir,
        "off.yaml",
        "extends: p.yaml\nrules:\n  line-length: disable\n",
    );
    let child = write(
        &dir,
        "c.yaml",
        "extends: off.yaml\nrules:\n  line-length: enable\n",
    );
    let line_length = ConfigFile::load(&child).unwrap().rules.line_length;
    assert_eq!(
        line_length.options.max.map(std::num::NonZero::get),
        Some(80)
    );
    assert_eq!(line_length.severity, Some(Severity::Error));
    assert!(line_length.enabled);
}

#[test]
fn enable_on_the_fast_yaml_only_rule_keeps_its_own_severity() {
    let rules = load("rules:\n  lint-directive: enable\n").unwrap().rules;
    assert_eq!(rules.lint_directive.severity, None);
    assert!(rules.lint_directive.enabled);
}

#[test]
fn enable_after_an_entry_in_the_same_apply_keeps_it() {
    let mut rules = RulesConfig::default();
    rules
        .apply(serde_norway::Deserializer::from_str(
            "line-length: {max: 120}",
        ))
        .unwrap();
    rules
        .apply(serde_norway::Deserializer::from_str("line-length: enable"))
        .unwrap();
    assert_eq!(
        rules.line_length.options.max.map(std::num::NonZero::get),
        Some(120)
    );
}

#[test]
fn utf8_c_locales_are_accepted_with_key_ordering_enabled() {
    for locale in ["C", "POSIX", "C.UTF-8", "c.utf8", "posix"] {
        let config = load(&format!(
            "locale: {locale}\nrules:\n  key-ordering: enable\n"
        ))
        .unwrap();
        assert!(config.rules.key_ordering.enabled, "{locale}");
        assert!(config.locale.unwrap().is_code_point_order(), "{locale}");
    }
}

#[test]
fn another_locale_is_an_error_only_while_key_ordering_is_enabled() {
    let error = load("locale: en_US.UTF-8\nrules:\n  key-ordering: enable\n").unwrap_err();
    let ConfigFileError::UnsupportedLocale { locale, .. } = &error else {
        panic!("{error:?}");
    };
    assert_eq!(locale, "en_US.UTF-8");
    assert!(error.to_string().contains("key-ordering"));

    assert!(load("locale: en_US.UTF-8\nextends: default\n").is_ok());
    assert!(load("locale: en_US.UTF-8\nrules:\n  key-ordering: disable\n").is_ok());
}

#[test]
fn locale_must_be_a_string() {
    let Err(ConfigFileError::InvalidKey { key, .. }) = load("locale: [C]\n") else {
        panic!("expected InvalidKey");
    };
    assert_eq!(key, TopLevelKey::Locale);
}

#[test]
fn locale_is_not_inherited_through_extends() {
    let dir = TempDir::new().unwrap();
    write(&dir, "base.yaml", "locale: de_DE.UTF-8\nextends: default\n");
    let child = write(
        &dir,
        "c.yaml",
        "extends: base.yaml\nrules:\n  key-ordering: enable\n",
    );
    let config = ConfigFile::load(&child).unwrap();
    assert_eq!(config.locale, None);
    assert!(config.rules.key_ordering.enabled);
}

#[test]
fn an_inline_enable_cannot_turn_on_a_disabled_key_ordering() {
    let config = load("locale: en_US.UTF-8\nextends: default\n").unwrap();
    let (lint_config, _) = config.into_parts();
    let source = "# yamllint enable rule:key-ordering\nb: 1\na: 2\n";
    let diagnostics = Linter::with_config(lint_config).lint(source).unwrap();
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code.as_str() != "key-ordering")
    );
}
