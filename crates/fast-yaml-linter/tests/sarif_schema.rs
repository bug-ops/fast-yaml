//! Validates the SARIF report against the official SARIF 2.1.0 JSON schema.

use std::path::Path;

use fast_yaml_linter::formatter::{
    FileReport, ReportFormat, ReportPath, ReportSource, input_error_diagnostic, syntax_diagnostic,
};
use fast_yaml_linter::{Diagnostic, Linter};
use serde_json::Value;

const SCHEMA: &str = include_str!("fixtures/sarif/sarif-schema-2.1.0.json");

fn validate(sarif: &str) {
    let schema: Value = serde_json::from_str(SCHEMA).unwrap();
    let validator = jsonschema::draft4::new(&schema).unwrap();
    let instance: Value = serde_json::from_str(sarif).unwrap();
    let errors: Vec<String> = validator
        .iter_errors(&instance)
        .map(|e| format!("{e} at {}", e.instance_path()))
        .collect();
    assert!(errors.is_empty(), "{errors:#?}\n{sarif}");
}

fn lint(source: &str) -> Vec<Diagnostic> {
    Linter::with_all_rules().lint(source).unwrap()
}

fn file(path: &str) -> ReportSource {
    ReportSource::File(ReportPath::from_absolute(Path::new(path)).unwrap())
}

#[test]
fn empty_report_is_valid() {
    validate(&ReportFormat::Sarif.render(&[]));
}

#[test]
fn lint_findings_are_valid() {
    let source = "key: 1\nkey: 2\n";
    let diagnostics = lint(source);
    assert!(!diagnostics.is_empty());
    let first = file("/work/dir/a b.yaml");
    let second = ReportSource::Stdin;
    let out = ReportFormat::Sarif.render(&[
        FileReport {
            source: &first,
            diagnostics: &diagnostics,
        },
        FileReport {
            source: &second,
            diagnostics: &diagnostics,
        },
    ]);
    validate(&out);
}

#[test]
fn syntax_and_input_errors_are_valid() {
    let source = "a: [";
    let err = Linter::with_all_rules().lint(source).unwrap_err();
    let diagnostics = [
        syntax_diagnostic(&err, source),
        input_error_diagnostic("bad"),
    ];
    let path = file("/work/broken.yaml");
    let out = ReportFormat::Sarif.render(&[FileReport {
        source: &path,
        diagnostics: &diagnostics,
    }]);
    validate(&out);
}
