//! NAPI-RS bindings for batch file processing.

use std::path::PathBuf;

use crate::limits::{
    max_input_bytes, parse_limits, reject_legacy_max_input_size, reject_removed_option, workers,
};
use fast_yaml_core::emitter::EmitterConfig;
use fast_yaml_parallel::{
    BatchResult as RustBatchResult, CommentPolicy, Config as RustConfig, Error, ErrorKind,
    FileProcessor,
};
use napi_derive::napi;

use crate::options::{U32_MAX, checked_opt_uint, emitter_indent};

/// Class of a failure, the same for every file-level error.
#[napi(string_enum = "camelCase")]
#[derive(Debug, Clone, Copy)]
pub enum FileErrorKind {
    /// Reading the file failed, including a path that is not a regular file
    Io,
    /// The file is not UTF-8 text
    Decode,
    /// The text is not valid YAML
    Parse,
    /// A resource limit was exceeded
    Limit,
    /// Emitting the formatted document failed
    Format,
    /// The file holds no YAML document
    EmptyDocument,
    /// Formatting would drop comments
    Comments,
    /// Writing the file failed
    Write,
    /// The worker pool could not be built
    ThreadPool,
}

impl From<ErrorKind> for FileErrorKind {
    fn from(kind: ErrorKind) -> Self {
        match kind {
            ErrorKind::Io => Self::Io,
            ErrorKind::Decode => Self::Decode,
            ErrorKind::Parse => Self::Parse,
            ErrorKind::Limit => Self::Limit,
            ErrorKind::Format => Self::Format,
            ErrorKind::EmptyDocument => Self::EmptyDocument,
            ErrorKind::Comments => Self::Comments,
            ErrorKind::Write => Self::Write,
            ErrorKind::ThreadPool => Self::ThreadPool,
        }
    }
}

/// Failure of one file: its class and the full message.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct FileError {
    /// Class of the failure
    pub kind: FileErrorKind,
    /// Human-readable message
    pub message: String,
}

impl From<&Error> for FileError {
    fn from(error: &Error) -> Self {
        Self {
            kind: error.kind().into(),
            message: error.to_string(),
        }
    }
}

/// Error entry for batch result.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct BatchError {
    /// Path to the failed file
    pub path: String,
    /// What went wrong
    pub error: FileError,
}

/// Aggregated results from batch processing.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct BatchResult {
    /// Total number of files processed
    pub total: u32,
    /// Number of files successfully processed
    pub success: u32,
    /// Number of files changed
    pub changed: u32,
    /// Number of files that failed
    pub failed: u32,
    /// Total processing duration in milliseconds
    pub duration_ms: f64,
    /// List of errors with file paths
    pub errors: Vec<BatchError>,
}

impl From<RustBatchResult> for BatchResult {
    fn from(result: RustBatchResult) -> Self {
        #[allow(clippy::cast_possible_truncation)]
        Self {
            total: result.total as u32,
            success: result.success as u32,
            changed: result.changed as u32,
            failed: result.failed as u32,
            duration_ms: result.duration.as_secs_f64() * 1000.0,
            errors: result
                .errors
                .iter()
                .map(|(p, e)| BatchError {
                    path: p.to_string_lossy().to_string(),
                    error: e.into(),
                })
                .collect(),
        }
    }
}

/// Configuration for batch file processing.
#[napi(object)]
#[derive(Debug, Clone, Default)]
pub struct BatchConfig {
    /// Worker count (omit = auto, 0 = sequential, 1..=128 = fixed pool)
    pub workers: Option<f64>,
    /// Maximum input size in bytes per file (integer, 1..=1073741824, default: 104857600)
    pub max_input_bytes: Option<f64>,
    /// Removed: renamed to `maxInputBytes`; passing it throws.
    pub max_input_size: Option<f64>,
    /// Sequential threshold (default: 4KB)
    pub sequential_threshold: Option<f64>,
    /// Indentation width in spaces (integer, 1..=9, default: 2)
    pub indent: Option<f64>,
    /// Removed: batch formatting never wrapped lines; passing it throws.
    pub width: Option<f64>,
    /// Removed: batch formatting never reorders keys; passing it throws.
    pub sort_keys: Option<bool>,
    /// Maximum collection nesting depth (integer, 1..=512, default: 256); flow collections stop at 255 levels;
    /// applies to `processFiles` and `formatFiles`.
    /// Stack note: the calling thread needs about 1 MiB of stack at depth 512 (roughly 980 KiB measured in release); on stacks of 512 KiB or less (e.g. a worker with stackSizeMb 0.5) the process can abort and the overflow cannot be caught, while the default 256 is safe. `formatFiles` rejects input nested deeper than this limit.
    pub max_depth: Option<f64>,
    /// Maximum estimated alias-expansion bytes per file (integer, 1..=1073741824,
    /// default: 67108864); applies to `processFiles` only; peak memory can reach workers x this budget
    pub max_alias_bytes: Option<f64>,
    /// Maximum characters the parser may read past the last node it reported (integer,
    /// 1..=1073741824, default: 4194304); applies to `processFiles` and `formatFiles`. A flow
    /// collection at the root or in a `- ` entry, one scalar, or a run of comments longer than
    /// this is rejected; parser memory is bounded by about 190 times this value.
    pub max_scan_ahead: Option<f64>,
    /// Maximum number of documents per file (integer, 1..=10000000, default: 100000); applies to
    /// `processFiles` and `formatFiles`.
    pub max_documents: Option<f64>,
}

