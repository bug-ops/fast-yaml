//! Comprehensive integration tests for the `fy` CLI tool.
//!
//! Tests cover all commands, options, flags, and error conditions.

#![allow(clippy::missing_docs_in_private_items)]
#![allow(deprecated)] // Command::cargo_bin is deprecated but still works

use assert_cmd::Command;
use indoc::indoc;
use predicates::prelude::*;
use std::fs;
use std::io::Write;
use tempfile::{NamedTempFile, TempDir};

// Helper function to create a temporary YAML file
fn create_temp_yaml(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    write!(file, "{content}").unwrap();
    file
}

// Helper function to create a temporary JSON file
fn create_temp_json(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    write!(file, "{content}").unwrap();
    file
}

// =============================================================================
// PARSE COMMAND TESTS
// =============================================================================

#[test]
fn test_parse_valid_yaml_stdin() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("parse")
        .write_stdin("name: test\nvalue: 123")
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains("YAML is valid"));
}

#[test]
fn test_parse_valid_yaml_file() {
    let file = create_temp_yaml("name: test\nvalue: 123");

    Command::cargo_bin("fy")
        .unwrap()
        .arg("parse")
        .arg(file.path())
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains("YAML is valid"));
}

#[test]
fn test_parse_with_stats_flag() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("parse")
        .arg("--stats")
        .write_stdin("name: test\nvalue: 123\nnested:\n  key: value")
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains("Statistics:"))
        .stdout(predicate::str::contains("Keys:"))
        .stdout(predicate::str::contains("Max depth:"));
}

#[test]
fn test_parse_quiet_mode() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("--quiet")
        .arg("parse")
        .write_stdin("name: test")
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::is_empty());
}

#[test]
fn test_parse_invalid_yaml_syntax() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("parse")
        .write_stdin("invalid: [unclosed")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("error:"));
}

#[test]
fn test_parse_empty_document() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("parse")
        .write_stdin("")
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains("YAML is valid"));
}

#[test]
fn test_parse_null_yaml() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("parse")
        .write_stdin("~")
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains("YAML is valid"));
}

#[test]
fn test_parse_comment_only_yaml() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("parse")
        .write_stdin("# just a comment\n")
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains("YAML is valid"));
}

#[test]
fn test_parse_complex_nested_yaml() {
    let yaml = indoc! {"
        server:
          host: localhost
          port: 8080
          ssl:
            enabled: true
            cert: /path/to/cert
        database:
          connections:
            - host: db1
              port: 5432
            - host: db2
              port: 5432
    "};

    Command::cargo_bin("fy")
        .unwrap()
        .arg("parse")
        .arg("--stats")
        .write_stdin(yaml)
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains("YAML is valid"));
}

// =============================================================================
// FORMAT COMMAND TESTS
// =============================================================================

#[test]
fn test_format_stdin_to_stdout() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("format")
        .write_stdin("name:   test\nvalue:    123")
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains("name:"))
        .stdout(predicate::str::contains("value:"));
}

#[test]
fn test_format_file_to_stdout() {
    let file = create_temp_yaml("name:   test\nvalue:    123");

    Command::cargo_bin("fy")
        .unwrap()
        .arg("format")
        .arg(file.path())
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains("name:"));
}

#[test]
fn test_format_with_custom_indent() {
    let yaml = "parent:\n  child: value";

    Command::cargo_bin("fy")
        .unwrap()
        .arg("format")
        .arg("--indent")
        .arg("4")
        .write_stdin(yaml)
        .assert()
        .success()
        .code(0);
}

#[test]
fn test_format_with_custom_width() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("format")
        .arg("--width")
        .arg("120")
        .write_stdin("name: test")
        .assert()
        .success()
        .code(0);
}

#[test]
fn test_format_in_place_flag() {
    let file = create_temp_yaml("name:   test\nvalue:    123");
    let path = file.path().to_path_buf();

    Command::cargo_bin("fy")
        .unwrap()
        .arg("format")
        .arg("-i")
        .arg(&path)
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::is_empty());

    // Verify file was modified
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("name:"));
}

