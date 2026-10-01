//! `PyO3` bindings for fast-yaml-parallel.
//!
//! Exposes multi-threaded YAML parsing and emission for large multi-document files.
//!
//! # Error Handling Strategy
//!
//! - `ValueError`: Used for input validation errors (limits exceeded, invalid config)
//! - `TypeError`: Used for type conversion errors (handled in conversion module)

use crate::conversion::value_to_python;
use crate::limits;
use crate::{check_output_len, check_output_size, python_to_yaml, sort_yaml_keys};
use fast_yaml_core::limits::{AliasBytes, Depth, Documents, InputBytes, ScanAhead};
use fast_yaml_core::{DumpBudget, Emitter, EmitterConfig, KeyDomain, MaxDocuments};
use fast_yaml_parallel::{
    Config as RustParallelConfig, Error as ParallelError, parse_parallel_with_config, shared_pool,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyList;
use rayon::prelude::*;
use std::num::NonZeroUsize;

/// Maximum thread count allowed (capped by Rust implementation).
const MAX_THREADS: usize = 128;

/// Configuration for parallel YAML processing.
///
/// Controls thread pool size, chunking thresholds, and resource limits.
///
/// Examples:
///     >>> from `fast_yaml`._core.parallel import `ParallelConfig`
///     >>> config = `ParallelConfig(thread_count=8`, `max_input_bytes=200`*1024*1024)
#[pyclass(
    module = "fast_yaml._core.parallel",
    name = "ParallelConfig",
    from_py_object
)]
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
        max_input_bytes=None,
        max_documents=None,
        auto_tune=true,
        max_depth=None,
        max_alias_bytes=None,
        max_scan_ahead=None
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        thread_count: Option<usize>,
        min_chunk_size: usize,
        max_chunk_size: usize,
        max_input_bytes: Option<&Bound<'_, PyAny>>,
        max_documents: Option<&Bound<'_, PyAny>>,
        auto_tune: bool,
        max_depth: Option<&Bound<'_, PyAny>>,
        max_alias_bytes: Option<&Bound<'_, PyAny>>,
        max_scan_ahead: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let parse_limits =
            limits::parse_limits(max_depth, max_alias_bytes, max_scan_ahead, max_documents)?;
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

        let config = python_config()
            .with_workers(thread_count)
            .with_sequential_threshold(min_chunk_size)
            .with_max_input_bytes(limits::bounded::<InputBytes>(
                "max_input_bytes",
                max_input_bytes,
            )?)
            .with_parse_limits(parse_limits);

        // Note: max_chunk_size is validated but not stored in new Config API
        let _ = max_chunk_size;

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
            inner: self.inner.clone().with_workers(count),
            auto_tune: self.auto_tune,
        })
    }

    /// Sets maximum total input size in bytes; `None` resets to the default.
    ///
    /// Default: 100 MiB (max: 1 GiB)
    ///
    /// Raises:
    ///     `ValueError`: If bytes is outside 1..=1 GiB
    ///     `TypeError`: If bytes is not an int (`bool` included)
    fn with_max_input_bytes(&self, bytes: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        Ok(Self {
            inner: self
                .inner
                .clone()
                .with_max_input_bytes(limits::bounded::<InputBytes>("max_input_bytes", bytes)?),
            ..self.clone()
        })
    }

    /// Sets maximum number of documents allowed; `None` resets to the default.
    ///
    /// Default: 100,000 (max: 10M)
    ///
    /// Raises:
    ///     `ValueError`: If count is outside 1..=10M
    ///     `TypeError`: If count is not an int (`bool` included)
    fn with_max_documents(&self, count: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let parse_limits = fast_yaml_core::ParseLimits {
            max_documents: limits::bounded::<Documents>("max_documents", count)?,
            ..self.inner.parse_limits()
        };
        Ok(Self {
            inner: self.inner.clone().with_parse_limits(parse_limits),
            ..self.clone()
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
            inner: self.inner.clone().with_sequential_threshold(size),
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
        // Note: max_chunk_size is validated but not stored in new Config API
        Ok(Self {
            inner: self.inner.clone(),
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

    /// Sets the maximum collection nesting depth; `None` resets to the default.
    ///
    /// Default: 256 (max: 512). Depth 512 needs about 1 MiB of thread stack and can
    /// abort on stacks of 512 KiB or less; 256 is safe.
    ///
    /// Raises:
    ///     `ValueError`: If depth is outside 1..=512
    ///     `TypeError`: If depth is not an int (`bool` included)
    fn with_max_depth(&self, depth: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let parse_limits = fast_yaml_core::ParseLimits {
            max_depth: limits::bounded::<Depth>("max_depth", depth)?,
            ..self.inner.parse_limits()
        };
        Ok(Self {
            inner: self.inner.clone().with_parse_limits(parse_limits),
            ..self.clone()
        })
    }

    /// Sets the alias-expansion budget in bytes, applied per stream; `None` resets to the default.
    ///
    /// Default: 64 MiB (max: 1 GiB)
    ///
    /// Raises:
    ///     `ValueError`: If bytes is outside 1..=1 GiB
    ///     `TypeError`: If bytes is not an int (`bool` included)
    fn with_max_alias_bytes(&self, bytes: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let parse_limits = fast_yaml_core::ParseLimits {
            max_alias_bytes: limits::bounded::<AliasBytes>("max_alias_bytes", bytes)?,
            ..self.inner.parse_limits()
        };
        Ok(Self {
            inner: self.inner.clone().with_parse_limits(parse_limits),
            ..self.clone()
        })
    }

    /// Sets the characters the parser may read past the last node; `None` resets to the default.
    ///
    /// Default: 4 Mi (max: 1 Gi). A flow collection at the root or in a `- ` entry, one scalar,
    /// or a run of comments longer than this is rejected.
    ///
    /// Raises:
    ///     `ValueError`: If chars is outside 1..=1 Gi
    ///     `TypeError`: If chars is not an int (`bool` included)
    fn with_max_scan_ahead(&self, chars: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let parse_limits = fast_yaml_core::ParseLimits {
            max_scan_ahead: limits::bounded::<ScanAhead>("max_scan_ahead", chars)?,
            ..self.inner.parse_limits()
        };
        Ok(Self {
            inner: self.inner.clone().with_parse_limits(parse_limits),
            ..self.clone()
        })
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

/// Configuration whose keys follow Python dict equality, so `1` and `true` collide with a position.
fn python_config() -> RustParallelConfig {
    RustParallelConfig::new().with_key_domain(KeyDomain::Python)
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
///     `ValueError`: If parsing fails, limits are exceeded, or a document holds
///         a decimal integer beyond the i64 range with more digits than `sys.get_int_max_str_digits()` (CPython's `int()` limit; `sys.set_int_max_str_digits()` raises it)
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
    let result = py.detach(|| match config {
        Some(cfg) => parse_parallel_with_config(source, &cfg.inner),
        None => parse_parallel_with_config(source, &python_config()),
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
///     ValueError: If limits exceeded, or an int has more digits than `sys.get_int_max_str_digits()` (CPython's limit)
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
    let mut budget = DumpBudget::default();

    for item in iter {
        let item = item?;
        let yaml = python_to_yaml(&item, &mut budget)?;
        yaml_values.push(yaml);
    }

    let max_docs = config.map_or(MaxDocuments::DEFAULT, |cfg| {
        cfg.inner.parse_limits().max_documents
    });
    if yaml_values.len() > max_docs.get() {
        return Err(PyValueError::new_err(format!(
            "input has {} documents, more than the maximum of {max_docs}",
            yaml_values.len(),
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
        .with_indent(limits::indent(indent)?)
        .with_width(limits::width(width)?)
        .with_default_flow_style(default_flow_style)
        .with_explicit_start(false); // We add separators manually

    // Determine thread count
    let thread_count = config.map_or(1, |cfg| {
        // If explicit workers is set, use it (takes precedence)
        if let Some(explicit) = cfg.inner.workers() {
            return explicit.min(128_usize);
        }

        // Otherwise, auto-tune if enabled
        if cfg.auto_tune {
            let avg_size = if yaml_values.is_empty() {
                0
            } else {
                yaml_values
                    .iter()
                    .map(crate::estimate_dump_yaml_size)
                    .sum::<usize>()
                    / yaml_values.len()
            };
            auto_tune_threads(yaml_values.len(), avg_size)
        } else {
            // Default to all CPUs when no workers and no auto_tune
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
        let pool = shared_pool(NonZeroUsize::new(thread_count).unwrap_or(NonZeroUsize::MIN))
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        py.detach(|| {
            pool.install(|| {
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
    check_output_len(emitted.iter().map(String::len).sum())?;
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

    check_output_size(output)
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
