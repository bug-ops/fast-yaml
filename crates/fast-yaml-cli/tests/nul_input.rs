//! Regression tests for #417 and #393: NUL input fails with a message; `.5` and `+.inf` are floats.

#![allow(clippy::missing_docs_in_private_items)]
#![allow(deprecated)] // Command::cargo_bin is deprecated but still works

use assert_cmd::Command;
use std::time::Duration;

fn run(args: &[&str], stdin: &[u8]) -> std::process::Output {
    Command::cargo_bin("fy")
        .unwrap()
        .args(args)
        .write_stdin(stdin)
        .timeout(Duration::from_secs(10))
        .output()
        .unwrap()
}

#[test]
fn every_subcommand_rejects_nul() {
    for args in [&["parse"][..], &["lint"], &["convert", "json"], &["format"]] {
        for input in [&b"a: 1\0\nb: 2\n"[..], b"\0a: 1\n", b"# c\0\na: 1\n"] {
            let output = run(args, input);
            assert_eq!(output.status.code(), Some(1), "{args:?} {input:?}");
            assert!(output.stdout.is_empty(), "{args:?} {input:?}");
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(stderr.contains("NUL"), "{args:?} {input:?}: {stderr}");
        }
    }
}

#[test]
fn convert_reads_leading_dot_as_float() {
    let output = run(&["convert", "json"], b"a: .5\nb: -.5\nd: +.5e1\n");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["a"], 0.5, "{stdout}");
    assert_eq!(json["b"], -0.5, "{stdout}");
    assert_eq!(json["d"], 5.0, "{stdout}");
}

#[test]
fn convert_treats_signed_infinity_as_float() {
    let output = run(&["convert", "json"], b"c: +.inf\n");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("infinity"), "{stderr}");
}

#[test]
fn file_and_directory_inputs_with_nul_fail_and_stay_unmodified() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("bad.yaml");
    std::fs::write(&file, b"a:   1\0\nb: 2\n").unwrap();
    std::fs::write(dir.path().join("ok.yaml"), b"a:   1\n").unwrap();
    for args in [
        vec!["format", "-i", file.to_str().unwrap()],
        vec!["format", "-i", dir.path().to_str().unwrap()],
    ] {
        let output = Command::cargo_bin("fy")
            .unwrap()
            .args(&args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{args:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("NUL"),
            "{args:?}"
        );
        assert_eq!(std::fs::read(&file).unwrap(), b"a:   1\0\nb: 2\n");
    }
}
