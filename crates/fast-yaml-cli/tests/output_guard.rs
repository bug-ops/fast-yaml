//! `fy format -o` and `fy convert -o` must not overwrite the file they read, as `fy lint -o`
//! does not (`OutputWriter::from_args`, #604).

#![allow(clippy::missing_docs_in_private_items)]

use std::fs;

use assert_cmd::Command;
use tempfile::TempDir;

fn fy() -> Command {
    assert_cmd::cargo_bin_cmd!("fy")
}

const SOURCE: &str = "b:   1\na: 2\n";

fn input() -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("in.yaml");
    fs::write(&path, SOURCE).unwrap();
    (dir, path)
}

#[test]
fn format_refuses_an_output_equal_to_the_input() {
    let (_dir, path) = input();
    let out = fy()
        .args(["format", "-o", path.to_str().unwrap()])
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("also an input file"));
    assert_eq!(fs::read_to_string(&path).unwrap(), SOURCE);
}

#[test]
fn convert_refuses_an_output_equal_to_the_input() {
    let (_dir, path) = input();
    let out = fy()
        .args(["convert", "json", "-o", path.to_str().unwrap()])
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("also an input file"));
    assert_eq!(fs::read_to_string(&path).unwrap(), SOURCE);
}

#[cfg(unix)]
#[test]
fn a_hard_link_or_symlink_to_the_input_is_refused() {
    let (dir, path) = input();
    let hard = dir.path().join("hard.yaml");
    let soft = dir.path().join("soft.yaml");
    fs::hard_link(&path, &hard).unwrap();
    std::os::unix::fs::symlink(&path, &soft).unwrap();
    for alias in [&hard, &soft] {
        for args in [["format"].as_slice(), ["convert", "json"].as_slice()] {
            let out = fy()
                .args(args)
                .args(["-o", alias.to_str().unwrap()])
                .arg(&path)
                .output()
                .unwrap();
            assert_eq!(out.status.code(), Some(1), "{alias:?} {args:?}");
        }
    }
    assert_eq!(fs::read_to_string(&path).unwrap(), SOURCE);
}

#[test]
fn another_destination_is_written_and_in_place_still_works() {
    let (dir, path) = input();
    let out_path = dir.path().join("out.yaml");
    let out = fy()
        .args(["format", "-o", out_path.to_str().unwrap()])
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(fs::read_to_string(&out_path).unwrap(), "b: 1\na: 2\n");
    assert_eq!(fs::read_to_string(&path).unwrap(), SOURCE);

    let out = fy().args(["format", "-i"]).arg(&path).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(fs::read_to_string(&path).unwrap(), "b: 1\na: 2\n");
}
