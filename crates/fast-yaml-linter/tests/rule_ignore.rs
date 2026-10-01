//! Per-rule `ignore` and `ignore-from-file` (#585) and `Linter::lint_file`.

use std::fs;
use std::path::{Path, PathBuf};

use fast_yaml_linter::config::{CanonicalPath, ConfigFileError};
use fast_yaml_linter::{ConfigFile, Linter};
use tempfile::TempDir;

const SOURCE: &str = "a: 1 \nb: 2\nb: 3\n";

fn write(dir: &TempDir, name: &str, content: &str) -> PathBuf {
    let path = dir.path().join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, content).unwrap();
    path
}

fn linter_for(config_path: &Path) -> Linter {
    let (config, _) = ConfigFile::load(config_path).unwrap().into_parts();
    Linter::with_config(config)
}

fn codes(linter: &Linter, path: Option<&Path>) -> Vec<String> {
    let found = path.map_or_else(
        || linter.lint(SOURCE),
        |path| linter.lint_file(SOURCE, &CanonicalPath::new(path).unwrap()),
    );
    let mut codes: Vec<String> = found
        .unwrap()
        .into_iter()
        .map(|d| d.code.as_str().to_owned())
        .collect();
    codes.sort();
    codes.dedup();
    codes
}

#[test]
fn ignore_patterns_skip_only_the_matching_files() {
    let dir = TempDir::new().unwrap();
    let config = write(
        &dir,
        "cfg.yaml",
        "rules:\n  trailing-spaces:\n    ignore: |\n      generated/\n  key-duplicates: {ignore: [src/]}\n",
    );
    let src = write(&dir, "src/a.yaml", SOURCE);
    let generated = write(&dir, "generated/a.yaml", SOURCE);
    let linter = linter_for(&config);

    assert_eq!(codes(&linter, Some(&src)), ["trailing-whitespace"]);
    assert_eq!(codes(&linter, Some(&generated)), ["duplicate-key"]);
}

#[test]
fn a_source_without_a_path_is_never_ignored() {
    let dir = TempDir::new().unwrap();
    let config = write(
        &dir,
        "cfg.yaml",
        "rules:\n  trailing-whitespace: {ignore: ['*']}\n  duplicate-key: {ignore: ['*']}\n",
    );
    let linter = linter_for(&config);
    assert_eq!(
        codes(&linter, None),
        ["duplicate-key", "trailing-whitespace"]
    );
}

#[test]
fn ignore_from_file_is_read_relative_to_the_config_directory() {
    let dir = TempDir::new().unwrap();
    write(&dir, "conf/ignores", "generated/\n# comment\n\n");
    let config = write(
        &dir,
        "conf/cfg.yaml",
        "rules:\n  trailing-whitespace:\n    ignore-from-file: ignores\n",
    );
    let kept = write(&dir, "conf/src/a.yaml", SOURCE);
    let skipped = write(&dir, "conf/generated/a.yaml", SOURCE);
    let linter = linter_for(&config);

    assert!(codes(&linter, Some(&kept)).contains(&"trailing-whitespace".to_owned()));
    assert!(!codes(&linter, Some(&skipped)).contains(&"trailing-whitespace".to_owned()));
}

#[test]
fn ignore_and_ignore_from_file_cannot_be_combined() {
    let dir = TempDir::new().unwrap();
    write(&dir, "ignores", "x/\n");
    let config = write(
        &dir,
        "cfg.yaml",
        "rules:\n  braces: {ignore: [x/], ignore-from-file: ignores}\n",
    );
    let error = ConfigFile::load(&config).unwrap_err();
    let ConfigFileError::InvalidRules { source, .. } = error else {
        panic!("expected InvalidRules");
    };
    let message = source.to_string();
    assert!(message.contains("cannot be used together"), "{message}");
}

#[test]
fn a_missing_ignore_file_names_the_path() {
    let dir = TempDir::new().unwrap();
    let config = write(
        &dir,
        "cfg.yaml",
        "rules:\n  braces: {ignore-from-file: nope.txt}\n",
    );
    let ConfigFileError::InvalidRules { source, .. } = ConfigFile::load(&config).unwrap_err()
    else {
        panic!("expected InvalidRules");
    };
    let message = source.to_string();
    assert!(
        message.contains("braces") && message.contains("nope.txt"),
        "{message}"
    );
}

