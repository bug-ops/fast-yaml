//! Diagnostic types for representing linting errors and warnings.

use crate::{Severity, SourceContext, Span};

#[cfg(feature = "json-output")]
use serde::{Deserialize, Serialize};

/// A diagnostic message with location and context.
///
/// Represents a single linting issue with severity, location,
/// message, source context, and optional suggestions for fixes.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{DiagnosticBuilder, DiagnosticCode, Severity, Location, Span};
///
/// let span = Span::new(Location::new(10, 5, 145), Location::new(10, 9, 149));
/// let diagnostic = DiagnosticBuilder::new(
///     DiagnosticCode::DUPLICATE_KEY,
///     Severity::Error,
///     "duplicate key 'name' found",
///     span
/// ).build("name: John\nage: 30\nname: Jane");
///
/// assert_eq!(diagnostic.severity, Severity::Error);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json-output", derive(Serialize, Deserialize))]
pub struct Diagnostic {
    /// Diagnostic code (e.g., "duplicate-key", "invalid-anchor").
    pub code: DiagnosticCode,
    /// Severity level.
    pub severity: Severity,
    /// Primary error message.
    pub message: String,
    /// Location span where the error occurred.
    pub span: Span,
    /// Additional context for display.
    #[cfg_attr(
        feature = "json-output",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub context: Option<DiagnosticContext>,
    /// Suggested fixes.
    #[cfg_attr(
        feature = "json-output",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub suggestions: Vec<Suggestion>,
}

/// Unique identifier for a diagnostic.
///
/// Represents the type of diagnostic issue being reported.
/// Diagnostic codes are used for filtering, configuration,
/// and programmatic handling of specific issue types.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::DiagnosticCode;
///
/// let code = DiagnosticCode::new("duplicate-key");
/// assert_eq!(code.as_str(), "duplicate-key");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "json-output", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "json-output", serde(transparent))]
pub struct DiagnosticCode(String);

impl DiagnosticCode {
    /// Predefined code for duplicate keys.
    pub const DUPLICATE_KEY: &'static str = "duplicate-key";
    /// Predefined code for invalid anchors.
    pub const INVALID_ANCHOR: &'static str = "invalid-anchor";
    /// Predefined code for undefined aliases.
    pub const UNDEFINED_ALIAS: &'static str = "undefined-alias";
    /// Predefined code for indentation issues.
    pub const INDENTATION: &'static str = "indentation";
    /// Predefined code for line length violations.
    pub const LINE_LENGTH: &'static str = "line-length";
    /// Predefined code for trailing whitespace.
    pub const TRAILING_WHITESPACE: &'static str = "trailing-whitespace";
    /// Predefined code for missing document start marker.
    pub const DOCUMENT_START: &'static str = "document-start";
    /// Predefined code for missing document end marker.
    pub const DOCUMENT_END: &'static str = "document-end";

    /// Creates a new diagnostic code.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::DiagnosticCode;
    ///
    /// let code = DiagnosticCode::new("custom-rule");
    /// assert_eq!(code.as_str(), "custom-rule");
    /// ```
    #[must_use]
    pub fn new(code: impl Into<String>) -> Self {
        Self(code.into())
    }

