//! JSON formatter for machine-readable output.

use std::borrow::Cow;
use std::io;

use serde::{Serialize, Serializer as _};

use crate::{
    Diagnostic, DiagnosticCode, DiagnosticContext, Formatter, Severity, Span, Suggestion,
    formatter::{Finding, Findings},
};

/// JSON formatter for machine-readable output.
///
/// Serializes diagnostics, with the source lines beside them, to JSON format for consumption
/// by tools and IDEs.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::formatter::Findings;
/// use fast_yaml_linter::{JsonFormatter, Formatter};
///
/// let formatter = JsonFormatter::new(true);
/// let output = formatter.format(Findings::EMPTY);
/// assert_eq!(output, "[]");
/// ```
#[cfg(feature = "json-output")]
pub struct JsonFormatter {
    /// Pretty-print JSON.
    pub pretty: bool,
}

#[cfg(feature = "json-output")]
impl JsonFormatter {
    /// Creates a new JSON formatter.
    ///
    /// # Parameters
    ///
    /// - `pretty`: Whether to pretty-print the JSON output
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::JsonFormatter;
    ///
    /// let formatter = JsonFormatter::new(true);
    /// assert!(formatter.pretty);
    /// ```
    #[must_use]
    pub const fn new(pretty: bool) -> Self {
        Self { pretty }
    }
}

#[cfg(feature = "json-output")]
impl Default for JsonFormatter {
    fn default() -> Self {
        Self::new(false)
    }
}

/// The JSON form of one diagnostic: the diagnostic fields, then `context`, then `suggestions`,
/// then the optional `file`.
///
/// Borrows everything, so a sequence of them is serialized without copying the diagnostics.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::formatter::{Findings, JsonDiagnostic};
/// use fast_yaml_linter::{DiagnosticBuilder, DiagnosticCode, Location, Severity, Span};
///
/// let span = Span::new(Location::new(1, 1, 0), Location::new(1, 2, 1));
/// let pairs = [(
///     DiagnosticBuilder::new(DiagnosticCode::TRUTHY, Severity::Warning, "m", span)
///         .build_without_excerpt(),
///     None,
/// )];
/// let finding = Findings::Given(&pairs).iter().next().unwrap();
/// let json = serde_json::to_string(&JsonDiagnostic::new(&finding, Some("a.yaml"))).unwrap();
/// assert!(json.starts_with(r#"{"code":"truthy","severity":"warning""#));
/// assert!(json.ends_with(r#""file":"a.yaml"}"#));
/// ```
#[cfg(feature = "json-output")]
#[derive(Serialize)]
pub struct JsonDiagnostic<'a> {
    code: &'a DiagnosticCode,
    severity: Severity,
    message: &'a str,
    span: Span,
    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<Cow<'a, DiagnosticContext>>,
    #[serde(skip_serializing_if = "<[Suggestion]>::is_empty")]
    suggestions: &'a [Suggestion],
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<&'a str>,
}

#[cfg(feature = "json-output")]
impl<'a> JsonDiagnostic<'a> {
    /// Describes `finding`, naming the `file` it was found in when there is one.
    #[must_use]
    pub fn new(finding: &Finding<'a>, file: Option<&'a str>) -> Self {
        let Diagnostic {
            code,
            severity,
            message,
            span,
            suggestions,
            ..
        } = finding.diagnostic();
        Self {
            code,
            severity: *severity,
            message,
            span: *span,
            context: finding.context(),
            suggestions,
            file,
        }
    }
}

#[cfg(feature = "json-output")]
impl Formatter for JsonFormatter {
    fn write(&self, out: &mut dyn io::Write, findings: Findings<'_>) -> io::Result<()> {
        let entries = findings
            .iter()
            .map(|finding| JsonDiagnostic::new(&finding, None));
        if self.pretty {
            (&mut serde_json::Serializer::pretty(out)).collect_seq(entries)
        } else {
            (&mut serde_json::Serializer::new(out)).collect_seq(entries)
        }
        .map_err(io::Error::from)
    }
}

#[cfg(all(test, feature = "json-output"))]
mod tests {
    use super::*;
    use crate::{DiagnosticBuilder, DiagnosticCode, Location, Severity, Span};

    #[test]
    fn test_json_formatter_empty() {
        let formatter = JsonFormatter::new(false);
        let output = formatter.format(Findings::EMPTY);
        assert_eq!(output, "[]");
    }

    #[test]
    fn test_json_formatter_pretty() {
        let formatter = JsonFormatter::new(true);
        let output = formatter.format(Findings::EMPTY);
        assert_eq!(output, "[]");
    }

    #[test]
    fn test_json_formatter_with_diagnostic() {
        let span = Span::new(Location::new(1, 1, 0), Location::new(1, 4, 3));

        let diagnostic =
            DiagnosticBuilder::new(DiagnosticCode::LINE_LENGTH, Severity::Info, "test", span)
                .build_without_excerpt();

        let formatter = JsonFormatter::new(false);
        let output = formatter.format(Findings::Given(&[(diagnostic, None)]));

        assert!(output.contains("\"code\":\"line-length\""));
        assert!(output.contains("\"severity\":\"info\""));
    }
}

#[cfg(all(test, feature = "json-output"))]
mod excerpt_tests {
    use super::*;
    use crate::{LintSource, Linter, SourceContext};

    #[test]
    fn keys_follow_the_diagnostic_layout_with_the_excerpt_after_the_span() {
        let source = LintSource::new("a:   1\n").unwrap();
        let diagnostics = Linter::with_all_rules().lint_source(&source).unwrap();
        let context = source.context();
        let json = JsonFormatter::new(false).format(Findings::FromSource {
            diagnostics: &diagnostics,
            source: &context,
        });
        let at = |key: &str| json.find(key).unwrap();
        assert!(at("\"message\"") < at("\"span\""));
        assert!(at("\"span\"") < at("\"context\""));
        assert!(!json.contains("\"excerpt\""));
    }

    #[test]
    fn given_and_lazy_excerpts_serialize_identically() {
        let source = LintSource::new("a:   1\n").unwrap();
        let diagnostics = Linter::with_all_rules().lint_source(&source).unwrap();
        let context = source.context();
        let lazy = JsonFormatter::new(true).format(Findings::FromSource {
            diagnostics: &diagnostics,
            source: &context,
        });
        let cut = Findings::cut(diagnostics, &context);
        assert_eq!(JsonFormatter::new(true).format(Findings::Given(&cut)), lazy);
    }

    #[test]
    fn omitted_excerpt_has_no_context_key() {
        let diagnostic = crate::formatter::syntax_diagnostic(
            &Linter::with_all_rules().lint("a: [").unwrap_err(),
            "a: [",
        );
        let context = SourceContext::new("a: [");
        let json = JsonFormatter::new(false).format(Findings::FromSource {
            diagnostics: &[diagnostic],
            source: &context,
        });
        assert!(!json.contains("\"context\""), "{json}");
    }

    #[test]
    fn a_diagnostic_serializes_without_the_excerpt_policy() {
        let source = LintSource::new("a:   1\n").unwrap();
        let diagnostics = Linter::with_all_rules().lint_source(&source).unwrap();
        let json = serde_json::to_string(&diagnostics).unwrap();
        assert!(!json.contains("excerpt") && !json.contains("context"));
    }
}