#[test]
fn test_format_to_output_file() {
    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("output.yaml");

    Command::cargo_bin("fy")
        .unwrap()
        .arg("format")
        .arg("--output")
        .arg(&output_path)
        .write_stdin("name: test")
        .assert()
        .success()
        .code(0);

    // Verify output file exists and contains formatted YAML
    assert!(output_path.exists());
    let content = fs::read_to_string(&output_path).unwrap();
    assert!(content.contains("name:"));
}

#[test]
fn test_format_invalid_yaml() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("format")
        .write_stdin("invalid: [")
        .assert()
        .failure()
        .code(1);
}

// =============================================================================
// CONVERT COMMAND TESTS
// =============================================================================

#[test]
fn test_convert_yaml_to_json() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("convert")
        .arg("json")
        .write_stdin("name: test\nvalue: 123")
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains("\"name\""))
        .stdout(predicate::str::contains("\"test\""));
}

#[test]
fn test_convert_yaml_to_json_scalar_resolution_edges() {
    let input = concat!(
        "v: !!int \"7\"\n",
        "w: !!float \"1.5\"\n",
        "x: !!bool \"true\"\n",
        "y: !!int true\n",
        "a: !!int 9223372036854775807\n",
        "b: !!int 9223372036854775808\n",
        "c: -9223372036854775809\n",
        "d: +-5\n",
        "e: !!int\n",
        "f: !!foo 7\n",
        "g: !\n",
        "h: [!!int \"1\", !!bool \"true\"]\n",
        "i: &x !!float \"2.5\"\n",
        "j: *x\n",
    );
    Command::cargo_bin("fy")
        .unwrap()
        .arg("convert")
        .arg("json")
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"v\": 7"))
        .stdout(predicate::str::contains("\"w\": 1.5"))
        .stdout(predicate::str::contains("\"x\": true"))
        .stdout(predicate::str::contains("\"y\": \"true\""))
        .stdout(predicate::str::contains("\"a\": 9223372036854775807"))
        .stdout(predicate::str::contains("\"b\": 9223372036854775808"))
        .stdout(predicate::str::contains("\"c\": -9223372036854775809"))
        .stdout(predicate::str::contains("\"d\": \"+-5\""))
        .stdout(predicate::str::contains("\"e\": \"\""))
        .stdout(predicate::str::contains("\"f\": \"7\""))
        .stdout(predicate::str::contains("\"g\": \"\""))
        .stdout(predicate::str::contains("\"i\": 2.5"))
        .stdout(predicate::str::contains("\"j\": 2.5"));
}

#[test]
fn test_convert_yaml_to_json_radix_big_integers() {
    let input = concat!(
        "hex: 0xFFFFFFFFFFFFFFFFFF\n",
        "neg: -0xFFFFFFFFFFFFFFFFFF\n",
        "oct: 0o7777777777777777777777\n",
        "min: -0x8000000000000000\n",
        "tagged: !!int 0x8000000000000000\n",
        "plus: +0099999999999999999999\n",
        "0xFFFFFFFFFFFFFFFFFF: key\n",
    );
    Command::cargo_bin("fy")
        .unwrap()
        .arg("convert")
        .arg("json")
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"hex\": 4722366482869645213695"))
        .stdout(predicate::str::contains("\"neg\": -4722366482869645213695"))
        .stdout(predicate::str::contains("\"oct\": 73786976294838206463"))
        .stdout(predicate::str::contains("\"min\": -9223372036854775808"))
        .stdout(predicate::str::contains("\"tagged\": 9223372036854775808"))
        .stdout(predicate::str::contains("\"plus\": 99999999999999999999"))
        .stdout(predicate::str::contains(
            "\"4722366482869645213695\": \"key\"",
        ));
}

#[test]
fn test_convert_yaml_to_json_negative_octal_i64_min() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("convert")
        .arg("json")
        .write_stdin("w: -0o1000000000000000000000\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"w\": -9223372036854775808"));
}

#[test]
fn test_convert_yaml_to_json_hex_beyond_bit_cap_is_string() {
    let literal = format!("0x1{}", "0".repeat(3571));
    Command::cargo_bin("fy")
        .unwrap()
        .arg("convert")
        .arg("json")
        .write_stdin(format!("v: {literal}\n"))
        .assert()
        .success()
        .stdout(predicate::str::contains(format!("\"v\": \"{literal}\"")));
}

