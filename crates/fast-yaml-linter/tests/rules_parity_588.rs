//! Positions reported for the key-ordering, quoted-strings and empty-values rules, checked
//! against yamllint 1.38 on the same inputs.
//!
//! Each expected `(line, column)` was taken from yamllint itself.

use fast_yaml_linter::{ConfigFile, DiagnosticCode, Linter};

/// Lints `source` with `rules` (a YAML mapping of rule settings) and returns the `(line, column)`
/// of every diagnostic with `code`.
fn positions(source: &str, rules: &str, code: &str) -> Vec<(usize, usize)> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, format!("rules:\n{rules}")).unwrap();
    let (config, _) = ConfigFile::load(&path).unwrap().into_parts();
    Linter::with_config(config)
        .lint(source)
        .unwrap()
        .iter()
        .filter(|d| d.code.as_str() == code)
        .map(|d| (d.span.start.line, d.span.start.column))
        .collect()
}

fn ordering(source: &str) -> Vec<(usize, usize)> {
    positions(
        source,
        "  key-ordering: enable\n",
        DiagnosticCode::KEY_ORDERING,
    )
}

fn quoting(source: &str) -> Vec<(usize, usize)> {
    positions(
        source,
        "  quoted-strings: {required: only-when-needed}\n",
        DiagnosticCode::QUOTED_STRINGS,
    )
}

fn empty(source: &str) -> Vec<(usize, usize)> {
    positions(
        source,
        "  empty-values: enable\n",
        DiagnosticCode::EMPTY_VALUES,
    )
}

#[test]
fn key_ordering_checks_flow_mappings_anywhere() {
    assert_eq!(ordering("- {y: 1, x: 2}\n"), [(1, 10)]);
    assert_eq!(ordering("a: {d: 1, c: 2}\n"), [(1, 11)]);
    assert_eq!(ordering("m: {z: 1, a: 2}\nn: 1\n"), [(1, 11)]);
    assert_eq!(
        ordering("--- {b: {z: 1, y: 2}, a: 2}\n"),
        [(1, 16), (1, 23)]
    );
    assert_eq!(ordering("{b: 1,\n a: 2}\n"), [(2, 2)]);
    assert_eq!(
        ordering("a: [{c: 1, b: 2}, {z: 1, y: 2}]\n"),
        [(1, 12), (1, 26)]
    );
    assert_eq!(ordering("[b: 1, a: 2]\n"), []);
}

#[test]
fn key_ordering_checks_explicit_and_special_keys() {
    let source = "z: 1\nnull: 2\nbooleans: 3\n~: 4\n<<: {a: 1}\n? y\n: 6\nm: 7\n";
    assert_eq!(ordering(source), [(2, 1), (3, 1), (5, 1), (6, 3), (8, 1)]);
    assert_eq!(ordering("a: 1\n? c\n: 2\n? b\n: 3\n"), [(4, 3)]);
    assert_eq!(ordering("? b\n? a\n"), [(2, 3)]);
}

#[test]
fn key_ordering_skips_anchored_tagged_and_collection_keys() {
    assert_eq!(ordering("b: 1\n&x a: 1\n!!str a2: 1\n"), []);
    assert_eq!(ordering("b: 1\n? [a, b]\n: 1\n"), []);
    assert_eq!(ordering("b: 1\n&y a: 1\n*y : 2\nc: 1\n"), []);
}

#[test]
fn key_ordering_reports_quoted_keys_at_the_quote() {
    assert_eq!(ordering("b: 1\n\"a\": 2\n'A': 3\n"), [(2, 1), (3, 1)]);
    assert_eq!(ordering("\"\": 1\nb: 2\na: 3\n"), [(3, 1)]);
    assert_eq!(ordering("\"日本\": 1\nключ: 2\nabc: 3\n"), [(2, 1), (3, 1)]);
    assert_eq!(ordering("Name: 1\nage: 2\n"), []);
}

#[test]
fn key_ordering_restarts_in_every_collection_and_document() {
    assert_eq!(
        ordering("-\n  b: 1\n  a: 2\n- c: 1\n  a: 3\n"),
        [(3, 3), (5, 3)]
    );
    assert_eq!(ordering("z: 1\na: 2\n---\nz: 3\na: 4\n"), [(2, 1), (5, 1)]);
    assert_eq!(ordering("s: !!set\n  b:\n  a:\nt: 1\n"), [(3, 3)]);
}

