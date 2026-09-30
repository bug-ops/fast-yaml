//! Regression tests for byte/char offset handling with non-ASCII keys and exotic line endings.

use std::num::NonZeroUsize;

use fast_yaml_linter::{
    Diagnostic, DiagnosticCode, LintConfig, Linter, SourceContext,
    config::Limit,
    rules::{DocumentEndPresence, DocumentStartPresence},
    source::SourceMapper,
};

fn lint_code(yaml: &str, code: &str) -> Vec<Diagnostic> {
    Linter::with_all_rules()
        .lint(yaml)
        .unwrap()
        .into_iter()
        .filter(|d| d.code.as_str() == code)
        .collect()
}

/// Asserts a single diagnostic whose span covers exactly `snippet` at `line:column` (char column).
fn assert_single_span(yaml: &str, code: &str, line: usize, column: usize, snippet: &str) {
    let diags = lint_code(yaml, code);
    assert_eq!(diags.len(), 1, "{code} diagnostics for {yaml:?}: {diags:?}");
    let span = diags[0].span;
    assert_eq!(
        (span.start.line, span.start.column),
        (line, column),
        "{yaml:?}"
    );
    assert_eq!(
        SourceContext::new(yaml).get_snippet(span),
        snippet,
        "{yaml:?}"
    );
}

#[test]
fn empty_value_cyrillic_key() {
    let yaml = include_str!("fixtures/edge_cases/empty_value_cyrillic_key.yaml");
    assert_single_span(yaml, DiagnosticCode::EMPTY_VALUES, 2, 5, ":");
}

#[test]
fn empty_value_cjk_key() {
    let yaml = include_str!("fixtures/edge_cases/empty_value_cjk_key.yaml");
    assert_single_span(yaml, DiagnosticCode::EMPTY_VALUES, 1, 3, ":");
}

#[test]
fn empty_value_emoji_key() {
    let yaml = include_str!("fixtures/edge_cases/empty_value_emoji_key.yaml");
    assert_single_span(yaml, DiagnosticCode::EMPTY_VALUES, 1, 2, ":");
}

#[test]
fn empty_value_u2028_key() {
    let yaml = include_str!("fixtures/edge_cases/empty_value_u2028_key.yaml");
    assert_single_span(yaml, DiagnosticCode::EMPTY_VALUES, 1, 4, ":");
}

#[test]
fn empty_value_flow_non_ascii_key() {
    let yaml = include_str!("fixtures/edge_cases/empty_value_flow_non_ascii.yaml");
    assert_single_span(yaml, DiagnosticCode::EMPTY_VALUES, 1, 13, ":");
}

#[test]
fn empty_value_minimal_crlf_does_not_panic() {
    assert_single_span("é:\r\n", DiagnosticCode::EMPTY_VALUES, 1, 2, ":");
}

#[test]
fn empty_value_crlf_second_line() {
    assert_single_span(
        "name: x\r\nпорт:\r\nk: 1\r\n",
        DiagnosticCode::EMPTY_VALUES,
        2,
        5,
        ":",
    );
}

#[test]
fn empty_value_lone_cr_line_endings() {
    let yaml = "name: x\rпорт:\rk2: 1\r";
    let diags = lint_code(yaml, DiagnosticCode::EMPTY_VALUES);
    assert_eq!(diags.len(), 1, "{diags:?}");
    let span = diags[0].span;
    assert_eq!((span.start.line, span.start.column), (2, 5));
    assert_eq!(span.start.offset, "name: x\rпорт".len());
    assert_eq!(SourceContext::new(yaml).get_snippet(span), ":");
}

#[test]
fn truthy_reports_real_column() {
    assert_single_span("ключ: yes\n", DiagnosticCode::TRUTHY, 1, 7, "yes");
    assert_single_span("a: 1\nk: yes\n", DiagnosticCode::TRUTHY, 2, 4, "yes");
    assert_single_span("🔑: On\n", DiagnosticCode::TRUTHY, 1, 4, "On");
    assert_single_span("k:\n  - yes\n", DiagnosticCode::TRUTHY, 2, 5, "yes");
}

