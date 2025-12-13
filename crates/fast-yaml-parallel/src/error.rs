//! Error types for parallel processing operations.

use fast_yaml_core::ParseError;
use thiserror::Error;

/// Errors that can occur during parallel processing.
#[derive(Error, Debug)]
pub enum ParallelError {
    /// Failed to parse a document at specific index.
    ///
    /// Preserves document index for debugging multi-document streams.
    #[error("failed to parse document at index {index}")]
    ParseError {
        /// Zero-based index of the document that failed.
        index: usize,

        /// The underlying parse error from fast-yaml-core.
        #[source]
        source: ParseError,
    },

    /// Document chunking failed (e.g., malformed separators).
    #[error("chunking failed: {0}")]
    ChunkingError(String),

    /// Thread pool initialization failed.
    #[error("thread pool error: {0}")]
    ThreadPoolError(String),

    /// Configuration error (invalid parameters).
    #[error("invalid configuration: {0}")]
    ConfigError(String),
}

/// Result type for parallel operations.
pub type Result<T> = std::result::Result<T, ParallelError>;
