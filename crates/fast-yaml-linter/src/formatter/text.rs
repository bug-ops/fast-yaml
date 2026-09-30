//! Human-readable text formatter (rustc-style).

use crate::{Diagnostic, Formatter, Severity, context::CharStarts};
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

/// Maximum number of source chars printed for a single context line.
const MAX_LINE_CHARS: usize = 160;
const ELLIPSIS: &str = "...";

/// A context line cut to a bounded window around its first highlight.
struct LineWindow<'a> {
    text: std::borrow::Cow<'a, str>,
    /// 1-indexed char columns relative to `text`, clipped to the window.
    highlights: Vec<(usize, usize)>,
}

impl<'a> LineWindow<'a> {
    /// Windows `content`, building a char table locally when it is not ASCII.
    #[cfg(test)]
    fn new(content: &'a str, highlights: &[(usize, usize)]) -> Self {
        let starts = (!content.is_ascii()).then(|| CharStarts::new(content));
        Self::build(content, highlights, starts.as_ref())
    }

    /// `table` must be `Some` exactly when `content` is not ASCII.
    fn build(content: &'a str, highlights: &[(usize, usize)], table: Option<&CharStarts>) -> Self {
        let char_len = table.map_or(content.len(), CharStarts::char_count);
        if char_len <= MAX_LINE_CHARS {
            return Self {
                text: content.into(),
                highlights: highlights.to_vec(),
            };
        }

        let anchor = highlights
            .first()
            .map_or(0, |&(start, _)| start.saturating_sub(1));
        let win_start = anchor
            .saturating_sub(MAX_LINE_CHARS / 4)
            .min(char_len - MAX_LINE_CHARS);
        let win_end = win_start + MAX_LINE_CHARS;

        let byte_at =
            |char_idx: usize| table.map_or(char_idx, |t| t.byte_of(char_idx, content.len()));
        let slice = content
            .get(byte_at(win_start)..byte_at(win_end))
            .unwrap_or_default();

        let lead = if win_start > 0 { ELLIPSIS } else { "" };
        let trail = if win_end < char_len { ELLIPSIS } else { "" };
        let shift = lead.len();

        let highlights = highlights
            .iter()
            .filter_map(|&(start, end)| {
                let start0 = start.saturating_sub(1).max(win_start);
                let end0 = end.saturating_sub(1).min(win_end);
                let fits = start0 < win_end || (start0 == win_end && win_end == char_len);
                (fits && start0 <= end0)
                    .then(|| (start0 - win_start + shift + 1, end0 - win_start + shift + 1))
            })
            .collect();

        Self {
            text: format!("{lead}{slice}{trail}").into(),
            highlights,
        }
    }
}

