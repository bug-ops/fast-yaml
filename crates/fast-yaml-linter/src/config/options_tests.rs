//! Behavior tests for the typed options of every built-in rule.

use std::num::NonZeroUsize;

use serde_norway::Value;

use super::test_support::config_with_rule;
use super::{
    EmptyInsideLimit, IndentSize, Limit, NoOptions, PatternList, RuleConfigError, RuleName,
    RuleOptions, RulesConfig,
};
use crate::Linter;
use crate::rules::{
    ColonsOptions, CommasOptions, CommentsOptions, DocumentEndOptions, DocumentStartOptions,
    DuplicateKeysOptions, EmptyLinesOptions, EmptyValuesOptions, FloatValuesOptions,
    FlowCollectionOptions, Forbid, HyphensOptions, IndentationOptions, InvalidAnchorsOptions,
    KeyOrderingOptions, LineEndingType, LineLengthOptions, MarkerPresence, NewLinesOptions,
    OctalValuesOptions, QuoteRequirement, QuoteType, QuotedStringsOptions, TruthyOptions,
    TruthySpelling,
};

fn apply(rules: &mut RulesConfig, yaml: &str) -> Result<(), RuleConfigError> {
    rules.apply(serde_norway::Deserializer::from_str(yaml))
}

fn applied(yaml: &str) -> RulesConfig {
    let mut rules = RulesConfig::default();
    apply(&mut rules, yaml).unwrap();
    rules
}

fn error_of(yaml: &str) -> String {
    apply(&mut RulesConfig::default(), yaml)
        .unwrap_err()
        .to_string()
}

fn messages(rule: RuleName, entry: &str, yaml: &str) -> Vec<String> {
    Linter::with_config(config_with_rule(rule, entry))
        .lint(yaml)
        .unwrap()
        .into_iter()
        .filter(|d| d.code.as_str() == rule.as_str())
        .map(|d| d.message)
        .collect()
}

fn round_trip<O: RuleOptions>(options: &O) {
    let value = serde_norway::to_value(options).unwrap();
    let back: O = serde_norway::from_value(value).unwrap();
    assert_eq!(&back, options);
}

#[test]
#[allow(clippy::too_many_lines)]
fn every_options_type_round_trips_with_non_default_values() {
    for forbid in [Forbid::No, Forbid::NonEmpty, Forbid::All] {
        for limit in [Limit::Disabled, Limit::Max(3)] {
            for empty in [EmptyInsideLimit::Inherit, EmptyInsideLimit::Spaces(2)] {
                round_trip(&FlowCollectionOptions {
                    forbid,
                    min_spaces_inside: limit,
                    max_spaces_inside: Limit::Max(4),
                    min_spaces_inside_empty: empty,
                    max_spaces_inside_empty: empty,
                });
            }
        }
    }
    round_trip(&ColonsOptions {
        max_spaces_before: Limit::Disabled,
        max_spaces_after: Limit::Max(5),
    });
    round_trip(&CommasOptions {
        max_spaces_before: Limit::Max(2),
        min_spaces_after: Limit::Disabled,
        max_spaces_after: Limit::Max(3),
    });
    round_trip(&HyphensOptions {
        max_spaces_after: Limit::Disabled,
    });
    round_trip(&CommentsOptions {
        require_starting_space: false,
        ignore_shebangs: false,
        min_spaces_from_content: Limit::Disabled,
    });
    round_trip(&EmptyLinesOptions {
        max: Limit::Disabled,
        max_start: Limit::Max(4),
        max_end: Limit::Max(1),
    });
    round_trip(&OctalValuesOptions {
        forbid_implicit_octal: false,
        forbid_explicit_octal: false,
    });
    round_trip(&FloatValuesOptions {
        require_numeral_before_decimal: false,
        forbid_scientific_notation: true,
        forbid_nan: true,
        forbid_inf: true,
    });
    round_trip(&KeyOrderingOptions {
        case_sensitive: false,
    });
    round_trip(&TruthyOptions {
        allowed_values: vec![
            TruthySpelling::new("yes").unwrap(),
            TruthySpelling::new("Off").unwrap(),
        ],
        check_keys: true,
    });
    for quote_type in [QuoteType::Any, QuoteType::Single, QuoteType::Double] {
        for required in [
            QuoteRequirement::Always,
            QuoteRequirement::NotRequired,
            QuoteRequirement::OnlyWhenNeeded,
            QuoteRequirement::Never,
        ] {
            round_trip(&QuotedStringsOptions {
                quote_type,
                required,
                extra_required: PatternList::new(["a"]).unwrap(),
                extra_allowed: PatternList::new(["b", "c"]).unwrap(),
                allow_quoted_quotes: true,
                check_keys: true,
            });
        }
    }
    for line_ending in [
        LineEndingType::Unix,
        LineEndingType::Dos,
        LineEndingType::Platform,
    ] {
        round_trip(&NewLinesOptions { line_ending });
    }
    for present in [
        MarkerPresence::Required,
        MarkerPresence::Forbidden,
        MarkerPresence::Allowed,
    ] {
        round_trip(&DocumentStartOptions { present });
    }
    for present in [
        MarkerPresence::Required,
        MarkerPresence::Forbidden,
        MarkerPresence::Allowed,
    ] {
        round_trip(&DocumentEndOptions { present });
    }
    round_trip(&EmptyValuesOptions {
        forbid_in_block_mappings: false,
        forbid_in_flow_mappings: false,
        forbid_in_block_sequences: false,
    });
    round_trip(&LineLengthOptions {
        max: None,
        ..LineLengthOptions::default()
    });
    round_trip(&LineLengthOptions {
        max: NonZeroUsize::new(120),
        allow_non_breakable_words: false,
        allow_non_breakable_inline_mappings: true,
    });
    round_trip(&DuplicateKeysOptions {
        forbid_duplicated_merge_keys: false,
    });
    round_trip(&IndentationOptions {
        indent_size: IndentSize::try_from(8u64).unwrap(),
    });
    round_trip(&NoOptions::default());
    round_trip(&DuplicateKeysOptions::default());
    round_trip(&InvalidAnchorsOptions::default());
}

