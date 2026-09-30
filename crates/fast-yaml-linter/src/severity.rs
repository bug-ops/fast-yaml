//! Diagnostic severity levels for categorizing linting issues.

use std::str::FromStr;

use crate::echo::{KEY_LIMIT, echo};

#[cfg(feature = "json-output")]
use serde::Serialize;
use serde::{Deserialize, Deserializer};

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
#[cfg_attr(feature = "json-output", derive(Serialize))]
#[cfg_attr(feature = "json-output", serde(rename_all = "lowercase"))]
#[non_exhaustive]
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

/// Error returned when a string is not a valid [`Severity`] name.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::Severity;
///
/// let err = "fatal".parse::<Severity>().unwrap_err();
/// assert_eq!(
///     err.to_string(),
///     "unknown severity 'fatal', expected one of: error, warning, info, hint"
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
#[error(
    "unknown severity '{}', expected one of: error, warning, info, hint",
    echo(.input, KEY_LIMIT)
)]
pub struct ParseSeverityError {
    /// The rejected input.
    pub input: String,
}

impl FromStr for Severity {
    type Err = ParseSeverityError;

    /// Parses a severity name, ignoring ASCII case.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::Severity;
    ///
    /// assert_eq!("Warning".parse::<Severity>(), Ok(Severity::Warning));
    /// assert!("loud".parse::<Severity>().is_err());
    /// ```
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        [Self::Error, Self::Warning, Self::Info, Self::Hint]
            .into_iter()
            .find(|severity| s.eq_ignore_ascii_case(severity.as_str()))
            .ok_or_else(|| ParseSeverityError {
                input: s.to_owned(),
            })
    }
}

struct SeverityVisitor;

impl serde::de::Visitor<'_> for SeverityVisitor {
    type Value = Severity;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a severity name (error, warning, info or hint)")
    }

    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Severity, E> {
        value.parse().map_err(E::custom)
    }
}

impl<'de> Deserialize<'de> for Severity {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(SeverityVisitor)
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

    #[test]
    fn test_severity_from_str_case_insensitive() {
        assert_eq!("ERROR".parse(), Ok(Severity::Error));
        assert_eq!("Warning".parse(), Ok(Severity::Warning));
        assert_eq!("info".parse(), Ok(Severity::Info));
        assert_eq!("hInT".parse(), Ok(Severity::Hint));
    }

    #[test]
    fn test_severity_from_str_rejects_unknown() {
        let err = "fatal".parse::<Severity>().unwrap_err();
        assert_eq!(err.input, "fatal");
        assert!("".parse::<Severity>().is_err());
    }

    #[test]
    fn test_severity_deserialize_without_json_feature() {
        let severity: Severity = serde_norway::from_str("Warning").unwrap();
        assert_eq!(severity, Severity::Warning);
        assert!(serde_norway::from_str::<Severity>("fatal").is_err());
        assert!(serde_norway::from_str::<Severity>("1").is_err());
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
