//! `PyO3` bindings for fast-yaml-parallel.
//!
//! Exposes multi-threaded YAML parsing and emission for large multi-document files.
//!
//! # Error Handling Strategy
//!
//! - `ValueError`: Used for input validation errors (limits exceeded, invalid config)
//! - `TypeError`: Used for type conversion errors (handled in conversion module)

use crate::conversion::value_to_python;
use crate::{python_to_yaml, sort_yaml_keys};
use fast_yaml_core::{Emitter, EmitterConfig};
use fast_yaml_parallel::{
    ParallelConfig as RustParallelConfig, ParallelError, parse_parallel as rust_parse_parallel,
    parse_parallel_with_config,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyList;
use rayon::prelude::*;

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
    auto_tune: bool,
}

#[pymethods]
impl PyParallelConfig {
    #[new]
    #[pyo3(signature = (
        thread_count=None,
        min_chunk_size=4096,
        max_chunk_size=10*1024*1024,
        max_input_size=100*1024*1024,
        max_documents=100_000,
        auto_tune=true
    ))]
    fn new(
        thread_count: Option<usize>,
        min_chunk_size: usize,
        max_chunk_size: usize,
        max_input_size: usize,
        max_documents: usize,
        auto_tune: bool,
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

        Ok(Self {
            inner: config,
            auto_tune,
        })
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
            auto_tune: self.auto_tune,
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
            auto_tune: self.auto_tune,
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
            auto_tune: self.auto_tune,
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
            auto_tune: self.auto_tune,
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
            auto_tune: self.auto_tune,
        })
    }

    /// Enable or disable automatic thread count tuning.
    ///
    /// When enabled and thread_count is None, the system analyzes the workload
    /// and chooses an optimal thread count based on document count and size.
    ///
    /// Default: true
    fn with_auto_tune(&self, enabled: bool) -> Self {
        Self {
            inner: self.inner.clone(),
            auto_tune: enabled,
        }
    }

    #[allow(clippy::unused_self)] // PyO3 requires &self for __repr__
    fn __repr__(&self) -> String {
        "ParallelConfig()".to_string()
    }
}

