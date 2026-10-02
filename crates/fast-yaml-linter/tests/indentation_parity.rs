//! Findings of the indentation rule, checked against yamllint 1.38 on the same inputs.
//!
//! Each expected `(line, column, message)` was taken from yamllint itself; regenerate with
//! `.local/testing/gen_indent_parity.py`.

use fast_yaml_linter::{ConfigFile, DiagnosticCode, Linter};

struct Case {
    name: &'static str,
    source: &'static str,
    options: &'static str,
    expected: &'static [(usize, usize, &'static str)],
}

const CASES: &[Case] = &[
    Case {
        name: "consistent_first_level_sets_width",
        source: r"a:
   b: 1
   c:
      d: 2
",
        options: "spaces: consistent",
        expected: &[],
    },
    Case {
        name: "consistent_rejects_a_different_width",
        source: r"a:
  b: 1
  c:
     d: 2
",
        options: "spaces: consistent",
        expected: &[(4, 6, "wrong indentation: expected 4 but found 5")],
    },
    Case {
        name: "fixed_two_flags_three",
        source: r"a:
   b: 1
",
        options: "spaces: 2",
        expected: &[(2, 4, "wrong indentation: expected 2 but found 3")],
    },
    Case {
        name: "fixed_four",
        source: r"a:
  b: 1
    # c
",
        options: "spaces: 4",
        expected: &[(2, 3, "wrong indentation: expected 4 but found 2")],
    },
    Case {
        name: "sequence_not_indented_default_true",
        source: r"a:
- 1
- 2
",
        options: "spaces: 2, indent-sequences: true",
        expected: &[(2, 1, "wrong indentation: expected 2 but found 0")],
    },
    Case {
        name: "sequence_indented_when_false",
        source: r"a:
  - 1
  - 2
",
        options: "spaces: 2, indent-sequences: false",
        expected: &[(2, 3, "wrong indentation: expected 0 but found 2")],
    },
    Case {
        name: "sequence_whatever_accepts_both",
        source: r"a:
- 1
b:
  - 2
",
        options: "spaces: 2, indent-sequences: whatever",
        expected: &[],
    },
    Case {
        name: "sequence_consistent_rejects_mix",
        source: r"a:
- 1
b:
  - 2
",
        options: "spaces: 2, indent-sequences: consistent",
        expected: &[(4, 3, "wrong indentation: expected 0 but found 2")],
    },
    Case {
        name: "sequence_in_sequence",
        source: r"- - a
  - b
-
  - c
",
        options: "spaces: 2",
        expected: &[],
    },
    Case {
        name: "mapping_in_sequence",
        source: r"- a: 1
  b: 2
-   c: 3
    d: 4
",
        options: "spaces: 2",
        expected: &[],
    },
    Case {
        name: "explicit_key",
        source: r"? a
: b
? c
:   d
",
        options: "spaces: 2",
        expected: &[],
    },
    Case {
        name: "explicit_key_block_value",
        source: r"? a
:
    - x
",
        options: "spaces: 2",
        expected: &[(3, 5, "wrong indentation: expected 2 but found 4")],
    },
    Case {
        name: "flow_sequence_continuation",
        source: r"a: [
  1,
    2,
]
",
        options: "spaces: 2",
        expected: &[(3, 5, "wrong indentation: expected 2 but found 4")],
    },
    Case {
        name: "flow_mapping_closer",
        source: r"a: {
  b: 1,
  }
",
        options: "spaces: 2",
        expected: &[(3, 3, "wrong indentation: expected 0 but found 2")],
    },
    Case {
        name: "flow_same_line_items",
        source: r"a: [b,
    c]
",
        options: "spaces: 2",
        expected: &[],
    },
    Case {
        name: "flow_wrong_continuation",
        source: r"a: [b,
  c]