#[test]
fn invalid_patterns_and_shapes_are_rejected() {
    let dir = TempDir::new().unwrap();
    for entry in ["ignore: 5", "ignore: [1]", "ignore-from-file: 5"] {
        let config = write(
            &dir,
            "cfg.yaml",
            &format!("rules:\n  braces: {{{entry}}}\n"),
        );
        assert!(
            matches!(
                ConfigFile::load(&config),
                Err(ConfigFileError::InvalidRules { .. })
            ),
            "{entry}"
        );
    }
}

#[test]
fn a_child_inherits_the_parent_ignore_anchored_at_the_parent_directory() {
    let dir = TempDir::new().unwrap();
    write(
        &dir,
        "shared/base.yaml",
        "rules:\n  trailing-whitespace: {ignore: [vendor/]}\n",
    );
    let config = write(
        &dir,
        "main.yaml",
        "extends: shared/base.yaml\nrules:\n  trailing-whitespace: {severity: error}\n",
    );
    let inside = write(&dir, "shared/vendor/a.yaml", SOURCE);
    let outside = write(&dir, "vendor/a.yaml", SOURCE);
    let linter = linter_for(&config);

    assert!(!codes(&linter, Some(&inside)).contains(&"trailing-whitespace".to_owned()));
    assert!(codes(&linter, Some(&outside)).contains(&"trailing-whitespace".to_owned()));
}

#[test]
fn a_child_ignore_replaces_the_parent_ignore() {
    let dir = TempDir::new().unwrap();
    write(
        &dir,
        "base.yaml",
        "rules:\n  trailing-whitespace: {ignore: [a/]}\n",
    );
    let config = write(
        &dir,
        "main.yaml",
        "extends: base.yaml\nrules:\n  trailing-whitespace: {ignore: [b/]}\n",
    );
    let a = write(&dir, "a/x.yaml", SOURCE);
    let b = write(&dir, "b/x.yaml", SOURCE);
    let linter = linter_for(&config);

    assert!(codes(&linter, Some(&a)).contains(&"trailing-whitespace".to_owned()));
    assert!(!codes(&linter, Some(&b)).contains(&"trailing-whitespace".to_owned()));
}

#[test]
fn lint_directive_diagnostics_follow_the_rule_ignore() {
    let dir = TempDir::new().unwrap();
    let config = write(
        &dir,
        "cfg.yaml",
        "rules:\n  lint-directive: {ignore: [skip/]}\n",
    );
    let source = "# fy: disable no-such-rule\na: 1\n";
    let linter = linter_for(&config);
    let kept = CanonicalPath::new(&write(&dir, "keep/a.yaml", source)).unwrap();
    let skipped = CanonicalPath::new(&write(&dir, "skip/a.yaml", source)).unwrap();

    let has_directive = |path: &CanonicalPath| {
        linter
            .lint_file(source, path)
            .unwrap()
            .iter()
            .any(|d| d.code.as_str() == "lint-directive")
    };
    assert!(has_directive(&kept));
    assert!(!has_directive(&skipped));
}

#[cfg(unix)]
#[test]
fn a_symlinked_path_is_matched_by_its_target() {
    let dir = TempDir::new().unwrap();
    let config = write(
        &dir,
        "cfg.yaml",
        "rules:\n  trailing-whitespace: {ignore: [real/]}\n",
    );
    let real = write(&dir, "real/a.yaml", SOURCE);
    let link = dir.path().join("link.yaml");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let linter = linter_for(&config);

    assert!(!codes(&linter, Some(&link)).contains(&"trailing-whitespace".to_owned()));
}

#[test]
fn a_missing_file_resolves_through_its_directory() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let path = CanonicalPath::new(&dir.path().join("later.yaml")).unwrap();
    assert_eq!(path.as_path(), root.join("later.yaml"));
    assert!(CanonicalPath::new(&dir.path().join("no-dir/later.yaml")).is_err());
}
