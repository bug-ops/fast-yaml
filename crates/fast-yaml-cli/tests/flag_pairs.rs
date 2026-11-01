//! Paired `--flag` / `--no-flag` options that must not swallow the next positional (#632).

#![allow(clippy::missing_docs_in_private_items)]

use assert_cmd::{Command, cargo_bin_cmd};
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn fy() -> Command {
    cargo_bin_cmd!("fy")
}

fn yaml_file(content: &str) -> (TempDir, String) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("in.yaml");
    fs::write(&path, content).unwrap();
    let path = path.to_str().unwrap().to_owned();
    (dir, path)
}

#[test]
fn pretty_before_the_path_does_not_take_it_as_a_value() {
    let (_dir, path) = yaml_file("a: 1\n");
    fy().args(["convert", "json", "--pretty", &path])
        .assert()
        .success()
        .stdout("{\n  \"a\": 1\n}\n");
}

#[test]
fn no_pretty_before_the_path_gives_compact_json() {
    let (_dir, path) = yaml_file("a: 1\n");
    fy().args(["convert", "json", "--no-pretty", &path])
        .assert()
        .success()
        .stdout("{\"a\":1}\n");
}

#[test]
fn the_last_of_pretty_and_no_pretty_wins() {
    let (_dir, path) = yaml_file("a: 1\n");
    fy().args(["convert", "json", "--pretty", "--no-pretty", &path])
        .assert()
        .success()
        .stdout("{\"a\":1}\n");
    fy().args(["convert", "json", "--no-pretty", "--pretty", &path])
        .assert()
        .success()
        .stdout("{\n  \"a\": 1\n}\n");
}

#[test]
fn the_old_value_form_is_rejected() {
    let (_dir, path) = yaml_file("a: 1\n");
    fy().args(["convert", "json", "--pretty=false", &path])
        .assert()
        .failure()
        .code(2);
}

#[cfg(feature = "linter")]
mod lint {
    use super::*;

    const DUPLICATE: &str = "a: 1\na: 2\n";

    #[test]
    fn allow_duplicate_keys_before_the_path_does_not_take_it_as_a_value() {
        let (_dir, path) = yaml_file(DUPLICATE);
        fy().args(["lint", "--allow-duplicate-keys", &path])
            .assert()
            .success();
    }

    #[test]
    fn the_last_of_the_duplicate_key_flags_wins() {
        let (_dir, path) = yaml_file(DUPLICATE);
        fy().args([
            "lint",
            "--allow-duplicate-keys",
            "--no-allow-duplicate-keys",
            &path,
        ])
        .assert()
        .code(2)
        .stdout(predicate::str::contains("duplicate key"));
        fy().args([
            "lint",
            "--no-allow-duplicate-keys",
            "--allow-duplicate-keys",
            &path,
        ])
        .assert()
        .success();
    }
}
