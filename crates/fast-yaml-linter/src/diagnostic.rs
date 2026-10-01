//! Diagnostic types for representing linting errors and warnings.

use std::borrow::Cow;

use crate::{Severity, Span};

#[cfg(feature = "json-output")]
use serde::{Deserialize, Serialize};

/// A diagnostic message with location and context.
///
/// Represents a single linting issue with severity, location, message, an [`Excerpt`] policy
/// and optional suggestions for fixes. The excerpt lines are not stored: formatters cut them
/// from the source when they print.
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
/// ).build();
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
    pub message: Cow<'static, str>,
    /// Location span where the error occurred.
    pub span: Span,
    /// Whether formatters show source lines for this diagnostic.
    #[cfg_attr(feature = "json-output", serde(skip))]
    pub excerpt: Excerpt,
    /// Suggested fixes.
    #[cfg_attr(
        feature = "json-output",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub suggestions: Vec<Suggestion>,
}

/// Whether a diagnostic is shown with the source lines around its span.
///
/// The lines are cut from the source when a diagnostic is printed, so a run that finds a million
/// diagnostics does not hold a million copies of source text. Serialization skips the policy;
/// serialize through `JsonFormatter` (feature `json-output`) to include the lines.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{DiagnosticBuilder, DiagnosticCode, Excerpt, Location, Severity, Span};
///
/// let span = Span::new(Location::new(1, 1, 0), Location::new(1, 2, 1));
/// let builder = DiagnosticBuilder::new(DiagnosticCode::LINE_LENGTH, Severity::Info, "m", span);
/// assert_eq!(builder.build().excerpt, Excerpt::SourceLines);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Excerpt {
    /// Show the source lines around the span.
    SourceLines,
    /// Show no source lines (input and syntax errors, diagnostics built without a source).
    #[default]
    Omitted,
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
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "json-output", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "json-output", serde(transparent))]
pub struct DiagnosticCode(Cow<'static, str>);

/// Declares the predefined codes once: the constants and the lookup that lets a known code borrow
/// its constant instead of allocating.
macro_rules! predefined_codes {
    ($($(#[$meta:meta])* $name:ident = $text:literal;)+) => {
        $($(#[$meta])* pub const $name: &'static str = $text;)+

        /// The predefined constant equal to `text`.
        fn predefined(text: &str) -> Option<&'static str> {
            match text {
                $($text => Some(Self::$name),)+
                _ => None,
            }
        }
    };
}

impl DiagnosticCode {
    predefined_codes! {
        /// Predefined code for duplicate keys.
        DUPLICATE_KEY = "duplicate-key";
        /// Predefined code for invalid anchors.
        INVALID_ANCHOR = "invalid-anchor";
        /// Predefined code for undefined aliases.
        UNDEFINED_ALIAS = "undefined-alias";
        /// Predefined code for indentation issues.
        INDENTATION = "indentation";
        /// Predefined code for line length violations.
        LINE_LENGTH = "line-length";
        /// Predefined code for trailing whitespace.
        TRAILING_WHITESPACE = "trailing-whitespace";
        /// Predefined code for missing document start marker.
        DOCUMENT_START = "document-start";
        /// Predefined code for missing document end marker.
        DOCUMENT_END = "document-end";
        /// Predefined code for empty values.
        EMPTY_VALUES = "empty-values";
        /// Predefined code for missing newline at end of file.
        NEW_LINE_AT_END_OF_FILE = "new-line-at-end-of-file";
        /// Predefined code for braces formatting.
        BRACES = "braces";
        /// Predefined code for brackets formatting.
        BRACKETS = "brackets";
        /// Predefined code for colons spacing.
        COLONS = "colons";
        /// Predefined code for commas spacing.
        COMMAS = "commas";
        /// Predefined code for hyphens spacing.
        HYPHENS = "hyphens";
        /// Predefined code for comment formatting.
        COMMENTS = "comments";
        /// Predefined code for comment indentation.
        COMMENTS_INDENTATION = "comments-indentation";
        /// Predefined code for empty lines.
        EMPTY_LINES = "empty-lines";
        /// Predefined code for line endings.
        NEW_LINES = "new-lines";
        /// Predefined code for octal values.
        OCTAL_VALUES = "octal-values";
        /// Predefined code for truthy values.
        TRUTHY = "truthy";
        /// Predefined code for quoted strings.
        QUOTED_STRINGS = "quoted-strings";
        /// Predefined code for key ordering.
        KEY_ORDERING = "key-ordering";
        /// Predefined code for float values.
        FLOAT_VALUES = "float-values";
        /// Predefined code for `!!set` members that carry a value.
        SET_VALUES = "set-values";
        /// Predefined code for YAML syntax errors in CI report formats (not a rule, never configurable).
        SYNTAX = "syntax";
        /// Predefined code for problems in inline lint directives (config-only, never suppressible).
        LINT_DIRECTIVE = "lint-directive";
    }

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
        Self::from(code.into())
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
        Self(Self::predefined(s).map_or_else(|| Cow::Owned(s.to_owned()), Cow::Borrowed))
    }
}

impl From<String> for DiagnosticCode {
    fn from(s: String) -> Self {
        Self(Self::predefined(&s).map_or(Cow::Owned(s), Cow::Borrowed))
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
///             column_offset: 0,
///             truncated_end: false,
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
/// to indicate the specific portion that has an issue. Long lines are cut to a
/// window of at most 120 chars around the highlight so diagnostic size stays bounded.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::ContextLine;
///
/// let line = ContextLine {
///     line_number: 1,
///     content: "b: c".to_string(),
///     column_offset: 10,
///     truncated_end: true,
///     highlights: vec![(11, 12)],
/// };
/// assert_eq!(line.column_offset + 1, line.highlights[0].0);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json-output", derive(Serialize, Deserialize))]
pub struct ContextLine {
    /// Line number (1-indexed).
    pub line_number: usize,
    /// Windowed source text; the full line when it is short.
    pub content: String,
    /// Number of chars of the line dropped before `content`.
    pub column_offset: usize,
    /// Whether chars of the line were dropped after `content`.
    pub truncated_end: bool,
    /// Highlight ranges (1-based start column, exclusive end column) in absolute line
    /// columns, clipped to the window.
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
/// Provides an ergonomic API for constructing diagnostics with optional suggestions.
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
///     "line too long",
///     span
/// ).build();
///
/// assert_eq!(diagnostic.message, "line too long");
/// ```
pub struct DiagnosticBuilder {
    code: DiagnosticCode,
    severity: Severity,
    message: Cow<'static, str>,
    span: Span,
    suggestions: Vec<Suggestion>,
}

impl DiagnosticBuilder {
    /// Creates a new diagnostic builder.
    ///
    /// A `&'static str` message is borrowed, so a fixed message costs no allocation.
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
        message: impl Into<Cow<'static, str>>,
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

    /// Builds the diagnostic; formatters show the source lines around its span.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{DiagnosticBuilder, DiagnosticCode, Excerpt, Severity, Location, Span};
    ///
    /// let span = Span::new(Location::new(1, 1, 0), Location::new(1, 4, 3));
    ///
    /// let diagnostic = DiagnosticBuilder::new(
    ///     DiagnosticCode::LINE_LENGTH,
    ///     Severity::Info,
    ///     "example",
    ///     span
    /// ).build();
    ///
    /// assert_eq!(diagnostic.excerpt, Excerpt::SourceLines);
    /// ```
    #[must_use]
    pub fn build(self) -> Diagnostic {
        self.finish(Excerpt::SourceLines)
    }

    /// Builds the diagnostic so that formatters show no source lines for it.
    ///
    /// Use this when the span does not refer to a source that is available at print time.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{DiagnosticBuilder, DiagnosticCode, Excerpt, Severity, Location, Span};
    ///
    /// let span = Span::new(Location::new(1, 1, 0), Location::new(1, 4, 3));
    ///
    /// let diagnostic = DiagnosticBuilder::new(
    ///     DiagnosticCode::LINE_LENGTH,
    ///     Severity::Info,
    ///     "example",
    ///     span
    /// ).build_without_excerpt();
    ///
    /// assert_eq!(diagnostic.excerpt, Excerpt::Omitted);
    /// ```
    #[must_use]
    pub fn build_without_excerpt(self) -> Diagnostic {
        self.finish(Excerpt::Omitted)
    }

    fn finish(self, excerpt: Excerpt) -> Diagnostic {
        Diagnostic {
            code: self.code,
            severity: self.severity,
            message: self.message,
            span: self.span,
            excerpt,
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
        assert_eq!(DiagnosticCode::SET_VALUES, "set-values");
        assert_eq!(DiagnosticCode::SYNTAX, "syntax");
    }

    #[test]
    fn test_diagnostic_builder() {
        let span = Span::new(Location::new(1, 1, 0), Location::new(1, 5, 4));
        let diagnostic = DiagnosticBuilder::new(
            DiagnosticCode::DUPLICATE_KEY,
            Severity::Error,
            "test message",
            span,
        )
        .build();

        assert_eq!(diagnostic.code.as_str(), DiagnosticCode::DUPLICATE_KEY);
        assert_eq!(diagnostic.severity, Severity::Error);
        assert_eq!(diagnostic.message, "test message");
        assert_eq!(diagnostic.span, span);
        assert_eq!(diagnostic.excerpt, Excerpt::SourceLines);
        assert_eq!(diagnostic.suggestions, []);
    }

    #[test]
    fn test_diagnostic_builder_with_suggestion() {
        let span = Span::new(Location::new(1, 1, 0), Location::new(1, 5, 4));
        let diagnostic = DiagnosticBuilder::new(
            DiagnosticCode::DUPLICATE_KEY,
            Severity::Error,
            "test message",
            span,
        )
        .with_suggestion("Remove duplicate", span, None)
        .build();

        assert_eq!(diagnostic.suggestions.len(), 1);
        assert_eq!(diagnostic.suggestions[0].message, "Remove duplicate");
    }

    #[test]
    fn test_diagnostic_builder_without_context() {
        let span = Span::new(Location::new(1, 1, 0), Location::new(1, 5, 4));

        let diagnostic =
            DiagnosticBuilder::new(DiagnosticCode::LINE_LENGTH, Severity::Info, "test", span)
                .build_without_excerpt();

        assert_eq!(diagnostic.excerpt, Excerpt::Omitted);
    }

    #[test]
    fn test_context_line() {
        let line = ContextLine {
            line_number: 10,
            content: "name: value".to_string(),
            column_offset: 0,
            truncated_end: false,
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
                .build_without_excerpt();

        let json = serde_json::to_string(&diagnostic).unwrap();
        let deserialized: Diagnostic = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.code, diagnostic.code);
        assert_eq!(deserialized.severity, diagnostic.severity);
    }
}

#[cfg(test)]
mod code_tests {
    use super::*;

    #[test]
    fn predefined_codes_borrow_their_constant() {
        for text in [
            DiagnosticCode::DUPLICATE_KEY,
            DiagnosticCode::LINT_DIRECTIVE,
            DiagnosticCode::NEW_LINE_AT_END_OF_FILE,
        ] {
            assert!(matches!(DiagnosticCode::from(text).0, Cow::Borrowed(_)));
            assert!(matches!(
                DiagnosticCode::from(text.to_owned()).0,
                Cow::Borrowed(_)
            ));
        }
    }

    #[test]
    fn custom_codes_are_owned_and_equal_by_text() {
        let code: DiagnosticCode = DiagnosticCode::from("always-flags");
        assert!(matches!(code.0, Cow::Owned(_)));
        assert_eq!(code, DiagnosticCode::new("always-flags"));
    }

    #[test]
    fn a_static_message_is_not_copied() {
        let span = Span::new(crate::Location::new(1, 1, 0), crate::Location::new(1, 1, 0));
        let diagnostic = DiagnosticBuilder::new("r", Severity::Info, "fixed", span).build();
        assert!(matches!(diagnostic.message, Cow::Borrowed("fixed")));
    }
}
