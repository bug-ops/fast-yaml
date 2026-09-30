//! Regression tests for #417: NUL in the source is an error, not end of input.

use fast_yaml_linter::Linter;

#[test]
fn lint_rejects_nul() {
    for source in ["a: 1\0\nb: 2\n", "# c\0\na: 1\n", "\0"] {
        assert!(Linter::with_all_rules().lint(source).is_err(), "{source:?}");
    }
}