impl Formatter for TextFormatter {
    fn format(&self, diagnostics: &[Diagnostic], _source: &str) -> String {
        let mut output = String::new();

        let mut wide_line: Option<(&str, CharStarts)> = None;

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
                    let starts = if line.content.len() <= MAX_LINE_CHARS || line.content.is_ascii()
                    {
                        None
                    } else {
                        if wide_line
                            .as_ref()
                            .is_none_or(|(content, _)| *content != line.content)
                        {
                            wide_line = Some((&line.content, CharStarts::new(&line.content)));
                        }
                        wide_line.as_ref().map(|(_, table)| table)
                    };
                    let view = LineWindow::build(&line.content, &line.highlights, starts);
                    writeln!(
                        output,
                        "{:width$} | {}",
                        line.line_number,
                        view.text,
                        width = line_num_width
                    )
                    .unwrap();

                    if !view.highlights.is_empty() {
                        write!(output, "{:width$} | ", "", width = line_num_width).unwrap();

                        for &(start, end) in &view.highlights {
                            let padding = start.saturating_sub(1);
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
    fn test_format_highlight_column_beyond_u16() {
        let source = "a".repeat(70_000);
        let span = Span::new(
            Location::new(1, 65_600, 65_599),
            Location::new(1, 65_610, 65_609),
        );
        let diagnostic =
            DiagnosticBuilder::new(DiagnosticCode::LINE_LENGTH, Severity::Info, "long", span)
                .build(&source);

        let output = TextFormatter::new().format(&[diagnostic], &source);
        assert!(output.len() < 1_000);
        assert!(output.contains("...aaa"));
        assert!(output.contains(&"^".repeat(10)));
        assert!(!output.contains(&" ".repeat(MAX_LINE_CHARS + 10)));
    }

    fn marked(window: &LineWindow<'_>, idx: usize) -> String {
        let (start, end) = window.highlights[idx];
        window
            .text
            .chars()
            .skip(start - 1)
            .take(end - start)
            .collect()
    }

    #[test]
    fn test_line_window_caret_aligns_with_highlight() {
        let content = format!("{}XYZ{}", "a".repeat(500), "b".repeat(500));
        let window = LineWindow::new(&content, &[(501, 504)]);
        assert_eq!(marked(&window, 0), "XYZ");
        assert!(window.text.starts_with(ELLIPSIS) && window.text.ends_with(ELLIPSIS));
    }

    #[test]
    fn test_line_window_multibyte_and_short_lines() {
        let content = format!("{}Ж{}", "я".repeat(400), "ю".repeat(400));
        let window = LineWindow::new(&content, &[(401, 402)]);
        assert_eq!(marked(&window, 0), "Ж");

        let short = LineWindow::new("key: value", &[(1, 4)]);
        assert_eq!(short.text, "key: value");
        assert_eq!(short.highlights, vec![(1, 4)]);
    }

    #[test]
    fn test_line_window_length_boundary() {
        let exact = "a".repeat(MAX_LINE_CHARS);
        let window = LineWindow::new(&exact, &[(10, 12)]);
        assert_eq!(window.text, exact.as_str());
        assert_eq!(window.highlights, vec![(10, 12)]);

        let over = "a".repeat(MAX_LINE_CHARS + 1);
        let window = LineWindow::new(&over, &[(10, 12)]);
        assert!(window.text.ends_with(ELLIPSIS));
        assert!(window.text.chars().count() <= MAX_LINE_CHARS + 2 * ELLIPSIS.len());
    }

    #[test]
    fn test_line_window_highlight_at_start_and_end() {
        let content = format!("XY{}", "a".repeat(400));
        let window = LineWindow::new(&content, &[(1, 3)]);
        assert!(!window.text.starts_with(ELLIPSIS));
        assert_eq!(marked(&window, 0), "XY");

        let content = format!("{}XY", "a".repeat(400));
        let window = LineWindow::new(&content, &[(401, 403)]);
        assert!(!window.text.ends_with(ELLIPSIS));
        assert_eq!(marked(&window, 0), "XY");
    }

    #[test]
    fn test_line_window_highlight_straddling_window_end_is_clipped() {
        let content = "a".repeat(1000);
        let window = LineWindow::new(&content, &[(100, 600)]);
        let (start, end) = window.highlights[0];
        let text_len = window.text.chars().count();
        assert!(start >= 1 && end <= text_len + 1);
        assert!(end > start);
    }

    #[test]
    fn test_line_window_second_highlight_outside_window_is_dropped() {
        let content = "a".repeat(1000);
        let window = LineWindow::new(&content, &[(100, 105), (900, 905)]);
        assert_eq!(window.highlights.len(), 1);
    }

    #[test]
    fn test_line_window_zero_width_highlight_keeps_caret() {
        let content = "a".repeat(500);
        let window = LineWindow::new(&content, &[(501, 501)]);
        assert_eq!(window.highlights.len(), 1);
        let (start, end) = window.highlights[0];
        assert_eq!(start, end);
        assert_eq!(start, window.text.chars().count() + 1);

        let window = LineWindow::new(&content, &[(200, 200)]);
        assert_eq!(window.highlights.len(), 1);
    }

    #[test]
    fn test_line_window_without_highlights() {
        let content = "a".repeat(500);
        let window = LineWindow::new(&content, &[]);
        assert!(window.highlights.is_empty());
        assert!(window.text.chars().count() <= MAX_LINE_CHARS + 2 * ELLIPSIS.len());
    }

    #[test]
    fn test_line_window_ascii_and_non_ascii_agree() {
        let ascii = format!("{}XYZ{}", "a".repeat(500), "b".repeat(500));
        let wide = format!("{}XYZ{}", "я".repeat(500), "ю".repeat(500));
        let a = LineWindow::new(&ascii, &[(501, 504)]);
        let w = LineWindow::new(&wide, &[(501, 504)]);
        assert_eq!(a.highlights, w.highlights);
        assert_eq!(a.text.chars().count(), w.text.chars().count());
        assert_eq!(marked(&a, 0), marked(&w, 0));
    }

    #[test]
    fn test_format_long_non_ascii_line_is_bounded() {
        let source = "я".repeat(70_000);
        let span = Span::new(
            Location::new(1, 65_600, 65_599),
            Location::new(1, 65_610, 65_609),
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
