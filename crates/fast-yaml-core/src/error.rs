use crate::limits::{LimitKind, MaxTagBytes};
use crate::merge::MergeError;
use saphyr_parser::Span;
use thiserror::Error;

/// Start of a node in the source text, as reported in error messages.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::{ParseError, Parser};
///
/// let err = Parser::parse_str("m:\n  <<: 1\n").unwrap_err();
/// let ParseError::Merge { line, column, .. } = err else { panic!("merge error expected") };
/// assert_eq!((line, column), (2, 3));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourcePosition {
    /// Line number (1-indexed).
    pub line: usize,
    /// Column number (1-indexed, in characters).
    pub column: usize,
}

impl From<Span> for SourcePosition {
    // Char-based column, 1-indexed like saphyr's own errors; no source text to convert from.
    #[allow(clippy::disallowed_methods)]
    fn from(span: Span) -> Self {
        Self {
            line: span.start.line(),
            column: span.start.col() + 1,
        }
    }
}

/// Renders " (document N)" for every document after the first, nothing for the first.
struct InDocument(usize);

impl std::fmt::Display for InDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            0 => Ok(()),
            index => write!(f, " (document {})", index + 1),
        }
    }
}

/// Errors that can occur during YAML parsing.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum ParseError {
    /// YAML scanner error from saphyr.
    #[error("YAML scanner error: {error}{}", InDocument(*.document))]
    Scanner {
        /// Underlying scanner error.
        error: saphyr::ScanError,
        /// Zero-based index of the document in the stream.
        document: usize,
    },

    /// Input exceeds a configured resource limit (nesting depth or alias expansion).
    #[error("YAML resource limit exceeded at line {line}, column {column}: {kind}{}", InDocument(*.document))]
    LimitExceeded {
        /// Which limit was exceeded.
        kind: LimitKind,
        /// Line number of the offending event (1-indexed).
        line: usize,
        /// Column number of the offending event (1-indexed, in characters).
        column: usize,
        /// Zero-based index of the document in the stream.
        document: usize,
    },

    /// A `<<` merge key has a value that cannot be merged.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{MergeError, ParseError, Parser};
    ///
    /// let err = Parser::parse_all("a: 1\n---\nm:\n  <<: 1\n").unwrap_err();
    /// assert!(matches!(
    ///     err,
    ///     ParseError::Merge { error: MergeError::NotMapping, line: 4, column: 3, document: 1 }
    /// ));
    /// ```
    #[error("{error} at line {line}, column {column}{}", InDocument(*.document))]
    Merge {
        /// Why the merge value is rejected.
        error: MergeError,
        /// Line number of the offending `<<` key (1-indexed).
        line: usize,
        /// Column number of the offending `<<` key (1-indexed, in characters).
        column: usize,
        /// Zero-based index of the document in the stream.
        document: usize,
    },
}

impl ParseError {
    /// Shifts source positions by the text that precedes the parsed fragment.
    ///
    /// Lets an error from a fragment be reported in the coordinates of the whole input:
    /// `lines` line breaks, `chars` characters and `documents` documents come before the fragment,
    /// which must start at the beginning of a line so columns stay valid.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{ParseError, Parser};
    ///
    /// let err = Parser::parse_str("a: [").unwrap_err().relocated(4, 20, 0);
    /// let ParseError::Scanner { error: scan, .. } = err else { panic!("scanner error expected") };
    /// assert!(scan.marker().line() > 4);
    /// assert!(scan.marker().index() >= 20);
    ///
    /// let err = Parser::parse_str("m:\n  <<: 1\n").unwrap_err().relocated(4, 20, 2);
    /// assert!(matches!(err, ParseError::Merge { line: 6, column: 3, document: 2, .. }));
    /// ```
    // Marker fields are char-based; shifted by char counts, no source text to convert from.
    #[allow(clippy::disallowed_methods)]
    #[must_use]
    pub fn relocated(self, lines: usize, chars: usize, documents: usize) -> Self {
        match self {
            Self::Scanner { error, document } => {
                let m = error.marker();
                Self::Scanner {
                    error: saphyr::ScanError::new(
                        saphyr_parser::Marker::new(m.index() + chars, m.line() + lines, m.col()),
                        error.info().to_owned(),
                    ),
                    document: document + documents,
                }
            }
            Self::LimitExceeded {
                kind,
                line,
                column,
                document,
            } => Self::LimitExceeded {
                kind,
                line: line + lines,
                column,
                document: document + documents,
            },
            Self::Merge {
                error,
                line,
                column,
                document,
            } => Self::Merge {
                error,
                line: line + lines,
                column,
                document: document + documents,
            },
        }
    }

