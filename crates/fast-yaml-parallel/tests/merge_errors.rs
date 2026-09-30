//! Merge-key errors in batch file processing: document index and formatter rejection.

use fast_yaml_core::limits::MaxDepth;
use fast_yaml_core::{EmitterConfig, ParseLimits};
use fast_yaml_parallel::{CommentPolicy, Config, Error, FileProcessor};
use std::fs;
use tempfile::TempDir;

const BAD_SECOND_DOCUMENT: &str = "x: 1\n---\nm:\n  <<: [1]\n";
const BAD_THIRD_DOCUMENT: &str = "x: 1\n---\ny: 2\n---\nm: {<<: 1}\n";

#[test]
fn parse_files_reports_the_failing_document_index() {
    let dir = TempDir::new().unwrap();
    let second = dir.path().join("second.yaml");
    let third = dir.path().join("third.yaml");
    fs::write(&second, BAD_SECOND_DOCUMENT).unwrap();
    fs::write(&third, BAD_THIRD_DOCUMENT).unwrap();

    let result = FileProcessor::new().parse_files(&[second.clone(), third]);

    assert_eq!(result.failed, 2);
    for (path, error) in &result.errors {
        let Error::Parse { index, .. } = error else {
            panic!("expected Error::Parse, got {error:?}");
        };
        let expected = if *path == second { 1 } else { 2 };
        assert_eq!(*index, expected, "{}", path.display());
    }
}

#[test]
fn parse_files_reports_the_failing_document_index_for_scanner_errors() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("broken.yaml");
    fs::write(&path, "x: 1\n---\ny: 2\n---\na: [\n").unwrap();

    let result = FileProcessor::new().parse_files(std::slice::from_ref(&path));

    let Some((_, Error::Parse { index, .. })) = result.errors.first() else {
        panic!("expected a parse error");
    };
    assert_eq!(*index, 2);
}

#[test]
fn parse_files_reports_index_zero_for_first_document_errors() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("broken.yaml");
    fs::write(&path, "a: [\n---\nx: 1\n").unwrap();

    let result = FileProcessor::new().parse_files(std::slice::from_ref(&path));

    let Some((_, Error::Parse { index, .. })) = result.errors.first() else {
        panic!("expected a parse error");
    };
    assert_eq!(*index, 0);
}

#[test]
fn parse_files_reports_the_failing_document_index_for_limit_errors() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("deep.yaml");
    let deep = format!("{}1{}", "[".repeat(10), "]".repeat(10));
    fs::write(&path, format!("x: 1\n---\ny: 2\n---\nz: {deep}\n")).unwrap();

    let limits = ParseLimits {
        max_depth: MaxDepth::new(4).unwrap(),
        ..ParseLimits::default()
    };
    let processor = FileProcessor::with_config(Config::new().with_parse_limits(limits));
    let result = processor.parse_files(std::slice::from_ref(&path));

    let Some((_, Error::Parse { index, source })) = result.errors.first() else {
        panic!("expected a parse error");
    };
    assert!(
        matches!(source, fast_yaml_core::ParseError::LimitExceeded { .. }),
        "{source:?}"
    );
    assert_eq!(*index, 2);
}

#[test]
fn parse_files_index_counts_empty_and_terminated_documents() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("marks.yaml");
    fs::write(&path, "---\n---\na: 1\n...\n---\nm:\n  <<: 1\n").unwrap();

    let result = FileProcessor::new().parse_files(std::slice::from_ref(&path));

    let Some((_, Error::Parse { index, source })) = result.errors.first() else {
        panic!("expected a parse error");
    };
    assert_eq!(*index, 2);
    assert!(source.to_string().contains("(document 3)"), "{source}");
}

#[test]
fn error_message_uses_one_based_document_numbers() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("second.yaml");
    fs::write(&path, BAD_SECOND_DOCUMENT).unwrap();

    let result = FileProcessor::new().parse_files(std::slice::from_ref(&path));

    let text = result.errors[0].1.to_string();
    assert!(text.starts_with("failed to parse YAML:"), "{text}");
    assert_eq!(text.matches("document 2").count(), 1, "{text}");
}

#[test]
fn format_in_place_rewrites_valid_merge_keys() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("ok.yaml");
    fs::write(&path, "a: &a {x: 1}\nm:\n    <<: *a\n").unwrap();

    let result = FileProcessor::new().format_in_place(
        std::slice::from_ref(&path),
        &EmitterConfig::new(),
        CommentPolicy::Reject,
    );

    assert_eq!(result.failed, 0, "{:?}", result.errors);
    assert_eq!(result.changed, 1);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "a: &a\n  x: 1\nm:\n  <<: *a\n"
    );
}

#[test]
fn format_in_place_rejects_invalid_merge_and_keeps_the_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bad.yaml");
    fs::write(&path, BAD_SECOND_DOCUMENT).unwrap();

    let result = FileProcessor::new().format_in_place(
        std::slice::from_ref(&path),
        &EmitterConfig::new(),
        CommentPolicy::Reject,
    );

    assert_eq!(result.failed, 1);
    assert_eq!(fs::read_to_string(&path).unwrap(), BAD_SECOND_DOCUMENT);
}