impl BatchConfig {
    fn to_rust_config(&self) -> napi::Result<RustConfig> {
        let mut config = RustConfig::new().with_workers(workers("workers", self.workers)?);
        reject_legacy_max_input_size(self.max_input_size)?;
        reject_removed_option("width", self.width.is_some())?;
        reject_removed_option("sortKeys", self.sort_keys.is_some())?;
        config = config.with_max_input_bytes(max_input_bytes(self.max_input_bytes)?);
        if let Some(t) =
            checked_opt_uint("sequentialThreshold", self.sequential_threshold, 0, U32_MAX)?
        {
            config = config.with_sequential_threshold(t);
        }
        Ok(config.with_parse_limits(parse_limits(
            self.max_depth,
            self.max_alias_bytes,
            self.max_scan_ahead,
            self.max_documents,
        )?))
    }

    fn to_emitter_config(&self) -> napi::Result<EmitterConfig> {
        Ok(EmitterConfig::new()
            .with_indent(emitter_indent(self.indent)?)
            .with_parse_limits(parse_limits(
                self.max_depth,
                None,
                self.max_scan_ahead,
                self.max_documents,
            )?))
    }
}

/// Formatted file result: `{ status: 'formatted', path, content }` or
/// `{ status: 'failed', path, error }`.
#[napi(discriminant = "status", discriminant_case = "lowercase")]
#[derive(Debug, Clone)]
pub enum FormatResult {
    /// The file was formatted
    Formatted {
        /// Path to the file
        path: String,
        /// Formatted content
        content: String,
    },
    /// The file could not be formatted
    Failed {
        /// Path to the file
        path: String,
        /// What went wrong
        error: FileError,
    },
}

/// Process files and return batch result.
///
/// Parses and validates YAML files in parallel.
///
/// # Arguments
///
/// * `paths` - Array of file paths to process
/// * `config` - Optional batch processing configuration
///
/// # Returns
///
/// `BatchResult` with processing statistics
///
/// # Example
///
/// ```javascript
/// const { processFiles } = require('fastyaml-rs');
/// const result = processFiles(['file1.yaml', 'file2.yaml']);
/// console.log(`Processed ${result.total} files, ${result.failed} failed`);
/// ```
#[napi(catch_unwind)]
#[allow(clippy::needless_pass_by_value)]
pub fn process_files(paths: Vec<String>, config: Option<BatchConfig>) -> napi::Result<BatchResult> {
    let config = config.unwrap_or_default();
    let rust_config = config.to_rust_config()?;
    let path_bufs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();

    let processor = FileProcessor::with_config(rust_config);
    let result = processor.parse_files(&path_bufs);

    Ok(result.into())
}

/// Format files and return formatted content (dry-run).
///
/// Formats YAML files without writing changes back.
///
/// # Arguments
///
/// * `paths` - Array of file paths to format
/// * `config` - Optional batch processing configuration
///
/// # Returns
///
/// Array of `FormatResult` objects
///
/// # Example
///
/// ```javascript
/// const { formatFiles } = require('fastyaml-rs');
/// const results = formatFiles(['file1.yaml']);
/// results.forEach(r => {
///   if (r.status === 'formatted') console.log(r.content);
/// });
/// ```
#[napi(catch_unwind)]
#[allow(clippy::needless_pass_by_value)]
pub fn format_files(
    paths: Vec<String>,
    config: Option<BatchConfig>,
) -> napi::Result<Vec<FormatResult>> {
    let config = config.unwrap_or_default();
    let rust_config = config.to_rust_config()?;
    let emitter_config = config.to_emitter_config()?;
    let path_bufs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();

    let processor = FileProcessor::with_config(rust_config);
    let results = processor.format_files(&path_bufs, &emitter_config, CommentPolicy::Strip);

    Ok(results
        .into_iter()
        .map(|(path, result)| {
            let path = path.to_string_lossy().to_string();
            match result {
                Ok(output) => FormatResult::Formatted {
                    path,
                    content: output.formatted,
                },
                Err(e) => FormatResult::Failed {
                    path,
                    error: (&e).into(),
                },
            }
        })
        .collect())
}

/// Format files in place (write changes back).
///
/// Formats YAML files and writes changes atomically.
/// Only modified files are written.
///
/// # Arguments
///
/// * `paths` - Array of file paths to format
/// * `config` - Optional batch processing configuration
///
/// # Returns
///
/// `BatchResult` with changed/unchanged counts
///
/// # Example
///
/// ```javascript
/// const { formatFilesInPlace } = require('fastyaml-rs');
/// const result = formatFilesInPlace(['file1.yaml', 'file2.yaml']);
/// console.log(`Changed ${result.changed} files`);
/// ```
#[napi(catch_unwind)]
#[allow(clippy::needless_pass_by_value)]
pub fn format_files_in_place(
    paths: Vec<String>,
    config: Option<BatchConfig>,
) -> napi::Result<BatchResult> {
    let config = config.unwrap_or_default();
    let rust_config = config.to_rust_config()?;
    let emitter_config = config.to_emitter_config()?;
    let path_bufs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();

    let processor = FileProcessor::with_config(rust_config);
    let result = processor.format_in_place(&path_bufs, &emitter_config, CommentPolicy::Strip);

    Ok(result.into())
}
