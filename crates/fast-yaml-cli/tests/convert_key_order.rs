//! `fy convert` keeps mapping keys in document order in both directions (#557).

#![allow(clippy::missing_docs_in_private_items)]
#![allow(deprecated)] // Command::cargo_bin is deprecated but still works

use assert_cmd::Command;

fn convert(to: &str, input: &str) -> String {
    let output = Command::cargo_bin("fy")
        .unwrap()
        .args(["convert", to])
        .write_stdin(input)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn yaml_to_json_follows_the_document_order() {
    let json = convert("json", "z: 1\na: 2\nm:\n  y: 1\n  b: 2\n");
    let compact: String = json.split_whitespace().collect();
    assert_eq!(compact, r#"{"z":1,"a":2,"m":{"y":1,"b":2}}"#);
}

#[test]
fn json_to_yaml_follows_the_document_order() {
    let yaml = convert("yaml", r#"{"z":1,"a":2,"m":{"y":1,"b":2}}"#);
    assert_eq!(yaml, "z: 1\na: 2\nm:\n  y: 1\n  b: 2\n");
}
