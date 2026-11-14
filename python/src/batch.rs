//! PyO3 bindings for batch file processing.

use std::ffi::OsString;
use std::path::PathBuf;

use crate::limits;
use fast_yaml_core::emitter::EmitterConfig;
use fast_yaml_core::limits::{AliasBytes, Depth, Documents, InputBytes, ScanAhead};
use fast_yaml_core::{Indent, ParseLimits};
use fast_yaml_parallel::{
    BatchResult as RustBatchResult, CommentPolicy, Config as RustConfig, Error, ErrorKind,
    FileProcessor, HardLinkPolicy, InPlaceTarget,
};
use pyo3::prelude::*;

/// Class of a failure, the same for every file-level error.
#[pyclass(
    module = "fast_yaml._core.batch",
    name = "FileErrorKind",
    frozen,
    eq,
    eq_int,
    hash
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PyFileErrorKind {
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

impl From<ErrorKind> for PyFileErrorKind {
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
#[pyclass(
    module = "fast_yaml._core.batch",
    name = "FileError",
    frozen,
    eq,
    get_all,
    from_py_object
)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PyFileError {
    kind: PyFileErrorKind,
    message: String,
}

#[pymethods]
impl PyFileError {
    fn __repr__(&self) -> String {
        format!(
            "FileError(kind={:?}, message={:?})",
            self.kind, self.message
        )
    }
}

impl From<&Error> for PyFileError {
    fn from(error: &Error) -> Self {
        Self {
            kind: error.kind().into(),
            message: error.to_string(),
        }
    }
}

/// Result of formatting one file: `Formatted(path, content)` or `Failed(path, error)`.
#[pyclass(module = "fast_yaml._core.batch", name = "FormatResult", frozen)]
pub enum PyFormatResult {
    /// The file was formatted
    Formatted {
        /// Path of the file
        path: OsString,
        /// Formatted content
        content: String,
    },
    /// The file could not be formatted
    Failed {
        /// Path of the file
        path: OsString,
        /// What went wrong
        error: PyFileError,
    },
}

/// Aggregated results from batch processing.
#[pyclass(module = "fast_yaml._core.batch", name = "BatchResult")]
pub struct PyBatchResult {
    #[pyo3(get)]
    total: usize,
    #[pyo3(get)]
    success: usize,
    #[pyo3(get)]
    changed: usize,
    #[pyo3(get)]
    failed: usize,
    #[pyo3(get)]
    duration_ms: f64,
    errors: Vec<(OsString, PyFileError)>,
}

#[pymethods]
impl PyBatchResult {
    const fn is_success(&self) -> bool {
        self.failed == 0
    }

    #[allow(clippy::cast_precision_loss)]
    fn files_per_second(&self) -> f64 {
        let secs = self.duration_ms / 1000.0;
        if secs > 0.0 {
            self.total as f64 / secs
        } else {
            0.0
        }
    }

    /// Returns list of (path, error) tuples
    fn errors(&self) -> Vec<(OsString, PyFileError)> {
        self.errors.clone()
    }

    fn __repr__(&self) -> String {
        format!(
            "BatchResult(total={}, success={}, changed={}, failed={}, duration_ms={:.2})",
            self.total, self.success, self.changed, self.failed, self.duration_ms
        )
    }
}

impl From<RustBatchResult> for PyBatchResult {
    fn from(result: RustBatchResult) -> Self {
        Self {
            total: result.total,
            success: result.success,
            changed: result.changed,
            failed: result.failed,
            duration_ms: result.duration.as_secs_f64() * 1000.0,
            errors: result
                .errors
                .iter()
                .map(|(path, error)| (path.clone().into_os_string(), error.into()))
                .collect(),
        }
    }
}

/// Configuration for batch file processing.
///
/// `max_depth`, `max_scan_ahead` and `max_documents` apply to `process_files` and `format_files`;
/// `max_alias_bytes` to `process_files` only. `indent` must be in 1..=9. Non-integer values
/// raise `TypeError`, out-of-range values `ValueError`.
#[pyclass(module = "fast_yaml._core.batch", name = "BatchConfig", from_py_object)]
#[derive(Clone)]
pub struct PyBatchConfig {
    inner: RustConfig,
    emitter_indent: Indent,
}

#[pymethods]
impl PyBatchConfig {
    #[new]
    #[pyo3(signature = (
        workers=None,
        max_input_bytes=None,
        indent=None,
        max_depth=None,
        max_alias_bytes=None,
        max_scan_ahead=None,
        max_documents=None
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        workers: Option<&Bound<'_, PyAny>>,
        max_input_bytes: Option<&Bound<'_, PyAny>>,
        indent: Option<&Bound<'_, PyAny>>,
        max_depth: Option<&Bound<'_, PyAny>>,
        max_alias_bytes: Option<&Bound<'_, PyAny>>,
        max_scan_ahead: Option<&Bound<'_, PyAny>>,
        max_documents: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let parse_limits =
            limits::parse_limits(max_depth, max_alias_bytes, max_scan_ahead, max_documents)?;
        let emitter_indent = limits::indent(indent)?;
        let config = RustConfig::new()
            .with_workers(limits::workers("workers", workers)?)
            .with_max_input_bytes(limits::bounded::<InputBytes>(
                "max_input_bytes",
                max_input_bytes,
            )?)
            .with_parse_limits(parse_limits);

        Ok(Self {
            inner: config,
            emitter_indent,
        })
    }

    fn with_workers(&self, workers: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        Ok(Self {
            inner: self
                .inner
                .clone()
                .with_workers(limits::workers("workers", workers)?),
            ..self.clone()
        })
    }

    /// Sets the maximum collection nesting depth for `process_files` and `format_files`;
    /// `None` resets to 256.
    ///
    /// Range: 1..=512.
    /// Depth 512 needs about 1 MiB of thread stack and can abort on stacks of 512 KiB or less.
    ///
    /// Raises:
    ///     `ValueError`: If depth is outside 1..=512
    ///     `TypeError`: If depth is not an int (`bool` included)
    fn with_max_depth(&self, depth: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let parse_limits = ParseLimits {
            max_depth: limits::bounded::<Depth>("max_depth", depth)?,
            ..self.inner.parse_limits()
        };
        Ok(Self {
            inner: self.inner.clone().with_parse_limits(parse_limits),
            ..self.clone()
        })
    }

    /// Sets the alias-expansion budget in bytes for `process_files`; `None` resets to 64 MiB.
    ///
    /// Ignored by `format_files`. Range: 1..=1 GiB.
    ///
    /// Raises:
    ///     `ValueError`: If bytes is outside 1..=1 GiB
    ///     `TypeError`: If bytes is not an int (`bool` included)
    fn with_max_alias_bytes(&self, bytes: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let parse_limits = ParseLimits {
            max_alias_bytes: limits::bounded::<AliasBytes>("max_alias_bytes", bytes)?,
            ..self.inner.parse_limits()
        };
        Ok(Self {
            inner: self.inner.clone().with_parse_limits(parse_limits),
            ..self.clone()
        })
    }

    /// Sets the characters the parser may read past the last node; `None` resets to 4 Mi.
    ///
    /// Applies to `process_files` and `format_files`. Range: 1..=1 Gi. A flow collection at the
    /// root or in a `- ` entry, one scalar, or a run of comments longer than this is rejected.
    ///
    /// Raises:
    ///     `ValueError`: If chars is outside 1..=1 Gi
    ///     `TypeError`: If chars is not an int (`bool` included)
    fn with_max_scan_ahead(&self, chars: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let parse_limits = ParseLimits {
            max_scan_ahead: limits::bounded::<ScanAhead>("max_scan_ahead", chars)?,
            ..self.inner.parse_limits()
        };
        Ok(Self {
            inner: self.inner.clone().with_parse_limits(parse_limits),
            ..self.clone()
        })
    }

    /// Sets the maximum number of documents per file; `None` resets to 100,000.
    ///
    /// Applies to `process_files` and `format_files`. Range: 1..=10M.
    ///
    /// Raises:
    ///     `ValueError`: If count is outside 1..=10M
    ///     `TypeError`: If count is not an int (`bool` included)
    fn with_max_documents(&self, count: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let parse_limits = ParseLimits {
            max_documents: limits::bounded::<Documents>("max_documents", count)?,
            ..self.inner.parse_limits()
        };
        Ok(Self {
            inner: self.inner.clone().with_parse_limits(parse_limits),
            ..self.clone()
        })
    }

    fn with_indent(&self, indent: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            emitter_indent: limits::indent(Some(indent))?,
            ..self.clone()
        })
    }

    fn __repr__(&self) -> String {
        format!(
            "BatchConfig(workers={}, indent={})",
            limits::workers_value(self.inner.workers())
                .map_or_else(|| "None".to_owned(), |n| n.to_string()),
            self.emitter_indent
        )
    }
}

