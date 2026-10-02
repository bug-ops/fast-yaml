use crate::keys::KeyError;
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
/// let ParseError::Merge { at, .. } = err else { panic!("merge error expected") };
/// assert_eq!((at.line, at.column), (2, 3));
/// assert_eq!(at.to_string(), "line 2, column 3");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourcePosition {
    /// Line number (1-indexed).
    pub line: usize,
    /// Column number (1-indexed, in characters).
    pub column: usize,
}

impl SourcePosition {
    /// A position at `line` and `column`, both counted from 1.
    #[must_use]
    pub const fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }

    // Char-based column, 1-indexed like saphyr's own errors; no source text to convert from.
    #[allow(clippy::disallowed_methods)]
    pub(crate) fn from_span(span: Span) -> Self {
        Self {
            line: span.start.line(),
            column: span.start.col() + 1,
        }
    }

    // Same convention as `from_span`, for the end of the span.
    #[allow(clippy::disallowed_methods)]
    pub(crate) fn end_of(span: Span) -> Self {
        Self {
            line: span.end.line(),
            column: span.end.col() + 1,
        }
    }
}

impl fmt::Display for SourcePosition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}, column {}", self.line, self.column)
    }
}

/// Index of a document in a stream, counted from 0.
///
/// A [`SourcePosition`] counts from 1, so the two are distinct types: an index cannot be passed
/// where a line or a column is expected, nor the other way round.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::{DocumentIndex, Parser};
///
/// let err = Parser::parse_all("a: 1\n---\nm: {<<: 1}\n").unwrap_err();
/// assert_eq!(err.document_index(), DocumentIndex::new(1));
/// assert_eq!(err.document_index().get(), 1);
/// assert_eq!(err.document_index().number(), 2);
/// assert_eq!(DocumentIndex::FIRST.number(), 1);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocumentIndex(usize);

impl DocumentIndex {
    /// The first document of a stream.
    pub const FIRST: Self = Self(0);

    /// The document at `index`, counted from 0.
    #[must_use]
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    /// The index, counted from 0.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }

    /// The ordinal of the document, counted from 1, as shown in messages.
    #[must_use]
    pub const fn number(self) -> usize {
        self.0 + 1
    }

    /// The index `documents` documents later.
    #[must_use]
    pub const fn after(self, documents: usize) -> Self {
        Self(self.0 + documents)
    }
}

/// Why a `!!set` member with a value is rejected.
const SET_VALUE_REASON: &str = "!!set member has a non-null value, but a set holds members only (write `key:` without a value)";

/// Renders " (document N)" for every document after the first, nothing for the first.
struct InDocument(DocumentIndex);

impl fmt::Display for InDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 == DocumentIndex::FIRST {
            Ok(())
        } else {
            write!(f, " (document {})", self.0.number())
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
    position: SourcePosition,
    document: DocumentIndex,
}

impl SyntaxError {
    const fn new(reason: SyntaxReason, position: SourcePosition, document: DocumentIndex) -> Self {
        Self {
            reason,
            position,
            document,
        }
    }

    pub(crate) const fn invalid_character(
        c: char,
        position: SourcePosition,
        document: DocumentIndex,
    ) -> Self {
        Self::new(SyntaxReason::InvalidCharacter(c), position, document)
    }

    /// Error for an alias that refers to an anchor whose node is still being defined.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{DocumentIndex, SourcePosition, SyntaxError};
    ///
    /// let err = SyntaxError::recursive_alias(SourcePosition::new(1, 5), DocumentIndex::FIRST);
    /// assert!(err.to_string().contains("still being defined"));
    /// ```
    #[must_use]
    pub const fn recursive_alias(position: SourcePosition, document: DocumentIndex) -> Self {
        Self::new(SyntaxReason::RecursiveAlias, position, document)
    }

    /// Line number of the error (1-indexed).
    #[must_use]
    pub const fn line(&self) -> usize {
        self.position.line
    }

    /// Column number of the error (1-indexed, in characters).
    #[must_use]
    pub const fn column(&self) -> usize {
        self.position.column
    }

