//! PyO3 bindings for batch file processing.

use std::path::PathBuf;

use fast_yaml_core::emitter::EmitterConfig;
use fast_yaml_parallel::{
    BatchResult as RustBatchResult, Config as RustConfig, FileOutcome as RustFileOutcome,
    FileProcessor, FileResult as RustFileResult,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

/// Outcome of processing a single file.
#[pyclass(module = "fast_yaml._core.batch", name = "FileOutcome")]
#[derive(Clone)]
pub enum PyFileOutcome {
    /// File processed successfully
    Success,
    /// File formatted and content changed
    Changed,
    /// File unchanged (already formatted)
    Unchanged,
    /// Processing failed
    Error,
}

#[pymethods]
impl PyFileOutcome {
    const fn __repr__(&self) -> &'static str {
        match self {
            Self::Success => "FileOutcome.Success",
            Self::Changed => "FileOutcome.Changed",
            Self::Unchanged => "FileOutcome.Unchanged",
            Self::Error => "FileOutcome.Error",
        }
    }

    fn __eq__(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }

    const fn __hash__(&self) -> u64 {
        match self {
            Self::Success => 0,
            Self::Changed => 1,
            Self::Unchanged => 2,
            Self::Error => 3,
        }
    }
}

/// Result for a single file with path context.
#[pyclass(module = "fast_yaml._core.batch", name = "FileResult")]
pub struct PyFileResult {
    #[pyo3(get)]
    path: String,
    #[pyo3(get)]
    outcome: PyFileOutcome,
    #[pyo3(get)]
    duration_ms: f64,
    #[pyo3(get)]
    error: Option<String>,
}

#[pymethods]
impl PyFileResult {
    const fn is_success(&self) -> bool {
        !matches!(self.outcome, PyFileOutcome::Error)
    }

    const fn was_changed(&self) -> bool {
        matches!(self.outcome, PyFileOutcome::Changed)
    }

    fn __repr__(&self) -> String {
        format!(
            "FileResult(path='{}', outcome={}, duration_ms={:.2})",
            self.path,
            self.outcome.__repr__(),
            self.duration_ms
        )
    }
}

impl From<RustFileResult> for PyFileResult {
    fn from(result: RustFileResult) -> Self {
        let (outcome, error) = match &result.outcome {
            RustFileOutcome::Success { .. } => (PyFileOutcome::Success, None),
            RustFileOutcome::Changed { .. } => (PyFileOutcome::Changed, None),
            RustFileOutcome::Unchanged { .. } => (PyFileOutcome::Unchanged, None),
            RustFileOutcome::Error { error, .. } => (PyFileOutcome::Error, Some(error.to_string())),
        };
        Self {
            path: result.path.to_string_lossy().to_string(),
            outcome,
            duration_ms: result.outcome.duration().as_secs_f64() * 1000.0,
            error,
        }
    }
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
    errors: Vec<(String, String)>,
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

    /// Returns list of (path, error_message) tuples
    fn errors(&self) -> Vec<(String, String)> {
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
                .map(|(p, e)| (p.to_string_lossy().to_string(), e.to_string()))
                .collect(),
        }
    }
}

/// Configuration for batch file processing.
#[pyclass(module = "fast_yaml._core.batch", name = "BatchConfig")]
#[derive(Clone)]
pub struct PyBatchConfig {
    inner: RustConfig,
    emitter_indent: usize,
    emitter_width: usize,
    sort_keys: bool,
}

const MAX_WORKERS: usize = 128;
const MAX_INPUT_SIZE: usize = 1024 * 1024 * 1024; // 1GB

#[pymethods]
impl PyBatchConfig {
    #[new]
    #[pyo3(signature = (
        workers=None,
        mmap_threshold=512*1024,
        max_input_size=100*1024*1024,
        sequential_threshold=4096,
        indent=2,
        width=80,
        sort_keys=false
    ))]
    fn new(
        workers: Option<usize>,
        mmap_threshold: usize,
        max_input_size: usize,
        sequential_threshold: usize,
        indent: usize,
        width: usize,
        sort_keys: bool,
    ) -> PyResult<Self> {
        if let Some(w) = workers
            && w > MAX_WORKERS
        {
            return Err(PyValueError::new_err(format!(
                "workers {w} exceeds maximum {MAX_WORKERS}"
            )));
        }
        if max_input_size > MAX_INPUT_SIZE {
            return Err(PyValueError::new_err("max_input_size exceeds 1GB limit"));
        }

        let config = RustConfig::new()
            .with_workers(workers)
            .with_mmap_threshold(mmap_threshold)
            .with_max_input_size(max_input_size)
            .with_sequential_threshold(sequential_threshold);

        Ok(Self {
            inner: config,
            emitter_indent: indent.clamp(1, 9),
            emitter_width: width.clamp(20, 1000),
            sort_keys,
        })
    }

    fn with_workers(&self, workers: Option<usize>) -> PyResult<Self> {
        if let Some(w) = workers
            && w > MAX_WORKERS
        {
            return Err(PyValueError::new_err(format!(
                "workers {w} exceeds maximum {MAX_WORKERS}"
            )));
        }
        Ok(Self {
            inner: self.inner.clone().with_workers(workers),
            ..self.clone()
        })
    }

    fn with_indent(&self, indent: usize) -> Self {
        Self {
            emitter_indent: indent.clamp(1, 9),
            ..self.clone()
        }
    }

    fn with_width(&self, width: usize) -> Self {
        Self {
            emitter_width: width.clamp(20, 1000),
            ..self.clone()
        }
    }

    fn with_sort_keys(&self, sort_keys: bool) -> Self {
        Self {
            sort_keys,
            ..self.clone()
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "BatchConfig(workers={:?}, indent={}, width={}, sort_keys={})",
            self.inner.workers(),
            self.emitter_indent,
            self.emitter_width,
            self.sort_keys
        )
    }
}

