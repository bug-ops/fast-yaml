//! `fy parse --stats` prints through the reporter (#628).

#![allow(clippy::missing_docs_in_private_items)]

use assert_cmd::cargo_bin_cmd;
use predicates::prelude::*;

const SOURCE: &str = "a:\n  b: 1\n  c: [1, 2]\n";

#[test]
fn stats_are_printed_to_stdout() {
    cargo_bin_cmd!("fy")
        .args(["--no-color", "parse", "--stats"])
        .write_stdin(SOURCE)
        .assert()
        .success()
        .stdout("✓ YAML is valid\n\nStatistics:\n  Keys: 3\n  Max depth: 3\n");
}

#[test]
fn quiet_still_prints_the_stats_that_were_asked_for() {
    cargo_bin_cmd!("fy")
        .args(["--no-color", "-q", "parse", "--stats"])
        .write_stdin(SOURCE)
        .assert()
        .success()
        .stdout(predicate::str::contains("Keys: 3"));
}

#[test]
fn stats_are_not_printed_without_the_flag() {
    cargo_bin_cmd!("fy")
        .args(["--no-color", "parse"])
        .write_stdin(SOURCE)
        .assert()
        .success()
        .stdout("✓ YAML is valid\n");
}
