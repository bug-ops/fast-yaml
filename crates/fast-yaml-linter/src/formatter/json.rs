//! JSON formatter for machine-readable output.

use crate::{Diagnostic, Formatter};

/// JSON formatter for machine-readable output.
///
/// Serializes diagnostics to JSON format for consumption by tools
/// and IDEs.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{JsonFormatter, Formatter};
///
/// let formatter = JsonFormatter::new(true);
/// let output = formatter.format(&[], "");
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

#[cfg(feature = "json-output")]
impl Formatter for JsonFormatter {
    fn format(&self, diagnostics: &[Diagnostic], _source: &str) -> String {
        if self.pretty {
            serde_json::to_string_pretty(diagnostics).unwrap_or_else(|_| "[]".to_string())
        } else {
            serde_json::to_string(diagnostics).unwrap_or_else(|_| "[]".to_string())
        }
    }
}

#[cfg(all(test, feature = "json-output"))]
mod tests {
    use super::*;
    use crate::{DiagnosticBuilder, DiagnosticCode, Location, Severity, Span};

    #[test]
    fn test_json_formatter_empty() {
        let formatter = JsonFormatter::new(false);
        let output = formatter.format(&[], "");
        assert_eq!(output, "[]");
    }

    #[test]
    fn test_json_formatter_pretty() {
        let formatter = JsonFormatter::new(true);
        let output = formatter.format(&[], "");
        assert_eq!(output, "[]");
    }

    #[test]
    fn test_json_formatter_with_diagnostic() {
        let source = "key: value";
        let span = Span::new(Location::new(1, 1, 0), Location::new(1, 4, 3));

        let diagnostic =
            DiagnosticBuilder::new(DiagnosticCode::LINE_LENGTH, Severity::Info, "test", span)
                .build_without_context();

        let formatter = JsonFormatter::new(false);
        let output = formatter.format(&[diagnostic], source);

        assert!(output.contains("\"code\":\"line-length\""));
        assert!(output.contains("\"severity\":\"info\""));
    }
}
