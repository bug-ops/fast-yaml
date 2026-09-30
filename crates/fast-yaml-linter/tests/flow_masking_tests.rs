//! End-to-end tests: comments and quoted scalars must not affect flow diagnostics.

use fast_yaml_linter::{DiagnosticCode, Linter};

fn count(yaml: &str, code: &str) -> usize {
    Linter::with_all_rules()
        .lint(yaml)
        .unwrap()
        .iter()
        .filter(|d| d.code.as_str() == code)
        .count()
}

#[test]
fn comment_brace_yields_no_braces_diagnostic() {
    assert_eq!(count("# a { b\nkey: value\n", DiagnosticCode::BRACES), 0);
    assert_eq!(count("key: value # a { b\n", DiagnosticCode::BRACES), 0);
}

#[test]
fn comment_bracket_in_multiline_flow_sequence_keeps_comma_diagnostic() {
    let yaml = "key: [\n  1 ,2, # open [ here\n  3\n]\n";
    assert_eq!(count(yaml, DiagnosticCode::COMMAS), 2);
    assert_eq!(count(yaml, DiagnosticCode::BRACKETS), 0);
}

#[test]
fn quoted_delimiters_yield_no_diagnostics() {
    let yaml = "a: \"x { y\"\nb: 'z [ w'\nc: \"multi {\n  line\"\n";
    assert_eq!(count(yaml, DiagnosticCode::BRACES), 0);
    assert_eq!(count(yaml, DiagnosticCode::BRACKETS), 0);
}

#[test]
fn stray_quotes_in_plain_scalars_do_not_mask_later_diagnostics() {
    let tails = "list: [ 1, 2 ]\nmap: {a: 1 }\nx: [1,2]\n";
    for head in [
        "note: the '90s were great\n",
        "desc: 12 \" wide\n",
        "a: foo\n  'bar\n",
        "a: foo \"bar {\n",
        "a: don 't {x\n",
    ] {
        let yaml = format!("{head}{tails}");
        assert_eq!(count(&yaml, DiagnosticCode::BRACKETS), 2, "{yaml}");
        assert_eq!(count(&yaml, DiagnosticCode::BRACES), 1, "{yaml}");
    }
}

#[test]
fn stray_quote_before_syntax_error_does_not_panic() {
    let _ = Linter::with_all_rules().lint("a: foo \"bar {\nb: [1,2 ,3\n");
}

#[test]
fn unterminated_directive_does_not_hang() {
    for yaml in ["%", "%YAML", "a: 1\n%", "a\n...\n%FOO bar"] {
        let _ = Linter::with_all_rules().lint(yaml);
    }
}
