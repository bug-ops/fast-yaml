//! NAPI-RS bindings for batch file processing.

use std::path::PathBuf;

use fast_yaml_core::emitter::EmitterConfig;
use fast_yaml_parallel::{
    BatchResult as RustBatchResult, Config as RustConfig, FileOutcome as RustFileOutcome,
    FileProcessor, FileResult as RustFileResult,
};
use napi::Result as NapiResult;
use napi_derive::napi;

/// Outcome of processing a single file.
#[napi(string_enum)]
#[derive(Debug, Clone, Copy)]
pub enum FileOutcome {
    /// File processed successfully
    Success,
    /// File formatted and content changed
    Changed,
    /// File unchanged (already formatted)
    Unchanged,
    /// Processing failed
    Error,
}

impl From<&RustFileOutcome> for FileOutcome {
    fn from(outcome: &RustFileOutcome) -> Self {
        match outcome {
            RustFileOutcome::Success { .. } => Self::Success,
            RustFileOutcome::Changed { .. } => Self::Changed,
            RustFileOutcome::Unchanged { .. } => Self::Unchanged,
            RustFileOutcome::Error { .. } => Self::Error,
        }
    }
}

/// Result for a single file with path context.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct FileResult {
    /// Path to the processed file
    pub path: String,
    /// Processing outcome
    pub outcome: FileOutcome,
    /// Processing duration in milliseconds
    pub duration_ms: f64,
    /// Error message if outcome is Error
    pub error: Option<String>,
}

impl From<RustFileResult> for FileResult {
    fn from(result: RustFileResult) -> Self {
        let error = match &result.outcome {
            RustFileOutcome::Error { error, .. } => Some(error.to_string()),
            _ => None,
        };
        Self {
            path: result.path.to_string_lossy().to_string(),
            outcome: (&result.outcome).into(),
            duration_ms: result.outcome.duration().as_secs_f64() * 1000.0,
            error,
        }
    }
}

/// Error entry for batch result.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct BatchError {
    /// Path to the failed file
    pub path: String,
    /// Error message
    pub message: String,
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
                    message: e.to_string(),
                })
                .collect(),
        }
    }
}

const MAX_WORKERS: u32 = 128;
const MAX_INPUT_SIZE: u32 = 1024 * 1024 * 1024; // 1GB

/// Configuration for batch file processing.
#[napi(object)]
#[derive(Debug, Clone, Default)]
pub struct BatchConfig {
    /// Worker count (null = auto, 0 = sequential)
    pub workers: Option<u32>,
    /// Mmap threshold for large file reading (default: 512KB)
    pub mmap_threshold: Option<u32>,
    /// Maximum input size in bytes (default: 100MB)
    pub max_input_size: Option<u32>,
    /// Sequential threshold (default: 4KB)
    pub sequential_threshold: Option<u32>,
    /// Indentation width in spaces (default: 2)
    pub indent: Option<u32>,
    /// Maximum line width (default: 80)
    pub width: Option<u32>,
    /// Sort dictionary keys alphabetically (default: false)
    pub sort_keys: Option<bool>,
}

impl BatchConfig {
    fn validate(&self) -> NapiResult<()> {
        if let Some(w) = self.workers
            && w > MAX_WORKERS
        {
            return Err(napi::Error::from_reason(format!(
                "workers {} exceeds maximum {}",
                w, MAX_WORKERS
            )));
        }
        if let Some(size) = self.max_input_size
            && size > MAX_INPUT_SIZE
        {
            return Err(napi::Error::from_reason("maxInputSize exceeds 1GB limit"));
        }
        Ok(())
    }

    fn to_rust_config(&self) -> RustConfig {
        let mut config = RustConfig::new();
        if let Some(w) = self.workers {
            config = config.with_workers(Some(w as usize));
        }
        if let Some(t) = self.mmap_threshold {
            config = config.with_mmap_threshold(t as usize);
        }
        if let Some(s) = self.max_input_size {
            config = config.with_max_input_size(s as usize);
        }
        if let Some(t) = self.sequential_threshold {
            config = config.with_sequential_threshold(t as usize);
        }
        config
    }

    fn to_emitter_config(&self) -> EmitterConfig {
        let indent = self.indent.unwrap_or(2).clamp(1, 9) as usize;
        let width = self.width.unwrap_or(80).clamp(20, 1000) as usize;
        EmitterConfig::new().with_indent(indent).with_width(width)
    }
}

/// Formatted file result.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct FormatResult {
    /// Path to the file
    pub path: String,
    /// Formatted content (null if error)
    pub content: Option<String>,
    /// Error message (null if success)
    pub error: Option<String>,
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
/// BatchResult with processing statistics
///
/// # Example
///
/// ```javascript
/// const { processFiles } = require('fastyaml-rs');
/// const result = processFiles(['file1.yaml', 'file2.yaml']);
/// console.log(`Processed ${result.total} files, ${result.failed} failed`);
/// ```
#[napi]
pub fn process_files(paths: Vec<String>, config: Option<BatchConfig>) -> NapiResult<BatchResult> {
    let config = config.unwrap_or_default();
    config.validate()?;

    let rust_config = config.to_rust_config();
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
/// Array of FormatResult objects
///
/// # Example
///
/// ```javascript
/// const { formatFiles } = require('fastyaml-rs');
/// const results = formatFiles(['file1.yaml']);
/// results.forEach(r => {
///   if (r.content) console.log(r.content);
/// });
/// ```
#[napi]
pub fn format_files(
    paths: Vec<String>,
    config: Option<BatchConfig>,
) -> NapiResult<Vec<FormatResult>> {
    let config = config.unwrap_or_default();
    config.validate()?;

    let rust_config = config.to_rust_config();
    let emitter_config = config.to_emitter_config();
    let path_bufs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();

    let processor = FileProcessor::with_config(rust_config);
    let results = processor.format_files(&path_bufs, &emitter_config);

    Ok(results
        .into_iter()
        .map(|(path, result)| {
            let path_str = path.to_string_lossy().to_string();
            match result {
                Ok(content) => FormatResult {
                    path: path_str,
                    content: Some(content),
                    error: None,
                },
                Err(e) => FormatResult {
                    path: path_str,
                    content: None,
                    error: Some(e.to_string()),
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
/// BatchResult with changed/unchanged counts
///
/// # Example
///
/// ```javascript
/// const { formatFilesInPlace } = require('fastyaml-rs');
/// const result = formatFilesInPlace(['file1.yaml', 'file2.yaml']);
/// console.log(`Changed ${result.changed} files`);
/// ```
#[napi]
pub fn format_files_in_place(
    paths: Vec<String>,
    config: Option<BatchConfig>,
) -> NapiResult<BatchResult> {
    let config = config.unwrap_or_default();
    config.validate()?;

    let rust_config = config.to_rust_config();
    let emitter_config = config.to_emitter_config();
    let path_bufs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();

    let processor = FileProcessor::with_config(rust_config);
    let result = processor.format_in_place(&path_bufs, &emitter_config);

    Ok(result.into())
}
