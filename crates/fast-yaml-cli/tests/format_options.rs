//! End-to-end tests for `fy format` option validation, depth limit and anchor names.

#![allow(clippy::missing_docs_in_private_items)]

use assert_cmd::cargo_bin_cmd;

fn format(args: &[&str], stdin: &str) -> (Option<i32>, String, String) {
    let output = cargo_bin_cmd!("fy")
        .arg("format")
        .args(args)
        .write_stdin(stdin)
        .output()
        .unwrap();
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn max_depth_flag_lowers_and_raises_the_formatter_limit() {
    let (code, _, stderr) = format(&["--max-depth", "2"], "[[[1]]]\n");
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.contains("nesting depth exceeds 2"), "{stderr}");
    assert!(stderr.contains("raise with --max-depth"), "{stderr}");

    let (code, stdout, stderr) = format(&["--max-depth", "3"], "[[[1]]]\n");
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains('1'), "{stdout}");
}

#[test]
fn deep_block_nesting_needs_a_raised_limit() {
    let deep = format!("{}x\n", "- ".repeat(300));
    let (code, _, stderr) = format(&[], &deep);
    assert_eq!(code, Some(1), "{stderr}");
    let (code, _, stderr) = format(&["--max-depth", "512"], &deep);
    assert_eq!(code, Some(0), "{stderr}");
}

#[test]
fn out_of_range_options_are_rejected() {
    for (flag, message) in [
        ("--max-depth=0", "must be between 1 and 512, got 0"),
        ("--max-depth=513", "must be between 1 and 512, got 513"),
        ("--width=19", "must be between 20 and 1000, got 19"),
        ("--width=1001", "must be between 20 and 1000, got 1001"),
        ("--indent=0", "must be between 1 and 9, got 0"),
        ("--indent=10", "must be between 1 and 9, got 10"),
    ] {
        let (code, _, stderr) = format(&[flag], "a: 1\n");
        assert_eq!(code, Some(2), "{flag}: {stderr}");
        assert!(stderr.contains(message), "{flag}: {stderr}");
    }
}

#[test]
fn in_range_width_and_indent_are_accepted() {
    let (code, stdout, stderr) = format(&["--width=20", "--indent=8"], "a:\n  b: 1\n");
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(stdout, "a:\n        b: 1\n");
    for indent in ["--indent=1", "--indent=9"] {
        let (code, _, stderr) = format(&[indent], "a:\n  b: 1\n");
        assert_eq!(code, Some(0), "{indent}: {stderr}");
    }
}

#[test]
fn anchor_names_survive_ampersands_in_scalars() {
    let (code, stdout, stderr) = format(&[], "a: foo &x\nb: &y v\nc: *y\n");
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(stdout, "a: foo &x\nb: &y v\nc: *y\n");
}

#[test]
fn two_byte_tag_escapes_round_trip() {
    let (code, stdout, stderr) = format(&[], "a: !a%D1%82 x\n");
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(stdout, "a: !a%D1%82 x\n");
}
