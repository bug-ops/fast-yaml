//! SARIF 2.1.0 report for code scanning and IDE integration.
//!
//! Rules carry only their `id`; consumers that need descriptions resolve them by code.

use std::collections::BTreeSet;

use serde::Serialize;

use crate::{Diagnostic, Severity};

use super::report::{FileReport, ReportSource};

const SCHEMA: &str =
    "https://docs.oasis-open.org/sarif/sarif/v2.1.0/errata01/os/schemas/sarif-schema-2.1.0.json";

#[derive(Serialize)]
struct SarifLog<'a> {
    version: &'static str,
    #[serde(rename = "$schema")]
    schema: &'static str,
    runs: [Run<'a>; 1],
}

#[derive(Serialize)]
struct Run<'a> {
    tool: Tool<'a>,
    results: Vec<SarifResult<'a>>,
}

#[derive(Serialize)]
struct Tool<'a> {
    driver: Driver<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Driver<'a> {
    name: &'static str,
    information_uri: &'static str,
    version: &'static str,
    rules: Vec<RuleDescriptor<'a>>,
}

#[derive(Serialize)]
struct RuleDescriptor<'a> {
    id: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifResult<'a> {
    rule_id: &'a str,
    level: Level,
    message: Message<'a>,
    locations: [Location; 1],
}

#[derive(Serialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum Level {
    Error,
    Warning,
    Note,
}

impl From<Severity> for Level {
    fn from(severity: Severity) -> Self {
        match severity {
            Severity::Error => Self::Error,
            Severity::Warning => Self::Warning,
            Severity::Info | Severity::Hint => Self::Note,
        }
    }
}

#[derive(Serialize)]
struct Message<'a> {
    text: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Location {
    physical_location: PhysicalLocation,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PhysicalLocation {
    artifact_location: ArtifactLocation,
    region: Region,
}

#[derive(Serialize)]
struct ArtifactLocation {
    uri: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Region {
    start_line: usize,
    start_column: usize,
    end_line: usize,
    end_column: usize,
}

impl Region {
    /// Builds the region of `d`, clamping the end to the start so it never precedes it.
    fn of(d: &Diagnostic) -> Self {
        let start = (d.span.start.line, d.span.start.column);
        let (end_line, end_column) = start.max((d.span.end.line, d.span.end.column));
        Self {
            start_line: start.0,
            start_column: start.1,
            end_line,
            end_column,
        }
    }
}

fn uri_of(source: &ReportSource) -> String {
    match source {
        ReportSource::Stdin => "stdin".to_owned(),
        ReportSource::File(path) => path.file_uri(),
    }
}

pub(super) fn render(files: &[FileReport<'_>]) -> String {
    let mut results = Vec::new();
    let mut codes = BTreeSet::new();
    for file in files {
        for d in file.diagnostics {
            codes.insert(d.code.as_str());
            results.push(SarifResult {
                rule_id: d.code.as_str(),
                level: d.severity.into(),
                message: Message { text: &d.message },
                locations: [Location {
                    physical_location: PhysicalLocation {
                        artifact_location: ArtifactLocation {
                            uri: uri_of(file.source),
                        },
                        region: Region::of(d),
                    },
                }],
            });
        }
    }

    let log = SarifLog {
        version: "2.1.0",
        schema: SCHEMA,
        runs: [Run {
            tool: Tool {
                driver: Driver {
                    name: "fast-yaml-linter",
                    information_uri: env!("CARGO_PKG_REPOSITORY"),
                    version: env!("CARGO_PKG_VERSION"),
                    rules: codes.into_iter().map(|id| RuleDescriptor { id }).collect(),
                },
            },
            results,
        }],
    };
    #[expect(
        clippy::expect_used,
        reason = "derived Serialize on string-keyed data cannot fail"
    )]
    let mut out = serde_json::to_string_pretty(&log).expect("SARIF log serializes");
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use crate::{DiagnosticBuilder, DiagnosticCode, Location as Loc, Span};
    #[cfg(unix)]
    use {crate::formatter::ReportPath, std::path::Path};

    #[cfg(unix)]
    fn diagnostic(span: Span) -> Diagnostic {
        DiagnosticBuilder::new(DiagnosticCode::LINE_LENGTH, Severity::Info, "test", span)
            .build_without_context()
    }

    #[test]
    fn empty_run_has_results_array() {
        let out = render(&[]);
        assert!(out.contains("\"version\": \"2.1.0\""));
        assert!(out.contains("\"results\": []"));
    }

    #[cfg(unix)]
    #[test]
    fn result_has_uri_level_and_clamped_region() {
        let inverted = Span::new(Loc::new(3, 9, 30), Loc::new(3, 2, 23));
        let source =
            ReportSource::File(ReportPath::from_absolute(Path::new("/w/a b.yaml")).unwrap());
        let diagnostics = [diagnostic(inverted)];
        let out = render(&[FileReport {
            source: &source,
            diagnostics: &diagnostics,
        }]);
        let json: serde_json::Value = serde_json::from_str(&out).unwrap();
        let result = &json["runs"][0]["results"][0];
        assert_eq!(result["ruleId"], "line-length");
        assert_eq!(result["level"], "note");
        let physical = &result["locations"][0]["physicalLocation"];
        assert_eq!(physical["artifactLocation"]["uri"], "file:///w/a%20b.yaml");
        assert_eq!(physical["region"]["endColumn"], 9);
        assert_eq!(
            json["runs"][0]["tool"]["driver"]["rules"][0]["id"],
            "line-length"
        );
    }
}
