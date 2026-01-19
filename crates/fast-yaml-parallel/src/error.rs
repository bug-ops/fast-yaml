//! Error types for parallel processing operations.

use std::path::PathBuf;

use fast_yaml_core::ParseError as CoreParseError;
use thiserror::Error;

/// Unified error type for all parallel operations.
#[derive(Error, Debug)]
pub enum Error {
    /// Failed to parse a document at specific index.
    #[error("failed to parse document at index {index}")]
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

    /// File is not valid UTF-8.
    #[error("file is not valid UTF-8: {source}")]
    Utf8 {
        /// The underlying UTF-8 error.
        #[source]
        source: std::str::Utf8Error,
    },

    /// Failed to format YAML.
    #[error("format error: {message}")]
    Format {
        /// Error message.
        message: String,
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

    /// Input too large (`DoS` protection).
    #[error("input size {size} bytes exceeds maximum {max} bytes")]
    InputTooLarge {
        /// Actual input size.
        size: usize,

        /// Maximum allowed size.
        max: usize,
    },

    /// Document chunking failed.
    #[error("chunking failed: {0}")]
    Chunking(String),

    /// Thread pool error.
    #[error("thread pool error: {0}")]
    ThreadPool(String),

    /// Configuration error.
    #[error("configuration error: {0}")]
    Config(String),
}

/// Result type for parallel operations.
pub type Result<T> = std::result::Result<T, Error>;

impl From<std::str::Utf8Error> for Error {
    fn from(source: std::str::Utf8Error) -> Self {
        Self::Utf8 { source }
    }
}
