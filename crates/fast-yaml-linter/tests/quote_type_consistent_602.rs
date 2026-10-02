//! `quoted-strings` with `quote-type: consistent` (#602), checked against yamllint 1.38 on the
//! same inputs.
//!
//! Each expected line was taken from yamllint itself; messages and columns are not compared.

use fast_yaml_linter::{ConfigFile, Linter};

/// Lints `source` with `quoted-strings: {quote-type: consistent, <options>}` and returns the
/// line of every `quoted-strings` diagnostic.
fn lines(source: &str, options: &str) -> Vec<usize> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(
        &path,
        format!("rules:\n  quoted-strings: {{quote-type: consistent, {options}}}\n"),
    )
    .unwrap();
    let (config, _) = ConfigFile::load(&path).unwrap().into_parts();
    Linter::with_config(config)
        .lint(source)
        .unwrap()
        .iter()
        .filter(|d| d.code.as_str() == "quoted-strings")
        .map(|d| d.span.start.line())
        .collect()
}

const MIXED: &str = "---\na: 'x'\nb: \"y\"\nc: 'z'\nd: \"it's\"\ne: 'say \"hi\"'\nf: \"w\"\n";

#[test]
fn the_first_quoted_string_sets_the_style_of_the_file() {
    for required in ["true", "false"] {
        let options = format!("required: {required}, allow-quoted-quotes: false");
        assert_eq!(lines(MIXED, &options), [3, 5, 7], "{options}");
    }
}

#[test]
fn allow_quoted_quotes_accepts_the_other_style_for_a_string_with_the_first_quote() {
    for required in ["true", "false"] {
        let options = format!("required: {required}, allow-quoted-quotes: true");
        assert_eq!(lines(MIXED, &options), [3, 7], "{options}");
    }
}

#[test]
fn a_file_with_one_style_is_consistent() {
    let source = "---\na: \"x\"\nb: \"y\"\nc: \"z\"\n";
    assert_eq!(lines(source, "required: true"), Vec::<usize>::new());
}

#[test]
fn unquoted_strings_do_not_set_the_style() {
    let source = "---\na: x\nb: \"y\"\nc: 'z'\n";
    assert_eq!(lines(source, "required: false"), [4]);
}

#[test]
fn a_redundantly_quoted_string_does_not_set_the_style() {
    let source = "---\na: 'x'\nb: \"y: z\"\nc: 'w: v'\nd: \"u: t\"\n";
    assert_eq!(lines(source, "required: only-when-needed"), [2, 4]);
}

#[test]
fn the_style_is_kept_across_documents() {
    let source = "---\na: 'x'\n---\nb: \"y\"\n";
    assert_eq!(lines(source, "required: true"), [4]);
}

#[test]
fn consistent_is_a_valid_rule_option() {
    assert_eq!(
        lines("---\na: 'x'\n", "required: true"),
        Vec::<usize>::new()
    );
}
