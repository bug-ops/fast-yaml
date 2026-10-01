//! End-to-end tests for parser resource limits: hostile input must fail cleanly
//! (non-zero exit, no signal) instead of overflowing the stack or exhausting memory.

#![allow(clippy::missing_docs_in_private_items)]

use assert_cmd::cargo_bin_cmd;
use std::fmt::Write as _;
use std::fs;
use tempfile::TempDir;

fn deep_yaml() -> String {
    format!("{}x\n", "- ".repeat(20_000))
}

fn bomb_yaml() -> String {
    let mut yaml = String::from("a0: &a0 [x,x,x,x,x,x,x,x,x]\n");
    for i in 1..=8 {
        let refs = vec![format!("*a{}", i - 1); 9].join(",");
        writeln!(yaml, "a{i}: &a{i} [{refs}]").unwrap();
    }
    yaml
}

fn strbomb_yaml() -> String {
    let mut yaml = format!("a0: &a0 \"{}\"\n", "x".repeat(1024));
    for i in 1..=6 {
        writeln!(
            yaml,
            "a{i}: &a{i} [{}]",
            vec![format!("*a{}", i - 1); 9].join(",")
        )
        .unwrap();
    }
    yaml
}

fn tagbomb_yaml() -> String {
    let mut yaml = format!("a0: &a0 !<tag:{}> \"\"\n", "x".repeat(10_000));
    for i in 1..=5 {
        writeln!(
            yaml,
            "a{i}: &a{i} [{}]",
            vec![format!("*a{}", i - 1); 9].join(",")
        )
        .unwrap();
    }
    yaml
}

fn tag_prefix_yaml() -> String {
    let mut yaml = format!("%TAG !e! tag:e.com,{}\n---\n", "a".repeat(100_000));
    for i in 0..1_000 {
        writeln!(yaml, "k{i}: !e!x v").unwrap();
    }
    yaml
}

const LEGIT_TAG_YAML: &str = "%TAG !e! tag:example.com,2000:\n---\nk: !e!x v\nl: !!str 1\n";

fn write_fixture(dir: &TempDir, name: &str, content: &str) -> std::path::PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, content).unwrap();
    path
}

fn assert_limit_failure(args: &[&str], path: &std::path::Path) {
    let output = cargo_bin_cmd!("fy").args(args).arg(path).output().unwrap();
    let code = output.status.code();
    assert!(code.is_some(), "terminated by signal: {:?}", output.status);
    assert_ne!(code, Some(0));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("limit exceeded"), "stderr: {stderr}");
}

#[test]
fn parse_rejects_deep_and_bomb_inputs() {
    let dir = TempDir::new().unwrap();
    for (name, content) in [
        ("deep.yaml", deep_yaml()),
        ("bomb.yaml", bomb_yaml()),
        ("strbomb.yaml", strbomb_yaml()),
        ("tagbomb.yaml", tagbomb_yaml()),
        ("tagprefix.yaml", tag_prefix_yaml()),
    ] {
        let path = write_fixture(&dir, name, &content);
        assert_limit_failure(&["parse"], &path);
    }
}

#[test]
fn lint_rejects_deep_and_bomb_inputs() {
    let dir = TempDir::new().unwrap();
    for (name, content) in [
        ("deep.yaml", deep_yaml()),
        ("bomb.yaml", bomb_yaml()),
        ("strbomb.yaml", strbomb_yaml()),
        ("tagbomb.yaml", tagbomb_yaml()),
        ("tagprefix.yaml", tag_prefix_yaml()),
    ] {
        let path = write_fixture(&dir, name, &content);
        assert_limit_failure(&["lint"], &path);
    }
}

#[test]
fn convert_json_rejects_deep_and_bomb_inputs() {
    let dir = TempDir::new().unwrap();
    for (name, content) in [
        ("deep.yaml", deep_yaml()),
        ("bomb.yaml", bomb_yaml()),
        ("strbomb.yaml", strbomb_yaml()),
        ("tagbomb.yaml", tagbomb_yaml()),
        ("tagprefix.yaml", tag_prefix_yaml()),
    ] {
        let path = write_fixture(&dir, name, &content);
        assert_limit_failure(&["convert", "json"], &path);
    }
}

#[test]
fn format_streams_the_bomb_without_expanding_aliases() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "bomb.yaml", &bomb_yaml());
    cargo_bin_cmd!("fy")
        .arg("format")
        .arg(&path)
        .assert()
        .success();
}

#[test]
fn format_rejects_tag_prefix_amplification() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "tagprefix.yaml", &tag_prefix_yaml());
    let output = cargo_bin_cmd!("fy")
        .arg("format")
        .arg(&path)
        .output()
        .unwrap();
    assert!(output.status.code().is_some_and(|c| c != 0));
    assert_eq!(output.stdout, []);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("tag prefix expansion"), "stderr: {stderr}");
}

#[test]
fn legitimate_tag_directive_passes_every_subcommand() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "legit.yaml", LEGIT_TAG_YAML);
    for args in [&["parse"][..], &["lint"], &["format"], &["convert", "json"]] {
        cargo_bin_cmd!("fy")
            .args(args)
            .arg(&path)
            .assert()
            .success();
    }
}
