//! Integration tests using YAML fixtures.

use fast_yaml_linter::{DiagnosticCode, LintConfig, Linter, Severity};

#[cfg(test)]
mod valid_fixtures {
    use super::*;

    #[test]
    fn test_valid_simple() {
        let yaml = include_str!("fixtures/valid/simple.yaml");
        let linter = Linter::with_all_rules();
        let diagnostics = linter.lint(yaml).unwrap();

        let has_errors = diagnostics.iter().any(|d| d.severity == Severity::Error);

        assert!(!has_errors, "Expected no errors in valid/simple.yaml");
    }

    #[test]
    fn test_valid_complex() {
        let yaml = include_str!("fixtures/valid/complex.yaml");
        let linter = Linter::with_all_rules();
        let diagnostics = linter.lint(yaml).unwrap();

        let has_errors = diagnostics.iter().any(|d| d.severity == Severity::Error);

        assert!(!has_errors, "Expected no errors in valid/complex.yaml");
    }

    #[test]
    fn test_valid_comments() {
        let yaml = include_str!("fixtures/valid/comments.yaml");
        let linter = Linter::with_all_rules();
        let diagnostics = linter.lint(yaml).unwrap();

        let has_errors = diagnostics.iter().any(|d| d.severity == Severity::Error);

        assert!(!has_errors, "Expected no errors in valid/comments.yaml");
    }
}

#[cfg(test)]
mod invalid_fixtures {
    use super::*;

    // Note: duplicate_keys test skipped because yaml-rust2 rejects duplicate keys at parser level

    #[test]
    fn test_invalid_long_lines() {
        let yaml = include_str!("fixtures/invalid/long_lines.yaml");
        let config = LintConfig::new().with_max_line_length(Some(80));
        let mut linter = Linter::with_config(config);
        linter.add_rule(Box::new(fast_yaml_linter::rules::LineLengthRule));

        let diagnostics = linter.lint(yaml).unwrap();

        let has_long_lines = diagnostics
            .iter()
            .any(|d| d.code.as_str() == DiagnosticCode::LINE_LENGTH);

        assert!(has_long_lines, "Expected long line violations");
    }

    #[test]
    fn test_invalid_empty_values() {
        let yaml = include_str!("fixtures/invalid/empty_values.yaml");
        let linter = Linter::with_all_rules();
        let diagnostics = linter.lint(yaml).unwrap();

        let has_empty_values = diagnostics
            .iter()
            .any(|d| d.code.as_str() == DiagnosticCode::EMPTY_VALUES);

        assert!(has_empty_values, "Expected empty value violations");
    }

    #[test]
    fn test_invalid_bad_comments() {
        let yaml = include_str!("fixtures/invalid/bad_comments.yaml");
        let linter = Linter::with_all_rules();
        let diagnostics = linter.lint(yaml).unwrap();

        let has_comment_errors = diagnostics.iter().any(|d| {
            d.code.as_str() == DiagnosticCode::COMMENTS
                || d.code.as_str() == DiagnosticCode::COMMENTS_INDENTATION
        });

        assert!(has_comment_errors, "Expected comment formatting violations");
    }

    #[test]
    fn test_invalid_octal_values() {
        let yaml = include_str!("fixtures/invalid/octal_values.yaml");
        let linter = Linter::with_all_rules();
        let diagnostics = linter.lint(yaml).unwrap();

        let has_octal_errors = diagnostics
            .iter()
            .any(|d| d.code.as_str() == DiagnosticCode::OCTAL_VALUES);

        assert!(has_octal_errors, "Expected octal value violations");
    }
}

#[cfg(test)]
mod edge_case_fixtures {
    use super::*;

    #[test]
    fn test_edge_case_empty() {
        let yaml = include_str!("fixtures/edge_cases/empty.yaml");
        let linter = Linter::with_all_rules();
        let result = linter.lint(yaml);

        assert!(result.is_ok(), "Should parse empty/comment YAML");
    }

    #[test]
    fn test_edge_case_unicode() {
        let yaml = include_str!("fixtures/edge_cases/unicode.yaml");
        let linter = Linter::with_all_rules();
        let diagnostics = linter.lint(yaml).unwrap();

        let has_errors = diagnostics.iter().any(|d| d.severity == Severity::Error);

        assert!(!has_errors, "Expected no errors in unicode.yaml");
    }

    #[test]
    fn test_edge_case_multiline() {
        let yaml = include_str!("fixtures/edge_cases/multiline.yaml");
        let linter = Linter::with_all_rules();
        let diagnostics = linter.lint(yaml).unwrap();

        let has_errors = diagnostics.iter().any(|d| d.severity == Severity::Error);

        assert!(!has_errors, "Expected no errors in multiline.yaml");
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_linter_with_disabled_rules() {
        let yaml = include_str!("fixtures/valid/simple.yaml");
        let config = LintConfig::new().with_disabled_rule(DiagnosticCode::LINE_LENGTH);
        let mut linter = Linter::with_config(config);
        linter.add_rule(Box::new(fast_yaml_linter::rules::LineLengthRule));

        let diagnostics = linter.lint(yaml).unwrap();

        let has_line_length = diagnostics
            .iter()
            .any(|d| d.code.as_str() == DiagnosticCode::LINE_LENGTH);

        assert!(
            !has_line_length,
            "No diagnostics should be for line-length when disabled"
        );
    }

    #[test]
    fn test_diagnostic_location_accuracy() {
        let yaml = include_str!("fixtures/invalid/long_lines.yaml");
        let config = LintConfig::new().with_max_line_length(Some(80));
        let mut linter = Linter::with_config(config);
        linter.add_rule(Box::new(fast_yaml_linter::rules::LineLengthRule));

        let diagnostics = linter.lint(yaml).unwrap();

        for diagnostic in &diagnostics {
            assert!(
                diagnostic.span.start.line > 0,
                "Diagnostic should have valid line number"
            );
            assert!(
                diagnostic.span.start.column > 0,
                "Diagnostic should have valid column number"
            );
        }
    }

    #[test]
    fn test_all_valid_fixtures_pass() {
        let fixtures = [
            (
                "valid/simple.yaml",
                include_str!("fixtures/valid/simple.yaml"),
            ),
            (
                "valid/complex.yaml",
                include_str!("fixtures/valid/complex.yaml"),
            ),
            (
                "valid/comments.yaml",
                include_str!("fixtures/valid/comments.yaml"),
            ),
        ];

        let linter = Linter::with_all_rules();

        for (name, yaml) in fixtures {
            let diagnostics = linter.lint(yaml).unwrap();
            let has_errors = diagnostics.iter().any(|d| d.severity == Severity::Error);

            assert!(!has_errors, "Expected no errors in {name}");
        }
    }
}
