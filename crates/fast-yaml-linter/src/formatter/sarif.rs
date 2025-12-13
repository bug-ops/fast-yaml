//! SARIF 2.1.0 formatter for IDE integration.

use crate::{Diagnostic, Formatter};

/// SARIF 2.1.0 formatter for IDE integration.
///
/// Generates output in the Static Analysis Results Interchange Format (SARIF)
/// for consumption by IDEs and analysis tools.
///
/// # Examples
///
/// ```ignore
/// use fast_yaml_linter::{SarifFormatter, Formatter};
///
/// let formatter = SarifFormatter;
/// let output = formatter.format(&[], "");
/// assert!(output.contains("\"version\":\"2.1.0\""));
/// ```
#[cfg(feature = "sarif-output")]
pub struct SarifFormatter;

#[cfg(feature = "sarif-output")]
impl Formatter for SarifFormatter {
    fn format(&self, diagnostics: &[Diagnostic], _source: &str) -> String {
        use serde_json::json;

        let results: Vec<_> = diagnostics
            .iter()
            .map(|d| {
                json!({
                    "ruleId": d.code.as_str(),
                    "level": match d.severity {
                        crate::Severity::Error => "error",
                        crate::Severity::Warning => "warning",
                        crate::Severity::Info | crate::Severity::Hint => "note",
                    },
                    "message": {
                        "text": d.message
                    },
                    "locations": [{
                        "physicalLocation": {
                            "artifactLocation": {
                                "uri": "input"
                            },
                            "region": {
                                "startLine": d.span.start.line,
                                "startColumn": d.span.start.column,
                                "endLine": d.span.end.line,
                                "endColumn": d.span.end.column
                            }
                        }
                    }]
                })
            })
            .collect();

        let sarif = json!({
            "version": "2.1.0",
            "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
            "runs": [{
                "tool": {
                    "driver": {
                        "name": "fast-yaml-linter",
                        "informationUri": "https://github.com/rabax/fast-yaml",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                },
                "results": results
            }]
        });

        serde_json::to_string_pretty(&sarif).unwrap_or_else(|_| "{}".to_string())
    }
}

#[cfg(all(test, feature = "sarif-output"))]
mod tests {
    use super::*;
    use crate::{DiagnosticBuilder, DiagnosticCode, Location, Severity, Span};

    #[test]
    fn test_sarif_formatter_empty() {
        let formatter = SarifFormatter;
        let output = formatter.format(&[], "");
        assert!(output.contains("\"version\": \"2.1.0\""));
        assert!(output.contains("\"results\": []"));
    }

    #[test]
    fn test_sarif_formatter_with_diagnostic() {
        let source = "key: value";
        let span = Span::new(Location::new(1, 1, 0), Location::new(1, 4, 3));

        let diagnostic =
            DiagnosticBuilder::new(DiagnosticCode::LINE_LENGTH, Severity::Info, "test", span)
                .build_without_context();

        let formatter = SarifFormatter;
        let output = formatter.format(&[diagnostic], source);

        assert!(output.contains("\"ruleId\": \"line-length\""));
        assert!(output.contains("\"level\": \"note\""));
    }
}
