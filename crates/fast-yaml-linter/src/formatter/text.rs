//! Human-readable text formatter (rustc-style).

use crate::{Formatter, Severity, formatter::Findings};
use std::io::{self, Write as _};

use super::WRITE_BUFFER;

const ELLIPSIS: &str = "\u{2026}";

/// Human-readable text formatter (rustc-style).
///
/// Formats diagnostics in a style similar to the Rust compiler,
/// with color support for terminals.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::formatter::Findings;
/// use fast_yaml_linter::{TextFormatter, Formatter};
///
/// let formatter = TextFormatter::new();
/// let output = formatter.format(Findings::EMPTY);
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
    fn write(&self, out: &mut dyn io::Write, findings: Findings<'_>) -> io::Result<()> {
        // The caller's writer is a trait object, so every small write of a diagnostic would be a
        // virtual call; a local buffer keeps them inlined
        let mut out = io::BufWriter::with_capacity(WRITE_BUFFER, out);
        for finding in findings.iter() {
            let diagnostic = finding.diagnostic();
            let severity_str = self.colorize(diagnostic.severity.as_str(), diagnostic.severity);

            writeln!(
                out,
                "{}[{}]: {}",
                severity_str,
                diagnostic.code.as_str(),
                diagnostic.message
            )?;

            writeln!(
                out,
                "  --> input:{}:{}",
                diagnostic.span.start.line(),
                diagnostic.span.start.column()
            )?;

            if self.show_context
                && let Some(context) = finding.context()
            {
                writeln!(out, "   |")?;

                for line in &context.lines {
                    let line_num_width = 4;
                    let head = if line.column_offset > 0 { ELLIPSIS } else { "" };
                    let tail = if line.truncated_end { ELLIPSIS } else { "" };
                    writeln!(
                        out,
                        "{:width$} | {head}{}{tail}",
                        line.line_number,
                        line.content,
                        width = line_num_width
                    )?;

                    if !line.highlights.is_empty() {
                        write!(out, "{:width$} | ", "", width = line_num_width)?;

                        for &(start, end) in &line.highlights {
                            let padding = start
                                .saturating_sub(line.column_offset.saturating_add(1))
                                .saturating_add(head.chars().count());
                            let length = end.saturating_sub(start).max(1);

                            write!(out, "{:padding$}{:^<length$}", "", "")?;
                        }

                        writeln!(out)?;
                    }
                }

                writeln!(out, "   |")?;
            }

            for suggestion in &diagnostic.suggestions {
                writeln!(out, "   = help: {}", suggestion.message)?;
            }

            writeln!(out)?;
        }

        let count = |severity| {
            findings
                .diagnostics()
                .filter(|d| d.severity == severity && !d.is_limit_summary())
                .count()
        };
        let (error_count, warning_count) = (count(Severity::Error), count(Severity::Warning));

        if error_count > 0 || warning_count > 0 {
            writeln!(out, "{error_count} errors, {warning_count} warnings")?;
        }

        out.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Diagnostic, DiagnosticBuilder, DiagnosticCode, Location, SourceContext, Span};

    fn show(diagnostics: &[Diagnostic], source: &str) -> String {
        let context = SourceContext::new(source);
        TextFormatter::new().format(Findings::FromSource {
            diagnostics,
            source: &context,
        })
    }

    #[test]
    fn test_format_long_line_is_windowed_and_aligned() {
        let source = "a".repeat(70_000);
        let span = Span::new(
            Location::new(1, 65_600, 65_599),
            Location::new(1, 65_610, 65_609),
        );
        let diagnostic =
            DiagnosticBuilder::new(DiagnosticCode::LINE_LENGTH, Severity::Info, "long", span)
                .build();

        let output = show(&[diagnostic], &source);
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
                    .build()
            })
            .collect();
        let output = show(&diagnostics, &source);
        assert!(output.len() < 4_000);
    }

    fn render(source: &str, bytes_per_char: usize, start_col: usize, end_col: usize) -> String {
        let span = Span::new(
            Location::new(1, start_col, (start_col - 1) * bytes_per_char),
            Location::new(1, end_col, (end_col - 1) * bytes_per_char),
        );
        let diagnostic =
            DiagnosticBuilder::new(DiagnosticCode::LINE_LENGTH, Severity::Info, "m", span).build();
        show(&[diagnostic], source)
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
        let output = formatter.format(Findings::EMPTY);
        assert_eq!(output, "");
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
        .build();

        let output = show(&[diagnostic], source);

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
        .build_without_excerpt();

        let warning = DiagnosticBuilder::new(
            DiagnosticCode::INDENTATION,
            Severity::Warning,
            "warning",
            span,
        )
        .build_without_excerpt();

        let output = show(&[error, warning], source);

        assert!(output.contains("1 errors, 1 warnings"));
    }
}

#[cfg(test)]
mod streaming_tests {
    use super::*;
    use crate::{
        DiagnosticBuilder, DiagnosticCode, LintSource, Linter, Location, SourceContext, Span,
    };

    #[test]
    fn write_produces_what_format_returns() {
        let source = LintSource::new("a:   1\nb: [1,2 ,3]\n").unwrap();
        let diagnostics = Linter::with_all_rules().lint_source(&source).unwrap();
        assert_ne!(diagnostics.len(), 0);
        let context = source.context();
        let findings = Findings::FromSource {
            diagnostics: &diagnostics,
            source: &context,
        };

        let mut written = Vec::new();
        TextFormatter::new().write(&mut written, findings).unwrap();
        assert_eq!(
            String::from_utf8(written).unwrap(),
            TextFormatter::new().format(findings)
        );
    }

    #[test]
    fn given_excerpts_print_like_excerpts_cut_from_the_source() {
        let source = LintSource::new("a:   1\n").unwrap();
        let diagnostics = Linter::with_all_rules().lint_source(&source).unwrap();
        let context = source.context();
        let lazy = TextFormatter::new().format(Findings::FromSource {
            diagnostics: &diagnostics,
            source: &context,
        });
        let cut = Findings::cut(diagnostics, &context);
        assert_eq!(TextFormatter::new().format(Findings::Given(&cut)), lazy);
    }

    #[test]
    fn zero_width_span_gets_a_single_caret_at_its_column() {
        let span = Span::new(Location::new(1, 3, 2), Location::new(1, 3, 2));
        let diagnostic =
            DiagnosticBuilder::new(DiagnosticCode::EMPTY_VALUES, Severity::Warning, "m", span)
                .build();
        let context = SourceContext::new("a: \n");
        let output = TextFormatter::new().format(Findings::FromSource {
            diagnostics: &[diagnostic],
            source: &context,
        });
        let source_row = output.lines().find(|l| l.starts_with("   1 |")).unwrap();
        let caret_row = output.lines().find(|l| l.contains('^')).unwrap();
        assert_eq!(caret_row.matches('^').count(), 1);
        assert_eq!(
            caret_row.chars().position(|c| c == '^'),
            source_row.chars().position(|c| c == ':').map(|p| p + 1)
        );
    }

    #[test]
    fn omitted_excerpt_prints_no_source_lines() {
        let span = Span::new(Location::new(1, 1, 0), Location::new(1, 2, 1));
        let diagnostic =
            DiagnosticBuilder::new(DiagnosticCode::SYNTAX, Severity::Error, "bad", span)
                .build_without_excerpt();
        let context = SourceContext::new("a: [\n");
        let output = TextFormatter::new().format(Findings::FromSource {
            diagnostics: &[diagnostic],
            source: &context,
        });
        assert!(!output.contains(" | "), "{output}");
    }
}
