//! Stress tests for parallel processing with large inputs and high concurrency.

use fast_yaml_parallel::{ParallelConfig, parse_parallel, parse_parallel_with_config};
use std::fmt::Write;

#[test]
fn test_large_document_count() {
    // 10,000 small documents
    let mut yaml = String::new();
    for i in 0..10_000 {
        yaml.push_str("---\n");
        let _ = writeln!(yaml, "id: {i}");
    }

    let docs = parse_parallel(&yaml).unwrap();
    assert_eq!(docs.len(), 10_000);
}

#[test]
fn test_large_individual_documents() {
    // 10 documents with 1000 entries each
    let mut yaml = String::new();
    for doc_idx in 0..10 {
        yaml.push_str("---\n");
        for i in 0..1000 {
            let _ = writeln!(yaml, "key_{i}: value_{}", doc_idx * 1000 + i);
        }
    }

    let docs = parse_parallel(&yaml).unwrap();
    assert_eq!(docs.len(), 10);

    // Verify each document has 1000 keys
    for doc in &docs {
        if let Some(hash) = doc.as_hash() {
            assert_eq!(hash.len(), 1000);
        }
    }
}

#[test]
fn test_deeply_nested_structures() {
    // Create deeply nested structures
    let mut yaml = String::new();
    for _ in 0..50 {
        yaml.push_str("---\n");
        yaml.push_str("level0:\n");
        for level in 1..20 {
            let indent = "  ".repeat(level);
            let _ = writeln!(yaml, "{indent}level{level}:");
        }
        yaml.push_str(&"  ".repeat(20));
        yaml.push_str("value: deep\n");
    }

    let docs = parse_parallel(&yaml).unwrap();
    assert_eq!(docs.len(), 50);
}

#[test]
fn test_large_sequences() {
    // Documents with large arrays
    let mut yaml = String::new();
    for doc_idx in 0..20 {
        yaml.push_str("---\n");
        yaml.push_str("items:\n");
        for i in 0..5000 {
            let _ = writeln!(yaml, "  - item_{}", doc_idx * 5000 + i);
        }
    }

    let docs = parse_parallel(&yaml).unwrap();
    assert_eq!(docs.len(), 20);
}

#[test]
fn test_long_scalar_values() {
    // Documents with very long scalar values
    let long_text = "a".repeat(10_000);
    let mut yaml = String::new();

    for _ in 0..100 {
        yaml.push_str("---\n");
        let _ = writeln!(yaml, "long_value: {long_text}");
    }

    let docs = parse_parallel(&yaml).unwrap();
    assert_eq!(docs.len(), 100);
}

#[test]
fn test_block_scalar_stress() {
    // Many block scalars
    let mut yaml = String::new();
    for i in 0..100 {
        yaml.push_str("---\n");
        let _ = writeln!(yaml, "literal_{i}: |");
        for line in 0..100 {
            let _ = writeln!(yaml, "  Line {line} of document {i}");
        }
    }

    let docs = parse_parallel(&yaml).unwrap();
    assert_eq!(docs.len(), 100);
}

#[test]
fn test_maximum_thread_count() {
    let yaml = "---\ntest: 1\n---\ntest: 2\n---\ntest: 3\n---\ntest: 4";

    // Test with very high thread count (should be capped by Rayon)
    let config = ParallelConfig::new().with_thread_count(Some(128));
    let docs = parse_parallel_with_config(yaml, &config).unwrap();
    assert_eq!(docs.len(), 4);
}

#[test]
fn test_single_thread_vs_multi_thread() {
    // Create a substantial multi-document YAML
    let mut yaml = String::new();
    for i in 0..100 {
        yaml.push_str("---\n");
        let _ = writeln!(yaml, "id: {i}");
        let _ = writeln!(yaml, "data: {}", i * 2);
    }

    // Parse with single thread
    let config_single = ParallelConfig::new().with_thread_count(Some(1));
    let docs_single = parse_parallel_with_config(&yaml, &config_single).unwrap();

    // Parse with multiple threads
    let config_multi = ParallelConfig::new().with_thread_count(Some(8));
    let docs_multi = parse_parallel_with_config(&yaml, &config_multi).unwrap();

    // Results should be identical
    assert_eq!(docs_single.len(), docs_multi.len());
    assert_eq!(docs_single.len(), 100);
}

