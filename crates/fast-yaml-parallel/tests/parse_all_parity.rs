//! Differential tests: `parse_parallel` must be observationally equal to `Parser::parse_all`.

use fast_yaml_core::Parser;
use fast_yaml_parallel::{Config, Error, parse_parallel, parse_parallel_with_config};
use std::fmt::Write as _;

const ERROR_INPUTS: &[&str] = &[
    "a:\n  - x\n  ---\n",
    "---\n---\na: 1\n---\nb: 'unclosed\n",
    "a: 1\n---\nb: 'unclosed\n---\nc: 3\n",
    "a: 1\n...\n\u{FEFF}x: [\n",
    "a: [x\n---\nb: 1\n",
];

const INPUTS: &[&str] = &[
    "日本\n...\n\u{FEFF}\n: v\n",
    "a\n...\n\u{FEFF}%YAML 1.2\n---\nb\n",
    "a\n...\n\u{FEFF}# c\n\u{FEFF}---\nb\n",
    "a\n\u{FEFF}b\n",
    "a: 1\n---\n\u{FEFF}--- b\n",
    "a: 1\n...\n\u{FEFF}b: 2\n",
    "\u{FEFF}\u{FEFF}a: 1\n",
    "\u{FEFF}\u{FEFF}",
    "a: 1\r\n...\r\n\u{FEFF}b: 2\r\n",
    "a: 1\r...\r\u{FEFF}b: 2\r",
    "a: 1\n...\n\u{FEFF}b: 2\n...\n\u{FEFF}c: 3\n",
    "a: |\n  x\n  ---\n  y\nb: 1\n",
    "|\n ---\n---\nx",
    "a: \"multi\n  ---\n  line\"\nb: 1\n",
    "a: 'multi\n ---\n line'\n---\nb: 2\n",
    "----\n---foo\n",
    "",
    " ",
    "\n",
    "  \n\t\n  ",
    "# only a comment\n",
    "\u{FEFF}",
    "\u{FEFF}a: 1\n---\nb: 2\n",
    "a: 1\n",
    "a: 1",
    "---",
    "---\n",
    "---\n---\n",
    "---\n---\n---\n",
    "---\n---\na: 1\n",
    "---\na: 1\n---\n",
    "a: 1\n---\n",
    "a: 1\n---\nb: 2\n---\n",
    "a: 1\n---\n---\nb: 2\n",
    "a: 1\n---\n\n\n---\nb: 2\n",
    "---\n\n---\nvalid: true\n",
    "---\n# comment only\n---\na: 1\n",
    "# leading comment\n---\na: 1\n",
    "# leading comment\n---\n",
    "\n\n---\na: 1\n",
    "--- # comment\na: 1\n",
    "--- # comment\n--- # another\n",
    "--- value\n--- other\n",
    "--- |\n  literal\n--- >\n  folded\n",
    "---\na: 1\n...\n---\nb: 2\n",
    "---\na: 1\n...\n",
    "a: 1\n...\n---\nb: 2\n",
    "---\n...\n---\nb: 2\n",
    "---\n...\n",
    "%YAML 1.2\n---\na: 1\n",
    "%YAML 1.2\n---\na: 1\n---\nb: 2\n",
    "%YAML 1.2\n---\n---\nb: 2\n",
    "key: ---value\n---\nfoo: 1\n",
    "---\r\na: 1\r\n---\r\n---\r\nb: 2\r\n",
    "---\n- 1\n- 2\n---\n---\n[1, 2]\n---\n{a: 1}\n",
    "---\n&anchor a: 1\n---\n---\nb: 2\n",
];

fn assert_parity(input: &str, config: &Config) {
    let expected = Parser::parse_all(input).unwrap();
    let actual = parse_parallel_with_config(input, config).unwrap();
    assert_eq!(actual, expected, "input: {input:?}");
}

#[test]
fn matches_parse_all_default_config() {
    for input in INPUTS {
        assert_eq!(
            parse_parallel(input).unwrap(),
            Parser::parse_all(input).unwrap(),
            "input: {input:?}"
        );
    }
}

#[test]
fn matches_parse_all_forced_parallel_path() {
    let config = Config::new()
        .with_workers(Some(2))
        .with_sequential_threshold(0);
    for input in INPUTS {
        assert_parity(input, &config);
    }
}

#[test]
fn matches_parse_all_forced_sequential_path() {
    let config = Config::new().with_workers(Some(0));
    for input in INPUTS {
        assert_parity(input, &config);
    }
}

#[test]
fn errors_exactly_when_parse_all_errors() {
    let configs = [
        Config::new().with_workers(Some(0)),
        Config::new()
            .with_workers(Some(2))
            .with_sequential_threshold(0),
    ];
    for input in ERROR_INPUTS {
        assert!(Parser::parse_all(input).is_err(), "parse_all: {input:?}");
        for config in &configs {
            assert!(
                parse_parallel_with_config(input, config).is_err(),
                "parse_parallel: {input:?}"
            );
        }
    }
}

