//! Human-readable text formatter (rustc-style).

use crate::{Diagnostic, Formatter, Severity};
use std::fmt::Write;

const ELLIPSIS: &str = "\u{2026}";

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
/// assert!(output.is_empty());
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
    /// Uses [`std::io::IsTerminal`] to check if stdout is a terminal.
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
                    let head = if line.column_offset > 0 { ELLIPSIS } else { "" };
                    let tail = if line.truncated_end { ELLIPSIS } else { "" };
                    writeln!(
                        output,
                        "{:width$} | {head}{}{tail}",
                        line.line_number,
                        line.content,
                        width = line_num_width
                    )
                    .unwrap();

                    if !line.highlights.is_empty() {
                        write!(output, "{:width$} | ", "", width = line_num_width).unwrap();

                        for &(start, end) in &line.highlights {
                            let padding = start
                                .saturating_sub(line.column_offset.saturating_add(1))
                                .saturating_add(head.chars().count());
                            let length = end.saturating_sub(start);

                            output.push_str(&" ".repeat(padding));
                            output.push_str(&"^".repeat(length));
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

        if error_count > 0 || warning_count > 0 {
            writeln!(output, "{error_count} errors, {warning_count} warnings").unwrap();
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DiagnosticBuilder, DiagnosticCode, Location, Span};

    #[test]
    fn test_format_long_line_is_windowed_and_aligned() {
        let source = "a".repeat(70_000);
        let span = Span::new(
            Location::new(1, 65_600, 65_599),
            Location::new(1, 65_610, 65_609),
        );
        let diagnostic =
            DiagnosticBuilder::new(DiagnosticCode::LINE_LENGTH, Severity::Info, "long", span)
                .build(&source);

        let output = TextFormatter::new().format(&[diagnostic], &source);
        assert!(output.len() < 2_000);
        let source_row = output.lines().find(|l| l.contains(ELLIPSIS)).unwrap();
        let caret_row = output.lines().find(|l| l.contains('^')).unwrap();
        let ellipsis_col = source_row.chars().position(|c| c == '\u{2026}').unwrap();
        let first_caret = caret_row.chars().position(|c| c == '^').unwrap();
        assert_eq!(first_caret - ellipsis_col, 1 + 60);
        assert!(caret_row.ends_with(&"^".repeat(10)));
    }

    #[test]
    fn test_format_long_non_ascii_line_is_bounded() {
        let source = "\u{44f}".repeat(70_000);
        let span = Span::new(
            Location::new(1, 65_600, 65_599 * 2),
            Location::new(1, 65_610, 65_609 * 2),
        );
        let diagnostics: Vec<_> = (0..3)
            .map(|_| {
                DiagnosticBuilder::new(DiagnosticCode::LINE_LENGTH, Severity::Info, "long", span)
                    .build(&source)
            })
            .collect();
        let output = TextFormatter::new().format(&diagnostics, &source);
        assert!(output.len() < 4_000);
    }

    fn render(source: &str, bytes_per_char: usize, start_col: usize, end_col: usize) -> String {
        let span = Span::new(
            Location::new(1, start_col, (start_col - 1) * bytes_per_char),
            Location::new(1, end_col, (end_col - 1) * bytes_per_char),
        );
        let diagnostic =
            DiagnosticBuilder::new(DiagnosticCode::LINE_LENGTH, Severity::Info, "m", span)
                .build(source);
        TextFormatter::new().format(&[diagnostic], source)
    }

    fn rows(output: &str) -> (&str, &str) {
        let source_row = output.lines().find(|l| l.starts_with("   1 |")).unwrap();
        let caret_row = output.lines().find(|l| l.contains('^')).unwrap();
        (source_row, caret_row)
    }

    fn char_pos(row: &str, needle: char) -> usize {
        row.chars().position(|c| c == needle).unwrap()
    }

    #[test]
    fn test_format_short_line_has_no_ellipsis() {
        let output = render("key: value", 1, 6, 11);
        assert!(!output.contains(ELLIPSIS));
        let (source_row, caret_row) = rows(&output);
        assert_eq!(char_pos(caret_row, '^'), char_pos(source_row, 'v'));
    }

    #[test]
    fn test_format_tail_only_ellipsis_at_line_start() {
        let output = render(&"a".repeat(1000), 1, 3, 8);
        let (source_row, caret_row) = rows(&output);
        assert!(source_row.ends_with(ELLIPSIS));
        assert_eq!(source_row.matches(ELLIPSIS).count(), 1);
        assert_eq!(char_pos(caret_row, '^'), char_pos(source_row, 'a') + 2);
    }

    #[test]
    fn test_format_head_only_ellipsis_at_line_end() {
        let output = render(&"a".repeat(1000), 1, 995, 1001);
        let (source_row, caret_row) = rows(&output);
        assert!(!source_row.ends_with(ELLIPSIS));
        assert_eq!(source_row.matches(ELLIPSIS).count(), 1);
        let head = char_pos(source_row, '\u{2026}');
        assert_eq!(char_pos(caret_row, '^') - head, 1 + (994 - 934));
    }

    #[test]
    fn test_format_both_ellipses_aligned_on_multibyte_text() {
        let output = render(&"\u{44f}".repeat(1000), 2, 500, 510);
        let (source_row, caret_row) = rows(&output);
        assert_eq!(source_row.matches(ELLIPSIS).count(), 2);
        let head = char_pos(source_row, '\u{2026}');
        assert_eq!(char_pos(caret_row, '^') - head, 1 + 60);
        assert!(caret_row.ends_with(&"^".repeat(10)));
    }

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
        assert!(output.is_empty());
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