#[test]
fn truthy_lone_cr_and_crlf() {
    assert_single_span("a: 1\rключ: yes\r", DiagnosticCode::TRUTHY, 2, 7, "yes");
    assert_single_span("a: 1\r\nключ: yes\r\n", DiagnosticCode::TRUTHY, 2, 7, "yes");
}

#[test]
fn truthy_cyrillic_key_fixture() {
    let yaml = include_str!("fixtures/edge_cases/truthy_cyrillic_key.yaml");
    assert_single_span(yaml, DiagnosticCode::TRUTHY, 1, 7, "yes");
}

#[test]
fn empty_value_repeated_key_reports_the_empty_one() {
    let yaml = include_str!("fixtures/edge_cases/empty_value_repeated_key.yaml");
    assert_single_span(yaml, DiagnosticCode::EMPTY_VALUES, 4, 4, ":");
}

#[test]
fn truthy_quoted_key_containing_colon_fixture() {
    let yaml = include_str!("fixtures/edge_cases/truthy_quoted_key_colon.yaml");
    assert_single_span(yaml, DiagnosticCode::TRUTHY, 1, 8, "yes");
}

#[test]
fn mapper_flow_key_with_non_ascii_neighbours_does_not_panic() {
    let mut mapper = SourceMapper::new("aé: 1, é: 2");
    let span = mapper.find_key_span("é", 1);
    assert!(span.is_none());
    let first = mapper.find_key_span("aé", 1).unwrap();
    assert_eq!((first.start.column, first.end.column), (1, 3));
    assert_eq!((first.start.offset, first.end.offset), (0, 3));
}

#[test]
fn mapper_key_span_uses_char_columns() {
    let mut mapper = SourceMapper::new("名前: 1\n");
    let span = mapper.find_key_span("名前", 1).unwrap();
    assert_eq!((span.start.column, span.end.column), (1, 3));
    assert_eq!((span.start.offset, span.end.offset), (0, 6));
}

#[test]
fn mapper_find_all_chars_uses_byte_offsets() {
    let yaml = "ключ: 1\r🔑: 2\r";
    let colons = SourceMapper::new(yaml).find_all_chars(':');
    assert_eq!(colons.len(), 2);
    assert_eq!(
        (colons[0].line, colons[0].column, colons[0].offset),
        (1, 5, 8)
    );
    assert_eq!((colons[1].line, colons[1].column), (2, 2));
    assert_eq!(colons[1].offset, "ключ: 1\r🔑".len());
}

#[test]
fn mapper_find_colon_after_key_multibyte() {
    let yaml = "x: 1\nключ: 2";
    let mut mapper = SourceMapper::new(yaml);
    let key = mapper.find_key_span("ключ", 2).unwrap();
    let colon = mapper.find_colon_after_key(key).unwrap();
    assert_eq!((colon.line, colon.column), (2, 5));
    assert_eq!(colon.offset, "x: 1\nключ".len());
}

/// Lints `yaml` with every rule and checks each span is in-bounds, on char boundaries, ordered,
/// and that line, column and offset of both ends agree with each other.
fn assert_spans_valid(yaml: &str) {
    let Ok(diags) = Linter::with_all_rules().lint(yaml) else {
        return;
    };
    let body = fast_yaml_core::strip_bom(yaml);
    let bom_len = yaml.len() - body.len();
    let ctx = SourceContext::new(body);
    let expected_location = |offset: usize| {
        let mut loc = ctx.offset_to_location(offset - bom_len);
        loc.offset = offset;
        loc
    };
    for d in diags {
        let spans = std::iter::once(d.span).chain(d.suggestions.iter().map(|s| s.span));
        for span in spans {
            let (s, e) = (span.start.offset, span.end.offset);
            assert!(s <= e && e <= yaml.len(), "{yaml:?}: {d:?}");
            assert!(
                yaml.is_char_boundary(s) && yaml.is_char_boundary(e),
                "{yaml:?}: {d:?}"
            );
            assert_eq!(span.start, expected_location(s), "{yaml:?}: {d:?}");
            assert_eq!(span.end, expected_location(e), "{yaml:?}: {d:?}");
        }
    }
}

