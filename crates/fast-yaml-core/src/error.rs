use crate::limits::{LimitKind, MaxTagBytes};
use thiserror::Error;

/// Errors that can occur during YAML parsing.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum ParseError {
    /// YAML scanner error from saphyr.
    #[error("YAML scanner error: {0}")]
    Scanner(saphyr::ScanError),

    /// Input exceeds a configured resource limit (nesting depth or alias expansion).
    #[error("YAML resource limit exceeded at line {line}, column {column}: {kind}")]
    LimitExceeded {
        /// Which limit was exceeded.
        kind: LimitKind,
        /// Line number of the offending event (1-indexed).
        line: usize,
        /// Column number of the offending event (1-indexed, in characters).
        column: usize,
    },
}

impl ParseError {
    /// Shifts source positions by the text that precedes the parsed fragment.
    ///
    /// Lets an error from a fragment be reported in the coordinates of the whole input:
    /// `lines` line breaks and `chars` characters come before the fragment, which must start
    /// at the beginning of a line so columns stay valid.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{ParseError, Parser};
    ///
    /// let err = Parser::parse_str("a: [").unwrap_err().relocated(4, 20);
    /// let ParseError::Scanner(scan) = err else { panic!("scanner error expected") };
    /// assert!(scan.marker().line() > 4);
    /// assert!(scan.marker().index() >= 20);
    /// ```
    // Marker fields are char-based; shifted by char counts, no source text to convert from.
    #[allow(clippy::disallowed_methods)]
    #[must_use]
    pub fn relocated(self, lines: usize, chars: usize) -> Self {
        match self {
            Self::Scanner(e) => {
                let m = e.marker();
                Self::Scanner(saphyr::ScanError::new(
                    saphyr_parser::Marker::new(m.index() + chars, m.line() + lines, m.col()),
                    e.info().to_owned(),
                ))
            }
            Self::Syntax {
                line,
                column,
                message,
            } => Self::Syntax {
                line: line + lines,
                column,
                message,
            },
            Self::LimitExceeded { kind, line, column } => Self::LimitExceeded {
                kind,
                line: line + lines,
                column,
            },
            invalid @ Self::InvalidFloat { .. } => invalid,
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

impl From<saphyr::ScanError> for ParseError {
    fn from(err: saphyr::ScanError) -> Self {
        Self::Scanner(err)
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
            kind: LimitKind::Depth(crate::limits::MaxDepth::new(8)),
            line: 3,
            column: 7,
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