#[test]
fn test_mixed_document_sizes() {
    // Mix of small and large documents
    let mut yaml = String::new();

    for i in 0..50 {
        yaml.push_str("---\n");
        if i % 2 == 0 {
            // Small document
            let _ = writeln!(yaml, "small: {i}");
        } else {
            // Large document
            for j in 0..1000 {
                let _ = writeln!(yaml, "key_{j}: value_{i}");
            }
        }
    }

    let docs = parse_parallel(&yaml).unwrap();
    assert_eq!(docs.len(), 50);
}

#[test]
fn test_many_empty_lines() {
    let mut yaml = String::new();
    for i in 0..100 {
        yaml.push_str("---\n");
        let _ = writeln!(yaml, "doc: {i}");
        // Add many empty lines between documents
        yaml.push_str(&"\n".repeat(50));
    }

    let docs = parse_parallel(&yaml).unwrap();
    assert_eq!(docs.len(), 100);
}

#[test]
#[ignore = "memory intensive stress test"]
fn test_extreme_document_count() {
    // 100,000 documents - very memory intensive
    let mut yaml = String::new();
    for i in 0..100_000 {
        let _ = writeln!(yaml, "---\nid: {i}");
    }

    let docs = parse_parallel(&yaml).unwrap();
    assert_eq!(docs.len(), 100_000);
}

#[test]
#[ignore = "memory intensive stress test"]
fn test_extreme_individual_document_size() {
    // Single massive document (10MB)
    let mut yaml = String::from("---\nitems:\n");
    for i in 0..100_000 {
        let _ = writeln!(yaml, "  - item_{i}: data_{}", i * 2);
    }

    let docs = parse_parallel(&yaml).unwrap();
    assert_eq!(docs.len(), 1);
}

// ===================
// SECURITY VALIDATION TESTS
// ===================

#[test]
fn test_input_size_validation_within_limit() {
    // Input well within default 100MB limit
    let yaml = "---\ntest: value\n";
    let result = parse_parallel(yaml);
    assert!(result.is_ok());
}

#[test]
fn test_input_size_validation_custom_limit() {
    // Set very small limit and test it's enforced
    let yaml = "x".repeat(2000); // 2KB
    let config = ParallelConfig::new().with_max_input_size(1000); // 1KB limit

    let result = parse_parallel_with_config(&yaml, &config);
    assert!(result.is_err());

    if let Err(e) = result {
        let error_msg = format!("{e}");
        assert!(error_msg.contains("input size"));
        assert!(error_msg.contains("exceeds maximum"));
    }
}

#[test]
fn test_document_count_validation_within_limit() {
    // Well within default 100,000 document limit
    let mut yaml = String::new();
    for i in 0..100 {
        let _ = writeln!(yaml, "---\nid: {i}");
    }

    let result = parse_parallel(&yaml);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 100);
}

#[test]
fn test_document_count_validation_custom_limit() {
    // Set low limit and test it's enforced
    let mut yaml = String::new();
    for i in 0..101 {
        let _ = writeln!(yaml, "---\nid: {i}");
    }

    let config = ParallelConfig::new().with_max_documents(100);
    let result = parse_parallel_with_config(&yaml, &config);
    assert!(result.is_err());

    if let Err(e) = result {
        let error_msg = format!("{e}");
        assert!(error_msg.contains("document count"));
        assert!(error_msg.contains("exceeds maximum"));
    }
}

#[test]
fn test_thread_count_capping() {
    // Request excessive threads (should be capped at 128 internally)
    let yaml = "---\nfoo: 1\n---\nbar: 2";
    let config = ParallelConfig::new().with_thread_count(Some(10_000));

    // Should not crash or create 10k threads - capping happens internally
    let result = parse_parallel_with_config(yaml, &config);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 2);
    // Note: Thread count is capped internally to 128 max
}

#[test]
fn test_integer_overflow_protection() {
    // Create many small chunks to test saturating_add
    let mut yaml = String::new();
    for i in 0..1000 {
        let _ = writeln!(yaml, "---\nid: {i}");
    }

    // Should not overflow when calculating total size
    let result = parse_parallel(&yaml);
    assert!(result.is_ok());
}

#[test]
#[ignore = "exceeds default document limit"]
fn test_dos_protection_excessive_documents() {
    // This would exceed default 100k document limit
    let mut yaml = String::new();
    for i in 0..100_001 {
        let _ = writeln!(yaml, "---\nn: {i}");
    }

    let result = parse_parallel(&yaml);
    assert!(result.is_err());

    if let Err(e) = result {
        let error_msg = format!("{e}");
        assert!(error_msg.contains("document count"));
    }
}