#[test]
fn forbid_no_serializes_as_false() {
    assert_eq!(
        serde_norway::to_value(Forbid::No).unwrap(),
        Value::Bool(false)
    );
    assert_eq!(
        serde_norway::to_value(Forbid::NonEmpty).unwrap(),
        Value::String("non-empty".to_owned())
    );
}

#[test]
fn bool_forms_of_yamllint_options() {
    assert_eq!(
        applied("document-start: {present: true}")
            .document_start
            .options
            .present,
        MarkerPresence::Required
    );
    assert_eq!(
        applied("document-start: {present: false}")
            .document_start
            .options
            .present,
        MarkerPresence::Forbidden
    );
    assert_eq!(
        applied("quoted-strings: {required: true}")
            .quoted_strings
            .options
            .required,
        QuoteRequirement::Always
    );
    assert_eq!(
        applied("quoted-strings: {required: false}")
            .quoted_strings
            .options
            .required,
        QuoteRequirement::NotRequired
    );
    let rules = applied("braces: {forbid: true}\nbrackets: {forbid: false}");
    assert_eq!(rules.braces.options.forbid, Forbid::All);
    assert_eq!(rules.brackets.options.forbid, Forbid::No);
    assert_eq!(
        applied("braces: {forbid: non-empty}").braces.options.forbid,
        Forbid::NonEmpty
    );
}

#[test]
fn bool_or_name_options_reject_unknown_names() {
    let message = error_of("braces: {forbid: sometimes}");
    assert!(
        message.contains("braces") && message.contains("forbid"),
        "{message}"
    );
    assert!(error_of("document-start: {present: maybe}").contains("present"));
    assert!(error_of("quoted-strings: {required: sometimes}").contains("required"));
    assert!(error_of("document-end: {present: maybe}").contains("present"));
}

#[test]
fn enum_options_reject_typos() {
    for (rule, key, bad) in [
        ("quoted-strings", "quote-type", "singel"),
        ("new-lines", "type", "windows"),
        ("document-start", "present", "requried"),
    ] {
        let message = error_of(&format!("{rule}: {{{key}: {bad}}}"));
        assert!(message.contains(rule), "{message}");
        assert!(message.contains(key), "{message}");
        assert!(message.contains(bad), "{message}");
    }
}

