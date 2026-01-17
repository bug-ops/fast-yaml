//! Error types for batch file processing.

use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during file discovery.
#[derive(Debug, Error)]
pub enum DiscoveryError {
    /// Invalid globset pattern (from include/exclude patterns)
    #[error("invalid glob pattern '{pattern}': {source}")]
    InvalidPattern {
        /// The pattern that was invalid
        pattern: String,
        /// The underlying error
        #[source]
        source: globset::Error,
    },

    /// Invalid glob expansion pattern
    #[error("invalid glob pattern '{pattern}': {source}")]
    InvalidGlobPattern {
        /// The pattern that was invalid
        pattern: String,
        /// The underlying error
        #[source]
        source: glob::PatternError,
    },

    /// IO error during directory traversal
    #[error("failed to read '{path}': {source}")]
    IoError {
        /// The path that caused the error
        path: PathBuf,
        /// The underlying IO error
        #[source]
        source: std::io::Error,
    },

    /// Permission denied
    #[error("permission denied: '{path}'")]
    PermissionDenied {
        /// The path where permission was denied
        path: PathBuf,
    },

    /// Broken symbolic link
    #[error("broken symbolic link: '{path}'")]
    BrokenSymlink {
        /// The path to the broken symlink
        path: PathBuf,
    },

    /// Path does not exist
    #[error("path does not exist: '{path}'")]
    PathNotFound {
        /// The path that was not found
        path: PathBuf,
    },

    /// Error reading from stdin
    #[error("failed to read file list from stdin: {source}")]
    StdinError {
        /// The underlying IO error
        #[source]
        source: std::io::Error,
    },

    /// Too many paths provided
    #[error("exceeded maximum of {max} paths")]
    TooManyPaths {
        /// The maximum allowed
        max: usize,
    },
}