#[test]
fn leading_empty_document_is_kept_as_null() {
    let docs = parse_parallel("---\n---\na: 1\n").unwrap();
    assert_eq!(docs.len(), 2);
    assert_eq!(docs, Parser::parse_all("---\n---\na: 1\n").unwrap());
}

#[test]
fn trailing_empty_document_is_kept_as_null() {
    let docs = parse_parallel("a: 1\n---\nb: 2\n---\n").unwrap();
    assert_eq!(docs.len(), 3);
}

#[test]
fn parse_error_index_matches_stream_position() {
    let input = "---\n---\na: 1\n---\nb: 'unclosed\n";
    assert!(Parser::parse_all(input).is_err());
    let err = parse_parallel(input).unwrap_err();
    match err {
        Error::Parse { index, .. } => assert_eq!(index, 2),
        other => panic!("expected Error::Parse, got {other:?}"),
    }
}

#[test]
fn merge_error_location_uses_stream_document_index() {
    let input = "---\na: 1\n---\nb: 2\n---\nm:\n  <<: 1\n";
    let expected = Parser::parse_all(input).unwrap_err();
    assert_eq!(expected.document_index(), 2);
    let config = Config::new()
        .with_workers(Some(2))
        .with_sequential_threshold(0);
    for result in [
        parse_parallel(input),
        parse_parallel_with_config(input, &config),
    ] {
        let Err(Error::Parse { index, source }) = result else {
            panic!("expected Error::Parse");
        };
        assert_eq!(index, 2);
        assert_eq!(source.to_string(), expected.to_string());
    }
}

#[test]
fn merge_error_location_with_crlf_and_multibyte_text_before_it() {
    let input = "a: \"héllo\"\r\n---\r\nк: 1\r\nm:\r\n  <<: 1\r\n";
    let expected = Parser::parse_all(input).unwrap_err().to_string();
    assert!(
        expected.contains("line 5, column 3 (document 2)"),
        "{expected}"
    );
    let config = Config::new()
        .with_workers(Some(2))
        .with_sequential_threshold(0);
    let Err(Error::Parse { source, .. }) = parse_parallel_with_config(input, &config) else {
        panic!("expected Error::Parse");
    };
    assert_eq!(source.to_string(), expected);
}

#[test]
fn parse_error_index_with_empty_documents_on_parallel_path() {
    let mut input = String::from("---\n---\n");
    for i in 0..8 {
        writeln!(input, "---\nk{i}: {i}\n---").unwrap();
    }
    input.push_str("---\nbad: 'unclosed\n");
    let config = Config::new()
        .with_workers(Some(2))
        .with_sequential_threshold(0);
    assert!(Parser::parse_all(&input).is_err());
    let err = parse_parallel_with_config(&input, &config).unwrap_err();
    match err {
        Error::Parse { index, .. } => assert_eq!(index, 2 + 8 * 2),
        other => panic!("expected Error::Parse, got {other:?}"),
    }
}

#[test]
fn merge_error_position_is_whole_input_on_parallel_path() {
    let mut input = String::new();
    for i in 0..8 {
        writeln!(input, "---\nk{i}: {i}").unwrap();
    }
    input.push_str("---\nm:\n  <<: 1\n");
    let expected = match Parser::parse_all(&input).unwrap_err() {
        fast_yaml_core::ParseError::Merge { line, column, .. } => (line, column),
        other => panic!("expected ParseError::Merge, got {other:?}"),
    };
    assert_eq!(expected, (19, 3));
    let configs = [
        Config::new()
            .with_workers(Some(2))
            .with_sequential_threshold(0),
        Config::new().with_workers(Some(0)),
    ];
    for config in &configs {
        match parse_parallel_with_config(&input, config).unwrap_err() {
            Error::Parse {
                source: fast_yaml_core::ParseError::Merge { line, column, .. },
                ..
            } => assert_eq!((line, column), expected),
            other => panic!("expected a merge error, got {other:?}"),
        }
    }
}

#[test]
fn scanner_and_limit_errors_agree_on_document_index_with_parse_all() {
    let config = Config::new()
        .with_workers(Some(2))
        .with_sequential_threshold(0);
    for input in [
        "a: 1\n---\nb: 2\n---\nc: \0\n",
        "a: 1\n---\nb: 2\n---\nc: [\n",
        "a: &x 1\n---\nb: 2\n---\nc: *x\n",
        "\0",
        "a: 1\n---\n\0",
        "a: 1\n...\n\0\n",
        "a: 1\n---\nb: 2\n...\n\0\n",
    ] {
        let expected = Parser::parse_all(input).unwrap_err();
        for result in [
            parse_parallel(input),
            parse_parallel_with_config(input, &config),
        ] {
            let Err(Error::Parse { index, source }) = result else {
                panic!("expected Error::Parse for {input:?}");
            };
            assert_eq!(index, expected.document_index(), "{input:?}");
            assert_eq!(source.document_index(), index, "{input:?}");
            assert_eq!(source.to_string(), expected.to_string(), "{input:?}");
        }
    }
}
