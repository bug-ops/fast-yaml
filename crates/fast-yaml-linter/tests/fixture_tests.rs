//! Integration tests using YAML fixtures.

use std::num::NonZeroUsize;

use fast_yaml_linter::{DiagnosticCode, LintConfig, Linter, Severity, config::RuleName};

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
        let config = LintConfig::new().with_max_line_length(NonZeroUsize::new(80));
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

    #[test]
    fn test_invalid_duplicate_merge_keys_is_a_diagnostic() {
        let yaml = include_str!("fixtures/invalid/duplicate_merge_keys.yaml");
        let diagnostics = Linter::with_all_rules().lint(yaml).unwrap();

        let duplicate = diagnostics
            .iter()
            .find(|d| d.code.as_str() == DiagnosticCode::DUPLICATE_KEY)
            .expect("Expected a duplicate-key diagnostic for the repeated <<");
        assert_eq!(duplicate.span.start.line, 7);
    }

    #[test]
    fn test_invalid_duplicate_merge_key_through_alias_is_a_diagnostic() {
        let yaml = include_str!("fixtures/invalid/duplicate_merge_keys_alias.yaml");
        let diagnostics = Linter::with_all_rules().lint(yaml).unwrap();

        let duplicate = diagnostics
            .iter()
            .find(|d| d.code.as_str() == DiagnosticCode::DUPLICATE_KEY)
            .expect("Expected a duplicate-key diagnostic for the alias-written <<");
        assert_eq!(duplicate.span.start.line, 5);
    }

    #[test]
    fn test_invalid_set_member_values_report_every_member() {
        let yaml = include_str!("fixtures/invalid/set_member_values_many.yaml");
        let diagnostics = Linter::with_all_rules().lint(yaml).unwrap();

        let lines: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code.as_str() == DiagnosticCode::SET_VALUES)
            .map(|d| d.span.start.line)
            .collect();
        assert_eq!(lines, vec![2, 4]);
    }

    #[test]
    fn test_invalid_set_member_value_is_a_diagnostic() {
        let yaml = include_str!("fixtures/invalid/set_member_value.yaml");
        let diagnostics = Linter::with_all_rules().lint(yaml).unwrap();

        let set_values: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code.as_str() == "set-values")
            .collect();
        assert_eq!(set_values.len(), 1, "{diagnostics:?}");
        assert_eq!(set_values[0].severity, Severity::Error);
        assert_eq!(set_values[0].span.start.line, 2);
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

    #[test]
    fn test_edge_case_non_ascii_block_scalar_braces() {
        let yaml = include_str!("fixtures/edge_cases/non_ascii_block_scalar_braces.yaml");
        let linter = Linter::with_all_rules();
        let diagnostics = linter.lint(yaml).unwrap();

        let has_errors = diagnostics.iter().any(|d| d.severity == Severity::Error);

        assert!(!has_errors, "Expected no errors in fixture");
    }

    #[test]
    fn test_edge_case_non_ascii_offsets_are_consistent() {
        let lf = include_str!("fixtures/edge_cases/non_ascii_offsets.yaml").replace("\r\n", "\n");
        assert_offsets_consistent(&lf);
        assert_offsets_consistent(&lf.replace('\n', "\r\n"));
    }

    fn assert_offsets_consistent(yaml: &str) {
        let linter = Linter::with_all_rules();
        let diagnostics = linter.lint(yaml).unwrap();
        assert_ne!(diagnostics, []);

        for d in &diagnostics {
            let (start, end) = (d.span.start, d.span.end);
            assert!(yaml.is_char_boundary(start.offset), "{d:?}");
            assert!(yaml.is_char_boundary(end.offset), "{d:?}");
            let line_start = yaml
                .split_inclusive('\n')
                .take(start.line - 1)
                .map(str::len)
                .sum::<usize>();
            assert_eq!(
                yaml[line_start..start.offset].chars().count() + 1,
                start.column,
                "{d:?}"
            );
        }
        assert!(
            !diagnostics.iter().any(
                |d| d.span.start.line == 7 && d.code.as_str() == DiagnosticCode::QUOTED_STRINGS
            ),
            "\\u escape must not be flagged: {diagnostics:?}"
        );
    }

    #[test]
    fn test_edge_case_stray_bracket_in_comment_no_panic() {
        let linter = Linter::with_all_rules();
        assert!(linter.lint("k: [a,\n  b]\n# x ]\nz: [ 1 ]\n").is_ok());
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_linter_with_disabled_rules() {
        let yaml = include_str!("fixtures/valid/simple.yaml");
        let config = LintConfig::new().with_disabled_rule(RuleName::LineLength);
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
        let config = LintConfig::new().with_max_line_length(NonZeroUsize::new(80));
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

#[cfg(test)]
mod config_fixtures {
    use std::path::{Path, PathBuf};

    use fast_yaml_linter::{
        ConfigFile, ConfigFileError, Linter, Severity,
        config::{Limit, RuleConfigError},
        rules::{Forbid, MarkerPresence, QuoteRequirement, QuoteType},
    };
    use std::num::NonZeroUsize;

    fn fixture(relative: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/config")
            .join(relative)
    }

    fn load(relative: &str) -> Result<ConfigFile, ConfigFileError> {
        ConfigFile::load(&fixture(relative))
    }

    #[test]
    fn every_valid_config_loads_and_lints() {
        let mut count = 0;
        for entry in std::fs::read_dir(fixture("valid")).unwrap() {
            let path = entry.unwrap().path();
            let config = ConfigFile::load(&path)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
                .into_parts()
                .0;
            Linter::with_config(config)
                .lint("a: 1\nb: [1, 2]\nc: {d: e}\n")
                .unwrap();
            count += 1;
        }
        assert!(count >= 4);
    }

    #[test]
    fn full_config_reaches_typed_options() {
        let rules = load("valid/full.yaml").unwrap().rules;
        assert_eq!(rules.braces.options.forbid, Forbid::NonEmpty);
        assert_eq!(rules.brackets.options.max_spaces_inside, Limit::Disabled);
        assert_eq!(rules.line_length.options.max, NonZeroUsize::new(120));
        assert_eq!(rules.indentation.options.indent_size.get(), 4);
        assert_eq!(rules.quoted_strings.options.quote_type, QuoteType::Single);
        assert!(!rules.comments_indentation.enabled);
        assert_eq!(rules.document_start.severity, Some(Severity::Error));
    }

    #[test]
    fn bool_forms_config() {
        let rules = load("valid/bool-forms.yaml").unwrap().rules;
        assert_eq!(
            rules.document_start.options.present,
            MarkerPresence::Required
        );
        assert_eq!(
            rules.quoted_strings.options.required,
            QuoteRequirement::NotRequired
        );
        assert_eq!(rules.braces.options.forbid, Forbid::All);
        assert_eq!(rules.brackets.options.forbid, Forbid::No);
        assert_eq!(rules.line_length.options.max, None);
    }

    #[test]
    fn document_end_forbidden_and_quoted_regex_config() {
        let rules = load("valid/document-end-forbidden.yaml").unwrap().rules;
        assert_eq!(
            rules.document_end.options.present,
            MarkerPresence::Forbidden
        );
        let options = load("valid/quoted-regex.yaml")
            .unwrap()
            .rules
            .quoted_strings
            .options;
        assert!(options.extra_required.is_match("http://x"));
        assert!(options.extra_required.is_match("README.md"));
        assert!(options.extra_allowed.is_match("ftp://x"));
        assert!(!options.extra_allowed.is_match("a ftp://x"));
    }

    #[test]
    fn shorthands_config() {
        let rules = load("valid/shorthands.yaml").unwrap().rules;
        assert!(!rules.key_ordering.enabled);
        assert!(rules.truthy.enabled);
        assert_eq!(rules.line_length.severity, Some(Severity::Warning));
    }

    #[test]
    fn invalid_configs_are_rejected_with_rule_and_key() {
        for (file, needles) in [
            (
                "quote-type-typo.yaml",
                &["quoted-strings", "quote-type", "singel"][..],
            ),
            ("unknown-rule.yaml", &["no-such-rule"][..]),
            ("wrong-type.yaml", &["line-length", "max"][..]),
            ("unknown-option.yaml", &["line-length", "maxx"][..]),
            (
                "unsupported-yamllint-option.yaml",
                &["indentation", "spaces", "yamllint"][..],
            ),
            ("bad-severity.yaml", &["braces", "loud"][..]),
            ("null-option.yaml", &["quoted-strings", "quote-type"][..]),
            (
                "inert-extra-required.yaml",
                &["quoted-strings", "extra-required", "cannot be combined"][..],
            ),
            (
                "always-extra-allowed.yaml",
                &["quoted-strings", "extra-allowed", "only-when-needed"][..],
            ),
        ] {
            let error = load(&format!("invalid/{file}")).unwrap_err();
            assert!(
                matches!(error, ConfigFileError::InvalidRules { .. }),
                "{file}: {error:?}"
            );
            let ConfigFileError::InvalidRules { source, .. } = error else {
                unreachable!();
            };
            let message = source.to_string();
            for needle in needles {
                assert!(message.contains(needle), "{file}: {message}");
            }
        }
    }

    #[test]
    fn top_level_keys_are_validated() {
        assert!(matches!(
            load("invalid/top-level-typo.yaml"),
            Err(ConfigFileError::UnknownKey { .. })
        ));
        for (file, missing) in [
            ("yamllint-extends.yaml", "base.yaml"),
            ("extends-unknown-preset.yaml", "strict"),
        ] {
            let error = load(&format!("invalid/{file}")).unwrap_err();
            let ConfigFileError::Extended { source, .. } = error else {
                panic!("{file}: expected Extended, got {error:?}");
            };
            assert!(
                matches!(*source, ConfigFileError::Io { ref path, .. } if path.ends_with(missing)),
                "{file}: {source:?}"
            );
        }
    }

    #[test]
    fn extends_ignore_and_yaml_files_config() {
        let config = load("valid/extends-ignore-yaml-files.yaml").unwrap();
        assert_eq!(config.rules.line_length.options.max, NonZeroUsize::new(120));
        assert_eq!(config.rules.line_length.severity, Some(Severity::Warning));
        assert!(config.rules.quoted_strings.enabled);
        assert_eq!(
            config.rules.quoted_strings.options.quote_type,
            QuoteType::Single
        );
        assert_eq!(
            config.rules.quoted_strings.options.required,
            QuoteRequirement::Always
        );
        assert!(!config.rules.document_start.enabled);

        let root = fixture("valid").canonicalize().unwrap();
        let ignore = config.selection.ignore.unwrap();
        assert!(ignore.matches(&root.join("vendor/a.yaml"), false));
        assert!(!ignore.matches(&root.join("vendor/keep.yaml"), false));
        assert!(ignore.matches(&root.join("x/a.generated.yaml"), false));
        let yaml_files = config.selection.yaml_files.unwrap();
        assert!(yaml_files.matches(Path::new("t.yaml.j2")));
        assert!(!yaml_files.matches(Path::new("t.json")));
    }

    #[test]
    fn yamllint_rule_name_and_per_rule_ignore_are_explained() {
        for (file, needles) in [
            (
                "yamllint-rule-name.yaml",
                &["key-duplicates", "duplicate-key"][..],
            ),
            (
                "per-rule-ignore.yaml",
                &["braces", "ignore", "top-level"][..],
            ),
        ] {
            let ConfigFileError::InvalidRules { source, .. } =
                load(&format!("invalid/{file}")).unwrap_err()
            else {
                panic!("{file}: expected InvalidRules");
            };
            let message = source.to_string();
            for needle in needles {
                assert!(message.contains(needle), "{file}: {message}");
            }
        }
    }

    #[test]
    fn unknown_rule_variant_is_reported() {
        let ConfigFileError::InvalidRules { source, .. } =
            load("invalid/unknown-rule.yaml").unwrap_err()
        else {
            panic!("expected InvalidRules");
        };
        let RuleConfigError::UnknownRule(unknown) = source else {
            panic!("expected UnknownRule");
        };
        assert_eq!(unknown.name, "no-such-rule");
    }
}
