//! Diagnostic output formatters.

mod text;

#[cfg(feature = "json-output")]
mod json;

#[cfg(feature = "sarif-output")]
mod sarif;

pub use text::TextFormatter;

#[cfg(feature = "json-output")]
pub use json::JsonFormatter;

#[cfg(feature = "sarif-output")]
pub use sarif::SarifFormatter;

use crate::Diagnostic;

/// Trait for formatting diagnostics.
///
/// Implementations convert a list of diagnostics into a specific output format.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{Formatter, TextFormatter, Diagnostic};
///
/// let formatter = TextFormatter::new();
/// let diagnostics: Vec<Diagnostic> = Vec::new();
/// let output = formatter.format(&diagnostics, "source code");
/// ```
pub trait Formatter {
    /// Formats diagnostics to a string.
    ///
    /// # Parameters
    ///
    /// - `diagnostics`: The diagnostics to format
    /// - `source`: The original source code (for context display)
    ///
    /// # Returns
    ///
    /// A formatted string representation of the diagnostics.
    fn format(&self, diagnostics: &[Diagnostic], source: &str) -> String;
}
