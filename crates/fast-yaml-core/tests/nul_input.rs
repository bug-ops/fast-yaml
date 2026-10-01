//! Regression tests for #417: a NUL character must be rejected, not treated as end of input.

use fast_yaml_core::streaming::format_streaming;
use fast_yaml_core::{
    Emitter, EmitterConfig, MaxScanAhead, ParseError, Parser, find_comments, has_comments,
};

const INPUTS: [&str; 8] = [
    "\0",
    "a: |\n  x\0y\n",
    "a: [x\0y]\n",
    "a: 1\0\nb: 2\n",
    "\0a: 1\n",
    "# c\0\na: 1\n",
    "a: \"x\0y\"\n",
    "a: 1\n---\nb: 2\0\n",
];

fn assert_nul_error(err: &ParseError) {
    let ParseError::Syntax(syntax) = err else {
        panic!("syntax error expected, got {err:?}");
    };
    assert!(syntax.to_string().contains("NUL"), "{syntax}");
}

#[test]
fn parse_rejects_nul() {
    for input in INPUTS {
        assert_nul_error(&Parser::parse_str(input).unwrap_err());
        assert_nul_error(&Parser::parse_all(input).unwrap_err());
    }
}

#[test]
fn error_points_at_the_nul() {
    let position = Parser::parse_all("a: 1\n# é\0\n").unwrap_err().position();
    assert_eq!((position.line, position.column), (2, 4));
}

#[test]
fn nul_after_bom_is_rejected() {
    assert!(Parser::parse_all("\u{FEFF}a: 1\0").is_err());
}

#[test]
fn format_rejects_nul() {
    for input in INPUTS {
        assert!(Emitter::format(input).is_err(), "{input:?}");
        assert!(
            format_streaming(input, &EmitterConfig::default()).is_err(),
            "{input:?}"
        );
        #[cfg(feature = "arena")]
        assert!(
            fast_yaml_core::streaming::format_streaming_arena(input, &EmitterConfig::default())
                .is_err(),
            "{input:?}"
        );
    }
}

#[test]
fn comment_scan_rejects_nul() {
    for input in INPUTS {
        assert!(
            find_comments(input, MaxScanAhead::DEFAULT).is_err(),
            "{input:?}"
        );
        assert!(
            has_comments(input, MaxScanAhead::DEFAULT).is_err(),
            "{input:?}"
        );
    }
}

#[test]
fn nul_free_input_still_parses() {
    assert_eq!(Parser::parse_all("a: 1\n---\nb: 2\n").unwrap().len(), 2);
}

#[test]
fn error_position_matches_scanner_line_breaks() {
    for input in ["a: 1\rb: 2\r c\0: d", "a: 1\r\nb: 2\n c\0: d"] {
        let position = Parser::parse_all(input).unwrap_err().position();
        assert_eq!((position.line, position.column), (3, 3), "{input:?}");
    }
}
