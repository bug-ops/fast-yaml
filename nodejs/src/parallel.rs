//! Parallel YAML processing for Node.js.
//!
//! Provides multi-threaded parsing for large multi-document YAML files.

use crate::conversion::yaml_to_js;
use crate::limits::parse_limits;
use crate::options::{U32_MAX, checked_opt_uint};
use fast_yaml_parallel::{Config as RustParallelConfig, parse_parallel_with_config};
use napi::{
    Env, Task,
    bindgen_prelude::{AsyncTask, Unknown, panic_to_error},
};
use napi_derive::napi;
use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind};

/// Maximum thread count allowed.
const MAX_THREADS: u64 = 128;

/// Maximum input size in bytes (default 100MB, can be configured up to 1GB).
const ABSOLUTE_MAX_INPUT_SIZE: u64 = 1024 * 1024 * 1024;

/// Maximum document count (default 100k, can be configured up to 10M).
const ABSOLUTE_MAX_DOCUMENTS: u64 = 10_000_000;

/// Document count allowed when `maxDocuments` is unset.
const DEFAULT_MAX_DOCUMENTS: usize = 100_000;

/// Configuration for parallel YAML processing.
///
/// Controls thread pool size, chunking thresholds, and resource limits.
///
/// # Example
///
/// ```javascript
/// const { parseParallel, ParallelConfig } = require('fastyaml-rs');
///
/// const config = {
///   threadCount: 8,
///   maxInputSize: 200 * 1024 * 1024
/// };
/// const docs = parseParallel(yamlString, config);
/// ```
#[napi(object)]
#[derive(Debug, Clone, Default)]
pub struct ParallelConfig {
    /// Thread pool size (null = CPU count, 0 = sequential).
    pub thread_count: Option<f64>,

    /// Minimum bytes per chunk (default: 4096).
    pub min_chunk_size: Option<f64>,

    /// Maximum total input size in bytes (default: 100MB, max: 1GB).
    pub max_input_size: Option<f64>,

    /// Maximum number of documents allowed (default: 100k, max: 10M).
    pub max_documents: Option<f64>,

    /// Maximum collection nesting depth (integer, 1..=512, default: 256).
    /// Stack note: the calling thread needs about 1 MiB of stack at depth 512 (roughly 980 KiB measured in release); on stacks of 512 KiB or less (e.g. a worker with stackSizeMb 0.5) the process can abort and the overflow cannot be caught, while the default 256 is safe. The emitter keeps its own fixed depth of 256, so data parsed deeper may fail to dump.
    pub max_depth: Option<f64>,

    /// Maximum estimated alias-expansion bytes, shared across chunks of one call (integer,
    /// 1..=1073741824, default: 67108864).
    pub max_alias_bytes: Option<f64>,
}

/// Maps a parallel-parse failure to a JS error; the document cap is an argument error.
fn parse_error(error: &fast_yaml_parallel::Error) -> napi::Error {
    match error {
        fast_yaml_parallel::Error::DocumentLimitExceeded { .. } => {
            napi::Error::new(napi::Status::InvalidArg, error.to_string())
        }
        other => napi::Error::from_reason(other.to_string()),
    }
}

impl ParallelConfig {
    /// Convert to Rust parallel config with validation.
    fn to_rust_config(&self) -> napi::Result<RustParallelConfig> {
        let thread_count = checked_opt_uint("threadCount", self.thread_count, 0, MAX_THREADS)?;
        let max_input_size = checked_opt_uint(
            "maxInputSize",
            self.max_input_size,
            1,
            ABSOLUTE_MAX_INPUT_SIZE,
        )?;
        let max_documents = checked_opt_uint(
            "maxDocuments",
            self.max_documents,
            1,
            ABSOLUTE_MAX_DOCUMENTS,
        )?;
        let min_chunk_size = checked_opt_uint("minChunkSize", self.min_chunk_size, 1, U32_MAX)?;

        let max_documents = NonZeroUsize::new(max_documents.unwrap_or(DEFAULT_MAX_DOCUMENTS))
            .ok_or_else(|| {
                napi::Error::new(napi::Status::InvalidArg, "maxDocuments must be at least 1")
            })?;
        let mut config = RustParallelConfig::new().with_max_documents(max_documents);

        if let Some(count) = thread_count {
            config = config.with_workers(Some(count));
        }
        if let Some(size) = max_input_size {
            config = config.with_max_input_size(size);
        }
        // The parallel API has no chunk-size bounds; minChunkSize maps to the sequential threshold.
        if let Some(size) = min_chunk_size {
            config = config.with_sequential_threshold(size);
        }

        Ok(config.with_parse_limits(parse_limits(self.max_depth, self.max_alias_bytes)?))
    }
}