#[test]
fn snake_case_option_names_are_rejected() {
    let message = error_of("line-length: {max_line: 100}");
    assert!(message.contains("max_line"), "{message}");
}

#[test]
fn every_rule_rejects_unknown_option_keys() {
    for name in RuleName::ALL {
        let message = error_of(&format!("{name}: {{definitely-not-an-option: 1}}"));
        assert!(message.contains(name.as_str()), "{message}");
        assert!(message.contains("definitely-not-an-option"), "{message}");
    }
}

#[test]
fn wrong_types_name_rule_and_key() {
    for (yaml, key) in [
        ("braces: {min-spaces-inside: many}", "min-spaces-inside"),
        (
            "brackets: {max-spaces-inside-empty: [1]}",
            "max-spaces-inside-empty",
        ),
        ("colons: {max-spaces-after: '1'}", "max-spaces-after"),
        ("commas: {min-spaces-after: true}", "min-spaces-after"),
        ("hyphens: {max-spaces-after: 1.5}", "max-spaces-after"),
        (
            "comments: {min-spaces-from-content: yes-please}",
            "min-spaces-from-content",
        ),
        ("empty-lines: {max-end: -5}", "max-end"),
        (
            "octal-values: {forbid-implicit-octal: 1}",
            "forbid-implicit-octal",
        ),
        ("float-values: {forbid-nan: nope}", "forbid-nan"),
        ("key-ordering: {case-sensitive: 0}", "case-sensitive"),
        ("truthy: {check-keys: ~}", "check-keys"),
        ("truthy: {allowed-values: yes}", "allowed-values"),
        (
            "quoted-strings: {extra-required: pattern}",
            "extra-required",
        ),
        (
            "empty-values: {forbid-in-block-mappings: 'true'}",
            "forbid-in-block-mappings",
        ),
        ("line-length: {max: 0}", "max"),
        ("line-length: {max: -1}", "max"),
        ("indentation: {indent-size: 0}", "indent-size"),
        ("indentation: {indent-size: 17}", "indent-size"),
        ("indentation: {indent-size: two}", "indent-size"),
    ] {
        let message = error_of(yaml);
        let rule = yaml.split(':').next().unwrap();
        assert!(message.contains(rule), "{yaml}: {message}");
        assert!(message.contains(key), "{yaml}: {message}");
    }
}

#[test]
fn null_options_are_errors_except_line_length_max() {
    for yaml in [
        "braces: {forbid: ~}",
        "colons: {max-spaces-after: ~}",
        "comments: {require-starting-space: ~}",
        "new-lines: {type: ~}",
        "indentation: {indent-size: ~}",
        "quoted-strings: {extra-allowed: ~}",
    ] {
        assert!(apply(&mut RulesConfig::default(), yaml).is_err(), "{yaml}");
    }
    assert_eq!(
        applied("line-length: {max: ~}").line_length.options.max,
        None
    );
}

#[test]
fn limit_minus_one_disables_and_below_is_error() {
    let rules = applied("colons: {max-spaces-before: -1}");
    assert_eq!(rules.colons.options.max_spaces_before, Limit::Disabled);
    assert!(error_of("colons: {max-spaces-before: -2}").contains("max-spaces-before"));
}

#[test]
fn truthy_spellings_must_be_quoted_known_strings() {
    let rules = applied("truthy: {allowed-values: ['true', 'False', yes]}");
    let spellings: Vec<&str> = rules
        .truthy
        .options
        .allowed_values
        .iter()
        .map(|v| v.as_str())
        .collect();
    assert_eq!(spellings, ["true", "False", "yes"]);
    let message = error_of("truthy: {allowed-values: [true, false]}");
    assert!(message.contains("allowed-values[0]"), "{message}");
    assert!(message.contains("quote the spelling"), "{message}");
    let message = error_of("truthy: {allowed-values: [maybe]}");
    assert!(message.contains("allowed-values[0]"), "{message}");
    assert!(message.contains("maybe"), "{message}");
}

#[test]
fn allowed_truthy_value_is_not_reported() {
    let reported = messages(
        RuleName::Truthy,
        "{allowed-values: [yes]}",
        "a: yes\nb: true\n",
    );
    assert!(reported.is_empty(), "{reported:?}");
    let reported = messages(RuleName::Truthy, "{}", "a: yes\n");
    assert_eq!(reported.len(), 1);
}

