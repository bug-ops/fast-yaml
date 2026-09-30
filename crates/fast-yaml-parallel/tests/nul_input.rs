//! Regression tests for #417: NUL in a later chunk is an error, not silent truncation.

use fast_yaml_parallel::parse_parallel;

#[test]
fn nul_in_a_late_document_is_rejected() {
    let mut yaml: String = "---\nk: 1\n".repeat(20_000);
    assert_eq!(parse_parallel(&yaml).unwrap().len(), 20_000);
    yaml.push_str("---\nz: 1\0\n");
    assert!(parse_parallel(&yaml).is_err());
}