    /// Zero-based index of the document the error belongs to.
    ///
    /// Every variant records the document in which it was detected. A scanner error that
    /// falls between two documents is attributed to the document that would follow.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::Parser;
    ///
    /// let err = Parser::parse_all("a: 1\n---\nm: {<<: 1}\n").unwrap_err();
    /// assert_eq!(err.document_index(), 1);
    /// assert_eq!(Parser::parse_all("a: 1\n---\nb: [\n").unwrap_err().document_index(), 1);
    /// assert_eq!(Parser::parse_all("a: [").unwrap_err().document_index(), 0);
    /// ```
    #[must_use]
    pub const fn document_index(&self) -> usize {
        match self {
            Self::Scanner { document, .. }
            | Self::LimitExceeded { document, .. }
            | Self::Merge { document, .. } => *document,
        }
    }
}

/// Errors that can occur during YAML emission.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum EmitError {
    /// Writing the emitted text failed.
    #[error("failed to write YAML output")]
    Format(#[from] std::fmt::Error),

    /// The input could not be parsed before emission.
    #[error(transparent)]
    Parse(#[from] ParseError),

    /// Attempted to serialize an unsupported type.
    #[error("unsupported type for serialization: {0}")]
    UnsupportedType(String),

    /// Collection nesting exceeds the formatter depth limit.
    #[error("nesting depth exceeds the formatter limit of {limit}")]
    DepthLimitExceeded {
        /// Maximum number of nested collections the formatter supports.
        limit: usize,
    },

    /// Anchor definitions in one document exceed the formatter anchor limit.
    #[error("anchor definitions in one document exceed the formatter limit of {limit}")]
    AnchorLimitExceeded {
        /// Maximum number of anchor definitions per document the formatter supports.
        limit: usize,
    },

    /// `%TAG` prefix expansion exceeds the tag budget.
    #[error("tag prefix expansion exceeds {limit} bytes")]
    TagLimitExceeded {
        /// Maximum bytes of expanded tag prefixes per stream.
        limit: MaxTagBytes,
    },
}

pub(crate) const fn from_saphyr(err: saphyr::EmitError) -> EmitError {
    let saphyr::EmitError::FmtError(source) = err;
    EmitError::Format(source)
}

/// Result type for parsing operations.
pub type ParseResult<T> = std::result::Result<T, ParseError>;

/// Result type for emission operations.
pub type EmitResult<T> = std::result::Result<T, EmitError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_limit_exceeded_display() {
        let err = ParseError::LimitExceeded {
            kind: LimitKind::Depth(crate::limits::MaxDepth::new(8).unwrap()),
            line: 3,
            column: 7,
            document: 0,
        };
        let msg = err.to_string();
        assert!(msg.contains("limit exceeded"));
        assert!(msg.contains("line 3"));
        assert!(msg.contains("column 7"));
        assert!(msg.contains('8'));
    }

    #[test]
    fn test_emit_format_error_keeps_source() {
        use std::error::Error as _;
        let err = EmitError::from(std::fmt::Error);
        assert!(err.source().is_some());
    }

    #[test]
    fn test_emit_error_display() {
        let err = EmitError::UnsupportedType("CustomType".to_string());
        assert!(err.to_string().contains("CustomType"));
    }
}