impl PyBatchConfig {
    fn to_emitter_config(&self) -> EmitterConfig {
        EmitterConfig::new()
            .with_indent(self.emitter_indent)
            .with_parse_limits(self.inner.parse_limits())
    }
}

/// Process files and return batch result.
///
/// Parses and validates YAML files in parallel.
///
/// Args:
///     paths: List of file paths to process
///     config: Optional batch processing configuration
///
/// Returns:
///     BatchResult with processing statistics
///
/// Raises:
///     ValueError: If configuration is invalid
///
/// Example:
///     >>> from fast_yaml._core.batch import process_files
///     >>> result = process_files(['file1.yaml', 'file2.yaml'])
///     >>> print(f"Processed {result.total} files, {result.failed} failed")
#[pyfunction]
#[pyo3(signature = (paths, config=None))]
#[allow(
    clippy::needless_pass_by_value,
    clippy::doc_link_with_quotes,
    clippy::unnecessary_wraps
)]
fn process_files(
    py: Python<'_>,
    paths: Vec<PathBuf>,
    config: Option<PyBatchConfig>,
) -> PyResult<PyBatchResult> {
    let rust_config = config
        .as_ref()
        .map_or_else(RustConfig::default, |c| c.inner.clone());

    let result = py.detach(|| {
        let processor = FileProcessor::with_config(rust_config);
        processor.parse_files(&paths)
    });

    Ok(result.into())
}

