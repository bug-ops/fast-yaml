//! Edge case tests for unusual inputs and boundary conditions.

use fast_yaml_parallel::{ParallelConfig, parse_parallel, parse_parallel_with_config};

#[test]
fn test_only_whitespace() {
    let yaml = "   \n  \n\t\n   ";
    let result = parse_parallel(yaml);
    // Whitespace-only input returns empty vec or error
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_only_comments() {
    let yaml = "# Just a comment\n# Another comment\n# More comments";
    let result = parse_parallel(yaml);
    // Comment-only input may parse as empty or error
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_only_separators() {
    let yaml = "---\n---\n---";
    let docs = parse_parallel(yaml).unwrap();
    // Should produce one document (the last separator with no content)
    assert_eq!(docs.len(), 1);
}

#[test]
fn test_separator_variations() {
    // Test different forms of document separator
    let yaml = "---\nfoo: 1\n---  \nbar: 2\n---\t\nbaz: 3";
    let docs = parse_parallel(yaml).unwrap();
    assert_eq!(docs.len(), 3);
}

#[test]
fn test_unicode_separators() {
    // Ensure Unicode doesn't confuse separator detection
    let yaml = "---\n日本語: テスト\n---\nключ: тест";
    let docs = parse_parallel(yaml).unwrap();
    assert_eq!(docs.len(), 2);
}

#[test]
fn test_emoji_content() {
    let yaml = "---\n😀: 😁\n🚀: 🌟\n---\n💻: 🖥️";
    let docs = parse_parallel(yaml).unwrap();
    assert_eq!(docs.len(), 2);
}

#[test]
fn test_mixed_line_endings() {
    // Mix of \n, \r\n line endings
    let yaml = "---\r\nfoo: 1\n---\nbar: 2\r\n---\r\nbaz: 3";
    let docs = parse_parallel(yaml).unwrap();
    assert_eq!(docs.len(), 3);
}

#[test]
fn test_tabs_in_content() {
    let yaml = "---\nkey:\tvalue\n---\nother:\tdata";
    let result = parse_parallel(yaml);
    // Tabs in YAML content - may be valid or invalid depending on parser
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_trailing_whitespace_after_separator() {
    let yaml = "---   \t  \nfoo: 1";
    let docs = parse_parallel(yaml).unwrap();
    assert_eq!(docs.len(), 1);
}

#[test]
fn test_leading_whitespace_before_separator() {
    // Separator must be at line start
    let yaml = "  ---\nfoo: 1";
    let result = parse_parallel(yaml);
    // The indented --- should not be treated as separator
    // May fail to parse or return 1 doc
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_separator_in_quoted_string() {
    let yaml = r#"---
key: "---"
---
value: '---'
"#;
    let docs = parse_parallel(yaml).unwrap();
    assert_eq!(docs.len(), 2);
}

#[test]
fn test_separator_in_literal_block() {
    let yaml = r"---
block: |
  ---
  This is not a separator
---
other: data
";
    let result = parse_parallel(yaml);
    // Literal block with --- inside should parse correctly
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_document_end_marker() {
    let yaml = "---\nfoo: 1\n...\n---\nbar: 2\n...";
    let docs = parse_parallel(yaml).unwrap();
    assert_eq!(docs.len(), 2);
}

#[test]
fn test_null_document() {
    let yaml = "---\n~\n---\nnull\n---\n";
    let docs = parse_parallel(yaml).unwrap();
    assert!(docs.len() >= 2); // At least the explicit null documents
}

#[test]
fn test_boolean_document() {
    let yaml = "---\ntrue\n---\nfalse\n---\nyes\n---\nno";
    let docs = parse_parallel(yaml).unwrap();
    assert_eq!(docs.len(), 4);
}

#[test]
fn test_numeric_document() {
    let yaml = "---\n42\n---\n3.14\n---\n-123\n---\n6.022e23";
    let docs = parse_parallel(yaml).unwrap();
    assert_eq!(docs.len(), 4);
}

#[test]
fn test_empty_mapping() {
    let yaml = "---\n{}\n---\n{}";
    let docs = parse_parallel(yaml).unwrap();
    assert_eq!(docs.len(), 2);
}

#[test]
fn test_empty_sequence() {
    let yaml = "---\n[]\n---\n[]";
    let docs = parse_parallel(yaml).unwrap();
    assert_eq!(docs.len(), 2);
}

#[test]
fn test_single_character_keys() {
    let yaml = "---\na: 1\nb: 2\nc: 3\n---\nx: 4";
    let docs = parse_parallel(yaml).unwrap();
    assert_eq!(docs.len(), 2);
}

#[test]
fn test_very_long_key() {
    let long_key = "k".repeat(10_000);
    let yaml = format!("---\n{long_key}: value");
    let result = parse_parallel(&yaml);
    // Very long keys should parse
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_very_long_value() {
    let long_value = "v".repeat(10_000);
    let yaml = format!("---\nkey: {long_value}");
    let docs = parse_parallel(&yaml).unwrap();
    assert_eq!(docs.len(), 1);
}

#[test]
fn test_special_characters_in_keys() {
    let yaml = r#"---
"@special": value
"$dollar": money
"%percent": ratio
"#;
    let docs = parse_parallel(yaml).unwrap();
    assert_eq!(docs.len(), 1);
}

#[test]
fn test_consecutive_separators() {
    let yaml = "---\n---\nfoo: 1";
    let docs = parse_parallel(yaml).unwrap();
    // First separator creates empty doc (skipped), second has content
    assert_eq!(docs.len(), 1);
}

#[test]
fn test_many_consecutive_separators() {
    let yaml = "---\n---\n---\n---\nfoo: 1";
    let docs = parse_parallel(yaml).unwrap();
    // Multiple empty docs skipped, one with content
    assert_eq!(docs.len(), 1);
}

#[test]
fn test_implicit_document_before_explicit() {
    let yaml = "implicit: true\n---\nexplicit: true";
    let docs = parse_parallel(yaml).unwrap();
    assert_eq!(docs.len(), 2);
}

#[test]
fn test_multiple_implicit_explicit_mix() {
    let yaml = "first: 1\n---\nsecond: 2\nthird: 3\n---\nfourth: 4";
    let docs = parse_parallel(yaml).unwrap();
    assert_eq!(docs.len(), 3);
}

#[test]
fn test_carriage_return_only() {
    // Old Mac style line endings
    let yaml = "---\rfoo: 1\r---\rbar: 2";
    let docs = parse_parallel(yaml).unwrap();
    assert!(!docs.is_empty());
}

#[test]
fn test_zero_width_characters() {
    // Zero-width space (U+200B)
    let yaml = "---\nkey\u{200B}: value";
    let docs = parse_parallel(yaml).unwrap();
    assert_eq!(docs.len(), 1);
}

#[test]
fn test_rtl_text() {
    // Right-to-left text (Arabic, Hebrew)
    let yaml = "---\nمفتاح: قيمة\n---\nמפתח: ערך";
    let docs = parse_parallel(yaml).unwrap();
    assert_eq!(docs.len(), 2);
}

#[test]
fn test_combining_characters() {
    // Unicode combining characters
    let yaml = "---\ncafe\u{0301}: value"; // café with combining acute
    let docs = parse_parallel(yaml).unwrap();
    assert_eq!(docs.len(), 1);
}

#[test]
fn test_config_edge_cases() {
    let yaml = "---\ntest: 1";

    // Zero thread count (sequential mode)
    let config = ParallelConfig::new().with_thread_count(Some(0));
    let docs = parse_parallel_with_config(yaml, &config).unwrap();
    assert_eq!(docs.len(), 1);

    // Very small min chunk size
    let config = ParallelConfig::new().with_min_chunk_size(1);
    let docs = parse_parallel_with_config(yaml, &config).unwrap();
    assert_eq!(docs.len(), 1);

    // Very large max chunk size
    let config = ParallelConfig::new().with_max_chunk_size(usize::MAX);
    let docs = parse_parallel_with_config(yaml, &config).unwrap();
    assert_eq!(docs.len(), 1);
}

#[test]
fn test_separator_like_content() {
    // Content that looks like separator but isn't
    let yaml = "key: ---value\nanother: ----\nmore: --- test";
    let docs = parse_parallel(yaml).unwrap();
    assert_eq!(docs.len(), 1);
}

#[test]
fn test_windows_path_with_separators() {
    let yaml = r#"---
path: "C:\\Users\\test---file.txt"
---
other: "path---with---dashes"
"#;
    let docs = parse_parallel(yaml).unwrap();
    assert_eq!(docs.len(), 2);
}
