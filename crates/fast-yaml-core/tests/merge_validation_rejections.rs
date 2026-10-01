//! Invalid merge values, repeated `<<` keys and set member values are rejected at their key.

use fast_yaml_core::{MergeError, ParseError, Parser};

fn merge_error(yaml: &str) -> Option<MergeError> {
    match Parser::parse_str(yaml) {
        Err(ParseError::Merge { error, .. }) => Some(error),
        _ => None,
    }
}

#[test]
fn scalar_merge_value_is_rejected() {
    assert_eq!(merge_error("m:\n  <<: 1\n"), Some(MergeError::NotMapping));
    assert_eq!(merge_error("<<: text\n"), Some(MergeError::NotMapping));
    assert_eq!(merge_error("<<:\n"), Some(MergeError::NotMapping));
}

#[test]
fn scalar_item_of_merge_sequence_is_rejected() {
    assert_eq!(
        merge_error("a: &a {x: 1}\nm:\n  <<: [*a, 2]\n"),
        Some(MergeError::NotMapping)
    );
    assert_eq!(merge_error("<<: [1, 2]\n"), Some(MergeError::NotMapping));
}

#[test]
fn anchored_sequence_with_scalars_is_rejected_when_merged_later() {
    assert_eq!(
        merge_error("s: &s [1, 2]\nm:\n  <<: *s\n"),
        Some(MergeError::NotMapping)
    );
    assert_eq!(
        merge_error("s: &s\n  - 1\nm:\n  <<: [*s]\n"),
        Some(MergeError::NotMapping)
    );
}

#[test]
fn tagged_and_anchored_merge_values_are_rejected() {
    assert_eq!(merge_error("!!merge <<: 1\n"), Some(MergeError::NotMapping));
    assert_eq!(merge_error("<<: &a 1\n"), Some(MergeError::NotMapping));
    assert_eq!(
        merge_error("a: &a 1\nm:\n  <<: *a\n"),
        Some(MergeError::NotMapping)
    );
}

#[test]
fn duplicate_merge_key_is_rejected() {
    assert_eq!(
        merge_error("a: &a {x: 1}\nm:\n  <<: *a\n  y: 1\n  <<: *a\n"),
        Some(MergeError::DuplicateKey)
    );
}

#[test]
fn set_member_value_is_rejected_but_null_is_not() {
    assert!(matches!(
        Parser::parse_str("!!set {a: 1}\n"),
        Err(ParseError::SetValue { .. })
    ));
    assert!(Parser::parse_str("!!set {a, b: }\n").is_ok());
}

#[test]
fn valid_documents_with_plain_scalars_pass() {
    let yaml = "base: &b {x: 1}\nitems: [1, 2, three]\nm:\n  <<: *b\n  y: 2\nl: [*b, *b]\nn:\n  <<: [*b]\n";
    assert!(Parser::parse_str(yaml).is_ok());
}

#[test]
fn anchors_do_not_leak_between_documents() {
    assert_eq!(
        merge_error("a: &a 1\n---\nb: &c {x: 1}\nm:\n  <<: 1\n"),
        Some(MergeError::NotMapping)
    );
    assert!(Parser::parse_all("a: &a {x: 1}\n---\nb: &c {y: 1}\nm:\n  <<: *c\n").is_ok());
    assert!(Parser::parse_all("a: &a {x: 1}\n---\nm:\n  <<: *a\n").is_err());
}

#[test]
fn error_position_is_the_merge_key() {
    let err = Parser::parse_str("a: 1\nm:\n  k: 2\n  <<: 3\n").unwrap_err();
    assert!(matches!(
        err,
        ParseError::Merge {
            line: 4,
            column: 3,
            ..
        }
    ));
}
