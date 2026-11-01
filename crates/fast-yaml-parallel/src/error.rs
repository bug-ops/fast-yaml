//! Error types for parallel processing operations.

use std::path::PathBuf;
use std::sync::Arc;

use fast_yaml_core::DecodeError;
use fast_yaml_core::ParseError as CoreParseError;
use fast_yaml_core::fs::DisplayPath;
use fast_yaml_core::limits::InputTooLarge;
use thiserror::Error;

/// Unified error type for all parallel operations.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    /// Failed to parse a document; the source error names the document when it is not the first.
    #[error("failed to parse YAML: {source}")]
    Parse {
        /// Zero-based index of the document that failed.
        index: usize,

        /// The underlying parse error from fast-yaml-core.
        #[source]
        source: CoreParseError,
    },

    /// File I/O error.
    #[error("failed to read '{}': {source}", DisplayPath::new(.path))]
    Io {
        /// Path to the file that failed.
        path: PathBuf,

        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// File content is not UTF-8 text (unsupported encoding or invalid bytes).
    #[error("{source}")]
    Decode {
        /// Path to the file that failed.
        path: PathBuf,

        /// The underlying decode error from fast-yaml-core.
        #[source]
        source: DecodeError,
    },

    /// Failed to format a file.
    ///
    /// The message includes the source error because bindings surface only `Display`.
    #[error("failed to format '{}': {source}", DisplayPath::new(.path))]
    Format {
        /// Path to the file that failed.
        path: PathBuf,

        /// The underlying emission error.
        #[source]
        source: fast_yaml_core::EmitError,
    },

    /// A file contains no YAML document.
    #[error("empty document in '{}'", DisplayPath::new(.path))]
    EmptyDocument {
        /// Path to the empty file.
        path: PathBuf,
    },

    /// Formatting would silently drop YAML comments and the caller did not allow it.
    #[error("file contains YAML comments that formatting would strip")]
    CommentsWouldBeStripped,

    /// Scanning the input for comments failed.
    #[error("failed to scan for comments: {source}")]
    CommentScan {
        /// The underlying parse error from fast-yaml-core.
        #[source]
        source: CoreParseError,
    },

    /// Failed to write file.
    #[error("failed to write '{}': {source}", DisplayPath::new(.path))]
    Write {
        /// Path to the file that failed.
        path: PathBuf,

        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// Input larger than the configured maximum (`DoS` protection).
    #[error(transparent)]
    InputTooLarge(#[from] InputTooLarge),

    /// Building the Rayon thread pool failed.
    #[error("failed to build thread pool")]
    ThreadPool(#[source] Arc<rayon::ThreadPoolBuildError>),
}

/// Coarse class of an [`enum@Error`], for bindings that report a failure as a kind plus a message.
///
/// Classified by the underlying cause, so a resource limit hit while scanning for comments is
/// still [`Limit`](Self::Limit).
///
/// The enum is exhaustive on purpose: bindings map every kind to their own type, so adding one
/// is a deliberate, compiler-checked change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    /// Reading the file failed, including a path that is not a regular file.
    Io,
    /// The file is not UTF-8 text.
    Decode,
    /// The text is not valid YAML.
    Parse,
    /// A resource limit was exceeded: input size, depth, aliases, scan-ahead or documents.
    Limit,
    /// Emitting the formatted document failed.
    Format,
    /// The file holds no YAML document.
    EmptyDocument,
    /// Formatting would drop comments and the caller did not allow it.
    Comments,
    /// Writing the file failed.
    Write,
    /// The worker pool could not be built.
    ThreadPool,
}

impl Error {
    /// Returns the coarse class of this error.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_parallel::{Error, ErrorKind};
    ///
    /// assert_eq!(Error::CommentsWouldBeStripped.kind(), ErrorKind::Comments);
    /// ```
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::Parse { source, .. } => parse_kind(source),
            Self::Io { .. } => ErrorKind::Io,
            Self::Decode { .. } => ErrorKind::Decode,
            Self::Format { source, .. } => emit_kind(source),
            Self::EmptyDocument { .. } => ErrorKind::EmptyDocument,
            Self::CommentsWouldBeStripped => ErrorKind::Comments,
            Self::CommentScan { source } => match parse_kind(source) {
                ErrorKind::Limit => ErrorKind::Limit,
                _ => ErrorKind::Comments,
            },
            Self::Write { .. } => ErrorKind::Write,
            Self::InputTooLarge(_) => ErrorKind::Limit,
            Self::ThreadPool(_) => ErrorKind::ThreadPool,
        }
    }
}

const fn emit_kind(error: &fast_yaml_core::EmitError) -> ErrorKind {
    use fast_yaml_core::EmitError;
    match error {
        EmitError::Parse(source) => parse_kind(source),
        EmitError::DepthLimitExceeded { .. }
        | EmitError::AnchorLimitExceeded { .. }
        | EmitError::TagLimitExceeded { .. } => ErrorKind::Limit,
        _ => ErrorKind::Format,
    }
}

const fn parse_kind(error: &CoreParseError) -> ErrorKind {
    match error {
        CoreParseError::LimitExceeded { .. } => ErrorKind::Limit,
        _ => ErrorKind::Parse,
    }
}

/// Result type for parallel operations.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;
    use fast_yaml_core::LimitKind;
    use fast_yaml_core::limits::MaxDepth;

    #[test]
    fn kind_follows_the_underlying_cause() {
        let limit = || CoreParseError::LimitExceeded {
            kind: LimitKind::Depth(MaxDepth::DEFAULT),
            at: fast_yaml_core::SourcePosition::START,
            document: fast_yaml_core::DocumentIndex::new(0),
        };
        assert_eq!(
            Error::Parse {
                index: 0,
                source: limit()
            }
            .kind(),
            ErrorKind::Limit
        );
        assert_eq!(
            Error::CommentScan { source: limit() }.kind(),
            ErrorKind::Limit
        );
        assert_eq!(Error::CommentsWouldBeStripped.kind(), ErrorKind::Comments);
        let format = |source| Error::Format {
            path: "f".into(),
            source,
        };
        assert_eq!(
            format(fast_yaml_core::EmitError::Parse(limit())).kind(),
            ErrorKind::Limit
        );
        assert_eq!(
            format(fast_yaml_core::EmitError::DepthLimitExceeded { limit: 2 }).kind(),
            ErrorKind::Limit
        );
        assert_eq!(
            format(fast_yaml_core::EmitError::SetAsKey).kind(),
            ErrorKind::Format
        );
        let syntax = fast_yaml_core::Parser::parse_str("a: [").unwrap_err();
        assert_eq!(
            Error::Parse {
                index: 0,
                source: syntax
            }
            .kind(),
            ErrorKind::Parse
        );
    }

    #[test]
    fn test_parse_error_display_includes_cause() {
        let err = Error::Parse {
            index: 1,
            source: CoreParseError::LimitExceeded {
                kind: LimitKind::Depth(MaxDepth::DEFAULT),
                at: fast_yaml_core::SourcePosition::START,
                document: fast_yaml_core::DocumentIndex::new(1),
            },
        };
        let text = err.to_string();
        assert_eq!(text.matches("document 2").count(), 1, "{text}");
        assert!(text.contains("nesting depth exceeds 256"), "{text}");
    }
}
