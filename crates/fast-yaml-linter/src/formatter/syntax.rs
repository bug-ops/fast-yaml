//! Diagnostics for inputs that cannot be linted, so report formats never print nothing.

use std::borrow::Cow;

use fast_yaml_core::NormalizedInput;

use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintError, Location, Severity, SourceContext,
    Span,
};

/// Builds the `syntax` diagnostic for an input that failed to lint.
///
/// The span is zero-length at the position the error reports, clamped to the text, so the
/// diagnostic is valid even for errors raised at end of input. Positions follow the linter's
/// convention: line, column and byte offset all refer to the text with prefix byte order marks
/// removed (map an offset back with `NormalizedInput::original_offset`). Errors without a position (size limit) point at 1:1.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::formatter::syntax_diagnostic;
/// use fast_yaml_linter::Linter;
///
/// let source = "a: [";
/// let err = Linter::with_all_rules().lint(source).unwrap_err();
/// let diagnostic = syntax_diagnostic(&err, source);
/// assert_eq!(diagnostic.code.as_str(), "syntax");
/// assert!(diagnostic.span.start.line() >= 1);
/// ```
#[must_use]
pub fn syntax_diagnostic(err: &LintError, source: &str) -> Diagnostic {
    let (span, message) = match err {
        LintError::ParseError(parse) => {
            let position = parse.position();
            (
                located_span(source, position.line, position.column),
                parse.reason(),
            )
        }
        _ => (unlocated_span(), err.to_string()),
    };
    build(message, span)
}

/// Builds the `syntax` diagnostic for an input that could not be read or decoded.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::formatter::input_error_diagnostic;
///
/// let diagnostic = input_error_diagnostic("input is not valid UTF-8");
/// assert_eq!((diagnostic.span.start.line(), diagnostic.span.start.column()), (1, 1));
/// ```
#[must_use]
pub fn input_error_diagnostic(message: impl Into<Cow<'static, str>>) -> Diagnostic {
    build(message, unlocated_span())
}

fn build(message: impl Into<Cow<'static, str>>, span: Span) -> Diagnostic {
    DiagnosticBuilder::new(DiagnosticCode::SYNTAX, Severity::Error, message, span)
        .build_without_excerpt()
}

const fn unlocated_span() -> Span {
    Span::new(Location::start(), Location::start())
}

fn located_span(source: &str, line: usize, column: usize) -> Span {
    let normalized = NormalizedInput::new(source).ok();
    let text = normalized.as_ref().map_or(source, NormalizedInput::as_str);
    let ctx = SourceContext::new(text);
    let offset = ctx.byte_offset_at(line, column.saturating_sub(1));
    ctx.span_at(offset, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Linter;

    fn diagnostic_of(source: &str) -> Diagnostic {
        let err = Linter::with_all_rules().lint(source).unwrap_err();
        syntax_diagnostic(&err, source)
    }

    #[test]
    fn bom_is_ignored_in_line_column_and_offset() {
        let bom = diagnostic_of("\u{FEFF}a: [1\nb: 2\n]x\n");
        let plain = diagnostic_of("a: [1\nb: 2\n]x\n");
        assert_eq!(bom.code.as_str(), "syntax");
        assert_eq!(bom.span, plain.span);
    }

    #[test]
    fn error_at_eof_is_clamped() {
        for source in ["a: [", "a: [\n", "a: \"x", "- ["] {
            let d = diagnostic_of(source);
            assert!(d.span.start.offset() <= source.len(), "{source:?}");
            assert!(d.span.start.line() >= 1 && d.span.start.column() >= 1);
        }
    }

    #[test]
    fn message_does_not_repeat_the_position() {
        let d = diagnostic_of("a: [\n");
        assert!(!d.message.contains("line"), "{}", d.message);
    }

    #[test]
    fn invalid_character_is_located() {
        let d = diagnostic_of("a: 1\nb: \u{7F}\n");
        assert_eq!((d.span.start.line(), d.span.start.column()), (2, 4));
    }
}