#[test]
fn flow_empty_limits_inherit_independently() {
    // A set min-empty with an unset max-empty must still enforce the non-empty max.
    let reported = messages(
        RuleName::Braces,
        "{min-spaces-inside-empty: 0}",
        "a: {  }\n",
    );
    assert!(
        reported.iter().any(|m| m.contains("too many spaces")),
        "{reported:?}"
    );
    // A set max-empty overrides only the max; min is inherited from the non-empty limit.
    let reported = messages(
        RuleName::Braces,
        "{max-spaces-inside-empty: 2}",
        "a: {  }\n",
    );
    assert!(reported.is_empty(), "{reported:?}");
    let reported = messages(
        RuleName::Braces,
        "{min-spaces-inside: 1, max-spaces-inside: 1, max-spaces-inside-empty: 2}",
        "a: {}\n",
    );
    assert!(
        reported.iter().any(|m| m.contains("too few spaces")),
        "{reported:?}"
    );
    // -1 means inherit, not disabled.
    let reported = messages(
        RuleName::Brackets,
        "{max-spaces-inside-empty: -1}",
        "a: [  ]\n",
    );
    assert!(
        reported.iter().any(|m| m.contains("too many spaces")),
        "{reported:?}"
    );
}

#[test]
fn explicit_empty_limits_override_non_empty_ones() {
    let entry = "{max-spaces-inside: 0, min-spaces-inside-empty: 1, max-spaces-inside-empty: 1}";
    assert_eq!(
        messages(RuleName::Braces, entry, "a: { }\nb: {x: 1}\n"),
        [] as [String; 0]
    );
    assert_ne!(
        messages(RuleName::Braces, entry, "a: {}\n"),
        [] as [String; 0]
    );
}

#[test]
fn min_above_max_for_empty_collections_flags_every_empty_collection() {
    let entry = "{min-spaces-inside-empty: 2, max-spaces-inside-empty: 1}";
    for yaml in ["a: {}\n", "a: { }\n", "a: {  }\n"] {
        assert!(
            !messages(RuleName::Braces, entry, yaml).is_empty(),
            "{yaml:?}"
        );
    }
}

#[test]
fn quoted_strings_not_required_allows_quotes_but_checks_quote_type() {
    let entry = "{required: false}";
    assert_eq!(
        messages(RuleName::QuotedStrings, entry, "a: 'John'\nb: plain\n"),
        [] as [String; 0]
    );
    let entry = "{required: false, quote-type: single}";
    let reported = messages(RuleName::QuotedStrings, entry, "a: \"John\"\n");
    assert_eq!(reported, ["string should use single quotes"]);
    let reported = messages(RuleName::QuotedStrings, "{required: never}", "a: 'John'\n");
    assert_eq!(reported, ["string should not be quoted"]);
}

#[test]
fn document_start_present_true_is_enforced() {
    let reported = messages(RuleName::DocumentStart, "{present: true}", "a: 1\n");
    assert_eq!(reported.len(), 1);
    assert_eq!(
        messages(RuleName::DocumentStart, "{present: true}", "---\na: 1\n"),
        [] as [String; 0]
    );
    assert_eq!(
        messages(RuleName::DocumentStart, "{present: false}", "---\na: 1\n").len(),
        1
    );
}

#[test]
fn line_length_null_max_disables_the_limit() {
    let long = format!("key: {}\n", "x".repeat(200));
    assert_eq!(
        messages(RuleName::LineLength, "{max: ~}", &long),
        [] as [String; 0]
    );
    assert_eq!(messages(RuleName::LineLength, "{}", &long).len(), 1);
    assert_eq!(
        messages(RuleName::LineLength, "{max: 500}", &long),
        [] as [String; 0]
    );
}

#[test]
fn empty_values_keys_are_kebab_case() {
    let reported = messages(
        RuleName::EmptyValues,
        "{forbid-in-block-mappings: false}",
        "a:\n",
    );
    assert!(reported.is_empty(), "{reported:?}");
    assert!(
        error_of("empty-values: {forbid_in_block_mappings: false}")
            .contains("forbid_in_block_mappings")
    );
}

