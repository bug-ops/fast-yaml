//! Lines flagged by the yamllint-parity options, checked against fixtures.
//!
//! The expected lines were taken from yamllint itself, with two exceptions noted in place.

use std::fs;

use fast_yaml_linter::{ConfigFile, DiagnosticCode, Linter};

/// Lints `fixture` with `rules` (a YAML mapping of rule settings) and returns the lines `code`
/// is reported on, once per line.
fn lines(fixture: &str, rules: &str, code: &str) -> Vec<usize> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    fs::write(&path, format!("rules:\n{rules}")).unwrap();
    let (config, _) = ConfigFile::load(&path).unwrap().into_parts();

    let source = fs::read_to_string(format!(
        "{}/tests/fixtures/yamllint/{fixture}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
    // A Windows checkout may have converted the fixture to CRLF
    .replace("\r\n", "\n");
    let mut found: Vec<usize> = Linter::with_config(config)
        .lint(&source)
        .unwrap()
        .iter()
        .filter(|d| d.code.as_str() == code)
        .map(|d| d.span.start.line)
        .collect();
    found.dedup();
    found
}

#[test]
fn document_start_checks_every_document() {
    let code = DiagnosticCode::DOCUMENT_START;
    let required = "  document-start: {present: true}\n";
    assert_eq!(lines("multi_document.yaml", required, code), [1]);
    let forbidden = "  document-start: {present: false}\n";
    assert_eq!(lines("multi_document.yaml", forbidden, code), [2, 5]);
}

#[test]
fn document_end_checks_every_document() {
    // yamllint reports the last document on line 6, the line before the stream end; fast-yaml
    // reports the end of the file
    assert_eq!(
        lines(
            "multi_document.yaml",
            "  document-end: {present: true}\n",
            DiagnosticCode::DOCUMENT_END
        ),
        [2, 7]
    );
}

#[test]
fn quoted_strings_skips_keys_and_core_tags_by_default() {
    let code = DiagnosticCode::QUOTED_STRINGS;
    assert_eq!(
        lines("quoted.yaml", "  quoted-strings: {required: true}\n", code),
        [1, 4]
    );
}

#[test]
fn quoted_strings_skips_anchored_scalars_and_checks_verbatim_str_tags() {
    let code = DiagnosticCode::QUOTED_STRINGS;
    let required = "  quoted-strings: {required: true}\n";
    assert_eq!(lines("quoted_anchors.yaml", required, code), [5, 7, 10]);
    let needed = "  quoted-strings: {required: only-when-needed}\n";
    assert_eq!(lines("quoted_anchors.yaml", needed, code), [8]);
}

#[test]
fn quoted_strings_check_keys() {
    let code = DiagnosticCode::QUOTED_STRINGS;
    let rules = "  quoted-strings: {required: true, check-keys: true}\n";
    assert_eq!(
        lines("quoted.yaml", rules, code),
        [1, 3, 4, 5, 6, 7, 8, 9, 10]
    );
    let rules = "  quoted-strings: {required: only-when-needed, check-keys: true}\n";
    assert_eq!(lines("quoted.yaml", rules, code), [2, 5, 6, 7, 8]);
}

#[test]
fn quoted_strings_allow_quoted_quotes() {
    let code = DiagnosticCode::QUOTED_STRINGS;
    let single = "  quoted-strings: {quote-type: single, required: false}\n";
    assert_eq!(lines("quoted.yaml", single, code), [2, 6, 7, 9]);
    let single =
        "  quoted-strings: {quote-type: single, required: false, allow-quoted-quotes: true}\n";
    assert_eq!(lines("quoted.yaml", single, code), [2, 6, 9]);
    let double =
        "  quoted-strings: {quote-type: double, required: false, allow-quoted-quotes: true}\n";
    assert_eq!(lines("quoted.yaml", double, code), [5]);
}

#[test]
fn line_length_non_breakable_words() {
    let code = DiagnosticCode::LINE_LENGTH;
    assert_eq!(
        lines("long_lines.yaml", "  line-length: {max: 40}\n", code),
        [2, 5, 7, 8, 9]
    );
    let strict = "  line-length: {max: 40, allow-non-breakable-words: false}\n";
    assert_eq!(
        lines("long_lines.yaml", strict, code),
        [2, 4, 5, 6, 7, 8, 9]
    );
}

#[test]
fn line_length_non_breakable_inline_mappings() {
    let rules = "  line-length: {max: 40, allow-non-breakable-inline-mappings: true}\n";
    assert_eq!(
        lines("long_lines.yaml", rules, DiagnosticCode::LINE_LENGTH),
        [7, 9]
    );
}

#[test]
fn duplicated_merge_keys_are_reported_only_when_forbidden() {
    let code = DiagnosticCode::DUPLICATE_KEY;
    let forbidden = "  duplicate-key: {forbid-duplicated-merge-keys: true}\n";
    assert_eq!(lines("merge_keys.yaml", forbidden, code), [7, 11]);
    let allowed = "  duplicate-key: {forbid-duplicated-merge-keys: false}\n";
    assert_eq!(lines("merge_keys.yaml", allowed, code), [11]);
}

#[test]
fn truthy_reports_true_and_false_when_they_are_not_allowed() {
    let code = DiagnosticCode::TRUTHY;
    let yes_only = "  truthy: {allowed-values: [yes], check-keys: true}\n";
    assert_eq!(
        lines("truthy_allowed.yaml", yes_only, code),
        [1, 2, 4, 5, 8, 9]
    );
    let none = "  truthy: {allowed-values: [], check-keys: true}\n";
    assert_eq!(
        lines("truthy_allowed.yaml", none, code),
        [1, 2, 3, 4, 5, 8, 9, 10]
    );
    let default = "  truthy: {check-keys: true}\n";
    assert_eq!(
        lines("truthy_allowed.yaml", default, code),
        [3, 4, 5, 8, 10]
    );
}

#[test]
fn truthy_does_not_report_single_letters() {
    assert_eq!(
        lines(
            "truthy_letters.yaml",
            "  truthy: {check-keys: true}\n",
            DiagnosticCode::TRUTHY
        ),
        [5, 6, 7]
    );
}