#[test]
fn mixed_line_endings_all_rules_keep_spans_valid() {
    let inputs = [
        "a: 1\rb: 2\n#ééééé\n",
        "\rч\n#",
        "\r007\r日本- ---\né\r #c\u{200d}",
        "a: 1 # é\rb: 2\r",
        "a: 1\rb: 2\ra: 3\r",
        "k: v\r#жж\rкл: 0777\r",
        "---\r\nа: &x 1\rб: *x\n...\r",
        "a: 1\r\nb: 2\rc: 3\nd:  \r#é\r\n",
        "# é\r\n# ж\rkey:   yes\n\n\n\r\n",
        "x: |\r  ééé\r  ж\ry: 1\n",
        "{a: 1,\rб: 2}\r\n",
        "é: 1\r\n\r\n\r\n\r\nb: 2\r\n",
        "a: 1\r",
        "é: 1\r...\r",
        "- yes\r-   no\r\n",
        "\u{feff}ключ:\r\nk: yes\r",
    ];
    for yaml in inputs {
        assert_spans_valid(yaml);
    }
}

#[test]
fn mixed_line_endings_comment_stays_on_its_line() {
    let yaml = "a: 1 # é\rb: 2\r";
    let ctx = fast_yaml_linter::LintContext::new(yaml);
    let comments = ctx.comments();
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].span.start.line, 1);
    assert_eq!(comments[0].span.end.line, 1);
}

#[test]
fn trailing_whitespace_crlf_non_ascii() {
    let yaml = "a: 1\r\nk: é \r\n";
    let diags = lint_code(yaml, DiagnosticCode::TRAILING_WHITESPACE);
    assert_eq!(diags.len(), 1);
    let span = diags[0].span;
    assert_eq!((span.start.line, span.start.column), (2, 5));
    assert_eq!((span.start.offset, span.end.offset), (11, 12));
    assert_eq!(SourceContext::new(yaml).get_snippet(span), " ");
    let sug = diags[0].suggestions[0].span;
    assert_eq!((sug.start.offset, sug.end.offset), (11, 12));
}

#[test]
fn trailing_whitespace_lone_cr() {
    let yaml = "a: 1\rk: é \r";
    let diags = lint_code(yaml, DiagnosticCode::TRAILING_WHITESPACE);
    assert_eq!(diags.len(), 1);
    assert_eq!(
        (diags[0].span.start.line, diags[0].span.start.offset),
        (2, 10)
    );
}

#[test]
fn empty_value_flow_multiple_non_ascii_keys() {
    let yaml = "{ключ: , b: 1, é: }\n";
    let diags = lint_code(yaml, DiagnosticCode::EMPTY_VALUES);
    let cols: Vec<_> = diags.iter().map(|d| d.span.start.column).collect();
    assert_eq!(cols, [6, 17]);
    for d in &diags {
        assert_eq!(SourceContext::new(yaml).get_snippet(d.span), ":");
    }

    let yaml = "{é: , ю: }\n";
    let cols: Vec<_> = lint_code(yaml, DiagnosticCode::EMPTY_VALUES)
        .iter()
        .map(|d| d.span.start.column)
        .collect();
    assert_eq!(cols, [3, 8]);
}

#[test]
fn empty_value_combining_mark_and_zwj_keys() {
    assert_single_span("e\u{301}:\nk: 1\n", DiagnosticCode::EMPTY_VALUES, 1, 3, ":");
    let zwj = "👨\u{200d}👩:\nk: 1\n";
    assert_single_span(zwj, DiagnosticCode::EMPTY_VALUES, 1, 4, ":");
}

#[test]
fn empty_value_with_bom() {
    let yaml = "\u{feff}ключ:\nk: 1\n";
    let diags = lint_code(yaml, DiagnosticCode::EMPTY_VALUES);
    assert_eq!(diags.len(), 1);
    let span = diags[0].span;
    assert_eq!(
        (span.start.line, span.start.column, span.start.offset),
        (1, 5, 11)
    );
    assert_eq!(SourceContext::new(yaml).get_snippet(span), ":");

    let yaml = "\u{feff}a: 1\r\nключ:\r\nk: 1\r\n";
    let diags = lint_code(yaml, DiagnosticCode::EMPTY_VALUES);
    assert_eq!(diags.len(), 1);
    let span = diags[0].span;
    assert_eq!((span.start.line, span.start.column), (2, 5));
    assert_eq!(SourceContext::new(yaml).get_snippet(span), ":");
}

