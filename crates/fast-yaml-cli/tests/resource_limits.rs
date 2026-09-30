//! End-to-end tests for parser resource limits: hostile input must fail cleanly
//! (non-zero exit, no signal) instead of overflowing the stack or exhausting memory.

#![allow(clippy::missing_docs_in_private_items)]
#![allow(deprecated)] // Command::cargo_bin is deprecated but still works

use assert_cmd::Command;
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

fn write_fixture(dir: &TempDir, name: &str, content: &str) -> std::path::PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, content).unwrap();
    path
}

fn assert_limit_failure(args: &[&str], path: &std::path::Path) {
    let output = Command::cargo_bin("fy")
        .unwrap()
        .args(args)
        .arg(path)
        .output()
        .unwrap();
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
    ] {
        let path = write_fixture(&dir, name, &content);
        assert_limit_failure(&["convert", "json"], &path);
    }
}

#[test]
fn format_streams_the_bomb_without_expanding_aliases() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "bomb.yaml", &bomb_yaml());
    Command::cargo_bin("fy")
        .unwrap()
        .arg("format")
        .arg(&path)
        .assert()
        .success();
}
