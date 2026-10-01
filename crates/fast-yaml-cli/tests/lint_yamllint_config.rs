//! End-to-end tests for yamllint-style config keys in `fy lint`: `extends`, `ignore`,
//! `yaml-files` and the rule-name hint (#420).

use std::fs;
use std::path::Path;

use assert_cmd::{Command, cargo_bin_cmd};
use predicates::prelude::*;
use tempfile::TempDir;

/// Trailing whitespace is an error here, so a visited file makes `fy lint` exit with 2.
const STRICT: &str = "rules:\n  trailing-whitespace: error\n";
const DIRTY: &str = "a: 1 \n";

fn write(dir: &Path, relative: &str, content: &str) {
    let path = dir.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn project(config: &str, files: &[&str]) -> TempDir {
    let dir = TempDir::new().unwrap();
    write(dir.path(), ".fast-yaml.yaml", config);
    for file in files {
        write(dir.path(), file, DIRTY);
    }
    dir
}

fn fy(cwd: &Path, args: &[&str]) -> Command {
    let mut cmd = cargo_bin_cmd!("fy");
    cmd.current_dir(cwd).arg("lint").args(args);
    cmd
}

fn with_strict(extra: &str) -> String {
    format!("{extra}\n{STRICT}")
}

#[test]
fn extends_default_enables_quoted_strings_with_enable_or_a_mapping() {
    for rule in ["enable", "{quote-type: single}"] {
        let dir = project(
            &format!("extends: default\nrules:\n  quoted-strings: {rule}\n"),
            &[],
        );
        fy(dir.path(), &[])
            .write_stdin("---\nname: hello\n")
            .assert()
            .code(2)
            .stdout(predicate::str::contains("quoted-strings"));
    }
}

#[test]
fn extends_default_leaves_quoted_strings_off_without_override() {
    let dir = project("extends: default\n", &[]);
    fy(dir.path(), &[])
        .write_stdin("---\nname: hello\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("quoted-strings").not());
}

#[test]
fn extends_default_requires_document_start_as_warning() {
    let dir = project("extends: default\n", &[]);
    fy(dir.path(), &[])
        .write_stdin("a: 1\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("missing document start marker"));
}

#[test]
fn rules_disabled_by_relaxed_report_errors_once_enabled() {
    let dir = project(
        "extends: relaxed\nrules:\n  truthy: {check-keys: false}\n",
        &[],
    );
    fy(dir.path(), &[])
        .write_stdin("a: yes\n")
        .assert()
        .code(2)
        .stdout(predicate::str::contains("truthy"));

    let dir = project("extends: default\n", &[]);
    fy(dir.path(), &[])
        .write_stdin("---\na: yes\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("truthy"));
}

#[test]
fn extends_relaxed_disables_document_start() {
    let dir = project("extends: relaxed\n", &[]);
    fy(dir.path(), &[])
        .write_stdin("a: 1\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("document-start").not());
}

#[test]
fn extends_a_missing_config_file_is_rejected() {
    for (config, needle) in [
        ("extends: ./base.yaml\n", "base.yaml"),
        ("extends: strict\n", "strict"),
    ] {
        let dir = project(config, &[]);
        fy(dir.path(), &[])
            .write_stdin("a: 1\n")
            .assert()
            .code(1)
            .stderr(predicate::str::contains("extends"))
            .stderr(predicate::str::contains(needle));
    }
}

#[test]
fn yamllint_rule_names_and_per_rule_ignore_are_accepted() {
    let dir = project("rules:\n  key-duplicates: enable\n", &[]);
    fy(dir.path(), &[]).write_stdin("a: 1\n").assert().code(0);

    let dir = project("rules:\n  braces:\n    ignore: vendor/\n", &[]);
    fy(dir.path(), &[]).write_stdin("a: 1\n").assert().code(0);

    let dir = project(
        "rules:\n  trailing-spaces: error\n  anchors: disable\n",
        &[],
    );
    fy(dir.path(), &[]).write_stdin("a: 1 \n").assert().code(2);
}

#[test]
fn ignore_drops_files_from_a_directory_walk() {
    let dir = project(
        &with_strict("ignore: |\n  vendor/\n  *.gen.yaml"),
        &["src/a.yaml", "vendor/b.yaml", "src/c.gen.yaml"],
    );
    fy(dir.path(), &["."])
        .assert()
        .code(2)
        .stdout(predicate::str::contains("a.yaml"))
        .stdout(predicate::str::contains("b.yaml").not())
        .stdout(predicate::str::contains("c.gen.yaml").not());
}

#[test]
fn negated_ignore_pattern_re_includes_a_file_below_an_ignored_directory() {
    let dir = project(
        &with_strict("ignore:\n  - vendor/\n  - '!vendor/keep.yaml'"),
        &["vendor/keep.yaml", "vendor/drop.yaml"],
    );
    fy(dir.path(), &["."])
        .assert()
        .code(2)
        .stdout(predicate::str::contains("keep.yaml"))
        .stdout(predicate::str::contains("drop.yaml").not());
}

#[test]
fn ignoring_every_input_exits_zero_for_dir_file_and_file_list() {
    let dir = project(&with_strict("ignore: vendor/"), &["vendor/a.yaml"]);
    fy(dir.path(), &["vendor"]).assert().success();
    fy(dir.path(), &["."]).assert().success();
    fy(dir.path(), &["vendor/a.yaml"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
    fy(dir.path(), &["vendor/a.yaml", "vendor/a.yaml"])
        .assert()
        .success();
}

#[cfg(unix)]
#[test]
fn ignored_directories_are_not_entered() {
    use std::os::unix::fs::PermissionsExt;

    let dir = project(
        &with_strict("ignore: vendor/"),
        &["vendor/locked/a.yaml", "b.yaml"],
    );
    let locked = dir.path().join("vendor/locked");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let readable = fs::read_dir(&locked).is_ok();
    let assertion = fy(dir.path(), &["."]).assert().code(2);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    if !readable {
        assertion.stderr(predicate::str::contains("failed to read entry").not());
    }
}

#[test]
fn exclude_flag_alone_still_fails_when_nothing_is_left() {
    let dir = project(STRICT, &["vendor/a.yaml"]);
    fy(dir.path(), &[".", "--exclude", "**/vendor/**"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("no YAML files found"));
}

#[test]
fn ignored_explicit_file_is_not_read() {
    let dir = project(&with_strict("ignore: secret.yaml"), &[]);
    fs::write(dir.path().join("secret.yaml"), [0xFF, 0xFE, 0x00]).unwrap();
    fy(dir.path(), &["secret.yaml"]).assert().success();
}

#[test]
fn ignore_matching_is_case_sensitive() {
    let dir = project(&with_strict("ignore: X.YAML"), &["x.yaml"]);
    fy(dir.path(), &["."])
        .assert()
        .code(2)
        .stdout(predicate::str::contains("x.yaml"));
}

#[test]
fn slashless_ignore_pattern_matches_at_any_depth() {
    let dir = project(
        &with_strict("ignore: x.yaml"),
        &["src/deep/x.yaml", "src/y.yaml"],
    );
    fy(dir.path(), &["."])
        .assert()
        .code(2)
        .stdout(predicate::str::contains("y.yaml"))
        .stdout(predicate::str::contains("x.yaml").not());
}

#[test]
fn explicit_config_anchors_ignore_at_its_own_directory() {
    let dir = TempDir::new().unwrap();
    let conf = dir.path().join("conf");
    let lint_root = dir.path().join("work");
    write(&conf, "c.yaml", &with_strict("ignore: /work/skip.yaml\n"));
    write(&lint_root, "skip.yaml", DIRTY);
    write(&lint_root, "keep.yaml", DIRTY);
    fy(
        &lint_root,
        &["--config", conf.join("c.yaml").to_str().unwrap(), "."],
    )
    .assert()
    .code(2)
    .stdout(predicate::str::contains("skip.yaml"));

    write(&conf, "c.yaml", &with_strict("ignore: /skip.yaml\n"));
    write(&conf, "skip.yaml", DIRTY);
    let skip = conf.join("skip.yaml");
    fy(
        &lint_root,
        &[
            "--config",
            conf.join("c.yaml").to_str().unwrap(),
            skip.to_str().unwrap(),
        ],
    )
    .assert()
    .success();
}

#[test]
fn explicit_file_matching_ignore_is_dropped_even_with_yaml_files() {
    let dir = project(
        &with_strict("yaml-files: ['*.yml']\nignore: t.yaml"),
        &["t.yaml"],
    );
    fy(dir.path(), &["t.yaml"]).assert().success();
}

#[test]
fn ignored_explicit_file_with_a_non_yaml_name_is_skipped_not_rejected() {
    let dir = project(
        &with_strict("ignore: '*.j2'"),
        &["top.yaml", "src/t.yaml.j2"],
    );
    fy(dir.path(), &["src/t.yaml.j2", "top.yaml"])
        .assert()
        .code(2)
        .stdout(predicate::str::contains("top.yaml"))
        .stdout(predicate::str::contains("t.yaml.j2").not());
}

#[test]
fn exit_code_for_an_ignored_empty_directory_does_not_depend_on_negations() {
    for ignore in ["zbuild/", "zbuild/\n!zzz"] {
        let dir = project(
            &with_strict(&format!("ignore: |\n  {}", ignore.replace('\n', "\n  "))),
            &[],
        );
        fs::create_dir(dir.path().join("zbuild")).unwrap();
        fy(dir.path(), &["."]).assert().success();
    }
}

/// yamllint exits 0 when nothing is left to lint, so a config `ignore` that matched a directory
/// turns an otherwise empty run into success, with or without a `!` pattern.
#[test]
fn ignored_directory_makes_an_empty_run_succeed_like_yamllint() {
    let dir = project(&with_strict("ignore: empty/"), &[]);
    fs::create_dir(dir.path().join("empty")).unwrap();
    write(dir.path(), "notes.txt", "text");
    fy(dir.path(), &["."]).assert().success();

    let dir = project(&with_strict("ignore: other/"), &[]);
    write(dir.path(), "notes.txt", "text");
    fy(dir.path(), &["."]).assert().code(1);
}

#[test]
fn ignore_is_evaluated_from_a_subdirectory() {
    let dir = project(
        &with_strict("ignore: vendor/"),
        &["vendor/a.yaml", "vendor/sub/b.yaml", "keep/c.yaml"],
    );
    fy(&dir.path().join("vendor"), &["."]).assert().success();
    fy(&dir.path().join("vendor/sub"), &["b.yaml"])
        .assert()
        .success();
    fy(&dir.path().join("keep"), &["c.yaml"]).assert().code(2);
}

#[test]
fn ignore_holds_for_a_path_given_through_a_symlink() {
    #[cfg(unix)]
    {
        let dir = project(
            &with_strict("ignore: vendor/"),
            &["vendor/a.yaml", "b.yaml"],
        );
        let link = TempDir::new().unwrap();
        std::os::unix::fs::symlink(dir.path(), link.path().join("proj")).unwrap();
        let via_link = link.path().join("proj");
        fy(
            dir.path(),
            &[via_link.join("vendor/a.yaml").to_str().unwrap()],
        )
        .assert()
        .success();
        fy(dir.path(), &[via_link.to_str().unwrap()])
            .assert()
            .code(2)
            .stdout(predicate::str::contains("b.yaml"))
            .stdout(predicate::str::contains("a.yaml").not());
    }
}

#[test]
fn json_output_is_a_valid_empty_array_when_everything_is_ignored() {
    let dir = project(&with_strict("ignore: vendor/"), &["vendor/a.yaml"]);
    for target in ["vendor", "vendor/a.yaml"] {
        let output = fy(dir.path(), &[target, "--format", "json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let parsed: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(parsed, serde_json::json!([]), "{target}");
    }
}

#[test]
fn yaml_files_selects_templates_in_a_walk_but_not_other_extensions() {
    let dir = project(
        &with_strict("yaml-files: ['*.yaml.j2']"),
        &["a.yaml.j2", "b.yaml", "c.yml"],
    );
    fy(dir.path(), &["."])
        .assert()
        .code(2)
        .stdout(predicate::str::contains("a.yaml.j2"))
        .stdout(predicate::str::contains("b.yaml:").not())
        .stdout(predicate::str::contains("c.yml").not());
}

#[test]
fn yaml_files_patterns_match_the_file_name_only() {
    for pattern in ["sub/*.j2", "config"] {
        let dir = project(
            &with_strict(&format!("yaml-files: ['{pattern}']")),
            &["sub/a.j2", "config/x.yaml"],
        );
        fy(dir.path(), &["."])
            .assert()
            .code(1)
            .stderr(predicate::str::contains("no YAML files found"));
    }
}

#[test]
fn explicit_files_are_linted_whatever_yaml_files_says() {
    let dir = project(
        &with_strict("yaml-files: ['*.yml']"),
        &["t.yaml", "sub/u.yaml", "v.yml"],
    );
    fy(dir.path(), &["t.yaml"]).assert().code(2);
    fy(dir.path(), &["t.yaml", "v.yml"]).assert().code(2);
    fy(dir.path(), &["."])
        .assert()
        .code(2)
        .stdout(predicate::str::contains("v.yml"))
        .stdout(predicate::str::contains("u.yaml").not());
}

#[test]
fn explicit_non_matching_file_still_fails_the_include_check() {
    let dir = project(&with_strict("yaml-files: ['*.yml']"), &[]);
    write(dir.path(), "notes.txt", DIRTY);
    fy(dir.path(), &["notes.txt", "."])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("notes.txt"));
}

#[test]
fn explicit_include_flag_beats_yaml_files() {
    let dir = project(
        &with_strict("yaml-files: ['*.yml']"),
        &["a.yaml", "b.yml", "c.conf"],
    );
    fy(dir.path(), &[".", "--include", "*.conf"])
        .assert()
        .code(2)
        .stdout(predicate::str::contains("c.conf"))
        .stdout(predicate::str::contains("b.yml").not())
        .stdout(predicate::str::contains("a.yaml").not());
}

#[test]
fn ignore_and_yaml_files_combine() {
    let dir = project(
        &with_strict("yaml-files: ['*.yaml.j2']\nignore: templates/skip/"),
        &["templates/a.yaml.j2", "templates/skip/b.yaml.j2"],
    );
    fy(dir.path(), &["."])
        .assert()
        .code(2)
        .stdout(predicate::str::contains("a.yaml.j2"))
        .stdout(predicate::str::contains("b.yaml.j2").not());
}

#[test]
fn level_is_an_alias_of_severity() {
    let dir = project(
        "rules:\n  trailing-whitespace: {level: error}\n  truthy: {level: warning}\n",
        &[],
    );
    fy(dir.path(), &[])
        .write_stdin("a: 1 \nb: yes\n")
        .assert()
        .code(2)
        .stdout(predicate::str::contains("error[trailing-whitespace]"))
        .stdout(predicate::str::contains("warning[truthy]"));
}

#[test]
fn single_letter_truthy_values_are_not_reported() {
    let dir = project("extends: default\n", &[]);
    fy(dir.path(), &[])
        .write_stdin("---\na: y\nb: N\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("truthy").not());
}

#[test]
fn quoted_strings_options_are_accepted_in_a_config_file() {
    let dir = project(
        "rules:\n  quoted-strings: {quote-type: single, required: false, allow-quoted-quotes: true, check-keys: true}\n",
        &[],
    );
    fy(dir.path(), &[])
        .write_stdin("\"a\": \"it's\"\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("quoted-strings").count(1));
}

#[test]
fn duplicated_merge_keys_follow_the_option_and_the_presets() {
    let source = "---\na: &a {x: 1}\nb: &b {y: 2}\nc:\n  <<: *a\n  <<: *b\n";
    let dir = project("rules:\n  trailing-whitespace: error\n", &[]);
    fy(dir.path(), &[])
        .write_stdin(source)
        .assert()
        .code(2)
        .stdout(predicate::str::contains("duplicate merge key '<<'"));

    let dir = project("extends: default\n", &[]);
    fy(dir.path(), &[])
        .write_stdin(source)
        .assert()
        .success()
        .stdout(predicate::str::contains("duplicate").not());
}

#[test]
fn line_length_non_breakable_options_are_accepted() {
    let url = "http://localhost/very/very/very/very/very/very/very/very/long/url";
    let dir = project("rules:\n  line-length: {max: 20}\n", &[]);
    fy(dir.path(), &[])
        .write_stdin(format!("- {url}\n"))
        .assert()
        .success()
        .stdout(predicate::str::contains("line-length").not());

    let dir = project(
        "rules:\n  line-length: {max: 20, allow-non-breakable-words: false}\n",
        &[],
    );
    fy(dir.path(), &[])
        .write_stdin(format!("- {url}\n"))
        .assert()
        .stdout(predicate::str::contains("line-length"));
}

#[test]
fn document_end_required_checks_every_document() {
    let dir = project("rules:\n  document-end: {present: true}\n", &[]);
    fy(dir.path(), &[])
        .write_stdin("a: 1\n---\nb: 2\n...\n")
        .assert()
        .stdout(predicate::str::contains("missing document end marker").count(1));
}