#[test]
fn new_lines_type_selects_line_ending() {
    assert_ne!(
        messages(RuleName::NewLines, "{type: dos}", "a: 1\n"),
        [] as [String; 0]
    );
    assert_eq!(
        messages(RuleName::NewLines, "{type: dos}", "a: 1\r\n"),
        [] as [String; 0]
    );
    assert_eq!(
        messages(RuleName::NewLines, "{}", "a: 1\n"),
        [] as [String; 0]
    );
}

#[test]
fn indentation_indent_size_is_applied() {
    assert_ne!(
        messages(RuleName::Indentation, "{indent-size: 4}", "a:\n  b: 1\n"),
        [] as [String; 0]
    );
    assert_eq!(
        messages(RuleName::Indentation, "{indent-size: 4}", "a:\n    b: 1\n"),
        [] as [String; 0]
    );
}

fn assert_unsupported_keys_are_not_fields<O: RuleOptions>() {
    let Value::Mapping(fields) = serde_norway::to_value(O::default()).unwrap() else {
        panic!("options must serialize to a mapping");
    };
    for key in O::YAMLLINT_UNSUPPORTED {
        assert!(
            !fields.contains_key(*key),
            "'{key}' is listed as unsupported but is a real option"
        );
    }
}

#[test]
fn document_end_present_false_is_forbidden() {
    assert_eq!(
        applied("document-end: {present: false}")
            .document_end
            .options
            .present,
        MarkerPresence::Forbidden
    );
    assert_eq!(
        applied("document-end: {present: forbidden}")
            .document_end
            .options
            .present,
        MarkerPresence::Forbidden
    );
}

#[test]
fn extra_patterns_are_regular_expressions() {
    let rules = applied("quoted-strings: {extra-required: ['^http', 'a|b']}");
    let patterns = &rules.quoted_strings.options.extra_required;
    assert!(patterns.is_match("http://x") && patterns.is_match("xb"));
    assert!(!patterns.is_match("x http"));
    let rules = applied("quoted-strings: {extra-allowed: ['\\.md$']}");
    assert!(rules.quoted_strings.options.extra_allowed.is_match("a.md"));
}

#[test]
fn invalid_regex_names_rule_option_index_and_hint() {
    let message = error_of("quoted-strings: {extra-required: ['ok', '(?=x)']}");
    for needle in [
        "quoted-strings",
        "extra-required",
        "pattern 1",
        "look-around and backreferences are unsupported",
    ] {
        assert!(message.contains(needle), "{needle}: {message}");
    }
}

#[test]
fn over_long_and_pathological_patterns_are_rejected() {
    let long = "a".repeat(257);
    let message = error_of(&format!("quoted-strings: {{extra-required: ['{long}']}}"));
    assert!(message.contains("256"), "{message}");
    let message = error_of("quoted-strings: {extra-required: ['(a{1000}){1000}']}");
    assert!(message.contains("extra-required"), "{message}");
    assert!(message.contains("compiles to more than"), "{message}");
    assert!(!message.contains("look-around"), "{message}");
}

#[test]
fn syntax_errors_are_single_line_with_the_hint() {
    let message = error_of("quoted-strings: {extra-required: ['(?=x)']}");
    assert!(
        !message.contains('\n') && !message.contains("\\n"),
        "{message}"
    );
    assert!(message.contains("look-around"), "{message}");
}

#[test]
fn control_characters_in_patterns_are_escaped() {
    let message = error_of("quoted-strings: {extra-required: [\"(\\e[2J\"]}");
    assert!(!message.contains('\u{1b}'), "{message:?}");
}

#[test]
fn too_many_patterns_are_rejected_before_compiling() {
    let start = std::time::Instant::now();
    let many = vec![r"'\w{1,20}[a-z]+x'"; 2000].join(", ");
    let message = error_of(&format!("quoted-strings: {{extra-required: [{many}]}}"));
    assert!(message.contains("64"), "{message}");
    let aliased = vec!["*a"; 2000].join(", ");
    let message = error_of(&format!(
        "x: &a '\\w{{1,20}}[a-z]+x'\nquoted-strings: {{extra-required: [{aliased}]}}"
    ));
    assert_ne!(message, "");
    assert!(start.elapsed().as_secs() < 5);
    assert!(PatternList::new(vec!["a"; 65]).is_err());
}

