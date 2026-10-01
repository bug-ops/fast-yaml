//! `--output` must never overwrite any file of a batch run (`OutputWriter::ensure_not_inputs`).

#![allow(clippy::missing_docs_in_private_items)]

use std::fs;

use assert_cmd::Command;
use tempfile::TempDir;

fn fy() -> Command {
    assert_cmd::cargo_bin_cmd!("fy")
}

fn batch(count: usize) -> TempDir {
    let dir = TempDir::new().unwrap();
    for index in 0..count {
        fs::write(dir.path().join(format!("f{index:03}.yaml")), "k: 1\n").unwrap();
    }
    dir
}

#[test]
fn any_batch_file_as_the_destination_is_refused_and_left_intact() {
    let dir = batch(40);
    for name in ["f000.yaml", "f020.yaml", "f039.yaml"] {
        let target = dir.path().join(name);
        let out = fy()
            .args(["-o", target.to_str().unwrap(), "lint"])
            .arg(dir.path())
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(1), "{name}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("also an input file"), "{name}: {stderr}");
        assert_eq!(fs::read_to_string(&target).unwrap(), "k: 1\n", "{name}");
    }
}

#[test]
fn a_destination_outside_the_batch_is_written() {
    let dir = batch(40);
    let out_dir = TempDir::new().unwrap();
    let target = out_dir.path().join("report.json");
    let out = fy()
        .args(["-o", target.to_str().unwrap(), "lint", "--format", "json"])
        .arg(dir.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(target.exists());
}

#[cfg(unix)]
#[test]
fn a_hard_link_to_a_batch_file_is_refused() {
    let dir = batch(10);
    let out_dir = TempDir::new().unwrap();
    let alias = out_dir.path().join("alias.yaml");
    fs::hard_link(dir.path().join("f005.yaml"), &alias).unwrap();
    let out = fy()
        .args(["-o", alias.to_str().unwrap(), "lint"])
        .arg(dir.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("also an input file"));
}
