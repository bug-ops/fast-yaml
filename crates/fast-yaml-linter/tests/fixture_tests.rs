//! Integration tests using YAML fixtures.

use fast_yaml_linter::{DiagnosticCode, LintConfig, Linter};

#[cfg(test)]
mod valid_fixtures {
    use super::*;

    #[test]
    fn test_valid_simple() {
        let yaml = include_str!("fixtures/valid/simple.yaml");
        let linter = Linter::with_all_rules();
        let diagnostics = linter.lint(yaml).unwrap();

        // Filter out info-level diagnostics (like missing document-start)
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == fast_yaml_linter::Severity::Error)
            .collect();

        assert!(
            errors.is_empty(),
            "Expected no errors in valid/simple.yaml, found: {:?}",
            errors
        );
    }

    #[test]
    fn test_valid_complex() {
        let yaml = include_str!("fixtures/valid/complex.yaml");
        let linter = Linter::with_all_rules();
        let diagnostics = linter.lint(yaml).unwrap();

        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == fast_yaml_linter::Severity::Error)
            .collect();

        assert!(
            errors.is_empty(),
            "Expected no errors in valid/complex.yaml, found: {:?}",
            errors
        );
    }

    #[test]
    fn test_valid_comments() {
        let yaml = include_str!("fixtures/valid/comments.yaml");
        let linter = Linter::with_all_rules();
        let diagnostics = linter.lint(yaml).unwrap();

        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == fast_yaml_linter::Severity::Error)
            .collect();

        assert!(
            errors.is_empty(),
            "Expected no errors in valid/comments.yaml, found: {:?}",
            errors
        );
    }
}

#[cfg(test)]
mod invalid_fixtures {
    use super::*;

    // Note: duplicate_keys test skipped because yaml-rust2 rejects duplicate keys at parser level
    // The DuplicateKeysRule works for cases where yaml-rust2 allows duplicates (e.g., some edge cases)

    #[test]
    fn test_invalid_long_lines() {
        let yaml = include_str!("fixtures/invalid/long_lines.yaml");
        let config = LintConfig::new().with_max_line_length(Some(80));
        let mut linter = Linter::with_config(config);
        linter.add_rule(Box::new(fast_yaml_linter::rules::LineLengthRule));

        let diagnostics = linter.lint(yaml).unwrap();

        let long_line_errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code.as_str() == DiagnosticCode::LINE_LENGTH)
            .collect();

        assert!(
            !long_line_errors.is_empty(),
            "Expected long line violations, found none"
        );
    }

    #[test]
    fn test_invalid_empty_values() {
        let yaml = include_str!("fixtures/invalid/empty_values.yaml");
        let linter = Linter::with_all_rules();
        let diagnostics = linter.lint(yaml).unwrap();

        let empty_value_errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code.as_str() == DiagnosticCode::EMPTY_VALUES)
            .collect();

        assert!(
            !empty_value_errors.is_empty(),
            "Expected empty value violations, found none"
        );
    }

    #[test]
    fn test_invalid_bad_comments() {
        let yaml = include_str!("fixtures/invalid/bad_comments.yaml");
        let linter = Linter::with_all_rules();
        let diagnostics = linter.lint(yaml).unwrap();

        let comment_errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| {
                d.code.as_str() == DiagnosticCode::COMMENTS
                    || d.code.as_str() == DiagnosticCode::COMMENTS_INDENTATION
            })
            .collect();

        assert!(
            !comment_errors.is_empty(),
            "Expected comment formatting violations, found none"
        );
    }

    #[test]
    fn test_invalid_octal_values() {
        let yaml = include_str!("fixtures/invalid/octal_values.yaml");
        let linter = Linter::with_all_rules();
        let diagnostics = linter.lint(yaml).unwrap();

        let octal_errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code.as_str() == DiagnosticCode::OCTAL_VALUES)
            .collect();

        assert!(
            !octal_errors.is_empty(),
            "Expected octal value violations, found none"
        );
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

        // Empty or comment-only YAML should parse without errors
        assert!(result.is_ok(), "Should parse empty/comment YAML");
    }

    #[test]
    fn test_edge_case_unicode() {
        let yaml = include_str!("fixtures/edge_cases/unicode.yaml");
        let linter = Linter::with_all_rules();
        let diagnostics = linter.lint(yaml).unwrap();

        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == fast_yaml_linter::Severity::Error)
            .collect();

        assert!(
            errors.is_empty(),
            "Expected no errors in unicode.yaml, found: {:?}",
            errors
        );
    }

    #[test]
    fn test_edge_case_multiline() {
        let yaml = include_str!("fixtures/edge_cases/multiline.yaml");
        let linter = Linter::with_all_rules();
        let diagnostics = linter.lint(yaml).unwrap();

        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == fast_yaml_linter::Severity::Error)
            .collect();

        assert!(
            errors.is_empty(),
            "Expected no errors in multiline.yaml, found: {:?}",
            errors
        );
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_linter_with_disabled_rules() {
        // Use a valid YAML file to test rule disabling
        let yaml = include_str!("fixtures/valid/simple.yaml");
        let config = LintConfig::new().with_disabled_rule(DiagnosticCode::LINE_LENGTH);
        let mut linter = Linter::with_config(config);
        linter.add_rule(Box::new(fast_yaml_linter::rules::LineLengthRule));

        let diagnostics = linter.lint(yaml).unwrap();

        assert!(
            diagnostics
                .iter()
                .all(|d| d.code.as_str() != DiagnosticCode::LINE_LENGTH),
            "No diagnostics should be for line-length rule when disabled"
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
        let fixtures = vec![
            ("valid/simple.yaml", include_str!("fixtures/valid/simple.yaml")),
            ("valid/complex.yaml", include_str!("fixtures/valid/complex.yaml")),
            ("valid/comments.yaml", include_str!("fixtures/valid/comments.yaml")),
        ];

        let linter = Linter::with_all_rules();

        for (name, yaml) in fixtures {
            let diagnostics = linter.lint(yaml).unwrap();
            let errors: Vec<_> = diagnostics
                .iter()
                .filter(|d| d.severity == fast_yaml_linter::Severity::Error)
                .collect();

            assert!(
                errors.is_empty(),
                "Expected no errors in {}, found: {:?}",
                name,
                errors
            );
        }
    }
}
