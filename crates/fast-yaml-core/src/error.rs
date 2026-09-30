use crate::limits::{LimitKind, MaxTagBytes};
use crate::merge::MergeError;
use saphyr_parser::Span;
use std::fmt;
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

impl fmt::Display for InDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            0 => Ok(()),
            index => write!(f, " (document {})", index + 1),
        }
    }
}

/// Why the YAML text is not well formed.
#[derive(Clone, Debug, PartialEq, Eq)]
enum SyntaxReason {
    Scanner(Box<str>),
    InvalidCharacter(char),
    RecursiveAlias,
}

impl fmt::Display for SyntaxReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Scanner(info) => f.write_str(info),
            Self::InvalidCharacter('\0') => f.write_str("NUL (U+0000) is not allowed in YAML"),
            Self::InvalidCharacter(c) => {
                write!(f, "U+{:04X} is not allowed in YAML", u32::from(*c))
            }
            Self::RecursiveAlias => {
                f.write_str("alias refers to an anchor that is still being defined")
            }
        }
    }
}

/// A well-formedness error in the YAML text, with its position.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::{ParseError, Parser};
///
/// let ParseError::Syntax(err) = Parser::parse_str("a: [").unwrap_err() else {
///     panic!("syntax error expected")
/// };
/// assert_eq!(err.line(), 2);
/// assert!(err.to_string().contains("line 2"));
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxError {
    reason: SyntaxReason,
    line: usize,
    column: usize,
    document: usize,
}

impl SyntaxError {
    const fn new(reason: SyntaxReason, position: SourcePosition, document: usize) -> Self {
        Self {
            reason,
            line: position.line,
            column: position.column,
            document,
        }
    }

    pub(crate) const fn invalid_character(
        c: char,
        position: SourcePosition,
        document: usize,
    ) -> Self {
        Self::new(SyntaxReason::InvalidCharacter(c), position, document)
    }

    /// Error for an alias that refers to an anchor whose node is still being defined.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{SourcePosition, SyntaxError};
    ///
    /// let err = SyntaxError::recursive_alias(SourcePosition { line: 1, column: 5 }, 0);
    /// assert!(err.to_string().contains("still being defined"));
    /// ```
    #[must_use]
    pub const fn recursive_alias(position: SourcePosition, document: usize) -> Self {
        Self::new(SyntaxReason::RecursiveAlias, position, document)
    }

    /// Line number of the error (1-indexed).
    #[must_use]
    pub const fn line(&self) -> usize {
        self.line
    }

    /// Column number of the error (1-indexed, in characters).
    #[must_use]
    pub const fn column(&self) -> usize {
        self.column
    }

    /// Position of the error in the source text.
    #[must_use]
    pub const fn position(&self) -> SourcePosition {
        SourcePosition {
            line: self.line,
            column: self.column,
        }
    }

    /// Zero-based index of the document in the stream.
    #[must_use]
    pub const fn document(&self) -> usize {
        self.document
    }
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at line {}, column {}",
            self.reason, self.line, self.column
        )
    }
}

impl std::error::Error for SyntaxError {}

/// Errors that can occur during YAML parsing.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum ParseError {
    /// The text is not well-formed YAML.
    #[error("YAML syntax error: {}{}", .0, InDocument(.0.document))]
    Syntax(SyntaxError),

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
    /// `lines` line breaks and `documents` documents come before the fragment, which must start
    /// at the beginning of a line so columns stay valid.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{ParseError, Parser};
    ///
    /// let err = Parser::parse_str("a: [").unwrap_err().relocated(4, 0);
    /// assert!(err.position().line > 4);
    ///
    /// let err = Parser::parse_str("m:\n  <<: 1\n").unwrap_err().relocated(4, 2);
    /// assert!(matches!(err, ParseError::Merge { line: 6, column: 3, document: 2, .. }));
    /// ```
    #[must_use]
    pub fn relocated(self, lines: usize, documents: usize) -> Self {
        match self {
            Self::Syntax(mut e) => {
                e.line += lines;
                e.document += documents;
                Self::Syntax(e)
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

    /// Source position the error points at.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::Parser;
    ///
    /// let position = Parser::parse_str("m:\n  <<: 1\n").unwrap_err().position();
    /// assert_eq!((position.line, position.column), (2, 3));
    /// ```
    #[must_use]
    pub const fn position(&self) -> SourcePosition {
        match self {
            Self::Syntax(e) => e.position(),
            Self::LimitExceeded { line, column, .. } | Self::Merge { line, column, .. } => {
                SourcePosition {
                    line: *line,
                    column: *column,
                }
            }
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
            Self::Syntax(e) => e.document,
            Self::LimitExceeded { document, .. } | Self::Merge { document, .. } => *document,
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

    /// A sequence or mapping was used as a mapping key in flow style, which the flow emitter
    /// cannot write.
    #[error("collections are not supported as mapping keys in flow style")]
    ComplexFlowKey,

    /// A `!!set` was used as a mapping key or set member, which YAML text cannot express.
    #[error("a !!set cannot be a mapping key or set member")]
    SetAsKey,

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

impl ParseError {
    /// Wraps a scanner error found in the document with zero-based index `document`.
    #[must_use]
    // Char-based column, 1-indexed like the scanner's own messages; no source text to convert from.
    #[allow(clippy::disallowed_methods)]
    pub fn scanner(error: &saphyr_parser::ScanError, document: usize) -> Self {
        let marker = error.marker();
        Self::Syntax(SyntaxError::new(
            SyntaxReason::Scanner(error.info().into()),
            SourcePosition {
                line: marker.line(),
                column: marker.col() + 1,
            },
            document,
        ))
    }
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
        assert!(EmitError::ComplexFlowKey.to_string().contains("flow style"));
    }

    #[test]
    fn syntax_error_display_has_no_byte_offset() {
        let err = crate::Parser::parse_str("a: [").unwrap_err();
        let msg = err.to_string();
        assert!(msg.starts_with("YAML syntax error: "), "{msg}");
        assert!(msg.contains("at line 2, column"), "{msg}");
        assert!(!msg.contains("byte"), "{msg}");
    }

    #[test]
    fn position_covers_every_variant() {
        let limit = ParseError::LimitExceeded {
            kind: LimitKind::Depth(crate::limits::MaxDepth::MIN),
            line: 3,
            column: 7,
            document: 0,
        };
        assert_eq!(limit.position(), SourcePosition { line: 3, column: 7 });
        let syntax = crate::Parser::parse_str("a: [").unwrap_err();
        assert_eq!(syntax.position().line, 2);
    }
}
