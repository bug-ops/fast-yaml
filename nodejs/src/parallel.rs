//! Parallel YAML processing for Node.js.
//!
//! Provides multi-threaded parsing for large multi-document YAML files.

use crate::conversion::yaml_to_js;
use fast_yaml_parallel::{
    ParallelConfig as RustParallelConfig, ParallelError, parse_parallel_with_config,
};
use napi::{Env, Result as NapiResult, Task, bindgen_prelude::*};
use napi_derive::napi;

/// Maximum thread count allowed.
const MAX_THREADS: u32 = 128;

/// Maximum input size in bytes (default 100MB, can be configured up to 1GB).
const ABSOLUTE_MAX_INPUT_SIZE: u32 = 1024 * 1024 * 1024;

/// Maximum document count (default 100k, can be configured up to 10M).
const ABSOLUTE_MAX_DOCUMENTS: u32 = 10_000_000;

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
    pub thread_count: Option<u32>,

    /// Minimum bytes per chunk (default: 4096).
    pub min_chunk_size: Option<u32>,

    /// Maximum bytes per chunk (default: 10MB).
    pub max_chunk_size: Option<u32>,

    /// Maximum total input size in bytes (default: 100MB, max: 1GB).
    pub max_input_size: Option<u32>,

    /// Maximum number of documents allowed (default: 100k, max: 10M).
    pub max_documents: Option<u32>,
}

impl ParallelConfig {
    /// Convert to Rust parallel config with validation.
    fn to_rust_config(&self) -> NapiResult<RustParallelConfig> {
        // Validate thread_count
        if let Some(count) = self.thread_count
            && count > MAX_THREADS
        {
            return Err(napi::Error::from_reason(format!(
                "threadCount {count} exceeds maximum allowed {MAX_THREADS}"
            )));
        }

        // Validate input size
        if let Some(size) = self.max_input_size
            && (size == 0 || size > ABSOLUTE_MAX_INPUT_SIZE)
        {
            return Err(napi::Error::from_reason(format!(
                "maxInputSize must be between 1 and {ABSOLUTE_MAX_INPUT_SIZE} (1GB)"
            )));
        }

        // Validate document count
        if let Some(count) = self.max_documents
            && (count == 0 || count > ABSOLUTE_MAX_DOCUMENTS)
        {
            return Err(napi::Error::from_reason(format!(
                "maxDocuments must be between 1 and {ABSOLUTE_MAX_DOCUMENTS} (10M)"
            )));
        }

        // Validate chunk sizes
        let min_chunk = self.min_chunk_size.unwrap_or(4096);
        let max_chunk = self.max_chunk_size.unwrap_or(10 * 1024 * 1024);

        if min_chunk == 0 {
            return Err(napi::Error::from_reason(
                "minChunkSize must be greater than 0",
            ));
        }
        if max_chunk < min_chunk {
            return Err(napi::Error::from_reason(
                "maxChunkSize must be >= minChunkSize",
            ));
        }

        // Build config
        let mut config = RustParallelConfig::new();

        if let Some(count) = self.thread_count {
            config = config.with_thread_count(Some(count as usize));
        }
        if let Some(size) = self.min_chunk_size {
            config = config.with_min_chunk_size(size as usize);
        }
        if let Some(size) = self.max_chunk_size {
            config = config.with_max_chunk_size(size as usize);
        }
        if let Some(size) = self.max_input_size {
            config = config.with_max_input_size(size as usize);
        }
        if let Some(count) = self.max_documents {
            config = config.with_max_documents(count as usize);
        }

        Ok(config)
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
#[napi]
#[allow(clippy::needless_pass_by_value)]
pub fn parse_parallel(
    env: Env,
    yaml_str: String,
    config: Option<ParallelConfig>,
) -> NapiResult<Vec<Unknown<'static>>> {
    // Convert config
    let rust_config = config.unwrap_or_default().to_rust_config()?;

    // Parse in parallel
    let values = parse_parallel_with_config(&yaml_str, &rust_config)
        .map_err(|e: ParallelError| napi::Error::from_reason(e.to_string()))?;

    // Convert to JavaScript
    let mut js_docs = Vec::with_capacity(values.len());
    for value in &values {
        let result = yaml_to_js(&env, value)?;
        #[allow(clippy::missing_transmute_annotations)]
        js_docs.push(unsafe { std::mem::transmute(result) });
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

    fn compute(&mut self) -> NapiResult<Self::Output> {
        // Validate config in compute phase to properly return errors to user
        let rust_config = self.config.to_rust_config()?;

        parse_parallel_with_config(&self.yaml_str, &rust_config)
            .map_err(|e| napi::Error::from_reason(e.to_string()))
    }

    fn resolve(&mut self, env: Env, output: Self::Output) -> NapiResult<Self::JsValue> {
        let mut js_docs = Vec::with_capacity(output.len());
        for value in &output {
            let result = yaml_to_js(&env, value)?;
            #[allow(clippy::missing_transmute_annotations)]
            js_docs.push(unsafe { std::mem::transmute(result) });
        }
        Ok(js_docs)
    }
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
#[napi]
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
            thread_count: Some(4),
            min_chunk_size: Some(2048),
            max_chunk_size: Some(5 * 1024 * 1024),
            max_input_size: Some(50 * 1024 * 1024),
            max_documents: Some(50_000),
        };
        assert!(config.to_rust_config().is_ok());

        // Invalid thread count
        let config = ParallelConfig {
            thread_count: Some(1000),
            ..Default::default()
        };
        assert!(config.to_rust_config().is_err());

        // Invalid chunk sizes
        let config = ParallelConfig {
            min_chunk_size: Some(10000),
            max_chunk_size: Some(1000),
            ..Default::default()
        };
        assert!(config.to_rust_config().is_err());
    }
}
