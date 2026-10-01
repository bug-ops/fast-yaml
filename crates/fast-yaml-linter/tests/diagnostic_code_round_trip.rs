//! `DiagnosticCode` conversions round-trip for every known code (#594 gap).

use std::str::FromStr;

use fast_yaml_linter::DiagnosticCode;
use fast_yaml_linter::config::RuleName;

const CONSTANTS: &[&str] = &[
    DiagnosticCode::DUPLICATE_KEY,
    DiagnosticCode::INVALID_ANCHOR,
    DiagnosticCode::UNDEFINED_ALIAS,
    DiagnosticCode::INDENTATION,
    DiagnosticCode::LINE_LENGTH,
    DiagnosticCode::TRAILING_WHITESPACE,
    DiagnosticCode::DOCUMENT_START,
    DiagnosticCode::DOCUMENT_END,
    DiagnosticCode::EMPTY_VALUES,
    DiagnosticCode::NEW_LINE_AT_END_OF_FILE,
    DiagnosticCode::BRACES,
    DiagnosticCode::BRACKETS,
    DiagnosticCode::COLONS,
    DiagnosticCode::COMMAS,
    DiagnosticCode::HYPHENS,
    DiagnosticCode::COMMENTS,
    DiagnosticCode::COMMENTS_INDENTATION,
    DiagnosticCode::EMPTY_LINES,
    DiagnosticCode::NEW_LINES,
    DiagnosticCode::OCTAL_VALUES,
    DiagnosticCode::TRUTHY,
    DiagnosticCode::QUOTED_STRINGS,
    DiagnosticCode::KEY_ORDERING,
    DiagnosticCode::FLOAT_VALUES,
    DiagnosticCode::SET_VALUES,
    DiagnosticCode::LINT_DIRECTIVE,
    DiagnosticCode::SYNTAX,
];

#[test]
fn every_constant_round_trips_through_from_and_as_str() {
    for &constant in CONSTANTS {
        assert_eq!(DiagnosticCode::from(constant).as_str(), constant);
        assert_eq!(DiagnosticCode::from(constant.to_owned()).as_str(), constant);
    }
}

#[test]
fn every_rule_code_is_a_known_constant_and_parses_back() {
    for name in RuleName::ALL {
        let code = name.as_str();
        assert!(CONSTANTS.contains(&code), "{code} has no constant");
        assert_eq!(
            RuleName::from_str(DiagnosticCode::from(code).as_str()),
            Ok(name)
        );
    }
}

#[test]
fn an_unknown_code_is_kept_verbatim() {
    assert_eq!(DiagnosticCode::from("custom-rule").as_str(), "custom-rule");
}
