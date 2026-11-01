//! `DiagnosticCode` text conversions round-trip for every known code (#594 gap, #642).

use fast_yaml_linter::DiagnosticCode;
use fast_yaml_linter::config::{CustomRuleCode, RuleName};

const fn reserved() -> [DiagnosticCode; 3] {
    [
        DiagnosticCode::UndefinedAlias,
        DiagnosticCode::Syntax,
        DiagnosticCode::DiagnosticLimit,
    ]
}

#[test]
fn every_rule_name_round_trips_through_its_text() {
    for name in RuleName::ALL {
        let code = DiagnosticCode::from(name);
        assert_eq!(code.as_str(), name.as_str());
        assert_eq!(code.as_str().parse::<DiagnosticCode>().unwrap(), code);
        assert_eq!(code.to_string(), name.as_str());
    }
}

#[test]
fn every_reserved_code_round_trips_through_its_text() {
    for code in reserved() {
        assert_eq!(code.as_str().parse::<DiagnosticCode>().unwrap(), code);
    }
}

#[test]
fn a_custom_code_parses_to_a_rule_code_and_keeps_its_text() {
    let code = "custom-rule".parse::<DiagnosticCode>().unwrap();
    assert_eq!(code.as_str(), "custom-rule");
    assert!(!reserved().contains(&code));
}

#[test]
fn an_empty_code_does_not_parse() {
    assert!("".parse::<DiagnosticCode>().is_err());
}

#[test]
fn a_custom_rule_cannot_take_a_reserved_or_built_in_code() {
    let reserved = reserved().map(|code| code.as_str().to_owned());
    let built_in = RuleName::ALL.map(|name| name.as_str().to_owned());
    for text in reserved.into_iter().chain(built_in) {
        assert!(CustomRuleCode::new(text.clone()).is_err(), "{text}");
    }
}

#[test]
fn codes_sort_by_their_text() {
    let mut codes: Vec<DiagnosticCode> = ["truthy", "syntax", "anchors-custom", "colons"]
        .into_iter()
        .map(|text| text.parse().unwrap())
        .collect();
    codes.sort();
    let sorted: Vec<&str> = codes.iter().map(DiagnosticCode::as_str).collect();
    assert_eq!(sorted, ["anchors-custom", "colons", "syntax", "truthy"]);
}

#[cfg(feature = "json-output")]
#[test]
fn serde_uses_the_text_form() {
    let code = DiagnosticCode::from(RuleName::Colons);
    let json = serde_json::to_string(&code).unwrap();
    assert_eq!(json, "\"colons\"");
    assert_eq!(serde_json::from_str::<DiagnosticCode>(&json).unwrap(), code);
    assert!(serde_json::from_str::<DiagnosticCode>("\"\"").is_err());
}