impl PyBatchConfig {
    fn to_emitter_config(&self) -> EmitterConfig {
        EmitterConfig::new()
            .with_indent(self.emitter_indent)
            .with_width(self.emitter_width)
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
#[allow(clippy::needless_pass_by_value, clippy::doc_link_with_quotes, clippy::unnecessary_wraps)]
fn process_files(
    py: Python<'_>,
    paths: Vec<String>,
    config: Option<PyBatchConfig>,
) -> PyResult<PyBatchResult> {
    let rust_config = config
        .as_ref()
        .map_or_else(RustConfig::default, |c| c.inner.clone());

    let path_bufs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();

    let result = py.detach(|| {
        let processor = FileProcessor::with_config(rust_config);
        processor.parse_files(&path_bufs)
    });

    Ok(result.into())
}

/// Format files and return formatted content (dry-run).
///
/// Formats YAML files without writing changes back.
/// Returns list of (path, formatted_content) tuples.
///
/// Args:
///     paths: List of file paths to format
///     config: Optional batch processing configuration
///
/// Returns:
///     List of (path, content) tuples where content is Result[str, str]
///
/// Example:
///     >>> from fast_yaml._core.batch import format_files
///     >>> results = format_files(['file1.yaml'])
///     >>> for path, content in results:
///     ...     if content is not None:
///     ...         print(content)
#[pyfunction]
#[pyo3(signature = (paths, config=None))]
#[allow(clippy::needless_pass_by_value, clippy::doc_link_with_quotes, clippy::unnecessary_wraps, clippy::type_complexity)]
fn format_files(
    py: Python<'_>,
    paths: Vec<String>,
    config: Option<PyBatchConfig>,
) -> PyResult<Vec<(String, Option<String>, Option<String>)>> {
    let (rust_config, emitter_config) = config.as_ref().map_or_else(
        || (RustConfig::default(), EmitterConfig::new()),
        |c| (c.inner.clone(), c.to_emitter_config()),
    );

    let path_bufs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();

    let results = py.detach(|| {
        let processor = FileProcessor::with_config(rust_config);
        processor.format_files(&path_bufs, &emitter_config)
    });

    Ok(results
        .into_iter()
        .map(|(path, result)| {
            let path_str = path.to_string_lossy().to_string();
            match result {
                Ok(content) => (path_str, Some(content), None),
                Err(e) => (path_str, None, Some(e.to_string())),
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
#[allow(clippy::needless_pass_by_value, clippy::doc_link_with_quotes, clippy::unnecessary_wraps)]
fn format_files_in_place(
    py: Python<'_>,
    paths: Vec<String>,
    config: Option<PyBatchConfig>,
) -> PyResult<PyBatchResult> {
    let (rust_config, emitter_config) = config.as_ref().map_or_else(
        || (RustConfig::default(), EmitterConfig::new()),
        |c| (c.inner.clone(), c.to_emitter_config()),
    );

    let path_bufs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();

    let result = py.detach(|| {
        let processor = FileProcessor::with_config(rust_config);
        processor.format_in_place(&path_bufs, &emitter_config)
    });

    Ok(result.into())
}

/// Register the batch submodule.
pub fn register_batch_module(py: Python<'_>, parent_module: &Bound<'_, PyModule>) -> PyResult<()> {
    let batch_module = PyModule::new(py, "batch")?;

    batch_module.add_class::<PyFileOutcome>()?;
    batch_module.add_class::<PyFileResult>()?;
    batch_module.add_class::<PyBatchResult>()?;
    batch_module.add_class::<PyBatchConfig>()?;
    batch_module.add_function(wrap_pyfunction!(process_files, &batch_module)?)?;
    batch_module.add_function(wrap_pyfunction!(format_files, &batch_module)?)?;
    batch_module.add_function(wrap_pyfunction!(format_files_in_place, &batch_module)?)?;

    parent_module.add_submodule(&batch_module)?;
    Ok(())
}
