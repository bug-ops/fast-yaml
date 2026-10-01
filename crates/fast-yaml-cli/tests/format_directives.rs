//! Regression tests for #447 (directives of later documents, directive comments) and #362 (plain `inf`/`NaN`).

#![allow(clippy::missing_docs_in_private_items)]

use assert_cmd::cargo_bin_cmd;

fn format(input: &str, extra: &[&str]) -> String {
    let output = cargo_bin_cmd!("fy")
        .arg("format")
        .args(extra)
        .write_stdin(input)
        .output()
        .unwrap();
    assert!(output.status.success(), "{input:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap()
}

fn assert_stable(input: &str, expected: &str) {
    let once = format(input, &[]);
    assert_eq!(once, expected);
    assert_eq!(format(&once, &[]), once);
}

#[test]
fn directive_of_second_document_is_kept() {
    let yaml = "a\n...\n%YAML 1.2\n---\nb\n";
    assert_stable(yaml, yaml);
}

#[test]
fn directives_of_every_document_are_kept() {
    let yaml = "%YAML 1.2\n---\na: 1\n...\n%TAG !e! tag:example.com,2000:\n---\nb: !<tag:example.com,2000:x> 2\n...\n%YAML 1.2\n---\nc: 3\n";
    assert_stable(yaml, yaml);
}

#[test]
fn documents_without_directives_get_no_end_marker() {
    assert_stable("a\n---\nb\n", "a\n---\nb\n");
}

#[test]
fn strip_comments_drops_directive_comment() {
    assert_eq!(
        format("%YAML 1.2 # c\n---\na: 1 # x\n", &["--strip-comments"]),
        "%YAML 1.2\n---\na: 1\n"
    );
    assert_eq!(
        format("a\n...\n%YAML 1.2 # c\n---\nb\n", &["--strip-comments"]),
        "a\n...\n%YAML 1.2\n---\nb\n"
    );
}

#[test]
fn strip_comments_keeps_hash_inside_directive_token() {
    let out = format("%TAG !e! tag:x#y # c\n---\n!e!a 1\n", &["--strip-comments"]);
    assert!(out.starts_with("%TAG !e! tag:x#y\n---\n"), "{out:?}");
}

#[test]
fn reserved_directive_is_kept_with_yaml() {
    let yaml = "a\n...\n%FOO x\n%YAML 1.1\n---\nb\n";
    assert_stable(yaml, yaml);
}

#[test]
fn reserved_directive_before_first_document_is_kept() {
    let yaml = "%FOO bar baz\n%YAML 1.2\n---\na: 1\n";
    assert_stable(yaml, yaml);
}

#[test]
fn reserved_directive_comment_is_cut_with_strip_comments() {
    assert_eq!(
        format("%FOO x # c\n---\na: 1\n", &["--strip-comments"]),
        "%FOO x\n---\na: 1\n"
    );
}

#[test]
fn lone_cr_line_endings_keep_directive() {
    let out = format("a\r...\r%YAML 1.2\r---\rb\r", &[]);
    assert!(out.contains("...\n%YAML 1.2\n---\n"), "{out:?}");
}

#[test]
fn comment_between_document_end_and_directive() {
    let out = format("a\n...\n# note\n%YAML 1.2\n---\nb\n", &["--strip-comments"]);
    assert_eq!(out, "a\n...\n%YAML 1.2\n---\nb\n");
}

#[test]
fn tab_separated_directive_is_kept() {
    let out = format("a\n...\n%YAML\t1.2\n---\nb\n", &[]);
    assert!(out.contains("...\n%YAML\t1.2\n---\n"), "{out:?}");
}

#[test]
fn inline_content_after_directive_document_start() {
    let yaml = "a\n...\n%YAML 1.2\n--- b\n";
    let once = format(yaml, &[]);
    assert!(once.contains("...\n%YAML 1.2\n---"), "{once:?}");
    assert_eq!(format(&once, &[]), once);
}

#[test]
fn non_ascii_crlf_directive_is_kept() {
    let out = format("k: \"\u{e9}\"\r\n...\r\n%YAML 1.2\r\n---\r\nb\r\n", &[]);
    assert_eq!(out, "k: \"\u{e9}\"\n...\n%YAML 1.2\n---\nb\n");
}

#[test]
fn plain_inf_and_nan_are_not_rewritten() {
    let yaml = "a: inf\nb: NaN\nc: -inf\nd: .inf\ne: .nan\nf: !!float inf\n";
    assert_stable(yaml, yaml);
}
