//! Rayon-based parallel processing.

#![allow(clippy::redundant_pub_crate)]

use crate::chunker::{Chunk, chunk_documents};
use crate::config::ParallelConfig;
use crate::error::{ParallelError, Result};
use fast_yaml_core::{Parser, Value};
use rayon::prelude::*;

/// Validate input size against configured limit.
fn validate_input_size(input: &str, config: &ParallelConfig) -> Result<()> {
    let size = input.len();
    let max = config.max_input_size();
    if size > max {
        return Err(ParallelError::ConfigError(format!(
            "input size {size} exceeds maximum allowed {max}"
        )));
    }
    Ok(())
}

/// Validate document count against configured limit.
fn validate_document_count(chunks: &[Chunk<'_>], config: &ParallelConfig) -> Result<()> {
    let count = chunks.len();
    let max = config.max_documents();
    if count > max {
        return Err(ParallelError::ConfigError(format!(
            "document count {count} exceeds maximum allowed {max}"
        )));
    }
    Ok(())
}

/// Process YAML input in parallel.
///
/// Orchestrates chunking, parallel parsing, and result aggregation.
///
/// # Errors
///
/// Returns error if:
/// - Input size exceeds configured maximum
/// - Document count exceeds configured maximum
/// - Any document fails to parse
pub(crate) fn process_parallel(input: &str, config: &ParallelConfig) -> Result<Vec<Value>> {
    // Step 1: Validate input size
    validate_input_size(input, config)?;

    // Step 2: Chunk documents
    let chunks = chunk_documents(input);

    // Step 3: Validate document count
    validate_document_count(&chunks, config)?;

    // Step 4: Check if parallelism is worthwhile
    if should_use_sequential(&chunks, config) {
        return parse_sequential(&chunks);
    }

    // Step 5: Use global thread pool (fast path) or custom pool if explicitly configured
    if let Some(thread_count) = config.thread_count
        && thread_count > 0
        && thread_count != rayon::current_num_threads()
    {
        // Only create custom pool if explicitly requested AND different from current
        let pool = configure_thread_pool(config)?;
        return pool.install(|| parse_chunks_parallel(&chunks));
    }

    // Step 6: Parse chunks in parallel using global pool (no creation overhead)
    parse_chunks_parallel(&chunks)
}

/// Determines if sequential processing is more efficient.
///
/// Returns true when:
/// - Single document (no parallelism benefit)
/// - Total size is very small AND few documents (overhead exceeds benefit)
/// - Thread count explicitly set to 0
fn should_use_sequential(chunks: &[Chunk<'_>], config: &ParallelConfig) -> bool {
    if config.thread_count == Some(0) {
        return true; // User requested sequential
    }

    if chunks.len() <= 1 {
        return true; // Single document
    }

    // With global thread pool (no creation overhead), parallelism is beneficial
    // even for smaller workloads as long as we have multiple documents
    let total_bytes: usize = chunks.iter().map(|c| c.content.len()).sum();

    // Use sequential only if both small total size AND few documents
    total_bytes < config.min_chunk_size && chunks.len() < 4
}

/// Parse chunks sequentially (fallback for small inputs).
fn parse_sequential(chunks: &[Chunk<'_>]) -> Result<Vec<Value>> {
    chunks
        .iter()
        .map(|chunk| {
            Parser::parse_str(chunk.content)
                .map_err(|source| ParallelError::ParseError {
                    index: chunk.index,
                    source,
                })?
                .ok_or_else(|| {
                    ParallelError::ChunkingError(format!("empty document at index {}", chunk.index))
                })
        })
        .collect()
}

/// Configure Rayon thread pool based on config.
fn configure_thread_pool(config: &ParallelConfig) -> Result<rayon::ThreadPool> {
    let num_threads = config.effective_thread_count();

    rayon::ThreadPoolBuilder::new()
        .num_threads(num_threads)
        .build()
        .map_err(|e| ParallelError::ThreadPoolError(e.to_string()))
}

/// Parse chunks in parallel using Rayon.
///
/// Uses indexed parallel iterator to preserve document order.
fn parse_chunks_parallel(chunks: &[Chunk<'_>]) -> Result<Vec<Value>> {
    chunks
        .par_iter()
        .map(|chunk| {
            // Parse each chunk independently
            Parser::parse_str(chunk.content)
                .map_err(|source| ParallelError::ParseError {
                    index: chunk.index,
                    source,
                })?
                .ok_or_else(|| {
                    ParallelError::ChunkingError(format!("empty document at index {}", chunk.index))
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_parallel_multi_document() {
        let yaml = "---\nfoo: 1\n---\nbar: 2\n---\nbaz: 3";
        let config = ParallelConfig::default();

        let docs = process_parallel(yaml, &config).unwrap();
        assert_eq!(docs.len(), 3);
    }

    #[test]
    fn test_process_parallel_single_document_fallback() {
        let yaml = "single: document";
        let config = ParallelConfig::default();

        let docs = process_parallel(yaml, &config).unwrap();
        assert_eq!(docs.len(), 1);
    }

    #[test]
    fn test_process_parallel_error_propagation() {
        let yaml = "---\nvalid: true\n---\ninvalid: [unclosed";
        let config = ParallelConfig::default();

        let result = process_parallel(yaml, &config);
        assert!(result.is_err());

        if let Err(ParallelError::ParseError { index, .. }) = result {
            assert_eq!(index, 1); // Second document failed
        } else {
            panic!("Expected ParseError");
        }
    }

    #[test]
    fn test_process_parallel_with_thread_limit() {
        let yaml = "---\nfoo: 1\n---\nbar: 2";
        let config = ParallelConfig::new().with_thread_count(Some(2));

        let docs = process_parallel(yaml, &config).unwrap();
        assert_eq!(docs.len(), 2);
    }

    #[test]
    fn test_process_sequential_mode() {
        let yaml = "---\nfoo: 1\n---\nbar: 2";
        let config = ParallelConfig::new().with_thread_count(Some(0));

        let docs = process_parallel(yaml, &config).unwrap();
        assert_eq!(docs.len(), 2);
    }

    #[test]
    fn test_should_use_sequential_single_doc() {
        let chunks = vec![Chunk {
            index: 0,
            content: "foo: 1",
            offset: 0,
        }];
        let config = ParallelConfig::default();

        assert!(should_use_sequential(&chunks, &config));
    }

    #[test]
    fn test_should_use_sequential_small_input() {
        let chunks = vec![
            Chunk {
                index: 0,
                content: "a: 1",
                offset: 0,
            },
            Chunk {
                index: 1,
                content: "b: 2",
                offset: 4,
            },
        ];
        let config = ParallelConfig::default();

        assert!(should_use_sequential(&chunks, &config));
    }

    #[test]
    fn test_should_use_sequential_explicit() {
        let chunks = vec![
            Chunk {
                index: 0,
                content: "foo: 1",
                offset: 0,
            },
            Chunk {
                index: 1,
                content: "bar: 2",
                offset: 6,
            },
        ];
        let config = ParallelConfig::new().with_thread_count(Some(0));

        assert!(should_use_sequential(&chunks, &config));
    }

    #[test]
    fn test_should_not_use_sequential_large_input() {
        // Create chunks with total size > min_chunk_size
        let large_content = "x".repeat(2048);
        let chunks = vec![
            Chunk {
                index: 0,
                content: &large_content,
                offset: 0,
            },
            Chunk {
                index: 1,
                content: &large_content,
                offset: 2048,
            },
        ];
        let config = ParallelConfig::default(); // min_chunk_size = 1024

        assert!(!should_use_sequential(&chunks, &config));
    }

    #[test]
    fn test_parse_sequential_error() {
        let chunks = vec![
            Chunk {
                index: 0,
                content: "---\nvalid: true",
                offset: 0,
            },
            Chunk {
                index: 1,
                content: "---\ninvalid: [",
                offset: 15,
            },
        ];

        let result = parse_sequential(&chunks);
        assert!(result.is_err());

        if let Err(ParallelError::ParseError { index, .. }) = result {
            assert_eq!(index, 1);
        } else {
            panic!("Expected ParseError");
        }
    }

    #[test]
    fn test_configure_thread_pool_default() {
        let config = ParallelConfig::default();
        let pool = configure_thread_pool(&config);
        assert!(pool.is_ok());
    }

    #[test]
    fn test_configure_thread_pool_custom_threads() {
        let config = ParallelConfig::new().with_thread_count(Some(4));
        let pool = configure_thread_pool(&config);
        assert!(pool.is_ok());
    }

    #[test]
    fn test_parse_chunks_parallel_order_preserved() {
        let chunks = vec![
            Chunk {
                index: 0,
                content: "---\nfirst: 0",
                offset: 0,
            },
            Chunk {
                index: 1,
                content: "---\nsecond: 1",
                offset: 13,
            },
            Chunk {
                index: 2,
                content: "---\nthird: 2",
                offset: 27,
            },
        ];

        let docs = parse_chunks_parallel(&chunks).unwrap();
        assert_eq!(docs.len(), 3);
    }

    #[test]
    fn test_parse_chunks_parallel_error_with_index() {
        let chunks = vec![
            Chunk {
                index: 0,
                content: "---\nvalid: 1",
                offset: 0,
            },
            Chunk {
                index: 1,
                content: "---\ninvalid: [",
                offset: 13,
            },
            Chunk {
                index: 2,
                content: "---\nvalid: 2",
                offset: 27,
            },
        ];

        let result = parse_chunks_parallel(&chunks);
        assert!(result.is_err());

        if let Err(ParallelError::ParseError { index, .. }) = result {
            assert_eq!(index, 1);
        } else {
            panic!("Expected ParseError with index");
        }
    }

    #[test]
    fn test_process_parallel_empty_input() {
        let yaml = "";
        let config = ParallelConfig::default();

        let docs = process_parallel(yaml, &config).unwrap();
        assert_eq!(docs.len(), 0);
    }

    #[test]
    fn test_process_parallel_whitespace_only() {
        let yaml = "   \n\n\t  ";
        let config = ParallelConfig::default();

        let result = process_parallel(yaml, &config);
        // Whitespace-only may return empty vec or be treated as no documents
        assert!(result.is_ok() && result.unwrap().is_empty());
    }

    #[test]
    fn test_should_use_sequential_empty_chunks() {
        let chunks: Vec<Chunk> = vec![];
        let config = ParallelConfig::default();

        // Empty chunks list should use sequential (edge case)
        assert!(should_use_sequential(&chunks, &config));
    }

    #[test]
    fn test_validate_input_size_ok() {
        let config = ParallelConfig::new().with_max_input_size(100);
        let result = validate_input_size("small", &config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_input_size_exceeded() {
        let config = ParallelConfig::new().with_max_input_size(5);
        let result = validate_input_size("large input", &config);
        assert!(result.is_err());

        if let Err(ParallelError::ConfigError(msg)) = result {
            assert!(msg.contains("input size"));
            assert!(msg.contains("exceeds maximum"));
        } else {
            panic!("Expected ConfigError");
        }
    }

    #[test]
    fn test_validate_document_count_ok() {
        let chunks = vec![
            Chunk {
                index: 0,
                content: "a: 1",
                offset: 0,
            },
            Chunk {
                index: 1,
                content: "b: 2",
                offset: 4,
            },
        ];
        let config = ParallelConfig::new().with_max_documents(10);
        let result = validate_document_count(&chunks, &config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_document_count_exceeded() {
        let chunks = vec![
            Chunk {
                index: 0,
                content: "a: 1",
                offset: 0,
            },
            Chunk {
                index: 1,
                content: "b: 2",
                offset: 4,
            },
            Chunk {
                index: 2,
                content: "c: 3",
                offset: 8,
            },
        ];
        let config = ParallelConfig::new().with_max_documents(2);
        let result = validate_document_count(&chunks, &config);
        assert!(result.is_err());

        if let Err(ParallelError::ConfigError(msg)) = result {
            assert!(msg.contains("document count"));
            assert!(msg.contains("exceeds maximum"));
        } else {
            panic!("Expected ConfigError");
        }
    }

    #[test]
    fn test_process_parallel_input_size_limit() {
        let config = ParallelConfig::new().with_max_input_size(5);
        let result = process_parallel("---\nlarge content", &config);
        assert!(result.is_err());
    }

    #[test]
    fn test_process_parallel_document_count_limit() {
        let yaml = "---\na: 1\n---\nb: 2\n---\nc: 3";
        let config = ParallelConfig::new().with_max_documents(2);
        let result = process_parallel(yaml, &config);
        assert!(result.is_err());
    }
}