#[test]
fn truthy_uses_token_position_not_first_substring() {
    assert_single_span("yesterday: yes\n", DiagnosticCode::TRUTHY, 1, 12, "yes");
    assert_single_span("k:\n  - no # note\n", DiagnosticCode::TRUTHY, 2, 5, "no");
}

#[test]
fn truthy_multiple_occurrences() {
    let yaml = "ключ: yes\nдругой:   No\n";
    let diags = lint_code(yaml, DiagnosticCode::TRUTHY);
    let pos: Vec<_> = diags
        .iter()
        .map(|d| (d.span.start.line, d.span.start.column))
        .collect();
    assert_eq!(pos, [(1, 7), (2, 11)]);

    let yaml = "ключ: yes\r\nдругой:   No\r\n";
    let diags = lint_code(yaml, DiagnosticCode::TRUTHY);
    let ctx = SourceContext::new(yaml);
    let snippets: Vec<_> = diags.iter().map(|d| ctx.get_snippet(d.span)).collect();
    assert_eq!(snippets, ["yes", "No"]);
}

#[test]
fn mapper_find_all_chars_quotes_and_non_ascii() {
    let yaml = "ключ: \"a: б\" # x: y\n";
    let colons = SourceMapper::new(yaml).find_all_chars(':');
    assert_eq!(colons.len(), 2);
    assert_eq!((colons[0].column, colons[0].offset), (5, 8));
    assert_eq!(colons[1].column, "ключ: \"a: б\" # x".chars().count() + 1);
}

#[test]
fn mapper_find_colon_after_key_edges() {
    use fast_yaml_linter::{Location, Span};
    let mapper = SourceMapper::new("ключ\nother: 1\n");
    let key = Span::new(Location::new(1, 1, 0), Location::new(1, 5, 8));
    assert!(mapper.find_colon_after_key(key).is_none());

    let past = Span::new(Location::new(1, 1, 0), Location::new(9, 1, 500));
    assert!(mapper.find_colon_after_key(past).is_none());
}

#[test]
fn empty_lines_span_points_at_first_empty_line() {
    let mut config = LintConfig::new();
    config.rules.empty_lines.options.max = Limit::Max(1);
    for yaml in [
        "é: 1\r\n\r\n\r\n\r\nb: 2\r\n",
        "é: 1\r\r\r\rb: 2\r",
        "é: 1\n\r\n\r\r\nb: 2\n",
    ] {
        let diags: Vec<_> = Linter::with_config(config.clone())
            .lint(yaml)
            .unwrap()
            .into_iter()
            .filter(|d| d.code.as_str() == DiagnosticCode::EMPTY_LINES)
            .collect();
        assert_eq!(diags.len(), 1, "{yaml:?}: {diags:?}");
        let loc = diags[0].span.start;
        assert_eq!((loc.line, loc.column), (2, 1), "{yaml:?}");
        assert_eq!(
            loc.offset,
            SourceContext::new(yaml).get_line_offset(2),
            "{yaml:?}"
        );
    }
}

#[test]
fn new_line_at_eof_respects_lone_cr_and_reports_eof_location() {
    for yaml in ["a: 1\r", "a: 1\r\n", "a: 1\n", "é: 1\rb: 2\r"] {
        assert!(
            lint_code(yaml, DiagnosticCode::NEW_LINE_AT_END_OF_FILE).is_empty(),
            "{yaml:?}"
        );
    }
    let yaml = "a: 1\r\né: 2";
    let diags = lint_code(yaml, DiagnosticCode::NEW_LINE_AT_END_OF_FILE);
    assert_eq!(diags.len(), 1);
    let loc = diags[0].span.start;
    assert_eq!((loc.line, loc.column, loc.offset), (2, 5, yaml.len()));
    assert_eq!(diags[0].span.end, loc);
    assert_eq!(diags[0].suggestions[0].replacement.as_deref(), Some("\n"));
}