#[test]
fn quoted_strings_flags_text_that_is_plain_safe() {
    let source = "a: 'http://x'\nb: \"How are you?\"\nc: \"5 * 5 = 25\"\nd: \"\\u00e9\"\ne: 'c:\\path'\nf: '\"Howdy!\" he cried.'\n";
    assert_eq!(quoting(source), [(1, 4), (2, 4), (3, 4), (4, 4), (5, 4)]);
    let source = "a: \"${{ x }}\"\nb: \"-x\"\nc: \"a#b\"\nd: \"---\"\ne: \"\\t\"\nf: \" a\"\ng: \"@x\"\nh: \"x:\"\ni: \"a: b\"\nj: \"a #b\"\n";
    assert_eq!(quoting(source), [(1, 4), (2, 4), (3, 4), (4, 4)]);
}

#[test]
fn quoted_strings_follows_the_yaml_1_1_resolver() {
    let source = "\
p1: \"1e3\"
p2: \"NaN\"
p3: \"1.0e3\"
p4: \"1.0e+3\"
p5: \".nan\"
p6: \"1_000.5\"
p7: \"190:20:30.15\"
q7: \"Yes\"
q8: \"yEs\"
q9: \"~\"
";
    assert_eq!(quoting(source), [(1, 5), (2, 5), (3, 5), (9, 5)]);
    let source = "- \"123\"\n- \"0o17\"\n- \"1:30\"\n- \"2001-12-14\"\n- \"<<\"\n- \"=\"\n- \"null\"\n- \"nULL\"\n- \"oN\"\n- \"tRUE\"\n";
    assert_eq!(quoting(source), [(8, 3), (9, 3), (10, 3)]);
}

#[test]
fn quoted_strings_tab_and_line_continuation_force_quotes() {
    assert_eq!(quoting("p8: \"a\\tb\"\nq1: \"a:\\tb\"\n"), []);
    assert_eq!(quoting("k: \"foo \\\n  bar\"\n"), []);
    assert_eq!(quoting("k: \"foo\n  bar\"\n"), [(1, 4)]);
    assert_eq!(quoting("k: 'foo\n  bar'\n"), [(1, 4)]);
}

#[test]
fn quoted_strings_uses_block_context_for_plain_text() {
    let source = "- \"a  b\"\n- \"a b \"\n- \" a b\"\n- \"a:b\"\n- \"a:\"\n- \":a\"\n- \": a\"\n- \"?a\"\n- \"? a\"\n- \"-\"\n- \"- a\"\n- \"-a\"\n- \"a-\"\n- \"#a\"\n- \"a #\"\n- \"a#\"\n- \"%a\"\n";
    assert_eq!(
        quoting(source),
        [(1, 3), (4, 3), (6, 3), (8, 3), (12, 3), (13, 3), (16, 3)]
    );
}

#[test]
fn quoted_strings_flow_indicators_need_quotes_in_flow_only() {
    let source =
        "a: [\"a,b\", \"c\", 'x y', \"z]\"]\nb: {k: \"v\", k2: \"a,b\", \"k3\": \"{x}\"}\n";
    assert_eq!(quoting(source), [(1, 12), (1, 17), (2, 8)]);
}

#[test]
fn quoted_strings_ignores_root_scalars_and_checks_after_anchors_like_yamllint() {
    assert_eq!(quoting("--- \"root\"\n"), []);
    assert_eq!(quoting("\"root\"\n"), []);
    let source =
        "- &a \"x\"\n- !!str \"y\"\n- !t \"z\"\n- !t &a \"w\"\n- &a !t \"v\"\n- !!str &a \"u\"\n";
    assert_eq!(quoting(source), [(3, 6), (5, 9)]);
}

#[test]
#[expect(
    clippy::literal_string_with_formatting_args,
    reason = "YAML flow mappings, not format strings"
)]
fn empty_values_are_reported_after_the_colon() {
    let source = [
        "a:",
        "b: {c:}",
        "d:   # note",
        "e: {f: , g: }",
        "? h",
        "j: !!null",
        "k:",
    ]
    .join("\n");
    assert_eq!(
        empty(&source),
        [(1, 3), (2, 7), (3, 3), (4, 7), (4, 12), (7, 3)]
    );
}

#[test]
fn empty_values_with_an_anchor_are_not_reported() {
    assert_eq!(empty("i: &anc\nk:\n"), [(2, 3)]);
}