#[test]
fn unicode_heavy_patterns_fit_the_budget() {
    let rules = applied(r"quoted-strings: {extra-required: ['\w{1,40}']}");
    assert!(rules.quoted_strings.options.extra_required.is_match("é"));
    let heavy = vec![r"'\w+\d\s'"; 64].join(", ");
    let rules = applied(&format!("quoted-strings: {{extra-required: [{heavy}]}}"));
    assert!(rules.quoted_strings.options.extra_required.is_match("é1 "));
}

#[test]
fn pattern_list_matches_any_of_the_individual_patterns() {
    let sources = ["^a", "b$", r"\d{2}", "x.z", "^$"];
    let list = PatternList::new(sources).unwrap();
    let singles: Vec<_> = sources
        .iter()
        .map(|source| regex::Regex::new(source).unwrap())
        .collect();
    for haystack in ["abc", "zzb", "12", "x-z", "", "q", "a1", "b1", "9"] {
        assert_eq!(
            list.is_match(haystack),
            singles.iter().any(|single| single.is_match(haystack)),
            "{haystack:?}"
        );
    }
}

#[test]
fn extra_options_conflict_with_required_modes() {
    for yaml in [
        "quoted-strings: {required: always, extra-required: [a]}",
        "quoted-strings: {required: never, extra-required: [a]}",
        "quoted-strings: {required: always, extra-allowed: [a]}",
        "quoted-strings: {required: false, extra-allowed: [a]}",
    ] {
        let message = error_of(yaml);
        assert!(message.contains("extra-"), "{yaml}: {message}");
    }
    applied("quoted-strings: {required: false, extra-required: [a]}");
    applied("quoted-strings: {extra-required: [a], extra-allowed: [b]}");
}

#[test]
fn stale_extra_patterns_do_not_block_changing_required() {
    let mut rules = applied("quoted-strings: {extra-required: [a]}");
    assert!(apply(&mut rules, "quoted-strings: {required: always}").is_ok());
    assert_eq!(
        rules.quoted_strings.options.required,
        QuoteRequirement::Always
    );
}

#[test]
fn conflict_names_the_option_the_patch_sets() {
    let mut rules = applied("quoted-strings: {required: always}");
    let message = apply(&mut rules, "quoted-strings: {extra-required: [a]}")
        .unwrap_err()
        .to_string();
    assert!(message.contains("extra-required"), "{message}");
}

#[test]
fn patterns_round_trip_through_serde() {
    let list = PatternList::new(["^a", r"b\d"]).unwrap();
    let value = serde_norway::to_value(&list).unwrap();
    assert_eq!(
        serde_norway::from_value::<PatternList>(value).unwrap(),
        list
    );
}

#[test]
fn unsupported_keys_are_not_real_options() {
    assert_unsupported_keys_are_not_fields::<FlowCollectionOptions>();
    assert_unsupported_keys_are_not_fields::<ColonsOptions>();
    assert_unsupported_keys_are_not_fields::<CommasOptions>();
    assert_unsupported_keys_are_not_fields::<HyphensOptions>();
    assert_unsupported_keys_are_not_fields::<CommentsOptions>();
    assert_unsupported_keys_are_not_fields::<DocumentStartOptions>();
    assert_unsupported_keys_are_not_fields::<DocumentEndOptions>();
    assert_unsupported_keys_are_not_fields::<EmptyLinesOptions>();
    assert_unsupported_keys_are_not_fields::<EmptyValuesOptions>();
    assert_unsupported_keys_are_not_fields::<FloatValuesOptions>();
    assert_unsupported_keys_are_not_fields::<IndentationOptions>();
    assert_unsupported_keys_are_not_fields::<KeyOrderingOptions>();
    assert_unsupported_keys_are_not_fields::<LineLengthOptions>();
    assert_unsupported_keys_are_not_fields::<NewLinesOptions>();
    assert_unsupported_keys_are_not_fields::<OctalValuesOptions>();
    assert_unsupported_keys_are_not_fields::<QuotedStringsOptions>();
    assert_unsupported_keys_are_not_fields::<TruthyOptions>();
    assert_unsupported_keys_are_not_fields::<NoOptions>();
    assert_unsupported_keys_are_not_fields::<DuplicateKeysOptions>();
    assert_unsupported_keys_are_not_fields::<InvalidAnchorsOptions>();
}