#[test]
fn truthy_token_positions_for_hyphens_quotes_and_urls() {
    assert_single_span("my-key: yes\n", DiagnosticCode::TRUTHY, 1, 9, "yes");
    assert_single_span("some-thing: yes\n", DiagnosticCode::TRUTHY, 1, 13, "yes");
    assert_single_span("- yes\n", DiagnosticCode::TRUTHY, 1, 3, "yes");
    assert_single_span("k:\n    -   No\n", DiagnosticCode::TRUTHY, 2, 9, "No");
    assert!(lint_code("url: http://a.b/yes\n", DiagnosticCode::TRUTHY).is_empty());
    assert!(lint_code("url: \"yes\"\n", DiagnosticCode::TRUTHY).is_empty());
    assert_single_span("ключ-2: yes\r\n", DiagnosticCode::TRUTHY, 1, 9, "yes");
}

#[test]
fn spans_consistent_for_non_ascii_rule_inputs() {
    let inputs = [
        "ключ: 0755\nдругой: 0o17\n",
        "ключ: .5\nдругой: 1e3\nx: .nan\ny: .inf\n",
        "я: &а 1\nб: &а 2\n",
        "ключ: 1\nб: 2\nа: 3\n",
        "a: 1\n  ключ: [1,2]\n",
    ];
    for yaml in inputs {
        assert_spans_valid(yaml);
    }
}

#[test]
fn octal_reports_char_column() {
    let yaml = include_str!("fixtures/edge_cases/octal_cyrillic_key.yaml");
    assert_single_span(yaml, DiagnosticCode::OCTAL_VALUES, 1, 7, "0755");
    assert_single_span(
        "a: 1\nключ: 0o17\n",
        DiagnosticCode::OCTAL_VALUES,
        2,
        7,
        "0o17",
    );
}

#[test]
fn float_values_report_char_column() {
    let cfg = LintConfig::default();
    let yaml = "ключ: .5\n";
    let diags: Vec<_> = Linter::with_config(cfg)
        .lint(yaml)
        .unwrap()
        .into_iter()
        .filter(|d| d.code.as_str() == DiagnosticCode::FLOAT_VALUES)
        .collect();
    assert!(!diags.is_empty());
    for d in diags {
        assert_eq!((d.span.start.line, d.span.start.column), (1, 7), "{d:?}");
        assert_eq!(SourceContext::new(yaml).get_snippet(d.span), ".5");
    }
}

#[test]
fn duplicate_anchor_reports_char_column() {
    let yaml = "a: &а 1\nб: &а 2\n";
    let diags = lint_code(yaml, DiagnosticCode::INVALID_ANCHOR);
    assert_eq!(diags.len(), 1, "{diags:?}");
    let span = diags[0].span;
    assert_eq!((span.start.line, span.start.column), (2, 4));
    assert_eq!(SourceContext::new(yaml).get_snippet(span), "&а");
    assert_eq!(span.end.column, 6);
}

#[test]
fn key_ordering_reports_key_position() {
    let yaml = "б: 1\nа: 2\n";
    let diags = lint_code(yaml, DiagnosticCode::KEY_ORDERING);
    assert_eq!(diags.len(), 1, "{diags:?}");
    let span = diags[0].span;
    assert_eq!(
        (span.start.line, span.start.column, span.start.offset),
        (2, 1, 6)
    );
    assert_eq!(span.end.column, 2);
    assert_eq!(SourceContext::new(yaml).get_snippet(span), "а");

    let yaml = "x:\n  б: 1\n  а: 2\n";
    let diags = lint_code(yaml, DiagnosticCode::KEY_ORDERING);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].span.start.column, 3);
    assert_eq!(SourceContext::new(yaml).get_snippet(diags[0].span), "а");
}

