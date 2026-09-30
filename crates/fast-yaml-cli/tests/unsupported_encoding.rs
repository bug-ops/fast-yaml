//! Regression tests for #334: UTF-16/UTF-32 input fails with an encoding-specific message.

#![allow(clippy::missing_docs_in_private_items)]
#![allow(deprecated)] // Command::cargo_bin is deprecated but still works

use assert_cmd::Command;
use std::time::Duration;

const BOMS: [(&[u8], &str); 4] = [
    (
        &[0xFF, 0xFE, b'a', 0x00, b':', 0x00, b'1', 0x00],
        "UTF-16LE",
    ),
    (
        &[0xFE, 0xFF, 0x00, b'a', 0x00, b':', 0x00, b'1'],
        "UTF-16BE",
    ),
    (&[0xFF, 0xFE, 0x00, 0x00, b'a', 0, 0, 0], "UTF-32LE"),
    (&[0x00, 0x00, 0xFE, 0xFF, 0, 0, 0, b'a'], "UTF-32BE"),
];

const SUBCOMMANDS: [&[&str]; 4] = [&["parse"], &["lint"], &["format"], &["convert", "json"]];

fn fy() -> Command {
    let mut cmd = Command::cargo_bin("fy").unwrap();
    cmd.timeout(Duration::from_secs(10));
    cmd
}

fn assert_rejected(output: &std::process::Output, name: &str, context: &str) {
    assert_eq!(output.status.code(), Some(1), "{context}");
    assert!(output.stdout.is_empty(), "{context}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unsupported encoding"),
        "{context}: {stderr}"
    );
    assert!(stderr.contains(name), "{context}: {stderr}");
    assert!(stderr.contains("iconv"), "{context}: {stderr}");
}

#[test]
fn stdin_with_unsupported_bom_fails() {
    for (bom, name) in BOMS {
        let output = fy().arg("parse").write_stdin(bom).output().unwrap();
        assert_rejected(&output, name, name);
    }
}

#[test]
fn file_with_unsupported_bom_fails() {
    let dir = tempfile::tempdir().unwrap();
    for (bom, name) in BOMS {
        let path = dir.path().join("input.yaml");
        std::fs::write(&path, bom).unwrap();
        for args in SUBCOMMANDS {
            let output = fy().args(args).arg(&path).output().unwrap();
            assert_rejected(&output, name, &format!("{args:?} {name}"));
        }
    }
}

#[test]
fn batch_lint_reports_unsupported_encoding() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.yaml"), BOMS[0].0).unwrap();
    std::fs::write(dir.path().join("b.yaml"), "k: v\n").unwrap();
    let output = fy().arg("lint").arg(dir.path()).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unsupported encoding"), "{stderr}");
    assert!(stderr.contains("UTF-16LE"), "{stderr}");
}

#[test]
fn batch_format_reports_path_once_and_processes_valid_files() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.yaml"), BOMS[1].0).unwrap();
    std::fs::write(dir.path().join("b.yaml"), "k:   v\n").unwrap();
    let output = fy()
        .args(["format", "--dry-run"])
        .arg(dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("UTF-16BE"), "{stderr}");
    assert_eq!(stderr.matches("a.yaml").count(), 1, "{stderr}");
    assert!(stderr.contains("1 would change"), "{stderr}");
    assert!(stderr.contains("1 failed"), "{stderr}");
}

#[test]
fn utf8_bom_still_works() {
    for args in SUBCOMMANDS {
        let output = fy()
            .args(args)
            .write_stdin(&b"\xEF\xBB\xBFa: 1\n"[..])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0), "{args:?}");
    }
}

#[test]
fn invalid_utf8_keeps_utf8_message() {
    let output = fy()
        .arg("parse")
        .write_stdin(&b"a: \xC3\x28\n"[..])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not valid UTF-8"), "{stderr}");
    assert!(!stderr.contains("unsupported encoding"), "{stderr}");
}