/// Auto-tune thread count based on document count and average size.
fn auto_tune_threads(doc_count: usize, avg_doc_size: usize) -> usize {
    let cpu_count = num_cpus::get().max(1); // Ensure at least 1 CPU

    // At least 4 documents to justify parallelism
    if doc_count < 4 {
        return 1;
    }

    // Small documents: limit threads to reduce overhead
    if avg_doc_size < 1024 {
        let max_threads = (cpu_count / 2).max(2); // Ensure max >= 2
        return (doc_count / 10).clamp(2, max_threads).max(2);
    }

    // Normal case: scale with document count
    let max_threads = cpu_count.max(2); // Ensure max >= 2
    let optimal = (doc_count / 4).clamp(2, max_threads);
    optimal.min(128)
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

/// Serialize multiple Python objects to YAML in parallel.
///
/// Documents are converted to YAML and emitted in parallel using Rayon thread pool.
/// Final output maintains document order.
///
/// Args:
///     documents: Iterable of Python objects to serialize
///     config: Optional parallel processing configuration
///     allow_unicode: Allow unicode characters (default: true)
///     sort_keys: Sort dictionary keys alphabetically (default: false)
///     indent: Indentation width in spaces (default: 2)
///     width: Maximum line width (default: 80)
///     default_flow_style: Force flow style for collections (default: None)
///     explicit_start: Add explicit document start marker (default: false)
///
/// Returns:
///     YAML string with documents separated by '---'
///
/// Raises:
///     TypeError: If any object cannot be serialized
///     ValueError: If limits exceeded
///
/// Example:
///     >>> from fast_yaml._core.parallel import dump_parallel
///     >>> docs = [{'id': i, 'data': f'value{i}'} for i in range(100)]
///     >>> yaml = dump_parallel(docs)
#[pyfunction]
#[pyo3(signature = (
    documents,
    config=None,
    allow_unicode=true,
    sort_keys=false,
    indent=2,
    width=80,
    default_flow_style=None,
    explicit_start=false
))]
#[allow(clippy::too_many_arguments)]
fn dump_parallel(
    py: Python<'_>,
    documents: &Bound<'_, PyAny>,
    config: Option<&PyParallelConfig>,
    allow_unicode: bool,
    sort_keys: bool,
    indent: usize,
    width: usize,
    default_flow_style: Option<bool>,
    explicit_start: bool,
) -> PyResult<String> {
    let _ = allow_unicode; // Accepted for API compatibility, always true in saphyr
    // Collect documents from Python iterator (requires GIL)
    let iter = documents.try_iter()?;
    let mut yaml_values = Vec::new();

    for item in iter {
        let item = item?;
        let yaml = python_to_yaml(&item)?;
        yaml_values.push(yaml);
    }

    // Validate against config limits
    let max_docs = config.map_or(100_000, |c| c.inner.max_documents());
    if yaml_values.len() > max_docs {
        return Err(PyValueError::new_err(format!(
            "document count {} exceeds maximum {}. Consider processing in batches.",
            yaml_values.len(),
            max_docs
        )));
    }

    // Sort keys if requested (serial, before parallel phase)
    let yaml_values: Vec<_> = if sort_keys {
        yaml_values
            .into_iter()
            .map(|y| sort_yaml_keys(&y))
            .collect()
    } else {
        yaml_values
    };

    // Create emitter config
    let emitter_config = EmitterConfig::new()
        .with_indent(indent)
        .with_width(width)
        .with_default_flow_style(default_flow_style)
        .with_explicit_start(false); // We add separators manually

    // Determine thread count
    let thread_count = config.map_or(1, |cfg| {
        // If explicit thread_count is set, use it (takes precedence)
        if let Some(explicit) = cfg.inner.thread_count() {
            return explicit.min(128);
        }

        // Otherwise, auto-tune if enabled
        if cfg.auto_tune {
            let avg_size = if yaml_values.is_empty() {
                0
            } else {
                yaml_values.iter().map(estimate_yaml_size).sum::<usize>() / yaml_values.len()
            };
            auto_tune_threads(yaml_values.len(), avg_size)
        } else {
            // Default to all CPUs when no thread_count and no auto_tune
            num_cpus::get().min(128)
        }
    });

    // Release GIL and emit in parallel
    let emitted: Vec<String> = if thread_count <= 1 || yaml_values.len() < 4 {
        // Sequential for small workloads
        yaml_values
            .iter()
            .map(|v| Emitter::emit_str_with_config(v, &emitter_config))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| PyValueError::new_err(e.to_string()))?
    } else {
        // Parallel emission
        py.detach(|| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(thread_count)
                .build()
                .map_err(|e| PyValueError::new_err(e.to_string()))?
                .install(|| {
                    yaml_values
                        .par_iter()
                        .map(|v| Emitter::emit_str_with_config(v, &emitter_config))
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|e| PyValueError::new_err(e.to_string()))
                })
        })?
    };

    // Combine outputs with document separators
    let total_size: usize = emitted.iter().map(String::len).sum::<usize>() + emitted.len() * 5;
    let mut output = String::with_capacity(total_size);

    for (i, doc) in emitted.iter().enumerate() {
        if i > 0 || explicit_start {
            output.push_str("---\n");
        }
        output.push_str(doc);
        if !output.ends_with('\n') {
            output.push('\n');
        }
    }

    Ok(output)
}

/// Estimate YAML output size for streaming threshold decision.
fn estimate_yaml_size(yaml: &saphyr::YamlOwned) -> usize {
    match yaml {
        saphyr::YamlOwned::Value(scalar) => match scalar {
            saphyr::ScalarOwned::Null => 4,
            saphyr::ScalarOwned::Boolean(_) => 5,
            saphyr::ScalarOwned::Integer(_) => 12,
            saphyr::ScalarOwned::FloatingPoint(_) => 20,
            saphyr::ScalarOwned::String(s) => s.len() + 2,
        },
        saphyr::YamlOwned::Sequence(arr) => arr.iter().map(|v| 3 + estimate_yaml_size(v)).sum(),
        saphyr::YamlOwned::Mapping(map) => map
            .iter()
            .map(|(k, v)| 10 + estimate_yaml_size(k) + estimate_yaml_size(v))
            .sum(),
        _ => 10,
    }
}

/// Register the parallel submodule.
pub fn register_parallel_module(
    py: Python<'_>,
    parent_module: &Bound<'_, PyModule>,
) -> PyResult<()> {
    let parallel_module = PyModule::new(py, "parallel")?;

    parallel_module.add_class::<PyParallelConfig>()?;
    parallel_module.add_function(wrap_pyfunction!(parse_parallel, &parallel_module)?)?;
    parallel_module.add_function(wrap_pyfunction!(dump_parallel, &parallel_module)?)?;

    parent_module.add_submodule(&parallel_module)?;
    Ok(())
}
