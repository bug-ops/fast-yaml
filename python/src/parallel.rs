//! `PyO3` bindings for fast-yaml-parallel.
//!
//! Exposes multi-threaded YAML parsing for large multi-document files.

use crate::conversion::value_to_python;
use fast_yaml_parallel::{
    ParallelConfig as RustParallelConfig, ParallelError, parse_parallel as rust_parse_parallel,
    parse_parallel_with_config,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyList;

/// Maximum thread count allowed (capped by Rust implementation).
const MAX_THREADS: usize = 128;

/// Maximum input size in bytes (default 100MB, can be configured up to 1GB).
const ABSOLUTE_MAX_INPUT_SIZE: usize = 1024 * 1024 * 1024; // 1GB

/// Maximum document count (default 100k, can be configured up to 10M).
const ABSOLUTE_MAX_DOCUMENTS: usize = 10_000_000;

/// Configuration for parallel YAML processing.
///
/// Controls thread pool size, chunking thresholds, and resource limits.
///
/// Examples:
///     >>> from `fast_yaml`._core.parallel import `ParallelConfig`
///     >>> config = `ParallelConfig(thread_count=8`, `max_input_size=200`*1024*1024)
#[pyclass(module = "fast_yaml._core.parallel", name = "ParallelConfig")]
#[derive(Clone)]
pub struct PyParallelConfig {
    inner: RustParallelConfig,
}

#[pymethods]
impl PyParallelConfig {
    #[new]
    #[pyo3(signature = (
        thread_count=None,
        min_chunk_size=4096,
        max_chunk_size=10*1024*1024,
        max_input_size=100*1024*1024,
        max_documents=100_000
    ))]
    fn new(
        thread_count: Option<usize>,
        min_chunk_size: usize,
        max_chunk_size: usize,
        max_input_size: usize,
        max_documents: usize,
    ) -> PyResult<Self> {
        // Validate thread_count (if specified, must be <= 128)
        if let Some(count) = thread_count
            && count > MAX_THREADS
        {
            return Err(PyValueError::new_err(format!(
                "thread_count {count} exceeds maximum allowed {MAX_THREADS}"
            )));
        }

        // Validate chunk sizes
        if min_chunk_size == 0 {
            return Err(PyValueError::new_err(
                "min_chunk_size must be greater than 0",
            ));
        }
        if max_chunk_size == 0 || max_chunk_size < min_chunk_size {
            return Err(PyValueError::new_err(
                "max_chunk_size must be greater than 0 and >= min_chunk_size",
            ));
        }

        // Validate input size limit (max 1GB to prevent OOM)
        if max_input_size == 0 || max_input_size > ABSOLUTE_MAX_INPUT_SIZE {
            return Err(PyValueError::new_err(format!(
                "max_input_size must be between 1 and {ABSOLUTE_MAX_INPUT_SIZE} (1GB)"
            )));
        }

        // Validate document count limit (max 10M to prevent resource exhaustion)
        if max_documents == 0 || max_documents > ABSOLUTE_MAX_DOCUMENTS {
            return Err(PyValueError::new_err(format!(
                "max_documents must be between 1 and {ABSOLUTE_MAX_DOCUMENTS} (10M)"
            )));
        }

        let config = RustParallelConfig::new()
            .with_thread_count(thread_count)
            .with_min_chunk_size(min_chunk_size)
            .with_max_chunk_size(max_chunk_size)
            .with_max_input_size(max_input_size)
            .with_max_documents(max_documents);

        Ok(Self { inner: config })
    }

    /// Sets thread pool size.
    ///
    /// - None: Use all available CPU cores (default, capped at 128)
    /// - Some(0): Sequential processing (no parallelism)
    /// - Some(n): Use exactly n threads (max 128)
    ///
    /// Raises:
    ///     `ValueError`: If thread count exceeds 128
    fn with_thread_count(&self, count: Option<usize>) -> PyResult<Self> {
        if let Some(c) = count
            && c > MAX_THREADS
        {
            return Err(PyValueError::new_err(format!(
                "thread_count {c} exceeds maximum allowed {MAX_THREADS}"
            )));
        }
        Ok(Self {
            inner: self.inner.clone().with_thread_count(count),
        })
    }

    /// Sets maximum total input size in bytes.
    ///
    /// Default: 100MB (max: 1GB)
    ///
    /// Raises:
    ///     `ValueError`: If size is 0 or exceeds 1GB
    fn with_max_input_size(&self, size: usize) -> PyResult<Self> {
        if size == 0 || size > ABSOLUTE_MAX_INPUT_SIZE {
            return Err(PyValueError::new_err(format!(
                "max_input_size must be between 1 and {ABSOLUTE_MAX_INPUT_SIZE} (1GB)"
            )));
        }
        Ok(Self {
            inner: self.inner.clone().with_max_input_size(size),
        })
    }

    /// Sets maximum number of documents allowed.
    ///
    /// Default: 100,000 (max: 10M)
    ///
    /// Raises:
    ///     `ValueError`: If count is 0 or exceeds 10M
    fn with_max_documents(&self, count: usize) -> PyResult<Self> {
        if count == 0 || count > ABSOLUTE_MAX_DOCUMENTS {
            return Err(PyValueError::new_err(format!(
                "max_documents must be between 1 and {ABSOLUTE_MAX_DOCUMENTS} (10M)"
            )));
        }
        Ok(Self {
            inner: self.inner.clone().with_max_documents(count),
        })
    }

    /// Sets minimum chunk size in bytes.
    ///
    /// Default: 4KB
    ///
    /// Raises:
    ///     `ValueError`: If size is 0
    fn with_min_chunk_size(&self, size: usize) -> PyResult<Self> {
        if size == 0 {
            return Err(PyValueError::new_err(
                "min_chunk_size must be greater than 0",
            ));
        }
        Ok(Self {
            inner: self.inner.clone().with_min_chunk_size(size),
        })
    }

    /// Sets maximum chunk size in bytes.
    ///
    /// Default: 10MB
    ///
    /// Raises:
    ///     `ValueError`: If size is 0
    fn with_max_chunk_size(&self, size: usize) -> PyResult<Self> {
        if size == 0 {
            return Err(PyValueError::new_err(
                "max_chunk_size must be greater than 0",
            ));
        }
        Ok(Self {
            inner: self.inner.clone().with_max_chunk_size(size),
        })
    }

    #[allow(clippy::unused_self)] // PyO3 requires &self for __repr__
    fn __repr__(&self) -> String {
        "ParallelConfig()".to_string()
    }
}

