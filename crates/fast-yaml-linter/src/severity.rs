//! Diagnostic severity levels for categorizing linting issues.

#[cfg(feature = "json-output")]
use serde::{Deserialize, Serialize};

/// Diagnostic severity levels.
///
/// Categorizes diagnostics by importance, from critical errors
/// to informational hints.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::Severity;
///
/// let error = Severity::Error;
/// assert_eq!(error.as_str(), "error");
/// assert!(error > Severity::Warning);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "json-output", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "json-output", serde(rename_all = "lowercase"))]
pub enum Severity {
    /// Suggestion for improvement.
    Hint,
    /// Informational message about style or best practices.
    Info,
    /// Potential issue that should be addressed.
    Warning,
    /// Critical error that prevents YAML parsing or violates spec.
    Error,
}

impl Severity {
    /// Returns the severity as a lowercase string.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::Severity;
    ///
    /// assert_eq!(Severity::Error.as_str(), "error");
    /// assert_eq!(Severity::Warning.as_str(), "warning");
    /// assert_eq!(Severity::Info.as_str(), "info");
    /// assert_eq!(Severity::Hint.as_str(), "hint");
    /// ```
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
            Self::Hint => "hint",
        }
    }

    /// Returns ANSI color code for terminal display.
    ///
    /// Returns the appropriate ANSI escape sequence for coloring
    /// diagnostic output in terminals.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::Severity;
    ///
    /// let error_color = Severity::Error.color_code();
    /// assert_eq!(error_color, "\x1b[31m"); // Red
    /// ```
    #[must_use]
    pub const fn color_code(self) -> &'static str {
        match self {
            Self::Error => "\x1b[31m",   // Red
            Self::Warning => "\x1b[33m", // Yellow
            Self::Info => "\x1b[34m",    // Blue
            Self::Hint => "\x1b[90m",    // Gray
        }
    }

    /// Returns the reset ANSI code.
    ///
    /// Use this after colored text to reset terminal colors.
    #[must_use]
    pub const fn reset_code() -> &'static str {
        "\x1b[0m"
    }

    /// Returns the symbol for this severity.
    ///
    /// Returns a visual symbol (emoji) representing the severity level,
    /// useful for terminal output.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::Severity;
    ///
    /// assert_eq!(Severity::Error.symbol(), "✗");
    /// assert_eq!(Severity::Warning.symbol(), "⚠");
    /// ```
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Error => "✗",
            Self::Warning => "⚠",
            Self::Info => "ℹ",
            Self::Hint => "💡",
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_severity_as_str() {
        assert_eq!(Severity::Error.as_str(), "error");
        assert_eq!(Severity::Warning.as_str(), "warning");
        assert_eq!(Severity::Info.as_str(), "info");
        assert_eq!(Severity::Hint.as_str(), "hint");
    }

    #[test]
    fn test_severity_display() {
        assert_eq!(format!("{}", Severity::Error), "error");
        assert_eq!(format!("{}", Severity::Warning), "warning");
        assert_eq!(format!("{}", Severity::Info), "info");
        assert_eq!(format!("{}", Severity::Hint), "hint");
    }

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Error > Severity::Warning);
        assert!(Severity::Warning > Severity::Info);
        assert!(Severity::Info > Severity::Hint);
    }

    #[test]
    fn test_severity_color_codes() {
        assert_eq!(Severity::Error.color_code(), "\x1b[31m");
        assert_eq!(Severity::Warning.color_code(), "\x1b[33m");
        assert_eq!(Severity::Info.color_code(), "\x1b[34m");
        assert_eq!(Severity::Hint.color_code(), "\x1b[90m");
        assert_eq!(Severity::reset_code(), "\x1b[0m");
    }

    #[test]
    fn test_severity_symbols() {
        assert_eq!(Severity::Error.symbol(), "✗");
        assert_eq!(Severity::Warning.symbol(), "⚠");
        assert_eq!(Severity::Info.symbol(), "ℹ");
        assert_eq!(Severity::Hint.symbol(), "💡");
    }

    #[test]
    fn test_severity_clone_copy() {
        let error = Severity::Error;
        let error_copy = error;
        assert_eq!(error, error_copy);
    }

    #[cfg(feature = "json-output")]
    #[test]
    fn test_severity_serialization() {
        use serde_json;

        let error = Severity::Error;
        let json = serde_json::to_string(&error).unwrap();
        assert_eq!(json, "\"error\"");

        let deserialized: Severity = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, Severity::Error);
    }
}