/// Widens a value to `Unknown<'static>` for `Task::JsValue`, which cannot carry an env lifetime.
///
/// Only call from `Task::resolve`: napi-rs converts the returned value to a raw `napi_value`
/// before that call's handle scope closes, so the handle never outlives the env.
#[inline]
fn to_static(v: Unknown<'_>) -> Unknown<'static> {
    // SAFETY: lifetime-only change; see the function docs for why the handle stays valid.
    #[allow(clippy::missing_transmute_annotations)]
    unsafe {
        std::mem::transmute(v)
    }
}

/// Parse multi-document YAML in parallel (synchronous).
///
/// Automatically splits YAML documents at '---' boundaries and
/// processes them in parallel using Rayon thread pool.
///
/// # Arguments
///
/// * `yaml_str` - YAML source potentially containing multiple documents
/// * `config` - Optional parallel processing configuration
///
/// # Returns
///
/// Array of parsed YAML documents
///
/// # Errors
///
/// Throws if parsing fails or limits exceeded
///
/// # Performance
///
/// - Single document: Falls back to sequential parsing
/// - Multi-document: 2-3x faster on 4-8 core systems
/// - Use for files > 1MB with multiple documents
///
/// # Example
///
/// ```javascript
/// const { parseParallel } = require('fastyaml-rs');
///
/// const yaml = '---\nfoo: 1\n---\nbar: 2\n---\nbaz: 3';
/// const docs = parseParallel(yaml);
/// console.log(docs.length); // 3
/// ```
#[napi(catch_unwind)]
#[allow(clippy::needless_pass_by_value)]
pub fn parse_parallel(
    env: &Env,
    yaml_str: String,
    config: Option<ParallelConfig>,
) -> napi::Result<Vec<Unknown<'_>>> {
    // Convert config
    let rust_config = match config.unwrap_or_default().to_rust_config() {
        Ok(c) => c,
        Err(e) => {
            env.throw_error(&e.reason, Some(e.status.as_ref()))?;
            return Ok(Vec::new());
        }
    };

    // Parse in parallel
    let values = match parse_parallel_with_config(&yaml_str, &rust_config) {
        Ok(v) => v,
        Err(e) => {
            let e = parse_error(&e);
            let code = (e.status != napi::Status::GenericFailure).then(|| e.status.as_ref());
            env.throw_error(&e.reason, code)?;
            return Ok(Vec::new());
        }
    };

    // Convert to JavaScript
    let mut js_docs = Vec::with_capacity(values.len());
    for value in &values {
        match yaml_to_js(env, value) {
            Ok(v) => js_docs.push(v),
            Err(e) => {
                env.throw_error(&e.to_string(), None)?;
                return Ok(Vec::new());
            }
        }
    }

    Ok(js_docs)
}

// -------------------------------------------------------------------------
// Async Task for non-blocking parallel parsing
// -------------------------------------------------------------------------

/// Task for async parallel parsing.
pub struct ParseParallelTask {
    yaml_str: String,
    config: ParallelConfig,
}

impl Task for ParseParallelTask {
    type Output = Vec<fast_yaml_parallel::Value>;
    type JsValue = Vec<Unknown<'static>>;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        contain_panic(|| {
            // Validate config in compute phase to properly return errors to user
            let rust_config = self.config.to_rust_config()?;

            parse_parallel_with_config(&self.yaml_str, &rust_config).map_err(|e| parse_error(&e))
        })
    }

    fn resolve(&mut self, env: Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        contain_panic(|| {
            let mut js_docs = Vec::with_capacity(output.len());
            for value in &output {
                js_docs.push(to_static(yaml_to_js(&env, value)?));
            }
            Ok(js_docs)
        })
    }
}

/// Task that panics in `compute`, used to verify worker-thread panic containment.
#[cfg(feature = "test-panic")]
pub struct PanicTask;

#[cfg(feature = "test-panic")]
impl Task for PanicTask {
    type Output = ();
    type JsValue = ();

    fn compute(&mut self) -> napi::Result<Self::Output> {
        contain_panic(|| panic!("test-panic: intentional async panic"))
    }

    fn resolve(&mut self, _env: Env, _output: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(())
    }
}

/// Runs `f`, converting a panic into a JS-visible error.
///
/// `AsyncTask` phases run outside the `#[napi(catch_unwind)]` trampolines, where an
/// unwinding panic would abort the Node process.
pub(crate) fn contain_panic<T>(f: impl FnOnce() -> napi::Result<T>) -> napi::Result<T> {
    catch_unwind(AssertUnwindSafe(f)).map_err(panic_to_error)?
}

/// Parse multi-document YAML in parallel (asynchronous).
///
/// Non-blocking version that runs parsing on Node.js worker thread pool.
/// Useful for keeping the event loop responsive during large file parsing.
///
/// # Arguments
///
/// * `yaml_str` - YAML source potentially containing multiple documents
/// * `config` - Optional parallel processing configuration
///
/// # Returns
///
/// Promise resolving to array of parsed YAML documents
///
/// # Example
///
/// ```javascript
/// const { parseParallelAsync } = require('fastyaml-rs');
///
/// const yaml = '---\nfoo: 1\n---\nbar: 2';
/// const docs = await parseParallelAsync(yaml);
/// console.log(docs); // [{ foo: 1 }, { bar: 2 }]
/// ```
#[napi(catch_unwind)]
#[allow(clippy::needless_pass_by_value)]
pub fn parse_parallel_async(
    yaml_str: String,
    config: Option<ParallelConfig>,
) -> AsyncTask<ParseParallelTask> {
    AsyncTask::new(ParseParallelTask {
        yaml_str,
        config: config.unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parallel_config_default() {
        let config = ParallelConfig::default();
        assert!(config.thread_count.is_none());
        assert!(config.min_chunk_size.is_none());
    }

    #[test]
    fn test_parallel_config_validation() {
        // Valid config
        let config = ParallelConfig {
            thread_count: Some(4.0),
            min_chunk_size: Some(2048.0),
            max_input_size: Some(52_428_800.0),
            max_documents: Some(50_000.0),
            ..Default::default()
        };
        assert!(config.to_rust_config().is_ok());

        // Invalid thread count
        let config = ParallelConfig {
            thread_count: Some(1000.0),
            ..Default::default()
        };
        assert!(config.to_rust_config().is_err());

        // Invalid chunk size
        let config = ParallelConfig {
            min_chunk_size: Some(0.0),
            ..Default::default()
        };
        assert!(config.to_rust_config().is_err());

        // Invalid parse limits
        let config = ParallelConfig {
            max_depth: Some(-1.0),
            ..Default::default()
        };
        assert!(config.to_rust_config().is_err());
    }
}