/// Format files and return formatted content (dry-run).
///
/// Formats YAML files without writing changes back.
///
/// Args:
///     paths: List of file paths (`str` or `os.PathLike`) to format
///     config: Optional batch processing configuration
///
/// Returns:
///     One `FormatResult` per file: `Formatted(path, content)` or `Failed(path, error)`
///
/// Example:
///     >>> from fast_yaml._core.batch import format_files, FormatResult
///     >>> for result in format_files(['file1.yaml']):
///     ...     match result:
///     ...         case FormatResult.Formatted(path=path, content=content):
///     ...             print(content)
///     ...         case FormatResult.Failed(path=path, error=error):
///     ...             print(error.kind, error.message)
#[pyfunction]
#[pyo3(signature = (paths, config=None))]
#[allow(
    clippy::needless_pass_by_value,
    clippy::doc_link_with_quotes,
    clippy::unnecessary_wraps
)]
fn format_files(
    py: Python<'_>,
    paths: Vec<PathBuf>,
    config: Option<PyBatchConfig>,
) -> PyResult<Vec<PyFormatResult>> {
    let (rust_config, emitter_config) = config.as_ref().map_or_else(
        || (RustConfig::default(), EmitterConfig::new()),
        |c| (c.inner.clone(), c.to_emitter_config()),
    );

    let results = py.detach(|| {
        let processor = FileProcessor::with_config(rust_config);
        processor.format_files(&paths, &emitter_config, CommentPolicy::Strip)
    });

    Ok(results
        .into_iter()
        .map(|(path, result)| {
            let path = path.into_os_string();
            match result {
                Ok(output) => PyFormatResult::Formatted {
                    path,
                    content: output.formatted,
                },
                Err(e) => PyFormatResult::Failed {
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
/// Args:
///     paths: List of file paths to format
///     config: Optional batch processing configuration
///
/// Returns:
///     BatchResult with changed/unchanged counts
///
/// Example:
///     >>> from fast_yaml._core.batch import format_files_in_place
///     >>> result = format_files_in_place(['file1.yaml', 'file2.yaml'])
///     >>> print(f"Changed {result.changed} files")
#[pyfunction]
#[pyo3(signature = (paths, config=None))]
#[allow(
    clippy::needless_pass_by_value,
    clippy::doc_link_with_quotes,
    clippy::unnecessary_wraps
)]
fn format_files_in_place(
    py: Python<'_>,
    paths: Vec<PathBuf>,
    config: Option<PyBatchConfig>,
) -> PyResult<PyBatchResult> {
    let (rust_config, emitter_config) = config.as_ref().map_or_else(
        || (RustConfig::default(), EmitterConfig::new()),
        |c| (c.inner.clone(), c.to_emitter_config()),
    );

    let result = py.detach(|| {
        let processor = FileProcessor::with_config(rust_config);
        let targets: Vec<InPlaceTarget> = paths
            .iter()
            .map(|path| InPlaceTarget::new(path.clone(), HardLinkPolicy::WriteInPlace))
            .collect();
        processor.format_in_place(&targets, &emitter_config, CommentPolicy::Strip)
    });

    Ok(result.into())
}

/// Register the batch submodule.
pub fn register_batch_module(py: Python<'_>, parent_module: &Bound<'_, PyModule>) -> PyResult<()> {
    let batch_module = PyModule::new(py, "batch")?;

    batch_module.add_class::<PyFileErrorKind>()?;
    batch_module.add_class::<PyFileError>()?;
    batch_module.add_class::<PyFormatResult>()?;
    batch_module.add_class::<PyBatchResult>()?;
    batch_module.add_class::<PyBatchConfig>()?;
    batch_module.add_function(wrap_pyfunction!(process_files, &batch_module)?)?;
    batch_module.add_function(wrap_pyfunction!(format_files, &batch_module)?)?;
    batch_module.add_function(wrap_pyfunction!(format_files_in_place, &batch_module)?)?;

    parent_module.add_submodule(&batch_module)?;
    Ok(())
}
