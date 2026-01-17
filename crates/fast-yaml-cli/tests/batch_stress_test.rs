//! Stress tests for batch processing with high file counts.
//!
//! These tests verify:
//! - No race conditions with 1000+ files
//! - Worker scaling performance characteristics
//! - Memory stability under load
//! - Result aggregation correctness at scale

use fast_yaml_cli::batch::{
    config::ProcessingConfig,
    discovery::{DiscoveryConfig, FileDiscovery},
    processor::BatchProcessor,
};
use std::fs;
use std::time::Instant;
use tempfile::TempDir;

#[test]
fn test_process_1000_files_concurrent() {
    // Create 1000 small YAML files
    let temp_dir = TempDir::new().unwrap();

    for i in 0..1000 {
        let content = format!("id: {i}\ndata: test_{i}\nitems: [1, 2, 3]\n");
        let path = temp_dir.path().join(format!("file_{i:04}.yaml"));
        fs::write(&path, content).unwrap();
    }

    // Discovery phase
    let config = DiscoveryConfig::new();
    let discovery = FileDiscovery::new(config).unwrap();
    let discovered = discovery
        .discover(&[temp_dir.path().to_path_buf()])
        .unwrap();

    assert_eq!(discovered.len(), 1000, "Should discover all 1000 files");

    // Process with parallel workers
    let config = ProcessingConfig::new().with_workers(8);
    let processor = BatchProcessor::new(config);

    let start = Instant::now();
    let result = processor.process(&discovered);
    let elapsed = start.elapsed();

    // Verify all files processed successfully
    assert_eq!(result.total, 1000, "Should process all 1000 files");
    assert_eq!(result.success_count(), 1000, "All files should succeed");
    assert_eq!(result.failed, 0, "No files should fail");
    assert!(result.is_success(), "Batch should succeed");

    // Performance check - should complete in reasonable time
    assert!(
        elapsed.as_secs() < 30,
        "1000 files should process in <30s, took {elapsed:?}"
    );
}

#[test]
fn test_worker_scaling_performance() {
    // Create 200 files for scaling test
    let temp_dir = TempDir::new().unwrap();

    for i in 0..200 {
        let content = format!("id: {i}\ndata:\n  key: value\n  nested: {{a: 1, b: 2}}\n");
        let path = temp_dir.path().join(format!("file_{i:03}.yaml"));
        fs::write(&path, content).unwrap();
    }

    // Discovery
    let config = DiscoveryConfig::new();
    let discovery = FileDiscovery::new(config).unwrap();
    let discovered = discovery
        .discover(&[temp_dir.path().to_path_buf()])
        .unwrap();
    assert_eq!(discovered.len(), 200);

    // Test different worker counts
    let worker_counts = [1, 2, 4, 8];
    let mut timings = Vec::new();

    for workers in worker_counts {
        let config = ProcessingConfig::new().with_workers(workers);
        let processor = BatchProcessor::new(config);

        let start = Instant::now();
        let result = processor.process(&discovered);
        let elapsed = start.elapsed();

        assert_eq!(result.total, 200);
        assert_eq!(result.success_count(), 200);

        timings.push((workers, elapsed));
        eprintln!("Workers: {workers}, Time: {elapsed:?}");
    }

    // Verify parallel speedup (relaxed check for CI stability)
    let single_thread = timings[0].1;
    let eight_threads = timings[3].1;

    // 8 workers should be faster than 1 worker
    assert!(
        eight_threads < single_thread,
        "8 workers ({eight_threads:?}) should be faster than 1 worker ({single_thread:?})"
    );

    // Some speedup should be observed (at least 1.5x for 8 workers)
    let speedup = single_thread.as_secs_f64() / eight_threads.as_secs_f64();
    assert!(
        speedup > 1.5,
        "Expected speedup > 1.5x with 8 workers, got {speedup:.2}x"
    );
}

