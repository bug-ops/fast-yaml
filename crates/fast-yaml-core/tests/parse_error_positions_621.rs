//! Every `ParseError` that carries a position reports the 1-based `at` and the 0-based document
//! it was found in (#621).

use fast_yaml_core::limits::{MaxDepth, ParseLimits};
use fast_yaml_core::{
    DocumentIndex, KeyDomain, LimitKind, LoadOptions, MergeError, ParseError, Parser,
    SourcePosition,
};

const fn at(line: usize, column: usize) -> SourcePosition {
    SourcePosition::new(line, column)
}

#[test]
fn merge_errors_carry_the_key_position_and_the_document() {
    let err = Parser::parse_all("a: 1\n---\nm:\n  <<: 1\n").unwrap_err();
    let ParseError::Merge {
        error,
        at: position,
        document,
    } = err
    else {
        panic!("merge error expected");
    };
    assert_eq!(error, MergeError::NotMapping);
    assert_eq!(position, at(4, 3));
    assert_eq!(document, DocumentIndex::new(1));
}

#[test]
fn set_value_errors_carry_the_member_position_and_the_document() {
    let err = Parser::parse_all("a: 1\n---\nb: 2\n---\n!!set {x: 1}\n").unwrap_err();
    let ParseError::SetValue {
        at: position,
        document,
    } = err
    else {
        panic!("set value error expected");
    };
    assert_eq!(position, at(5, 8));
    assert_eq!(document, DocumentIndex::new(2));
}

#[test]
fn key_errors_carry_the_later_key_position_and_the_document() {
    let options = LoadOptions::new().with_keys(KeyDomain::StringKeys);
    let err = Parser::parse_all_with_options(
        "a: 1\n---\n1: x\n\"1\": y\n",
        &ParseLimits::default(),
        options,
    )
    .unwrap_err();
    let ParseError::Key {
        at: position,
        document,
        ..
    } = err
    else {
        panic!("key error expected");
    };
    assert_eq!(position, at(4, 1));
    assert_eq!(document, DocumentIndex::new(1));
}

#[test]
fn limit_errors_carry_the_event_position_and_the_document() {
    let limits = ParseLimits {
        max_depth: MaxDepth::new(2).unwrap(),
        ..ParseLimits::default()
    };
    let err =
        Parser::parse_all_with_limits("a: 1\n---\nb:\n  c:\n    d: 1\n", &limits).unwrap_err();
    let ParseError::LimitExceeded {
        kind,
        at: position,
        document,
    } = err
    else {
        panic!("limit error expected");
    };
    assert!(matches!(kind, LimitKind::Depth(_)));
    assert_eq!(position.line, 5);
    assert_eq!(document, DocumentIndex::new(1));
}

#[test]
fn syntax_errors_carry_the_position_and_the_document() {
    let err = Parser::parse_all("a: 1\n---\nb: 2\n---\nc: [\n").unwrap_err();
    assert!(matches!(err, ParseError::Syntax(_)));
    assert_eq!(err.document_index(), DocumentIndex::new(2));
    assert!(err.position().line >= 5);
}

#[test]
fn display_names_the_document_only_after_the_first() {
    let first = Parser::parse_all("m:\n  <<: 1\n").unwrap_err().to_string();
    assert!(first.ends_with("line 2, column 3"), "{first}");
    let later = Parser::parse_all("a: 1\n---\nm:\n  <<: 1\n")
        .unwrap_err()
        .to_string();
    assert!(later.ends_with("line 4, column 3 (document 2)"), "{later}");
}

#[test]
fn relocated_moves_the_line_and_the_document_but_not_the_column() {
    let err = Parser::parse_all("m:\n  <<: 1\n")
        .unwrap_err()
        .relocated(10, 3);
    let ParseError::Merge {
        at: position,
        document,
        ..
    } = err
    else {
        panic!("merge error expected");
    };
    assert_eq!(position, at(12, 3));
    assert_eq!(document, DocumentIndex::new(3));
}

#[test]
fn document_index_arithmetic_does_not_overflow() {
    let last = DocumentIndex::new(usize::MAX);
    assert_eq!(last.after(1), last);
    assert_eq!(last.number(), usize::MAX);
}

#[test]
fn relocating_by_a_huge_amount_saturates() {
    let err = Parser::parse_all("m:\n  <<: 1\n")
        .unwrap_err()
        .relocated(usize::MAX, usize::MAX);
    assert_eq!(err.position(), at(usize::MAX, 3));
    assert_eq!(err.document_index(), DocumentIndex::new(usize::MAX));
}