    /// Position of the error in the source text.
    #[must_use]
    pub const fn position(&self) -> SourcePosition {
        self.position
    }

    /// Index of the document in the stream.
    #[must_use]
    pub const fn document(&self) -> DocumentIndex {
        self.document
    }
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}", self.reason, self.position)
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
    #[error("YAML resource limit exceeded at {at}: {kind}{}", InDocument(*.document))]
    LimitExceeded {
        /// Which limit was exceeded.
        kind: LimitKind,
        /// Where the offending event starts.
        at: SourcePosition,
        /// The document in the stream.
        document: DocumentIndex,
    },

    /// A `<<` merge key has a value that cannot be merged.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{DocumentIndex, MergeError, ParseError, Parser, SourcePosition};
    ///
    /// let err = Parser::parse_all("a: 1\n---\nm:\n  <<: 1\n").unwrap_err();
    /// let ParseError::Merge { error, at, document } = err else { panic!("merge error expected") };
    /// assert_eq!(error, MergeError::NotMapping);
    /// assert_eq!(at, SourcePosition::new(4, 3));
    /// assert_eq!(document, DocumentIndex::new(1));
    /// ```
    #[error("{error} at {at}{}", InDocument(*.document))]
    Merge {
        /// Why the merge value is rejected.
        error: MergeError,
        /// Where the offending `<<` key starts.
        at: SourcePosition,
        /// The document in the stream.
        document: DocumentIndex,
    },

    /// A `!!set` member has a non-null value; a set holds members only.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{DocumentIndex, ParseError, Parser, SourcePosition};
    ///
    /// let err = Parser::parse_str("!!set {a: 1}").unwrap_err();
    /// let ParseError::SetValue { at, document } = err else { panic!("set error expected") };
    /// assert_eq!(at, SourcePosition::new(1, 8));
    /// assert_eq!(document, DocumentIndex::FIRST);
    /// assert!(Parser::parse_str("!!set {a: , b: null}").is_ok());
    /// ```
    #[error("{} at {at}{}", SET_VALUE_REASON, InDocument(*.document))]
    SetValue {
        /// Where the member starts.
        at: SourcePosition,
        /// The document in the stream.
        document: DocumentIndex,
    },

    /// Two keys that YAML keeps distinct are one key in the requested [`KeyDomain`](crate::KeyDomain).
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{DocumentIndex, KeyDomain, LoadOptions, ParseError, Parser, SourcePosition};
    /// use fast_yaml_core::limits::ParseLimits;
    ///
    /// let options = LoadOptions::new().with_keys(KeyDomain::StringKeys);
    /// let err = Parser::parse_all_with_options("a: 1\n1: x\n\"1\": y\n", &ParseLimits::default(), options)
    ///     .unwrap_err();
    /// let ParseError::Key { at, document, .. } = err else { panic!("key error expected") };
    /// assert_eq!(at, SourcePosition::new(3, 1));
    /// assert_eq!(document, DocumentIndex::FIRST);
    /// ```
    #[error("{error} at {at}{}", InDocument(*.document))]
    Key {
        /// Which keys collide.
        error: KeyError,
        /// Where the later key starts.
        at: SourcePosition,
        /// The document in the stream.
        document: DocumentIndex,
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
    /// use fast_yaml_core::{DocumentIndex, ParseError, Parser, SourcePosition};
    ///
    /// let err = Parser::parse_str("a: [").unwrap_err().relocated(4, 0);
    /// assert!(err.position().line > 4);
    ///
    /// let err = Parser::parse_str("m:\n  <<: 1\n").unwrap_err().relocated(4, 2);
    /// let ParseError::Merge { at, document, .. } = err else { panic!("merge error expected") };
    /// assert_eq!(at, SourcePosition::new(6, 3));
    /// assert_eq!(document, DocumentIndex::new(2));
    /// ```
    #[must_use]
    pub fn relocated(self, lines: usize, documents: usize) -> Self {
        let line_down = |at: SourcePosition| SourcePosition::new(at.line + lines, at.column);
        match self {
            Self::Syntax(mut e) => {
                e.position = line_down(e.position);
                e.document = e.document.after(documents);
                Self::Syntax(e)
            }
            Self::LimitExceeded { kind, at, document } => Self::LimitExceeded {
                kind,
                at: line_down(at),
                document: document.after(documents),
            },
            Self::Merge {
                error,
                at,
                document,
            } => Self::Merge {
                error,
                at: line_down(at),
                document: document.after(documents),
            },
            Self::SetValue { at, document } => Self::SetValue {
                at: line_down(at),
                document: document.after(documents),
            },
            Self::Key {
                error,
                at,
                document,
            } => Self::Key {
                error,
                at: line_down(at),
                document: document.after(documents),
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
            Self::LimitExceeded { at, .. }
            | Self::Merge { at, .. }
            | Self::SetValue { at, .. }
            | Self::Key { at, .. } => *at,
        }
    }

    /// What went wrong, without the position or the document index.
    ///
    /// For callers that report the position separately.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::Parser;
    ///
    /// let err = Parser::parse_str("m:\n  <<: 1\n").unwrap_err();
    /// assert!(!err.reason().contains("line"));
    /// assert!(err.to_string().contains("line 2"));
    /// ```
    #[must_use]
    pub fn reason(&self) -> String {
        match self {
            Self::Syntax(e) => format!("YAML syntax error: {}", e.reason),
            Self::LimitExceeded { kind, .. } => {
                format!("YAML resource limit exceeded: {kind}")
            }
            Self::Merge { error, .. } => error.to_string(),
            Self::SetValue { .. } => SET_VALUE_REASON.to_owned(),
            Self::Key { error, .. } => error.to_string(),
        }
    }

    /// Index of the document the error belongs to.
    ///
    /// Every variant records the document in which it was detected. A scanner error that
    /// falls between two documents is attributed to the document that would follow.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{DocumentIndex, Parser};
    ///
    /// let err = Parser::parse_all("a: 1\n---\nm: {<<: 1}\n").unwrap_err();
    /// assert_eq!(err.document_index(), DocumentIndex::new(1));
    /// let second = Parser::parse_all("a: 1\n---\nb: [\n").unwrap_err();
    /// assert_eq!(second.document_index(), DocumentIndex::new(1));
    /// assert_eq!(Parser::parse_all("a: [").unwrap_err().document_index(), DocumentIndex::FIRST);
    /// ```
    #[must_use]
    pub const fn document_index(&self) -> DocumentIndex {
        match self {
            Self::Syntax(e) => e.document,
            Self::LimitExceeded { document, .. }
            | Self::Merge { document, .. }
            | Self::SetValue { document, .. }
            | Self::Key { document, .. } => *document,
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

/// Text of the scanner's flow nesting error; a test pins it so a parser upgrade cannot change it
/// unnoticed.
const SCANNER_FLOW_NESTING_INFO: &str = "recursion limit exceeded";

impl ParseError {
    /// Wraps a scanner error found in the document with zero-based index `document`.
    #[must_use]
    // Char-based column, 1-indexed like the scanner's own messages; no source text to convert from.
    #[allow(clippy::disallowed_methods)]
    pub(crate) fn scanner(error: &saphyr_parser::ScanError, document: DocumentIndex) -> Self {
        let marker = error.marker();
        let position = SourcePosition {
            line: marker.line(),
            column: marker.col() + 1,
        };
        if error.info() == SCANNER_FLOW_NESTING_INFO {
            return Self::LimitExceeded {
                kind: LimitKind::FlowNesting,
                at: position,
                document,
            };
        }
        Self::Syntax(SyntaxError::new(
            SyntaxReason::Scanner(error.info().into()),
            position,
            document,
        ))
    }
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
            at: SourcePosition::new(3, 7),
            document: DocumentIndex::FIRST,
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
            at: SourcePosition::new(3, 7),
            document: DocumentIndex::FIRST,
        };
        assert_eq!(limit.position(), SourcePosition { line: 3, column: 7 });
        let syntax = crate::Parser::parse_str("a: [").unwrap_err();
        assert_eq!(syntax.position().line, 2);
    }
}
