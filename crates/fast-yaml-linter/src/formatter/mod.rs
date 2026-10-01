//! Diagnostic output formatters.

mod findings;
mod github;
mod parsable;
mod report;
mod syntax;
mod text;

#[cfg(feature = "json-output")]
mod json;

#[cfg(feature = "sarif-output")]
mod sarif;

use std::io;

/// Size of the buffer a formatter puts between its many small writes and the caller's writer.
const WRITE_BUFFER: usize = 64 * 1024;

pub use findings::{Finding, Findings};
pub use report::{FileReport, NotAbsolute, ReportFormat, ReportPath, ReportSource};
pub use syntax::{input_error_diagnostic, syntax_diagnostic};
pub use text::TextFormatter;

#[cfg(feature = "json-output")]
pub use json::{JsonDiagnostic, JsonFormatter};

/// Trait for formatting diagnostics.
///
/// Implementations print [`Findings`] in a specific output format. Printing streams to a
/// writer, so the output of a run with many diagnostics is never held in memory whole.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::formatter::Findings;
/// use fast_yaml_linter::{Formatter, TextFormatter};
///
/// let formatter = TextFormatter::new();
/// let output = formatter.format(Findings::EMPTY);
/// assert!(output.is_empty());
/// ```
pub trait Formatter {
    /// Writes `findings` to `out`.
    ///
    /// # Errors
    ///
    /// Returns the error of `out`.
    fn write(&self, out: &mut dyn io::Write, findings: Findings<'_>) -> io::Result<()>;

    /// Formats `findings` to a string.
    fn format(&self, findings: Findings<'_>) -> String {
        let mut buffer = Vec::new();
        // A `Vec` never fails to take bytes and formatters write only UTF-8 text
        let _ = self.write(&mut buffer, findings);
        String::from_utf8(buffer).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into())
    }
}
