//! Glob expansion does not follow symlinks below the pattern's base (#692).

#![cfg(unix)]
#![allow(clippy::missing_docs_in_private_items)]

use assert_cmd::{Command, cargo_bin_cmd};
use predicates::prelude::*;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::time::Duration;
use tempfile::TempDir;

const JSON: &str = "{\"auths\": {\"r\": {\"auth\": \"x\"}}}\n";
const SECRET: &str = "token: ghp_SECRETSECRET\n";

fn fy(dir: &Path) -> Command {
    let mut cmd = cargo_bin_cmd!("fy");
    cmd.current_dir(dir).timeout(Duration::from_secs(10));
    cmd
}

fn tree() -> TempDir {
    let temp = TempDir::new().unwrap();
    fs::create_dir(temp.path().join("repo")).unwrap();
    fs::create_dir(temp.path().join("outside")).unwrap();
    temp
}

#[test]
fn format_in_place_does_not_write_through_a_file_link() {
    let temp = tree();
    let target = temp.path().join("outside/config.json");
    fs::write(&target, JSON).unwrap();
    symlink(
        "../outside/config.json",
        temp.path().join("repo/docker.yaml"),
    )
    .unwrap();

    fy(temp.path())
        .args(["format", "-i", "repo/**/*.yaml"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("glob pattern matched no files"));

    assert_eq!(fs::read_to_string(target).unwrap(), JSON);
}

#[test]
fn format_in_place_does_not_enter_a_linked_directory() {
    let temp = tree();
    let target = temp.path().join("outside/a.yaml");
    fs::write(&target, "a:   1\n").unwrap();
    symlink("../outside", temp.path().join("repo/sublink")).unwrap();

    fy(temp.path())
        .args(["format", "-i", "repo/**/*.yaml"])
        .assert()
        .code(1);

    assert_eq!(fs::read_to_string(target).unwrap(), "a:   1\n");
}

#[test]
fn lint_does_not_print_a_linked_file() {
    let temp = tree();
    fs::write(temp.path().join("outside/secrets.yaml"), SECRET).unwrap();
    symlink(
        "../outside/secrets.yaml",
        temp.path().join("repo/link.yaml"),
    )
    .unwrap();

    fy(temp.path())
        .args(["lint", "repo/*.yaml"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("ghp_SECRETSECRET").not())
        .stderr(predicate::str::contains("ghp_SECRETSECRET").not());
}

#[test]
fn links_to_the_parent_do_not_make_the_walk_loop() {
    let temp = TempDir::new().unwrap();
    for side in ["a", "b"] {
        let dir = temp.path().join("r").join(side);
        fs::create_dir_all(&dir).unwrap();
        symlink("../..", dir.join("up")).unwrap();
    }

    fy(temp.path())
        .args(["lint", "r/**/*.yaml"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("glob pattern matched no files"));
}

#[test]
fn a_real_file_is_still_found_next_to_links() {
    let temp = tree();
    fs::write(temp.path().join("repo/real.yaml"), "a: 1\n").unwrap();
    symlink("../outside", temp.path().join("repo/sublink")).unwrap();

    fy(temp.path())
        .args(["format", "-n", "repo/**/*.yaml"])
        .assert()
        .success()
        .stderr(predicate::str::contains("1 unchanged"));
}

#[test]
fn a_symlink_given_as_the_base_is_followed() {
    let temp = tree();
    fs::write(temp.path().join("outside/a.yaml"), "a: 1\n").unwrap();
    symlink("outside", temp.path().join("linked")).unwrap();

    fy(temp.path())
        .args(["format", "-n", "linked/*.yaml"])
        .assert()
        .success()
        .stderr(predicate::str::contains("1 unchanged"));
}

#[test]
fn a_missing_base_matches_nothing_without_a_warning() {
    let temp = tree();

    fy(temp.path())
        .args(["format", "-n", "nodir/*.yaml"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("glob pattern matched no files"))
        .stderr(predicate::str::contains("Warning").not());
}