    /// Returns the code as a string slice.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::DiagnosticCode;
    ///
    /// let code = DiagnosticCode::new(DiagnosticCode::DUPLICATE_KEY);
    /// assert_eq!(code.as_str(), "duplicate-key");
    /// ```
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for DiagnosticCode {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for DiagnosticCode {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// Source code context for diagnostics.
///
/// Contains the source lines surrounding a diagnostic,
/// with highlighting information to show exactly where
/// the issue occurs.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{DiagnosticContext, ContextLine};
///
/// let context = DiagnosticContext {
///     lines: vec![
///         ContextLine {
///             line_number: 10,
///             content: "name: value".to_string(),
///             highlights: vec![(6, 11)],
///         },
///     ],
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json-output", derive(Serialize, Deserialize))]
pub struct DiagnosticContext {
    /// Source lines to display (typically ±2 lines around error).
    pub lines: Vec<ContextLine>,
}

/// A single line of source context.
///
/// Represents one line of source code with optional highlighting
/// to indicate the specific portion that has an issue.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json-output", derive(Serialize, Deserialize))]
pub struct ContextLine {
    /// Line number (1-indexed).
    pub line_number: usize,
    /// Source text content.
    pub content: String,
    /// Highlight ranges (column start, column end) within this line.
    pub highlights: Vec<(usize, usize)>,
}

/// A suggested fix for a diagnostic.
///
/// Represents a concrete fix that could be applied to resolve
/// the diagnostic issue. Can include replacement text or indicate
/// deletion.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{Suggestion, Location, Span};
///
/// let suggestion = Suggestion {
///     message: "Remove duplicate key".to_string(),
///     span: Span::new(Location::new(3, 1, 20), Location::new(3, 11, 30)),
///     replacement: None,
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json-output", derive(Serialize, Deserialize))]
pub struct Suggestion {
    /// Description of the fix.
    pub message: String,
    /// Span to replace.
    pub span: Span,
    /// Replacement text (None = deletion).
    #[cfg_attr(
        feature = "json-output",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub replacement: Option<String>,
}

/// Builder for creating diagnostics.
///
/// Provides an ergonomic API for constructing diagnostics
/// with optional suggestions and automatic context extraction.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{DiagnosticBuilder, DiagnosticCode, Severity, Location, Span};
///
/// let span = Span::new(Location::new(1, 1, 0), Location::new(1, 4, 3));
/// let source = "key: value";
///
/// let diagnostic = DiagnosticBuilder::new(
///     DiagnosticCode::LINE_LENGTH,
///     Severity::Info,
///     "line too long",
///     span
/// ).build(source);
///
/// assert_eq!(diagnostic.message, "line too long");
/// ```
pub struct DiagnosticBuilder {
    code: DiagnosticCode,
    severity: Severity,
    message: String,
    span: Span,
    suggestions: Vec<Suggestion>,
}

impl DiagnosticBuilder {
    /// Creates a new diagnostic builder.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{DiagnosticBuilder, DiagnosticCode, Severity, Location, Span};
    ///
    /// let builder = DiagnosticBuilder::new(
    ///     DiagnosticCode::DUPLICATE_KEY,
    ///     Severity::Error,
    ///     "duplicate key found",
    ///     Span::new(Location::new(1, 1, 0), Location::new(1, 5, 4))
    /// );
    /// ```
    #[must_use]
    pub fn new(
        code: impl Into<DiagnosticCode>,
        severity: Severity,
        message: impl Into<String>,
        span: Span,
    ) -> Self {
        Self {
            code: code.into(),
            severity,
            message: message.into(),
            span,
            suggestions: Vec::new(),
        }
    }

    /// Adds a suggestion.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{DiagnosticBuilder, DiagnosticCode, Severity, Location, Span};
    ///
    /// let span = Span::new(Location::new(1, 1, 0), Location::new(1, 5, 4));
    /// let builder = DiagnosticBuilder::new(
    ///     DiagnosticCode::TRAILING_WHITESPACE,
    ///     Severity::Hint,
    ///     "trailing whitespace",
    ///     span
    /// ).with_suggestion("Remove whitespace", span, None);
    /// ```
    #[must_use]
    pub fn with_suggestion(
        mut self,
        message: impl Into<String>,
        span: Span,
        replacement: Option<String>,
    ) -> Self {
        self.suggestions.push(Suggestion {
            message: message.into(),
            span,
            replacement,
        });
        self
    }

    /// Builds the diagnostic with source context.
    ///
    /// Extracts context lines from the source around the diagnostic span.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{DiagnosticBuilder, DiagnosticCode, Severity, Location, Span};
    ///
    /// let source = "name: John\nage: 30";
    /// let span = Span::new(Location::new(1, 1, 0), Location::new(1, 4, 3));
    ///
    /// let diagnostic = DiagnosticBuilder::new(
    ///     DiagnosticCode::LINE_LENGTH,
    ///     Severity::Info,
    ///     "example",
    ///     span
    /// ).build(source);
    ///
    /// assert!(diagnostic.context.is_some());
    /// ```
    #[must_use]
    pub fn build(self, source: &str) -> Diagnostic {
        let context = SourceContext::new(source).extract_context(self.span, 2);

        Diagnostic {
            code: self.code,
            severity: self.severity,
            message: self.message,
            span: self.span,
            context: Some(context),
            suggestions: self.suggestions,
        }
    }

    /// Builds the diagnostic without source context.
    ///
    /// Use this when source context is not available or not needed.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{DiagnosticBuilder, DiagnosticCode, Severity, Location, Span};
    ///
    /// let span = Span::new(Location::new(1, 1, 0), Location::new(1, 4, 3));
    ///
    /// let diagnostic = DiagnosticBuilder::new(
    ///     DiagnosticCode::LINE_LENGTH,
    ///     Severity::Info,
    ///     "example",
    ///     span
    /// ).build_without_context();
    ///
    /// assert!(diagnostic.context.is_none());
    /// ```
    #[must_use]
    pub fn build_without_context(self) -> Diagnostic {
        Diagnostic {
            code: self.code,
            severity: self.severity,
            message: self.message,
            span: self.span,
            context: None,
            suggestions: self.suggestions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Location;

    #[test]
    fn test_diagnostic_code_new() {
        let code = DiagnosticCode::new("test-code");
        assert_eq!(code.as_str(), "test-code");
    }

    #[test]
    fn test_diagnostic_code_from_str() {
        let code: DiagnosticCode = "test-code".into();
        assert_eq!(code.as_str(), "test-code");
    }

    #[test]
    fn test_diagnostic_code_constants() {
        assert_eq!(DiagnosticCode::DUPLICATE_KEY, "duplicate-key");
        assert_eq!(DiagnosticCode::INVALID_ANCHOR, "invalid-anchor");
        assert_eq!(DiagnosticCode::INDENTATION, "indentation");
    }

    #[test]
    fn test_diagnostic_builder() {
        let span = Span::new(Location::new(1, 1, 0), Location::new(1, 5, 4));
        let source = "name: value";

        let diagnostic = DiagnosticBuilder::new(
            DiagnosticCode::DUPLICATE_KEY,
            Severity::Error,
            "test message",
            span,
        )
        .build(source);

        assert_eq!(diagnostic.code.as_str(), DiagnosticCode::DUPLICATE_KEY);
        assert_eq!(diagnostic.severity, Severity::Error);
        assert_eq!(diagnostic.message, "test message");
        assert_eq!(diagnostic.span, span);
        assert!(diagnostic.context.is_some());
        assert!(diagnostic.suggestions.is_empty());
    }

    #[test]
    fn test_diagnostic_builder_with_suggestion() {
        let span = Span::new(Location::new(1, 1, 0), Location::new(1, 5, 4));
        let source = "name: value";

        let diagnostic = DiagnosticBuilder::new(
            DiagnosticCode::DUPLICATE_KEY,
            Severity::Error,
            "test message",
            span,
        )
        .with_suggestion("Remove duplicate", span, None)
        .build(source);

        assert_eq!(diagnostic.suggestions.len(), 1);
        assert_eq!(diagnostic.suggestions[0].message, "Remove duplicate");
    }

    #[test]
    fn test_diagnostic_builder_without_context() {
        let span = Span::new(Location::new(1, 1, 0), Location::new(1, 5, 4));

        let diagnostic =
            DiagnosticBuilder::new(DiagnosticCode::LINE_LENGTH, Severity::Info, "test", span)
                .build_without_context();

        assert!(diagnostic.context.is_none());
    }

    #[test]
    fn test_context_line() {
        let line = ContextLine {
            line_number: 10,
            content: "name: value".to_string(),
            highlights: vec![(6, 11)],
        };

        assert_eq!(line.line_number, 10);
        assert_eq!(line.content, "name: value");
        assert_eq!(line.highlights, vec![(6, 11)]);
    }

    #[test]
    fn test_suggestion() {
        let span = Span::new(Location::new(1, 1, 0), Location::new(1, 5, 4));
        let suggestion = Suggestion {
            message: "Remove this".to_string(),
            span,
            replacement: None,
        };

        assert_eq!(suggestion.message, "Remove this");
        assert_eq!(suggestion.span, span);
        assert!(suggestion.replacement.is_none());
    }

    #[cfg(feature = "json-output")]
    #[test]
    fn test_diagnostic_serialization() {
        use serde_json;

        let span = Span::new(Location::new(1, 1, 0), Location::new(1, 5, 4));
        let diagnostic =
            DiagnosticBuilder::new(DiagnosticCode::DUPLICATE_KEY, Severity::Error, "test", span)
                .build_without_context();

        let json = serde_json::to_string(&diagnostic).unwrap();
        let deserialized: Diagnostic = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.code, diagnostic.code);
        assert_eq!(deserialized.severity, diagnostic.severity);
    }
}
