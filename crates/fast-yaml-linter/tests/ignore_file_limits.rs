//! Bounds and secrecy of `ignore-from-file` (security audit M1, L1).

use std::error::Error;
use std::fmt::Write as _;
use std::fs;

use fast_yaml_linter::{ConfigFile, ConfigFileError};
use tempfile::TempDir;

const SECRET: &str = "export API_TOKEN=\"ghp_S3cr3t{1234\"";

fn chain(error: &dyn Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text = format!("{text} | {cause}");
        source = cause.source();
    }
    text
}

fn load(dir: &TempDir, config: &str) -> Result<ConfigFile, ConfigFileError> {
    let path = dir.path().join("cfg.yaml");
    fs::write(&path, config).unwrap();
    ConfigFile::load(&path)
}

fn top_and_rule(files: &str) -> [String; 2] {
    [
        format!("ignore-from-file: {files}\n"),
        format!("rules:\n  braces:\n    ignore-from-file: {files}\n"),
    ]
}

#[test]
fn a_repeated_file_name_is_read_once() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("p.txt"), "vendor/\n".repeat(100)).unwrap();
    let names = vec!["p.txt"; 400].join(", ");
    for config in top_and_rule(&format!("[{names}]")) {
        assert!(load(&dir, &config).is_ok(), "{config:.60}");
    }
}

#[test]
fn a_file_over_the_pattern_cap_stops_at_the_cap() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("big.txt"), "\n".repeat(100_000)).unwrap();
    for config in top_and_rule("big.txt") {
        let error = load(&dir, &config).unwrap_err();
        let text = chain(&error);
        assert!(text.contains("pattern lines"), "{text}");
    }
}

#[test]
fn many_different_files_are_capped() {
    let dir = TempDir::new().unwrap();
    let mut names = Vec::new();
    for index in 0..40 {
        let name = format!("f{index}.txt");
        fs::write(dir.path().join(&name), "x/\n").unwrap();
        names.push(name);
    }
    for config in top_and_rule(&format!("[{}]", names.join(", "))) {
        let text = chain(&load(&dir, &config).unwrap_err());
        assert!(text.contains("different files"), "{text}");
    }
}

#[test]
fn an_invalid_line_is_reported_by_position_without_its_text() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("ig.txt"), format!("ok/\n{SECRET}\n")).unwrap();
    for config in top_and_rule("ig.txt") {
        let error = load(&dir, &config).unwrap_err();
        let text = chain(&error);
        assert!(text.contains("ig.txt"), "{text}");
        assert!(text.contains("line 2"), "{text}");
        assert!(
            !text.contains("ghp_") && !text.contains("API_TOKEN"),
            "{text}"
        );
    }
}

#[test]
fn an_invalid_inline_pattern_still_names_the_pattern() {
    let dir = TempDir::new().unwrap();
    let text = chain(&load(&dir, "ignore: ['a{1']\n").unwrap_err());
    assert!(text.contains("a{1"), "{text}");
}

fn write_lines(dir: &TempDir, name: &str, count: usize, line: impl Fn(usize) -> String) {
    let text: String = (0..count).map(|i| line(i) + "\n").collect();
    fs::write(dir.path().join(name), text).unwrap();
}

fn budget_error(config: &str, dir: &TempDir) -> String {
    let text = chain(&load(dir, config).unwrap_err());
    assert!(text.contains("budget"), "{text}");
    text
}

#[test]
fn a_shared_ignore_file_is_charged_once_across_keys() {
    let dir = TempDir::new().unwrap();
    write_lines(&dir, "build.txt", 1024, |i| format!("**/build{i}/*.log"));
    write_lines(&dir, "vendor.txt", 1024, |i| format!("vendor{i}/"));
    let config = "ignore-from-file: build.txt\nrules:\n  braces:\n    ignore-from-file: build.txt\n  \
                  brackets:\n    ignore-from-file: build.txt\n  colons:\n    ignore-from-file: build.txt\n  \
                  commas:\n    ignore-from-file: vendor.txt\n";
    if let Err(error) = load(&dir, config) {
        panic!("{}", chain(&error));
    }
}

#[test]
fn a_shared_ignore_file_is_charged_once_across_an_extends_chain() {
    let dir = TempDir::new().unwrap();
    write_lines(&dir, "build.txt", 1024, |i| format!("**/build{i}/*.log"));
    fs::write(
        dir.path().join("base.yaml"),
        "ignore-from-file: build.txt\nrules:\n  braces:\n    ignore-from-file: build.txt\n",
    )
    .unwrap();
    let config = "extends: base.yaml\nignore-from-file: build.txt\nrules:\n  colons:\n    ignore-from-file: build.txt\n";
    if let Err(error) = load(&dir, config) {
        panic!("{}", chain(&error));
    }
}

#[test]
fn different_files_exhaust_the_wildcard_budget() {
    let dir = TempDir::new().unwrap();
    for name in ["a.txt", "b.txt"] {
        write_lines(&dir, name, 1024, |i| format!("**/{name}{i}/*.log"));
    }
    write_lines(&dir, "c.txt", 1, |_| "**/c/*.log".to_owned());
    let config = "ignore-from-file: a.txt\nrules:\n  braces:\n    ignore-from-file: b.txt\n  \
                  colons:\n    ignore-from-file: c.txt\n";
    let text = budget_error(config, &dir);
    assert!(!text.contains("a.txt0"), "{text}");
}