#[test]
fn line_length_offsets_follow_lines() {
    let config = LintConfig::new().with_max_line_length(NonZeroUsize::new(5));
    let yaml = "ок\nдлинная строка\nx\nещё одна длинная\n";
    let diags: Vec<_> = Linter::with_config(config)
        .lint(yaml)
        .unwrap()
        .into_iter()
        .filter(|d| d.code.as_str() == DiagnosticCode::LINE_LENGTH)
        .collect();
    let ctx = SourceContext::new(yaml);
    let got: Vec<_> = diags
        .iter()
        .map(|d| {
            (
                d.span.start.line,
                d.span.start.offset,
                ctx.get_snippet(d.span),
            )
        })
        .collect();
    assert_eq!(
        got,
        [
            (2, "ок\n".len(), "длинная строка"),
            (4, "ок\nдлинная строка\nx\n".len(), "ещё одна длинная"),
        ]
    );
    assert_eq!(
        diags[0].span.end.column,
        "длинная строка".chars().count() + 1
    );
}

#[test]
fn commas_ignore_directives_and_verbatim_tags() {
    let flagged = |yaml: &str| lint_code(yaml, DiagnosticCode::COMMAS).len();
    let fixture = include_str!("fixtures/edge_cases/commas_directive_verbatim_tag.yaml");
    assert_eq!(flagged(fixture), 0);
    assert_eq!(flagged("%TAG !e! tag:example.com,2026:\n---\na: 1\n"), 0);
    assert_eq!(flagged("a: !<tag:example.com,2026:x> 1\n"), 0);
    assert_eq!(flagged("- !<tag:a.com,2026:x>  v\n"), 0);
    assert_eq!(flagged("a: [1 ,2]\n"), 2);
    assert_eq!(flagged("%TAG !e! tag:a.com,2026:\n---\na: [1 ,2]\n"), 2);
}

fn lint_with(yaml: &str, config: LintConfig, code: &str) -> Vec<Diagnostic> {
    Linter::with_all_rules_and_config(config)
        .lint(yaml)
        .unwrap()
        .into_iter()
        .filter(|d| d.code.as_str() == code)
        .collect()
}

#[test]
fn commas_ignore_verbatim_tags_after_flow_indicators() {
    let flagged = |yaml: &str| lint_code(yaml, DiagnosticCode::COMMAS).len();
    assert_eq!(flagged("k: [!<a,b> x, y]\n"), 0);
    assert_eq!(flagged("seq: [!<tag:yaml.org,2002:str> a, b]\n"), 0);
    assert_eq!(flagged("m: {!<tag:e.com,2000:k> v}\n"), 0);
    assert_eq!(flagged("k: [a,!<a,b> x]\n"), 1);
    assert_eq!(flagged("seq: [!<tag:yaml.org,2002:str> a ,b]\n"), 2);
}

#[test]
fn commas_ignore_directive_in_lone_cr_file() {
    let yaml = "%TAG !e! tag:a.com,2026:\r---\ra: 1\r";
    assert!(lint_code(yaml, DiagnosticCode::COMMAS).is_empty());
}

#[test]
fn key_ordering_quoted_key_span_starts_at_quote() {
    for (yaml, snippet) in [("b: 1\n\"a\": 2\n", "\"a\""), ("b: 1\n'a': 2\n", "'a'")] {
        let diags = lint_code(yaml, DiagnosticCode::KEY_ORDERING);
        assert_eq!(diags.len(), 1, "{diags:?}");
        let span = diags[0].span;
        assert_eq!((span.start.line, span.start.column), (2, 1), "{yaml:?}");
        assert_eq!(SourceContext::new(yaml).get_snippet(span), snippet);
    }
}

#[test]
fn document_start_missing_span_is_file_start() {
    let config = LintConfig::new().with_document_start(DocumentStartPresence::Required);
    for yaml in ["ключ: 1\n", "ключ: 1\r\n", "\u{feff}ключ: 1\r\n"] {
        let diags = lint_with(yaml, config.clone(), DiagnosticCode::DOCUMENT_START);
        assert_eq!(diags.len(), 1, "{yaml:?}");
        let span = diags[0].span;
        assert_eq!((span.start.line, span.start.column), (1, 1), "{yaml:?}");
        assert_eq!(span.start, span.end);
        assert_eq!(diags[0].suggestions[0].span, span);
    }
}

#[test]
fn document_start_forbidden_reports_char_position() {
    let config = LintConfig::new().with_document_start(DocumentStartPresence::Forbidden);
    let yaml = "# é\r\n---\r\nключ: 1\r\n";
    let diags = lint_with(yaml, config, DiagnosticCode::DOCUMENT_START);
    assert_eq!(diags.len(), 1);
    let span = diags[0].span;
    assert_eq!((span.start.line, span.start.column), (2, 1));
    assert_eq!(SourceContext::new(yaml).get_snippet(span), "---");
}