#[test]
fn test_convert_yaml_to_json_tagged_multi_document() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("convert")
        .arg("json")
        .write_stdin("--- !!int \"1\"\n--- !!bool \"false\"\n")
        .assert()
        .success()
        .stdout("[\n  1,\n  false\n]\n");
}

#[test]
fn test_convert_json_to_yaml() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("convert")
        .arg("yaml")
        .write_stdin(r#"{"name": "test", "value": 123}"#)
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains("name:"))
        .stdout(predicate::str::contains("test"));
}

#[test]
fn test_convert_yaml_to_json_file_input() {
    let file = create_temp_yaml("name: test\nvalue: 123");

    Command::cargo_bin("fy")
        .unwrap()
        .arg("convert")
        .arg("json")
        .arg(file.path())
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains("\"name\""));
}

#[test]
fn test_convert_json_to_yaml_file_input() {
    let file = create_temp_json(r#"{"name": "test", "value": 123}"#);

    Command::cargo_bin("fy")
        .unwrap()
        .arg("convert")
        .arg("yaml")
        .arg(file.path())
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains("name:"));
}

#[test]
fn test_convert_with_pretty_flag_true() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("convert")
        .arg("json")
        .arg("--pretty")
        .write_stdin("name: test\nvalue: 123")
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains("{\n"));
}

#[test]
fn test_convert_with_pretty_flag_false() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("convert")
        .arg("json")
        .arg("--pretty=false")
        .write_stdin("name: test\nvalue: 123")
        .assert()
        .success()
        .code(0);
}

#[test]
fn test_convert_complex_yaml_to_json() {
    let yaml = indoc! {"
        server:
          host: localhost
          port: 8080
        features:
          - authentication
          - logging
          - monitoring
    "};

    Command::cargo_bin("fy")
        .unwrap()
        .arg("convert")
        .arg("json")
        .write_stdin(yaml)
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains("\"server\""))
        .stdout(predicate::str::contains("\"features\""));
}

#[test]
fn test_convert_invalid_yaml_to_json() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("convert")
        .arg("json")
        .write_stdin("invalid: [")
        .assert()
        .failure()
        .code(1);
}

#[test]
fn test_convert_invalid_json_to_yaml() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("convert")
        .arg("yaml")
        .write_stdin("{invalid json}")
        .assert()
        .failure()
        .code(1);
}

#[test]
fn test_convert_with_output_file() {
    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("output.json");

    Command::cargo_bin("fy")
        .unwrap()
        .arg("convert")
        .arg("json")
        .arg("--output")
        .arg(&output_path)
        .write_stdin("name: test")
        .assert()
        .success()
        .code(0);

    assert!(output_path.exists());
    let content = fs::read_to_string(&output_path).unwrap();
    assert!(content.contains("\"name\""));
}

// =============================================================================
// LINT COMMAND TESTS (requires 'linter' feature)
// =============================================================================

#[test]
#[cfg(feature = "linter")]
fn test_lint_valid_yaml() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("lint")
        .write_stdin("name: test\nvalue: 123\n")
        .assert()
        .success()
        .code(0);
}

#[test]
#[cfg(feature = "linter")]
fn test_lint_with_warnings() {
    let long_line = "name: this is a very very very very very very very very very very very very very very very very very very very very long line";

    // line-length rule emits info-severity diagnostics.
    Command::cargo_bin("fy")
        .unwrap()
        .arg("lint")
        .arg("--max-line-length")
        .arg("80")
        .write_stdin(long_line)
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains("line-length"));
}

#[test]
#[cfg(feature = "linter")]
fn test_lint_invalid_yaml() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("lint")
        .write_stdin("invalid: [unclosed")
        .assert()
        .failure()
        .code(1);
}

#[test]
#[cfg(feature = "linter")]
fn test_lint_with_max_line_length_option() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("lint")
        .arg("--max-line-length")
        .arg("120")
        .write_stdin("name: test\n")
        .assert()
        .success()
        .code(0);
}

#[test]
#[cfg(feature = "linter")]
fn test_lint_with_indent_size_option() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("lint")
        .arg("--indent-size")
        .arg("4")
        .write_stdin("name: test\n")
        .assert()
        .success()
        .code(0);
}

