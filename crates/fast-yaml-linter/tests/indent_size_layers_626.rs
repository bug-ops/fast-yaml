//! `indent-size` in a later config layer replaces the `spaces` an earlier layer set (#626).

use fast_yaml_linter::config::IndentSpaces;
use fast_yaml_linter::{ConfigFile, LintConfig};

fn load(files: &[(&str, &str)]) -> LintConfig {
    let dir = tempfile::tempdir().unwrap();
    for (name, content) in files {
        std::fs::write(dir.path().join(name), content).unwrap();
    }
    let (config, _) = ConfigFile::load(&dir.path().join(files.last().unwrap().0))
        .unwrap()
        .into_parts();
    config
}

const fn width(config: &LintConfig) -> IndentSpaces {
    config.rules.indentation.options.width()
}

const fn fixed(config: &LintConfig) -> Option<usize> {
    match width(config) {
        IndentSpaces::Fixed(size) => Some(size.get()),
        IndentSpaces::Consistent => None,
    }
}

#[test]
fn indent_size_replaces_the_spaces_of_the_base_config() {
    let config = load(&[
        ("base.yaml", "rules:\n  indentation: {spaces: 4}\n"),
        (
            "child.yaml",
            "extends: base.yaml\nrules:\n  indentation: {indent-size: 3}\n",
        ),
    ]);
    assert_eq!(fixed(&config), Some(3));
}

#[test]
fn indent_size_replaces_the_consistent_width_of_a_preset() {
    let config = load(&[(
        "child.yaml",
        "extends: default\nrules:\n  indentation: {indent-size: 2}\n",
    )]);
    assert_eq!(fixed(&config), Some(2));
}

#[test]
fn spaces_replaces_the_indent_size_of_the_base_config() {
    let config = load(&[
        ("base.yaml", "rules:\n  indentation: {indent-size: 3}\n"),
        (
            "child.yaml",
            "extends: base.yaml\nrules:\n  indentation: {spaces: 6}\n",
        ),
    ]);
    assert_eq!(fixed(&config), Some(6));
    let consistent = load(&[
        ("base.yaml", "rules:\n  indentation: {indent-size: 3}\n"),
        (
            "child.yaml",
            "extends: base.yaml\nrules:\n  indentation: {spaces: consistent}\n",
        ),
    ]);
    assert_eq!(width(&consistent), IndentSpaces::Consistent);
}

#[test]
fn spaces_wins_when_one_layer_writes_both() {
    let config = load(&[(
        "child.yaml",
        "rules:\n  indentation: {spaces: 2, indent-size: 4}\n",
    )]);
    assert_eq!(fixed(&config), Some(2));
}

#[test]
fn other_indentation_options_survive_the_replacement() {
    let config = load(&[
        (
            "base.yaml",
            "rules:\n  indentation: {spaces: 4, check-multi-line-strings: true}\n",
        ),
        (
            "child.yaml",
            "extends: base.yaml\nrules:\n  indentation: {indent-size: 3}\n",
        ),
    ]);
    assert_eq!(fixed(&config), Some(3));
    assert!(config.rules.indentation.options.check_multi_line_strings);
}