#[test]
fn different_files_exhaust_the_line_budget() {
    let dir = TempDir::new().unwrap();
    let mut config = String::from("rules:\n");
    for (index, rule) in ["braces", "brackets", "colons", "commas", "hyphens"]
        .iter()
        .enumerate()
    {
        let name = format!("f{index}.txt");
        write_lines(&dir, &name, 1024, |i| format!("d{index}_{i}/"));
        writeln!(config, "  {rule}:\n    ignore-from-file: {name}").unwrap();
    }
    budget_error(&config, &dir);
}

#[test]
fn different_files_exhaust_the_byte_budget() {
    let dir = TempDir::new().unwrap();
    for name in ["a.txt", "b.txt"] {
        write_lines(&dir, name, 1024, |i| {
            format!("{name}{i}{}", "x".repeat(150))
        });
    }
    let config = "ignore-from-file: a.txt\nrules:\n  braces:\n    ignore-from-file: b.txt\n";
    budget_error(config, &dir);
}

#[test]
fn inline_patterns_draw_on_the_same_budget() {
    let dir = TempDir::new().unwrap();
    write_lines(&dir, "a.txt", 1024, |i| format!("**/a{i}/*.log"));
    write_lines(&dir, "b.txt", 1024, |i| format!("**/b{i}/*.log"));
    let inline = (0..1024)
        .map(|i| format!("'**/c{i}/*.log'"))
        .collect::<Vec<_>>()
        .join(", ");
    let config = format!(
        "ignore-from-file: a.txt\nrules:\n  braces:\n    ignore-from-file: b.txt\n  colons:\n    ignore: [{inline}]\n"
    );
    budget_error(&config, &dir);
}

#[test]
fn a_pattern_with_too_many_wildcards_is_rejected_without_its_text() {
    let dir = TempDir::new().unwrap();
    let pattern = "*a".repeat(17);
    let text = chain(&load(&dir, &format!("ignore: ['{pattern}']\n")).unwrap_err());
    assert!(text.contains("wildcards"), "{text}");
    assert!(!text.contains(&pattern), "{text}");

    fs::write(dir.path().join("ig.txt"), format!("ok/\n{pattern}\n")).unwrap();
    for config in top_and_rule("ig.txt") {
        let text = chain(&load(&dir, &config).unwrap_err());
        assert!(
            text.contains("line 2") && text.contains("wildcards"),
            "{text}"
        );
        assert!(!text.contains(&pattern), "{text}");
    }
    assert!(load(&dir, &format!("ignore: ['{}']\n", "*a".repeat(16))).is_ok());
}

const RULES: [&str; 5] = ["braces", "brackets", "colons", "commas", "hyphens"];

/// A config whose rules each name one new file of `lines_per_file` lines made by `line`.
fn config_with_files(
    dir: &TempDir,
    files: usize,
    lines_per_file: usize,
    line: impl Fn(usize, usize) -> String,
) -> String {
    let mut config = String::from("rules:\n");
    for (index, rule) in RULES.iter().take(files).enumerate() {
        let name = format!("f{index}.txt");
        write_lines(dir, &name, lines_per_file, |i| line(index, i));
        writeln!(config, "  {rule}:\n    ignore-from-file: {name}").unwrap();
    }
    config
}

#[test]
fn the_line_budget_is_exactly_4096() {
    let dir = TempDir::new().unwrap();
    let at_limit = config_with_files(&dir, 4, 1024, |f, i| format!("d{f}_{i}/"));
    assert!(load(&dir, &at_limit).is_ok());

    let dir = TempDir::new().unwrap();
    let mut over = config_with_files(&dir, 4, 1024, |f, i| format!("d{f}_{i}/"));
    write_lines(&dir, "extra.txt", 1, |_| "extra/".to_owned());
    over.push_str("  hyphens:\n    ignore-from-file: extra.txt\n");
    budget_error(&over, &dir);
}

#[test]
fn the_wildcard_budget_is_exactly_4096() {
    let dir = TempDir::new().unwrap();
    let at_limit = config_with_files(&dir, 2, 1024, |f, i| format!("*d{f}_{i}*"));
    assert!(load(&dir, &at_limit).is_ok());

    let dir = TempDir::new().unwrap();
    let mut over = config_with_files(&dir, 2, 1024, |f, i| format!("*d{f}_{i}*"));
    write_lines(&dir, "extra.txt", 1, |_| "*extra".to_owned());
    over.push_str("  colons:\n    ignore-from-file: extra.txt\n");
    let text = budget_error(&over, &dir);
    assert!(text.contains("4096 wildcards"), "{text}");
}

#[test]
fn a_megabyte_of_repeated_names_loads_fast() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("p.txt"), "vendor/\n").unwrap();
    let names = vec!["p.txt"; 140_000].join(", ");
    let started = std::time::Instant::now();
    for config in top_and_rule(&format!("[{names}]")) {
        assert!(load(&dir, &config).is_ok());
    }
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
}
