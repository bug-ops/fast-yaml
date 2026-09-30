//! Error types for parallel processing operations.

use std::path::PathBuf;

use fast_yaml_core::DecodeError;
use fast_yaml_core::ParseError as CoreParseError;
use fast_yaml_core::limits::{InputTooLarge, MaxDocuments};
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
    #[error("failed to read '{path}': {source}")]
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
    #[error("failed to format '{path}': {source}")]
    Format {
        /// Path to the file that failed.
        path: PathBuf,

        /// The underlying emission error.
        #[source]
        source: fast_yaml_core::EmitError,
    },

    /// A file contains no YAML document.
    #[error("empty document in '{path}'")]
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
    #[error("failed to write '{path}': {source}")]
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

    /// Input holds more documents than the configured maximum (`DoS` protection).
    #[error("input has at least {count} documents, more than the maximum of {limit}")]
    TooManyDocuments {
        /// Documents counted when the limit was hit; exact after parsing, a lower bound before.
        count: usize,

        /// The limit that was exceeded.
        limit: MaxDocuments,
    },

    /// Building the Rayon thread pool failed.
    #[error("failed to build thread pool")]
    ThreadPool(#[source] rayon::ThreadPoolBuildError),
}

/// Result type for parallel operations.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;
    use fast_yaml_core::LimitKind;
    use fast_yaml_core::limits::MaxDepth;

    #[test]
    fn test_parse_error_display_includes_cause() {
        let err = Error::Parse {
            index: 1,
            source: CoreParseError::LimitExceeded {
                kind: LimitKind::Depth(MaxDepth::DEFAULT),
                line: 1,
                column: 1,
                document: 1,
            },
        };
        let text = err.to_string();
        assert_eq!(text.matches("document 2").count(), 1, "{text}");
        assert!(text.contains("nesting depth exceeds 256"), "{text}");
    }
}
