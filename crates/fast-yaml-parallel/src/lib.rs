//! fast-yaml-parallel: Multi-threaded YAML processing.
//!
//! Provides parallel processing for:
//! - Multi-document YAML streams (document-level parallelism)
//! - Multiple YAML files (file-level parallelism)
//!
//! # Performance
//!
//! Expected speedup on multi-document files:
//! - 4 cores: 3-3.5x faster
//! - 8 cores: 6-6.5x faster
//! - 16 cores: 10-12x faster
//!
//! # When to Use
//!
//! Use parallel processing when:
//! - Processing multi-document YAML streams (logs, configs, data dumps)
//! - Batch processing multiple YAML files
//! - Input size > 1MB with multiple documents
//! - Running on multi-core hardware (4+ cores recommended)
//!
//! Use sequential processing when:
//! - Single document files
//! - Small files (<100KB)
//! - Memory constrained environments
//!
//! # Examples
//!
//! Document-level parallelism:
//!
//! ```
//! use fast_yaml_parallel::parse_parallel;
//!
//! let yaml = "---\nfoo: 1\n---\nbar: 2\n---\nbaz: 3";
//! let docs = parse_parallel(yaml)?;
//! assert_eq!(docs.len(), 3);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! File-level parallelism:
//!
//! ```no_run
//! use fast_yaml_parallel::{FileProcessor, Config};
//! use std::path::PathBuf;
//!
//! let processor = FileProcessor::new();
//! let paths = vec![PathBuf::from("file1.yaml"), PathBuf::from("file2.yaml")];
//! let result = processor.parse_files(&paths);
//!
//! println!("Processed {} files, {} successful, {} failed",
//!          result.total, result.success, result.failed);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Custom configuration:
//!
//! ```
//! use fast_yaml_parallel::{parse_parallel_with_config, Config};
//!
//! let config = Config::new()
//!     .with_workers(Some(8))
//!     .with_sequential_threshold(2048);
//!
//! let yaml = "---\nfoo: 1\n---\nbar: 2";
//! let docs = parse_parallel_with_config(yaml, &config)?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![warn(missing_docs)]

mod atomic;
mod chunker;
mod config;
mod error;
mod processor;

// New modules
mod files;
mod io;
mod result;

// Core public API
pub use atomic::write_atomic;
pub use config::Config;
pub use error::{Error, Result};
pub use fast_yaml_core::Value;
pub use fast_yaml_core::limits::{MaxDocuments, MaxInputBytes};

// File-level parallelism
pub use files::{CommentPolicy, FileProcessor, FormatOutput};
pub use io::{FileContent, SmartReader};
pub use result::{BatchResult, FileOutcome, FileResult};

/// Parse multi-document YAML stream in parallel.
///
/// Automatically detects document boundaries and distributes
/// parsing across multiple threads. Falls back to sequential
/// parsing for single-document inputs.
///
/// # Errors
///
/// Returns `Error::Parse` if any document fails to parse.
/// The error includes the document index for debugging.
///
/// Returns `Error::InputTooLarge` if the input exceeds [`Config::max_input_bytes`] (default
/// 100 MiB) and `Error::TooManyDocuments` if it holds more than [`Config::max_documents`]
/// (default 100 000) documents; the latter is checked before any document is parsed.
///
/// # Known differences
///
/// A column-0 `---` inside a top-level literal or folded block scalar is content for both this
/// function and `saphyr` (`Parser::parse_all`) when the header has no indentation indicator
/// and the first content line starts at column 0 (`--- |\nx\n---\nb: 1\n` is one document).
/// With an indented body or an explicit indent indicator the marker ends the document; for
/// `--- |2\n---\nb: 1\n` `parse_all` fails where this function returns two documents. A
/// column-0 `...` ends the scalar in both.
///
/// An empty block scalar before `---` (`a: |\n---\nx\n`) is `""` from `parse_all`, which is
/// the spec result, and `"\n"` from this function: the chunk ends right after the header,
/// where `saphyr` yields `"\n"` (as `Parser::parse_all("a: |\n")` does).
///
/// An unclosed quoted scalar or flow collection cut by `---` is an error in both; the
/// message and position may differ.
///
/// # Examples
///
/// ```
/// use fast_yaml_parallel::parse_parallel;
///
/// let yaml = "---\nfoo: 1\n---\nbar: 2";
/// let docs = parse_parallel(yaml)?;
/// assert_eq!(docs.len(), 2);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn parse_parallel(input: &str) -> Result<Vec<Value>> {
    let config = Config::default();
    processor::process_parallel(input, &config)
}

/// Parse multi-document YAML with custom configuration.
///
/// Allows fine-tuning of parallelism parameters for specific
/// workloads and hardware configurations.
///
/// # Errors
///
/// Returns `Error` if parsing or configuration fails.
///
/// # Known differences
///
/// Same as [`parse_parallel`]: a column-0 `---` inside a top-level block scalar splits the
/// document here but is scalar content for `Parser::parse_all`, and an empty block scalar
/// before `---` is `"\n"` here (a `saphyr` EOF behavior) versus `""` there.
///
/// # Examples
///
/// ```
/// use fast_yaml_parallel::{parse_parallel_with_config, Config};
///
/// let config = Config::new()
///     .with_workers(Some(4));
///
/// let yaml = "---\nfoo: 1\n---\nbar: 2";
/// let docs = parse_parallel_with_config(yaml, &config)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn parse_parallel_with_config(input: &str, config: &Config) -> Result<Vec<Value>> {
    processor::process_parallel(input, config)
}

/// Process multiple YAML files in parallel.
///
/// Convenience function for batch file processing with default config.
///
/// # Examples
///
/// ```no_run
/// use fast_yaml_parallel::process_files;
/// use std::path::PathBuf;
///
/// let paths = vec![PathBuf::from("file1.yaml"), PathBuf::from("file2.yaml")];
/// let result = process_files(&paths);
///
/// println!("Processed {} files", result.total);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn process_files(paths: &[std::path::PathBuf]) -> BatchResult {
    FileProcessor::new().parse_files(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_parallel_basic() {
        let yaml = "---\nfoo: 1\n---\nbar: 2";
        let docs = parse_parallel(yaml).unwrap();
        assert_eq!(docs.len(), 2);
    }

    #[test]
    fn test_parse_parallel_single_document() {
        let yaml = "foo: 1\nbar: 2";
        let docs = parse_parallel(yaml).unwrap();
        assert_eq!(docs.len(), 1);
    }

    #[test]
    fn test_parse_parallel_error() {
        let yaml = "---\nvalid: true\n---\ninvalid: [";
        let result = parse_parallel(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_parallel_with_config() {
        let config = Config::new().with_workers(Some(2));
        let yaml = "---\nfoo: 1\n---\nbar: 2";
        let docs = parse_parallel_with_config(yaml, &config).unwrap();
        assert_eq!(docs.len(), 2);
    }

    #[test]
    fn test_parse_parallel_empty_input() {
        let yaml = "";
        let docs = parse_parallel(yaml).unwrap();
        assert_eq!(docs.len(), 0);
    }

    #[test]
    fn test_parse_parallel_many_documents() {
        use std::fmt::Write;
        let mut yaml = String::new();
        for i in 0..100 {
            yaml.push_str("---\n");
            let _ = writeln!(yaml, "id: {i}");
        }

        let docs = parse_parallel(&yaml).unwrap();
        assert_eq!(docs.len(), 100);
    }

    #[test]
    fn test_process_files_empty() {
        let result = process_files(&[]);
        assert_eq!(result.total, 0);
        assert!(result.is_success());
    }
}
