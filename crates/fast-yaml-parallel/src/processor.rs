//! Rayon-based parallel processing.

#![allow(clippy::redundant_pub_crate)]

use crate::chunker::{Chunk, chunk_documents};
use crate::config::Config;
use crate::error::{Error, Result};
use fast_yaml_core::limits::StreamBudget;
use fast_yaml_core::{Parser, ScalarOwned, Value};
use rayon::prelude::*;

/// Validate input size against configured limit.
const fn validate_input_size(input: &str, config: &Config) -> Result<()> {
    let size = input.len();
    let max = config.max_input_size();
    if size > max {
        return Err(Error::InputTooLarge { size, max });
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
/// - Any document fails to parse
pub(crate) fn process_parallel(input: &str, config: &Config) -> Result<Vec<Value>> {
    // Step 1: Validate input size
    validate_input_size(input, config)?;

    // Step 2: Chunk documents
    let chunks = chunk_documents(fast_yaml_core::strip_bom(input));

    // A BOM-only stream is one null document, like `Parser::parse_all`.
    if chunks.is_empty() && !input.is_empty() {
        return Ok(vec![Value::Value(ScalarOwned::Null)]);
    }

    let budget = StreamBudget::new(config.parse_limits());

    // Step 3: Check if parallelism is worthwhile
    if should_use_sequential(&chunks, config) {
        return parse_sequential(&chunks, &budget);
    }

    // Step 4: Use global thread pool (fast path) or custom pool if explicitly configured
    if let Some(workers) = config.workers()
        && workers > 0
        && workers != rayon::current_num_threads()
    {
        // Only create custom pool if explicitly requested AND different from current
        let pool = configure_thread_pool(config)?;
        return pool.install(|| parse_chunks_parallel(&chunks, &budget));
    }

    // Step 5: Parse chunks in parallel using global pool (no creation overhead)
    parse_chunks_parallel(&chunks, &budget)
}

/// Determines if sequential processing is more efficient.
///
/// Returns true when:
/// - Single document (no parallelism benefit)
/// - Total size is very small AND few documents (overhead exceeds benefit)
/// - Workers explicitly set to 0
fn should_use_sequential(chunks: &[Chunk<'_>], config: &Config) -> bool {
    if config.workers() == Some(0) {
        return true; // User requested sequential
    }

    if chunks.len() <= 1 {
        return true; // Single document
    }

    // With global thread pool (no creation overhead), parallelism is beneficial
    // even for smaller workloads as long as we have multiple documents
    let total_bytes: usize = chunks.iter().map(|c| c.content.len()).sum();

    // Use sequential only if both small total size AND few documents
    total_bytes < config.sequential_threshold() && chunks.len() < 4
}

/// Parse chunks sequentially (fallback for small inputs).
fn parse_sequential(chunks: &[Chunk<'_>], budget: &StreamBudget) -> Result<Vec<Value>> {
    let mut docs = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        docs.extend(parse_chunk(chunk, budget)?);
    }
    Ok(docs)
}

/// Parse one chunk with the stream-wide budget; a document without content is `null`.
///
/// Error marks are relocated to whole-input coordinates.
fn parse_chunk(chunk: &Chunk<'_>, budget: &StreamBudget) -> Result<Vec<Value>> {
    Parser::parse_all_with_budget(chunk.content, budget).map_err(|source| Error::Parse {
        index: chunk.index,
        source: source.relocated(chunk.origin.line, chunk.origin.char_index),
    })
}

/// Configure Rayon thread pool based on config.
fn configure_thread_pool(config: &Config) -> Result<rayon::ThreadPool> {
    let num_threads = config.effective_workers();

    rayon::ThreadPoolBuilder::new()
        .num_threads(num_threads)
        .build()
        .map_err(Error::ThreadPool)
}

/// Parse chunks in parallel using Rayon.
///
/// Uses indexed parallel iterator to preserve document order; the error of the lowest-index
/// failing chunk is returned. Whether a stream exceeds the shared alias budget is
/// deterministic, but which chunk reports it depends on scheduling.
fn parse_chunks_parallel(chunks: &[Chunk<'_>], budget: &StreamBudget) -> Result<Vec<Value>> {
    let parsed: Vec<Result<Vec<Value>>> = chunks
        .par_iter()
        .map(|chunk| parse_chunk(chunk, budget))
        .collect();
    let mut docs = Vec::with_capacity(parsed.len());
    for chunk_docs in parsed {
        docs.extend(chunk_docs?);
    }
    Ok(docs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunker::SourceOrigin;
    use fast_yaml_core::limits::ParseLimits;

    fn budget() -> StreamBudget {
        StreamBudget::new(ParseLimits::default())
    }

    #[test]
    fn test_process_parallel_rejects_alias_bomb() {
        use std::fmt::Write as _;
        let mut yaml = String::from("a0: &a0 [x,x,x,x,x,x,x,x,x]\n");
        for i in 1..=8 {
            let refs = vec![format!("*a{}", i - 1); 9].join(",");
            writeln!(yaml, "a{i}: &a{i} [{refs}]").unwrap();
        }
        let result = process_parallel(&yaml, &Config::default());
        assert!(matches!(result, Err(Error::Parse { .. })));
    }

    #[test]
    fn test_process_parallel_multi_document() {
        let yaml = "---\nfoo: 1\n---\nbar: 2\n---\nbaz: 3";
        let config = Config::default();

        let docs = process_parallel(yaml, &config).unwrap();
        assert_eq!(docs.len(), 3);
    }

    #[test]
    fn test_process_parallel_single_document_fallback() {
        let yaml = "single: document";
        let config = Config::default();

        let docs = process_parallel(yaml, &config).unwrap();
        assert_eq!(docs.len(), 1);
    }

    #[test]
    fn test_process_parallel_error_propagation() {
        let yaml = "---\nvalid: true\n---\ninvalid: [unclosed";
        let config = Config::default();

        let result = process_parallel(yaml, &config);
        assert!(result.is_err());

        if let Err(Error::Parse { index, .. }) = result {
            assert_eq!(index, 1); // Second document failed
        } else {
            panic!("Expected ParseError");
        }
    }

    #[test]
    fn test_process_parallel_rejects_tag_prefix_amplification() {
        let mut yaml = format!("%TAG !e! tag:e.com,{}\n---\n", "a".repeat(100_000));
        yaml.extend((0..1_000).map(|i| format!("k{i}: !e!x v\n")));
        let err = process_parallel(&yaml, &Config::default()).unwrap_err();
        assert!(matches!(
            err,
            Error::Parse {
                source: fast_yaml_core::ParseError::LimitExceeded {
                    kind: fast_yaml_core::LimitKind::TagBytes(_),
                    ..
                },
                ..
            }
        ));
    }

    #[test]
    fn test_process_parallel_honors_parse_limits() {
        use fast_yaml_core::limits::{MaxAliasBytes, MaxDepth};
        let nested = "[[[1]]]\n---\nb: 2\n";
        let depth = Config::new().with_parse_limits(ParseLimits {
            max_depth: MaxDepth::new(2).unwrap(),
            ..ParseLimits::default()
        });
        assert!(process_parallel(nested, &depth).is_err());
        assert!(process_parallel(nested, &Config::new()).is_ok());

        let aliased = "a: &x [1, 2, 3]\nb: *x\n---\nc: 1\n";
        let alias = Config::new().with_parse_limits(ParseLimits {
            max_alias_bytes: MaxAliasBytes::new(64).unwrap(),
            ..ParseLimits::default()
        });
        assert!(process_parallel(aliased, &alias).is_err());
        assert!(process_parallel(aliased, &Config::new()).is_ok());
    }

    #[test]
    fn test_parse_limits_propagate_through_parallel_chunks() {
        use fast_yaml_core::limits::{MaxAliasBytes, MaxDepth};
        let parallel = || Config::new().with_sequential_threshold(0);

        let nested = "a: 1\n---\n[[[1]]]\n---\nb: 2\n";
        assert!(chunk_documents(nested).len() >= 3);
        let depth = parallel().with_parse_limits(ParseLimits {
            max_depth: MaxDepth::new(2).unwrap(),
            ..ParseLimits::default()
        });
        assert!(matches!(
            process_parallel(nested, &depth),
            Err(Error::Parse {
                source: fast_yaml_core::ParseError::LimitExceeded {
                    kind: fast_yaml_core::LimitKind::Depth(_),
                    ..
                },
                ..
            })
        ));
        assert!(process_parallel(nested, &parallel()).is_ok());

        let aliased = "a: &x [1, 2, 3]\nb: *x\n---\nc: &y [1, 2, 3]\nd: *y\n";
        assert!(chunk_documents(aliased).len() >= 2);
        let alias = parallel().with_parse_limits(ParseLimits {
            max_alias_bytes: MaxAliasBytes::new(300).unwrap(),
            ..ParseLimits::default()
        });
        assert!(matches!(
            process_parallel(aliased, &alias),
            Err(Error::Parse {
                source: fast_yaml_core::ParseError::LimitExceeded {
                    kind: fast_yaml_core::LimitKind::AliasBytes(_),
                    ..
                },
                ..
            })
        ));
        assert!(process_parallel(aliased, &parallel()).is_ok());
    }

    #[test]
    fn test_tag_budget_is_shared_across_chunks() {
        use fast_yaml_core::limits::MaxTagBytes;
        const CHUNKS: usize = 4;
        let doc = format!(
            "%TAG !e! tag:e.com,{}\n---\nk: !e!x v\n...\n",
            "a".repeat(4_000)
        );
        let stream = doc.repeat(CHUNKS);
        let chunks = chunk_documents(&stream);
        assert!(chunks.len() >= CHUNKS);
        let fresh = || {
            StreamBudget::new(ParseLimits {
                max_tag_bytes: MaxTagBytes::new(10_000),
                ..ParseLimits::default()
            })
        };
        for chunk in &chunks {
            assert!(parse_chunk(chunk, &fresh()).is_ok());
        }
        for result in [
            parse_sequential(&chunks, &fresh()),
            parse_chunks_parallel(&chunks, &fresh()),
        ] {
            assert!(matches!(
                result,
                Err(Error::Parse {
                    source: fast_yaml_core::ParseError::LimitExceeded {
                        kind: fast_yaml_core::LimitKind::TagBytes(_),
                        ..
                    },
                    ..
                })
            ));
        }
    }

    #[test]
    fn test_process_parallel_with_thread_limit() {
        let yaml = "---\nfoo: 1\n---\nbar: 2";
        let config = Config::new().with_workers(Some(2));

        let docs = process_parallel(yaml, &config).unwrap();
        assert_eq!(docs.len(), 2);
    }

    #[test]
    fn test_process_sequential_mode() {
        let yaml = "---\nfoo: 1\n---\nbar: 2";
        let config = Config::new().with_workers(Some(0));

        let docs = process_parallel(yaml, &config).unwrap();
        assert_eq!(docs.len(), 2);
    }

    #[test]
    fn test_should_use_sequential_single_doc() {
        let chunks = vec![Chunk {
            index: 0,
            content: "foo: 1",
            origin: SourceOrigin::default(),
        }];
        let config = Config::default();

        assert!(should_use_sequential(&chunks, &config));
    }

    #[test]
    fn test_should_use_sequential_small_input() {
        let chunks = vec![
            Chunk {
                index: 0,
                content: "a: 1",
                origin: SourceOrigin::default(),
            },
            Chunk {
                index: 1,
                content: "b: 2",
                origin: SourceOrigin::default(),
            },
        ];
        let config = Config::default();

        assert!(should_use_sequential(&chunks, &config));
    }

    #[test]
    fn test_should_use_sequential_explicit() {
        let chunks = vec![
            Chunk {
                index: 0,
                content: "foo: 1",
                origin: SourceOrigin::default(),
            },
            Chunk {
                index: 1,
                content: "bar: 2",
                origin: SourceOrigin::default(),
            },
        ];
        let config = Config::new().with_workers(Some(0));

        assert!(should_use_sequential(&chunks, &config));
    }

    #[test]
    fn test_should_not_use_sequential_large_input() {
        // Create chunks with total size > sequential_threshold
        let large_content = "x".repeat(2048);
        let chunks = vec![
            Chunk {
                index: 0,
                content: &large_content,
                origin: SourceOrigin::default(),
            },
            Chunk {
                index: 1,
                content: &large_content,
                origin: SourceOrigin::default(),
            },
        ];
        let config = Config::default(); // sequential_threshold = 4096

        assert!(!should_use_sequential(&chunks, &config));
    }

    #[test]
    fn test_parse_sequential_error() {
        let chunks = vec![
            Chunk {
                index: 0,
                content: "---\nvalid: true",
                origin: SourceOrigin::default(),
            },
            Chunk {
                index: 1,
                content: "---\ninvalid: [",
                origin: SourceOrigin::default(),
            },
        ];

        let result = parse_sequential(&chunks, &budget());
        assert!(result.is_err());

        if let Err(Error::Parse { index, .. }) = result {
            assert_eq!(index, 1);
        } else {
            panic!("Expected ParseError");
        }
    }

    #[test]
    fn test_configure_thread_pool_default() {
        let config = Config::default();
        let pool = configure_thread_pool(&config);
        assert!(pool.is_ok());
    }

    #[test]
    fn test_configure_thread_pool_custom_threads() {
        let config = Config::new().with_workers(Some(4));
        let pool = configure_thread_pool(&config);
        assert!(pool.is_ok());
    }

    #[test]
    fn test_parse_chunks_parallel_order_preserved() {
        let chunks = vec![
            Chunk {
                index: 0,
                content: "---\nfirst: 0",
                origin: SourceOrigin::default(),
            },
            Chunk {
                index: 1,
                content: "---\nsecond: 1",
                origin: SourceOrigin::default(),
            },
            Chunk {
                index: 2,
                content: "---\nthird: 2",
                origin: SourceOrigin::default(),
            },
        ];

        let docs = parse_chunks_parallel(&chunks, &budget()).unwrap();
        assert_eq!(docs.len(), 3);
    }

    #[test]
    fn test_parse_chunks_parallel_error_with_index() {
        let chunks = vec![
            Chunk {
                index: 0,
                content: "---\nvalid: 1",
                origin: SourceOrigin::default(),
            },
            Chunk {
                index: 1,
                content: "---\ninvalid: [",
                origin: SourceOrigin::default(),
            },
            Chunk {
                index: 2,
                content: "---\nvalid: 2",
                origin: SourceOrigin::default(),
            },
        ];

        let result = parse_chunks_parallel(&chunks, &budget());
        assert!(result.is_err());

        if let Err(Error::Parse { index, .. }) = result {
            assert_eq!(index, 1);
        } else {
            panic!("Expected ParseError with index");
        }
    }

    #[test]
    fn test_process_parallel_empty_input() {
        let yaml = "";
        let config = Config::default();

        let docs = process_parallel(yaml, &config).unwrap();
        assert_eq!(docs.len(), 0);
    }

    #[test]
    fn test_process_parallel_whitespace_only() {
        let yaml = "   \n\n\t  ";
        let config = Config::default();

        let result = process_parallel(yaml, &config);
        // Non-empty whitespace-only input is one null document, like `Parser::parse_all`
        assert_eq!(result.unwrap(), vec![Value::Value(ScalarOwned::Null)]);
    }

    #[test]
    fn test_should_use_sequential_empty_chunks() {
        let chunks: Vec<Chunk> = vec![];
        let config = Config::default();

        // Empty chunks list should use sequential (edge case)
        assert!(should_use_sequential(&chunks, &config));
    }

    #[test]
    fn test_validate_input_size_ok() {
        let config = Config::new().with_max_input_size(100);
        let result = validate_input_size("small", &config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_input_size_exceeded() {
        let config = Config::new().with_max_input_size(5);
        let result = validate_input_size("large input", &config);
        assert!(result.is_err());

        if let Err(Error::InputTooLarge { size, max }) = result {
            assert_eq!(size, 11);
            assert_eq!(max, 5);
        } else {
            panic!("Expected InputTooLarge");
        }
    }

    #[test]
    fn test_process_parallel_input_size_limit() {
        let config = Config::new().with_max_input_size(5);
        let result = process_parallel("---\nlarge content", &config);
        assert!(result.is_err());
    }

    #[test]
    fn test_process_parallel_bom_before_document_start() {
        let yaml = "\u{FEFF}---\nfoo: 1\n---\nbar: 2";
        let docs = process_parallel(yaml, &Config::default()).unwrap();
        assert_eq!(docs.len(), 2);
    }

    fn sequential() -> Config {
        Config::new().with_workers(Some(0))
    }

    fn bomb_doc() -> String {
        use std::fmt::Write as _;
        let mut yaml = String::from("---\na0: &a0 [x,x,x,x,x,x,x,x,x]\n");
        for i in 1..=5 {
            let refs = vec![format!("*a{}", i - 1); 9].join(",");
            writeln!(yaml, "a{i}: &a{i} [{refs}]").unwrap();
        }
        yaml
    }

    fn assert_alias_limit(result: &Result<Vec<Value>>) {
        use fast_yaml_core::ParseError;
        use fast_yaml_core::limits::LimitKind;
        assert!(matches!(
            result,
            Err(Error::Parse {
                source: ParseError::LimitExceeded {
                    kind: LimitKind::AliasBytes(_),
                    ..
                },
                ..
            })
        ));
    }

    #[test]
    fn test_alias_budget_is_shared_across_chunks() {
        let stream = bomb_doc().repeat(4);
        assert!(Parser::parse_all(&bomb_doc()).is_ok());
        assert!(matches!(
            Parser::parse_all(&stream),
            Err(fast_yaml_core::ParseError::LimitExceeded {
                kind: fast_yaml_core::LimitKind::AliasBytes(_),
                ..
            })
        ));
        assert_alias_limit(&process_parallel(&stream, &sequential()));
        assert_alias_limit(&process_parallel(&stream, &Config::default()));
        assert_alias_limit(&process_parallel(
            &stream,
            &Config::new().with_workers(Some(2)),
        ));
    }

    const PARITY_INPUTS: &[&str] = &[
        "a: 1\n---\u{A0}b\n",
        "a: 1\n---\u{85}b\n",
        "a\n...\nb",
        "a\n... # c\nb",
        "a\n...\n...\nb",
        "--- |\nfoo\n...\nbar",
        "a\r...\rb",
        "---\ra: 1\r---\rb: 2\r",
        "---\r---\r",
        "a: 1\n...\n# c\n",
        "...\n%YAML 1.2\n---\nb\n",
        "a\n...\n%TAG !e! tag:x,2000:\n---\nb: !e!y 1\n",
        "%YAML 1.2\n---\na\n...\n%YAML 1.2\n---\nb\n",
        "a\r\n...\r\nb\r\n",
        "a\n...x\n",
        "a\n...\n# c\n%YAML 1.2\n---\nb\n",
        "%YAML 1.2\n",
        "# only a comment\n",
        "\n\n---\nfoo: 1\n",
        "%YAML 1.2\na: 1\n",
        "%FOO bar\nkey: value\n",
        "... junk\na: 1\n",
        "%\n...x\r",
        "a\n...\n%YAML 1.2\nb\n",
    ];

    #[test]
    fn test_parity_with_parse_all() {
        for input in PARITY_INPUTS {
            let expected = Parser::parse_all(input);
            let actual = process_parallel(input, &sequential());
            match (expected, actual) {
                (Ok(e), Ok(a)) => assert_eq!(e, a, "{input:?}"),
                (Err(_), Err(_)) => {}
                (e, a) => panic!("{input:?}: parse_all {e:?} vs parallel {a:?}"),
            }
        }
    }

    #[test]
    fn test_error_marks_match_parse_all() {
        for input in [
            "---\na: 1\n---\nb: *x\n",
            "---\na: 1\n---\nb: 2\n---\nc: [\n",
            "a: 1\r---\rb: 2\r---\rc: [\r",
            "---\nключ: 1\n---\nb: [\n",
        ] {
            let expected = Parser::parse_all(input).unwrap_err().to_string();
            let Err(Error::Parse { source, .. }) = process_parallel(input, &sequential()) else {
                panic!("{input:?}: expected parse error");
            };
            assert_eq!(expected, source.to_string(), "{input:?}");
            let Err(Error::Parse { source, .. }) = process_parallel(input, &Config::default())
            else {
                panic!("{input:?}: expected parse error");
            };
            assert_eq!(expected, source.to_string(), "{input:?}");
        }
    }
}
