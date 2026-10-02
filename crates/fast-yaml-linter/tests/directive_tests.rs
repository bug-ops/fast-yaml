//! Fixture-based test for yamllint-style inline directives; the rest is covered by the unit
//! tests in `src/directives.rs`.

use fast_yaml_linter::{DiagnosticCode, Linter};

#[test]
fn yamllint_spelling_and_aliases() {
    let yaml = include_str!("fixtures/directives/yamllint_aliases.yaml");
    let diagnostics = Linter::with_all_rules().lint(yaml).unwrap();
    let lines = |code: &str| -> Vec<usize> {
        diagnostics
            .iter()
            .filter(|d| d.code.as_str() == code)
            .map(|d| d.span.start.line())
            .collect()
    };
    assert_eq!(lines(DiagnosticCode::DUPLICATE_KEY), [6]);
    assert_eq!(lines(DiagnosticCode::TRAILING_WHITESPACE), [7]);
}