",
        options: "spaces: 2",
        expected: &[(2, 3, "wrong indentation: expected 4 but found 2")],
    },
    Case {
        name: "block_scalar_values",
        source: r"a: |
  text
   more
b: >
    folded
c: 1
",
        options: "spaces: 2",
        expected: &[],
    },
    Case {
        name: "anchor_then_value_line",
        source: r"a: &x
  b: 1
c: !!map
    d: 2
",
        options: "spaces: 2",
        expected: &[(4, 5, "wrong indentation: expected 2 but found 4")],
    },
    Case {
        name: "multiple_documents",
        source: r"a:
   b: 1
---
c:
  d: 2
",
        options: "spaces: consistent",
        expected: &[(5, 3, "wrong indentation: expected 3 but found 2")],
    },
    Case {
        name: "comments_do_not_count",
        source: r"a:
  # c
   b: 1
",
        options: "spaces: 2",
        expected: &[(3, 4, "wrong indentation: expected 2 but found 3")],
    },
    Case {
        name: "deep_nesting",
        source: r"a:
  b:
    c:
     d: 1
",
        options: "spaces: 2",
        expected: &[(4, 6, "wrong indentation: expected 6 but found 5")],
    },
    Case {
        name: "empty_values",
        source: r"a:
b:
  c:
d: 1
",
        options: "spaces: 2",
        expected: &[],
    },
    Case {
        name: "set_keys",
        source: r"? a
? b
",
        options: "spaces: 2",
        expected: &[],
    },
    Case {
        name: "sequence_under_key_misindented",
        source: r"a:
    - 1
",
        options: "spaces: 2",
        expected: &[(2, 5, "wrong indentation: expected 2 but found 4")],
    },
    Case {
        name: "sequence_leading_indent_document",
        source: r"  a: 1
  b: 2
",
        options: "spaces: 2",
        expected: &[(1, 3, "wrong indentation: expected 0 but found 2")],
    },
    Case {
        name: "multi_line_plain_default",
        source: r"a: x
  y
   z
",
        options: "spaces: 2",
        expected: &[],
    },
    Case {
        name: "mls_plain",
        source: r"a: x
  y
   z
",
        options: "spaces: 2, check-multi-line-strings: true",
        expected: &[(2, 3, "wrong indentation: expected 3 but found 2")],
    },
    Case {
        name: "mls_quoted",
        source: r"a: 'x
   y
  z'
",
        options: "spaces: 2, check-multi-line-strings: true",
        expected: &[
            (2, 4, "wrong indentation: expected 4 but found 3"),
            (3, 3, "wrong indentation: expected 4 but found 2"),
        ],
    },
    Case {
        name: "mls_literal",
        source: r"a: |
  x
   y
b: >
    p
     q
",
        options: "spaces: 2, check-multi-line-strings: true",
        expected: &[
            (3, 4, "wrong indentation: expected 2 but found 3"),
            (5, 5, "wrong indentation: expected 2 but found 4"),
            (6, 6, "wrong indentation: expected 2 but found 5"),
        ],
    },
    Case {
        name: "mls_sequence_literal",
        source: r"- |
  x
    y
-
  z
",
        options: "spaces: 2, check-multi-line-strings: true",
        expected: &[(2, 3, "wrong indentation: expected 4 but found 2")],
    },
    Case {
        name: "mls_explicit_key",
        source: r"? |
    k
: >
     v
     w
",
        options: "spaces: 2, check-multi-line-strings: true",
        expected: &[
            (4, 6, "wrong indentation: expected 4 but found 5"),
            (5, 6, "wrong indentation: expected 4 but found 5"),
        ],
    },
    Case {
        name: "mls_nested_literal",
        source: r"a:
  b: |
     deep
      more
",
        options: "spaces: 2, check-multi-line-strings: true",
        expected: &[
            (3, 6, "wrong indentation: expected 4 but found 5"),
            (4, 7, "wrong indentation: expected 4 but found 6"),
        ],
    },
];

fn indentation_findings(case: &Case) -> Vec<(usize, usize, String)> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(
        &path,
        format!("rules:\n  indentation: {{{}}}\n", case.options),
    )
    .unwrap();
    let (config, _) = ConfigFile::load(&path).unwrap().into_parts();
    Linter::with_config(config)
        .lint(case.source)
        .unwrap()
        .iter()
        .filter(|d| d.code.as_str() == DiagnosticCode::INDENTATION)
        .map(|d| {
            (
                d.span.start.line(),
                d.span.start.column(),
                d.message.to_string(),
            )
        })
        .collect()
}

#[test]
fn findings_match_yamllint() {
    for case in CASES {
        let expected: Vec<(usize, usize, String)> = case
            .expected
            .iter()
            .map(|&(line, column, message)| (line, column, message.to_owned()))
            .collect();
        assert_eq!(indentation_findings(case), expected, "{}", case.name);
    }
}
