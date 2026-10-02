//! `empty-values.forbid-in-block-sequences` (spec D-8); expected positions are yamllint 1.38's.

use fast_yaml_linter::config::{RuleName, RulesConfig};
use fast_yaml_linter::{LintConfig, Linter};

fn positions(source: &str, options: &str) -> Vec<(usize, usize)> {
    let mut rules = RulesConfig::default();
    rules
        .apply_rule(
            RuleName::EmptyValues,
            serde_norway::Deserializer::from_str(options),
        )
        .unwrap();
    let linter = Linter::with_config(LintConfig {
        rules,
        ..LintConfig::default()
    });
    linter
        .lint(source)
        .unwrap()
        .into_iter()
        .filter(|d| d.code.as_str() == "empty-values")
        .map(|d| (d.span.start.line(), d.span.start.column()))
        .collect()
}

#[test]
fn empty_block_sequence_items_are_reported_where_yamllint_reports_them() {
    for (source, expected) in [
        ("- a\n-\n- b\n", vec![(2, 2)]),
        ("-\n", vec![(1, 2)]),
        ("- # c\n- b\n", vec![(1, 2)]),
        ("k:\n  -\n  - x\n", vec![(2, 4)]),
        ("- -\n  - a\n", vec![(1, 4)]),
        ("- a\r\n-\r\n- b\r\n", vec![(2, 2)]),
        ("-\n-\n", vec![(1, 2), (2, 2)]),
        ("- {a: 1}\n-\n", vec![(2, 2)]),
    ] {
        assert_eq!(positions(source, "{}"), expected, "{source:?}");
    }
}

#[test]
fn explicit_and_anchored_items_are_not_empty() {
    for source in ["- &a\n- b\n", "- null\n- ~\n", "- !!null\n", "- ''\n"] {
        assert!(positions(source, "{}").is_empty(), "{source:?}");
    }
}

#[test]
fn the_option_switches_the_check_off() {
    assert_eq!(
        positions("- a\n-\n", "{forbid-in-block-sequences: false}"),
        vec![]
    );
    assert_eq!(
        positions("- a\n-\n", "{forbid-in-block-sequences: true}").len(),
        1
    );
}

#[test]
fn the_message_matches_yamllint() {
    let linter = Linter::with_all_rules();
    let found = linter.lint("- a\n-\n").unwrap();
    assert!(
        found
            .iter()
            .any(|d| d.message == "empty value in block sequence")
    );
}