#[test]
#[cfg(feature = "linter")]
fn test_lint_text_format() {
    // Clean YAML produces no diagnostics — stdout must be empty.
    Command::cargo_bin("fy")
        .unwrap()
        .arg("lint")
        .arg("--format")
        .arg("text")
        .write_stdin("name: test\nvalue: 123\n")
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::is_empty());
}

#[test]
#[cfg(feature = "linter")]
fn test_lint_json_format() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("lint")
        .arg("--format")
        .arg("json")
        .write_stdin("name: test\nvalue: 123\n")
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::starts_with("["));
}

#[test]
#[cfg(feature = "linter")]
fn test_lint_quiet_mode() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("--quiet")
        .arg("lint")
        .write_stdin("name: test\n")
        .assert()
        .success()
        .code(0);
}

#[test]
#[cfg(feature = "linter")]
fn test_lint_verbose_mode() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("--verbose")
        .arg("lint")
        .write_stdin("name: test\n")
        .assert()
        .success()
        .code(0)
        .stderr(predicate::str::contains("Lint time:"));
}

#[test]
#[cfg(feature = "linter")]
fn test_lint_file_input() {
    let file = create_temp_yaml("name: test\nvalue: 123\n");

    Command::cargo_bin("fy")
        .unwrap()
        .arg("lint")
        .arg(file.path())
        .assert()
        .success()
        .code(0);
}

#[test]
#[cfg(feature = "linter")]
fn test_lint_trailing_whitespace() {
    // trailing-whitespace rule emits hint-severity diagnostics.
    Command::cargo_bin("fy")
        .unwrap()
        .arg("lint")
        .write_stdin("name: test   \nvalue: 123\n")
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains("trailing-whitespace"));
}

// =============================================================================
// DEFAULT COMMAND (no subcommand - should format)
// =============================================================================

#[test]
fn test_default_command_formats() {
    Command::cargo_bin("fy")
        .unwrap()
        .write_stdin("name:   test\nvalue:    123")
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains("name:"));
}

// Default command no longer supports file argument - use `fy format <file>` instead
// #[test]
// fn test_default_command_with_file() {
//     let file = create_temp_yaml("name: test");
//
//     Command::cargo_bin("fy")
//         .unwrap()
//         .arg(file.path())
//         .assert()
//         .success()
//         .code(0)
//         .stdout(predicate::str::contains("name:"));
// }

// =============================================================================
// GLOBAL FLAGS TESTS
// =============================================================================

#[test]
fn test_no_color_flag() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("--no-color")
        .arg("parse")
        .write_stdin("name: test")
        .assert()
        .success();
}

#[test]
fn test_quiet_flag_suppresses_output() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("--quiet")
        .arg("parse")
        .write_stdin("name: test")
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn test_verbose_flag() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("--verbose")
        .arg("parse")
        .write_stdin("name: test")
        .assert()
        .success();
}

#[test]
fn test_version_flag() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("fy"));
}

#[test]
fn test_help_flag() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Fast YAML"))
        .stdout(predicate::str::contains("Commands:"));
}

#[test]
fn test_parse_help() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("parse")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Parse and validate YAML"));
}

#[test]
fn test_format_help() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("format")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Format YAML"));
}

#[test]
fn test_convert_help() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("convert")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Convert between YAML and JSON"));
}

#[test]
#[cfg(feature = "linter")]
fn test_lint_help() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("lint")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Lint YAML"));
}

// =============================================================================
// ERROR HANDLING AND EXIT CODES
// =============================================================================

#[test]
fn test_file_not_found() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("parse")
        .arg("/nonexistent/file.yaml")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("Failed to read file"));
}

#[test]
fn test_invalid_argument() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("parse")
        .arg("--invalid-flag")
        .write_stdin("name: test")
        .assert()
        .failure();
}

#[test]
fn test_in_place_without_file() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("format")
        .arg("-i")
        .write_stdin("name: test")
        .assert()
        .failure()
        .stderr(predicate::str::contains("requires"));
}

#[test]
fn test_parse_error_exit_code() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("parse")
        .write_stdin("invalid: [")
        .assert()
        .failure()
        .code(1);
}