#[test]
fn test_mixed_file_sizes_stress() {
    // Create mix of small and large files
    let temp_dir = TempDir::new().unwrap();

    // 100 small files (< 10KB)
    for i in 0..100 {
        let content = format!("id: {i}\nkey: value\n");
        let path = temp_dir.path().join(format!("small_{i:03}.yaml"));
        fs::write(&path, content).unwrap();
    }

    // 10 medium files (~ 100KB each)
    for i in 0..10 {
        let items: String = (0..2000).map(|j| format!("  - item_{i}_{j}\n")).collect();
        let content = format!("id: {i}\nitems:\n{items}");
        let path = temp_dir.path().join(format!("medium_{i:02}.yaml"));
        fs::write(&path, content).unwrap();
    }

    // 5 large files (> 512KB to trigger mmap)
    for i in 0..5 {
        let items: String = (0..20000).map(|j| format!("  - item_{i}_{j}\n")).collect();
        let content = format!("id: {i}\nitems:\n{items}");
        let path = temp_dir.path().join(format!("large_{i:02}.yaml"));
        fs::write(&path, content).unwrap();
    }

    // Discovery
    let config = DiscoveryConfig::new();
    let discovery = FileDiscovery::new(config).unwrap();
    let discovered = discovery
        .discover(&[temp_dir.path().to_path_buf()])
        .unwrap();
    assert_eq!(discovered.len(), 115, "100 + 10 + 5 files");

    // Process with default workers
    let config = ProcessingConfig::new();
    let processor = BatchProcessor::new(config);

    let result = processor.process(&discovered);

    // All files should succeed
    assert_eq!(result.total, 115);
    assert_eq!(result.success_count(), 115);
    assert_eq!(result.failed, 0);
}

#[test]
fn test_error_handling_under_load() {
    // Create 500 files with some invalid YAML
    let temp_dir = TempDir::new().unwrap();

    for i in 0..500 {
        let content = if i % 10 == 0 {
            // Every 10th file is invalid
            format!("id: {i}\nbroken: [unclosed\n")
        } else {
            format!("id: {i}\nvalid: data\n")
        };
        let path = temp_dir.path().join(format!("file_{i:03}.yaml"));
        fs::write(&path, content).unwrap();
    }

    // Discovery
    let config = DiscoveryConfig::new();
    let discovery = FileDiscovery::new(config).unwrap();
    let discovered = discovery
        .discover(&[temp_dir.path().to_path_buf()])
        .unwrap();

    // Process with parallel workers
    let config = ProcessingConfig::new().with_workers(4);
    let processor = BatchProcessor::new(config);

    let result = processor.process(&discovered);

    // Verify counts
    assert_eq!(result.total, 500);
    assert_eq!(result.success_count(), 450, "450 valid files");
    assert_eq!(result.failed, 50, "50 invalid files (every 10th)");
    assert_eq!(result.errors.len(), 50, "Should have 50 error details");
}

#[test]
fn test_in_place_modification_stress() {
    // Create 100 files and modify them all in-place
    let temp_dir = TempDir::new().unwrap();

    for i in 0..100 {
        let content = format!("id: {i}\ndata:   {{key:value}}\n");
        let path = temp_dir.path().join(format!("file_{i:03}.yaml"));
        fs::write(&path, content).unwrap();
    }

    // Discovery
    let config = DiscoveryConfig::new();
    let discovery = FileDiscovery::new(config).unwrap();
    let discovered = discovery
        .discover(&[temp_dir.path().to_path_buf()])
        .unwrap();

    // Process with in_place modification
    let config = ProcessingConfig::new().with_in_place(true).with_workers(4);
    let processor = BatchProcessor::new(config);

    let result = processor.process(&discovered);

    // All should succeed
    assert_eq!(result.success_count(), 100);

    // Verify all files still exist and are valid YAML
    for i in 0..100 {
        let path = temp_dir.path().join(format!("file_{i:03}.yaml"));
        assert!(path.exists(), "File {i} should still exist");

        let content = fs::read_to_string(&path).unwrap();
        let parsed = fast_yaml_core::parser::Parser::parse_str(&content);
        assert!(parsed.is_ok(), "File {i} should still be valid YAML");
    }
}
