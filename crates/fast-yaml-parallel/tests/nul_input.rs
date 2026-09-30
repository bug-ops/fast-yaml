//! Regression tests for #417: NUL in a later chunk is an error, not silent truncation.

use fast_yaml_parallel::parse_parallel;

#[test]
fn nul_in_a_late_document_is_rejected() {
    let mut yaml: String = "---\nk: 1\n".repeat(20_000);
    assert_eq!(parse_parallel(&yaml).unwrap().len(), 20_000);
    yaml.push_str("---\nz: 1\0\n");
    assert!(parse_parallel(&yaml).is_err());
}

#[test]
fn control_characters_are_rejected_in_every_document() {
    for c in ['\u{7F}', '\u{86}', '\u{FFFE}', '\u{FFFF}', '\u{1}'] {
        let yaml = format!("---\nk: 1\n---\nz: a{c}b\n");
        let err = parse_parallel(&yaml).unwrap_err().to_string();
        assert!(err.contains("not allowed in YAML"), "{c:?}: {err}");
        let first = format!("k: a{c}b\n---\nz: 1\n");
        assert!(parse_parallel(&first).is_err(), "{c:?}");
    }
}

#[test]
fn rejected_character_reports_its_document_index() {
    for (yaml, marker) in [
        ("a: \u{1}\n", None),
        ("a: 1\n---\nb: \u{1}\n", Some("(document 2)")),
        ("a: 1\n---\nb: 2\n---\nc: \u{1}", Some("(document 3)")),
        (
            "\u{FEFF}a: 1\n...\n\u{FEFF}b: \u{7F}\n",
            Some("(document 2)"),
        ),
    ] {
        let message = parse_parallel(yaml).unwrap_err().to_string();
        assert!(message.contains("not allowed in YAML"), "{message}");
        match marker {
            Some(marker) => assert!(message.contains(marker), "{yaml:?}: {message}"),
            None => assert!(!message.contains("(document"), "{yaml:?}: {message}"),
        }
    }
}
