//! Human-readable text formatter (rustc-style).

use crate::{Diagnostic, Formatter, Severity};
use std::fmt::Write;

/// Human-readable text formatter (rustc-style).
///
/// Formats diagnostics in a style similar to the Rust compiler,
/// with color support for terminals.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{TextFormatter, Formatter};
///
/// let formatter = TextFormatter::new();
/// let output = formatter.format(&[], "");
/// assert!(output.contains("0 errors"));
/// ```
pub struct TextFormatter {
    /// Show source context.
    pub show_context: bool,
    /// Use ANSI colors.
    pub use_color: bool,
    /// Maximum context lines to show.
    pub context_lines: usize,
}

impl TextFormatter {
    /// Creates a new text formatter with defaults.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::TextFormatter;
    ///
    /// let formatter = TextFormatter::new();
    /// assert!(formatter.show_context);
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        Self {
            show_context: true,
            use_color: false,
            context_lines: 2,
        }
    }

    /// Detects if color should be enabled based on terminal.
    ///
    /// Uses the `is-terminal` crate to check if stdout is a terminal.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::TextFormatter;
    ///
    /// let formatter = TextFormatter::with_color_auto();
    /// ```
    #[must_use]
    pub fn with_color_auto() -> Self {
        Self {
            show_context: true,
            use_color: std::io::IsTerminal::is_terminal(&std::io::stdout()),
            context_lines: 2,
        }
    }

    /// Enables or disables color output.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::TextFormatter;
    ///
    /// let formatter = TextFormatter::new().with_color(true);
    /// assert!(formatter.use_color);
    /// ```
    #[must_use]
    pub const fn with_color(mut self, use_color: bool) -> Self {
        self.use_color = use_color;
        self
    }

    /// Sets whether to show source context.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::TextFormatter;
    ///
    /// let formatter = TextFormatter::new().with_context(false);
    /// assert!(!formatter.show_context);
    /// ```
    #[must_use]
    pub const fn with_context(mut self, show_context: bool) -> Self {
        self.show_context = show_context;
        self
    }

    fn colorize(&self, text: &str, severity: Severity) -> String {
        if self.use_color {
            format!(
                "{}{}{}",
                severity.color_code(),
                text,
                Severity::reset_code()
            )
        } else {
            text.to_string()
        }
    }
}

impl Default for TextFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl Formatter for TextFormatter {
    fn format(&self, diagnostics: &[Diagnostic], _source: &str) -> String {
        let mut output = String::new();

        for diagnostic in diagnostics {
            let severity_str = self.colorize(diagnostic.severity.as_str(), diagnostic.severity);

            writeln!(
                output,
                "{}[{}]: {}",
                severity_str,
                diagnostic.code.as_str(),
                diagnostic.message
            )
            .unwrap();

            writeln!(
                output,
                "  --> input:{}:{}",
                diagnostic.span.start.line, diagnostic.span.start.column
            )
            .unwrap();

            if self.show_context
                && let Some(context) = &diagnostic.context
            {
                writeln!(output, "   |").unwrap();

                for line in &context.lines {
                    let line_num_width = 4;
                    writeln!(
                        output,
                        "{:width$} | {}",
                        line.line_number,
                        line.content,
                        width = line_num_width
                    )
                    .unwrap();

                    if !line.highlights.is_empty() {
                        write!(output, "{:width$} | ", "", width = line_num_width).unwrap();

                        for &(start, end) in &line.highlights {
                            let padding = start.saturating_sub(1);
                            let length = end.saturating_sub(start);

                            write!(output, "{:padding$}", "", padding = padding).unwrap();
                            write!(output, "{}", "^".repeat(length)).unwrap();
                        }

                        writeln!(output).unwrap();
                    }
                }

                writeln!(output, "   |").unwrap();
            }

            if !diagnostic.suggestions.is_empty() {
                for suggestion in &diagnostic.suggestions {
                    writeln!(output, "   = help: {}", suggestion.message).unwrap();
                }
            }

            writeln!(output).unwrap();
        }

        let error_count = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count();
        let warning_count = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .count();

        let summary = if error_count > 0 || warning_count > 0 {
            format!("{error_count} errors, {warning_count} warnings")
        } else {
            "0 errors, 0 warnings".to_string()
        };

        writeln!(output, "{summary}").unwrap();

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DiagnosticBuilder, DiagnosticCode, Location, Span};

    #[test]
    fn test_formatter_new() {
        let formatter = TextFormatter::new();
        assert!(formatter.show_context);
        assert!(!formatter.use_color);
    }

    #[test]
    fn test_formatter_with_color() {
        let formatter = TextFormatter::new().with_color(true);
        assert!(formatter.use_color);
    }

    #[test]
    fn test_formatter_empty() {
        let formatter = TextFormatter::new();
        let output = formatter.format(&[], "");
        assert!(output.contains("0 errors, 0 warnings"));
    }

    #[test]
    fn test_formatter_single_diagnostic() {
        let source = "key: value";
        let span = Span::new(Location::new(1, 1, 0), Location::new(1, 4, 3));

        let diagnostic = DiagnosticBuilder::new(
            DiagnosticCode::LINE_LENGTH,
            Severity::Info,
            "test diagnostic",
            span,
        )
        .build(source);

        let formatter = TextFormatter::new();
        let output = formatter.format(&[diagnostic], source);

        assert!(output.contains("info[line-length]"));
        assert!(output.contains("test diagnostic"));
    }

    #[test]
    fn test_formatter_counts() {
        let source = "key: value";
        let span = Span::new(Location::new(1, 1, 0), Location::new(1, 4, 3));

        let error = DiagnosticBuilder::new(
            DiagnosticCode::DUPLICATE_KEY,
            Severity::Error,
            "error",
            span,
        )
        .build_without_context();

        let warning = DiagnosticBuilder::new(
            DiagnosticCode::INDENTATION,
            Severity::Warning,
            "warning",
            span,
        )
        .build_without_context();

        let formatter = TextFormatter::new();
        let output = formatter.format(&[error, warning], source);

        assert!(output.contains("1 errors, 1 warnings"));
    }
}