#[test]
#[cfg(feature = "linter")]
fn test_lint_error_exit_code() {
    Command::cargo_bin("fy")
        .unwrap()
        .arg("lint")
        .write_stdin("invalid: [")
        .assert()
        .failure()
        .code(1);
}

// =============================================================================
// EDGE CASES AND SPECIAL SCENARIOS
// =============================================================================

#[test]
fn test_unicode_content() {
    let yaml = "name: тест\nvalue: 日本語\nemoji: 🚀";

    Command::cargo_bin("fy")
        .unwrap()
        .arg("parse")
        .arg("--quiet")
        .write_stdin(yaml)
        .assert()
        .success();
}

#[test]
fn test_multiline_strings() {
    let yaml = indoc! {"
        description: |
          This is a multiline
          string with multiple
          lines of content.
        folded: >
          This is a folded
          string.
    "};

    Command::cargo_bin("fy")
        .unwrap()
        .arg("parse")
        .arg("--quiet")
        .write_stdin(yaml)
        .assert()
        .success();
}

#[test]
fn test_large_numbers() {
    let yaml = "big_int: 9223372036854775807\nfloat: 3.14159265358979323846";

    Command::cargo_bin("fy")
        .unwrap()
        .arg("parse")
        .arg("--quiet")
        .write_stdin(yaml)
        .assert()
        .success();
}

#[test]
fn test_special_yaml_values() {
    let yaml = indoc! {r#"
        null_value: null
        bool_true: true
        bool_false: false
        empty_string: ""
        number_string: "123"
    "#};

    Command::cargo_bin("fy")
        .unwrap()
        .arg("parse")
        .arg("--quiet")
        .write_stdin(yaml)
        .assert()
        .success();
}

#[test]
fn test_empty_collections() {
    let yaml = indoc! {"
        empty_map: {}
        empty_array: []
        empty_nested:
          map: {}
          array: []
    "};

    Command::cargo_bin("fy")
        .unwrap()
        .arg("parse")
        .arg("--quiet")
        .write_stdin(yaml)
        .assert()
        .success();
}

#[test]
fn test_convert_preserves_types() {
    let yaml = indoc! {r#"
        string: "hello"
        number: 123
        float: 3.14
        bool: true
        null_value: null
    "#};

    let output = Command::cargo_bin("fy")
        .unwrap()
        .arg("convert")
        .arg("json")
        .write_stdin(yaml)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json_str = String::from_utf8(output).unwrap();
    assert!(json_str.contains("\"string\": \"hello\""));
    assert!(json_str.contains("\"number\": 123"));
    assert!(json_str.contains("\"bool\": true"));
}

#[test]
fn test_roundtrip_yaml_json_yaml() {
    let original_yaml = "name: test\nvalue: 123\n";

    // Convert to JSON
    let json_output = Command::cargo_bin("fy")
        .unwrap()
        .arg("convert")
        .arg("json")
        .write_stdin(original_yaml)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    // Convert back to YAML
    Command::cargo_bin("fy")
        .unwrap()
        .arg("convert")
        .arg("yaml")
        .write_stdin(String::from_utf8(json_output).unwrap())
        .assert()
        .success()
        .stdout(predicate::str::contains("name:"))
        .stdout(predicate::str::contains("value:"));
}

#[test]
fn test_convert_yaml_non_string_keys_to_json() {
    // YAML maps with null, boolean, and integer keys should convert to JSON
    // with those keys coerced to their string representations.
    let yaml = "null: null_val\ntrue: bool_val\n42: int_val\n";
    let output = Command::cargo_bin("fy")
        .unwrap()
        .arg("convert")
        .arg("json")
        .write_stdin(yaml)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json_str = String::from_utf8(output).unwrap();
    let json: serde_json::Value = serde_json::from_str(json_str.trim()).unwrap();
    assert_eq!(json["null"], "null_val");
    assert_eq!(json["true"], "bool_val");
    assert_eq!(json["42"], "int_val");
}

// =============================================================================
// FILE ARGUMENT POSITION TESTS
// =============================================================================
// Tests that file argument works both before and after subcommand

#[test]
fn test_parse_file_after_subcommand() {
    let file = create_temp_yaml("name: test\nvalue: 123");

    // New syntax: fy parse file.yaml
    Command::cargo_bin("fy")
        .unwrap()
        .arg("parse")
        .arg(file.path())
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains("YAML is valid"));
}

// Old syntax (file before subcommand) is no longer supported
// Use: fy parse file.yaml instead
// #[test]
// fn test_parse_file_before_subcommand() {
//     let file = create_temp_yaml("name: test\nvalue: 123");
//
//     // Old syntax: fy file.yaml parse
//     Command::cargo_bin("fy")
//         .unwrap()
//         .arg(file.path())
//         .arg("parse")
//         .assert()
//         .success()
//         .code(0)
//         .stdout(predicate::str::contains("YAML is valid"));
// }

#[test]
fn test_format_file_after_subcommand() {
    let file = create_temp_yaml("name:   test\nvalue:    123");

    Command::cargo_bin("fy")
        .unwrap()
        .arg("format")
        .arg(file.path())
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains("name:"));
}

#[test]
fn test_convert_file_after_subcommand() {
    let file = create_temp_yaml("name: test\nvalue: 123");

    Command::cargo_bin("fy")
        .unwrap()
        .arg("convert")
        .arg("json")
        .arg(file.path())
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains("\"name\""));
}

#[test]
#[cfg(feature = "linter")]
fn test_lint_file_after_subcommand() {
    let file = create_temp_yaml("name: test\nvalue: 123\n");

    Command::cargo_bin("fy")
        .unwrap()
        .arg("lint")
        .arg(file.path())
        .assert()
        .success()
        .code(0);
}

#[test]
fn test_format_file_with_flags_after_subcommand() {
    let file = create_temp_yaml("parent:\n  child: value");

    Command::cargo_bin("fy")
        .unwrap()
        .arg("format")
        .arg("--indent")
        .arg("4")
        .arg(file.path())
        .assert()
        .success()
        .code(0);
}

#[test]
fn test_parse_file_with_stats_after_subcommand() {
    let file = create_temp_yaml("name: test\nvalue: 123");

    Command::cargo_bin("fy")
        .unwrap()
        .arg("parse")
        .arg("--stats")
        .arg(file.path())
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains("Statistics:"));
}