#[test]
fn document_end_missing_span_is_eof() {
    let config = LintConfig::new().with_document_end(DocumentEndPresence::Required);
    let cases = [
        ("ключ: 1\n", 2, 1),
        ("ключ: 1", 1, 8),
        ("ключ: 1\r\n", 2, 1),
        ("a: 1\rключ: 1\r", 3, 1),
    ];
    for (yaml, line, column) in cases {
        let diags = lint_with(yaml, config.clone(), DiagnosticCode::DOCUMENT_END);
        assert_eq!(diags.len(), 1, "{yaml:?}");
        let loc = diags[0].span.start;
        assert_eq!(
            (loc.line, loc.column, loc.offset),
            (line, column, yaml.len()),
            "{yaml:?}"
        );
        assert_eq!(diags[0].span.end, loc);
        assert_eq!(diags[0].suggestions[0].span, diags[0].span);
        let expected = if yaml.ends_with(['\n', '\r']) {
            "..."
        } else {
            "\n..."
        };
        assert_eq!(
            diags[0].suggestions[0].replacement.as_deref(),
            Some(expected)
        );
    }
}

#[test]
fn document_end_marker_with_trailing_spaces_is_present() {
    let config = LintConfig::new().with_document_end(DocumentEndPresence::Required);
    for yaml in ["ключ: 1\n...  \n", "ключ: 1\r\n...\r\n", "ключ: 1\r...\r"] {
        let diags = lint_with(yaml, config.clone(), DiagnosticCode::DOCUMENT_END);
        assert!(diags.is_empty(), "{yaml:?}");
    }
}

#[test]
fn new_lines_span_starts_at_line_with_wrong_ending() {
    let yaml = "a: 1\r\nключ: 2\r\nb: 3\n";
    let diags = lint_code(yaml, DiagnosticCode::NEW_LINES);
    let got: Vec<_> = diags
        .iter()
        .map(|d| (d.span.start.line, d.span.start.column, d.span.start.offset))
        .collect();
    assert_eq!(got, [(1, 1, 0), (2, 1, 6)]);

    let yaml = "ключ: 1\nb: 2\r\n";
    let diags = lint_code(yaml, DiagnosticCode::NEW_LINES);
    assert_eq!(diags.len(), 1);
    assert_eq!(
        (diags[0].span.start.line, diags[0].span.start.offset),
        (2, "ключ: 1\n".len())
    );
}

#[test]
fn empty_values_block_sequence_nulls_are_not_reported() {
    assert!(lint_code("-\n-\n", DiagnosticCode::EMPTY_VALUES).is_empty());
}

#[test]
fn empty_values_non_ascii_crlf_with_markers_required() {
    let config = LintConfig::new()
        .with_document_start(DocumentStartPresence::Required)
        .with_document_end(DocumentEndPresence::Required);
    let yaml = "ключ:\r\nдругой: 1\r\n";
    let diags = lint_with(yaml, config, DiagnosticCode::EMPTY_VALUES);
    assert_eq!(diags.len(), 1);
    assert_eq!(
        (diags[0].span.start.line, diags[0].span.start.column),
        (1, 5)
    );
}

#[test]
fn spans_consistent_with_markers_required() {
    let config = LintConfig::new()
        .with_document_start(DocumentStartPresence::Required)
        .with_document_end(DocumentEndPresence::Required);
    let inputs = [
        "ключ: 1\n",
        "ключ: 1",
        "a: 1\r\nб: 2\r\n",
        "a: 1\rб: 2\r",
        "é: 1\r\n...\r\n",
    ];
    for yaml in inputs {
        let diags = Linter::with_all_rules_and_config(config.clone())
            .lint(yaml)
            .unwrap();
        let ctx = SourceContext::new(yaml);
        for d in diags {
            for loc in [d.span.start, d.span.end] {
                assert_eq!(loc, ctx.offset_to_location(loc.offset), "{yaml:?}: {d:?}");
            }
        }
    }
}