/// Parse multi-document YAML in parallel.
///
/// Automatically splits YAML documents at '---' boundaries and
/// processes them in parallel using Rayon thread pool.
///
/// Args:
///     source: YAML source potentially containing multiple documents
///     config: Optional parallel processing configuration
///
/// Returns:
///     List of parsed YAML documents
///
/// Raises:
///     `ValueError`: If parsing fails or limits exceeded
///
/// Performance:
///     - Single document: Falls back to sequential parsing
///     - Multi-document: 3-6x faster on 4-8 core systems
///     - Use for files > 1MB with multiple documents
///
/// Example:
///     >>> from `fast_yaml`._core.parallel import `parse_parallel`
///     >>> yaml = "---\\nfoo: 1\\n---\\nbar: 2\\n---\\nbaz: 3"
///     >>> docs = `parse_parallel(yaml)`
///     >>> len(docs)
///     3
#[pyfunction]
#[pyo3(signature = (source, config=None))]
fn parse_parallel(
    py: Python<'_>,
    source: &str,
    config: Option<PyParallelConfig>,
) -> PyResult<Py<PyAny>> {
    // Release GIL for parallel processing
    let result = py.detach(|| match config {
        Some(cfg) => parse_parallel_with_config(source, &cfg.inner),
        None => rust_parse_parallel(source),
    });

    let values = result.map_err(|e: ParallelError| PyValueError::new_err(e.to_string()))?;

    // Convert Vec<Value> to Python list with pre-allocated capacity
    let mut py_values = Vec::with_capacity(values.len());
    for value in &values {
        py_values.push(value_to_python(py, value)?);
    }

    let list = PyList::new(py, &py_values)?;
    Ok(list.into_any().unbind())
}

/// Register the parallel submodule.
pub fn register_parallel_module(
    py: Python<'_>,
    parent_module: &Bound<'_, PyModule>,
) -> PyResult<()> {
    let parallel_module = PyModule::new(py, "parallel")?;

    parallel_module.add_class::<PyParallelConfig>()?;
    parallel_module.add_function(wrap_pyfunction!(parse_parallel, &parallel_module)?)?;

    parent_module.add_submodule(&parallel_module)?;
    Ok(())
}