#[test]
fn test_convert_json_rejects_distinct_keys_with_the_same_json_key() {
    Command::cargo_bin("fy")
        .unwrap()
        .args(["convert", "json"])
        .write_stdin("1: a\n1.0: b\n")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains(
            "float key \"1\" is distinct in YAML but converts to the same string key as a key of type int at line 2, column 1",
        ));
}

#[test]
fn test_rejected_character_reports_its_document_on_every_command() {
    for args in [
        &["parse"][..],
        &["format"][..],
        &["lint"][..],
        &["convert", "json"][..],
    ] {
        for (input, document) in [
            ("a: \u{1}\n", None),
            ("a: 1\n---\nb: \u{1}\n", Some("(document 2)")),
            ("a: 1\n---\nb: 2\n---\nc: \u{1}", Some("(document 3)")),
            (
                "\u{FEFF}a: 1\n...\n\u{FEFF}b: \u{7F}\n",
                Some("(document 2)"),
            ),
        ] {
            let assert = Command::cargo_bin("fy")
                .unwrap()
                .args(args)
                .write_stdin(input)
                .assert()
                .failure();
            let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
            match document {
                Some(marker) => assert!(stderr.contains(marker), "{args:?} {input:?}: {stderr}"),
                None => assert!(
                    !stderr.contains("(document"),
                    "{args:?} {input:?}: {stderr}"
                ),
            }
        }
    }
}

#[test]
fn test_non_printable_characters_are_rejected_by_every_command() {
    for (args, input) in [
        (&["parse"][..], "a: \u{FFFE}\n"),
        (&["format"][..], "a: 1\n\u{7F}\n"),
        (&["lint"][..], "a: \"x\u{86}\"\n"),
        (&["convert", "json"][..], "a: 1\u{FFFF}\n"),
    ] {
        Command::cargo_bin("fy")
            .unwrap()
            .args(args)
            .write_stdin(input)
            .assert()
            .failure()
            .stderr(predicate::str::contains("not allowed in YAML"));
    }
}
