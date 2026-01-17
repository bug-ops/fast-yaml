//! Integration tests for batch processing pipeline (discovery + processor).
//!
//! These tests verify end-to-end functionality:
//! - FileDiscovery finds YAML files
//! - BatchProcessor processes them with proper configuration
//! - BatchResult aggregates results correctly
//! - In-place modification works atomically

use fast_yaml_cli::batch::{
    config::ProcessingConfig,
    discovery::{DiscoveryConfig, FileDiscovery},
    processor::BatchProcessor,
};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper to create a test YAML file with specific content
fn create_yaml_file(dir: &TempDir, name: &str, content: &str) -> PathBuf {
    let path = dir.path().join(name);
    // Create parent directory if it contains nested path
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, content).unwrap();
    path
}

#[test]
fn test_batch_end_to_end_success() {
    // Setup: Create temp directory with valid YAML files
    let temp_dir = TempDir::new().unwrap();

    create_yaml_file(&temp_dir, "file1.yaml", "key1: value1\nkey2: value2\n");
    create_yaml_file(&temp_dir, "file2.yml", "items:\n  - one\n  - two\n");
    create_yaml_file(&temp_dir, "nested/file3.yaml", "nested:\n  data: test\n");

    // Step 1: Discovery phase
    let config = DiscoveryConfig::new();
    let discovery = FileDiscovery::new(config).unwrap();
    let discovered = discovery.discover(&[temp_dir.path().to_path_buf()]).unwrap();

    assert_eq!(discovered.len(), 3, "Should discover 3 YAML files");

    // Step 2: Processing phase with default config
    let proc_config = ProcessingConfig::new();
    let processor = BatchProcessor::new(proc_config);

    let result = processor.process(&discovered);

    // Step 3: Verify results
    assert_eq!(result.total, 3, "Should process all 3 files");
    assert_eq!(result.success_count(), 3, "All files should succeed");
    assert_eq!(result.failed, 0, "No files should fail");
    assert!(result.is_success(), "Batch should succeed");
    assert!(result.errors.is_empty(), "Should have no errors");
}

#[test]
fn test_batch_mixed_valid_invalid_files() {
    // Setup: Create mix of valid and invalid YAML
    let temp_dir = TempDir::new().unwrap();

    create_yaml_file(&temp_dir, "valid.yaml", "key: value\n");
    create_yaml_file(&temp_dir, "invalid.yaml", "key: [unclosed\n");
    create_yaml_file(&temp_dir, "also_valid.yml", "items: [1, 2, 3]\n");

    // Discovery
    let config = DiscoveryConfig::new();
    let discovery = FileDiscovery::new(config).unwrap();
    let discovered = discovery.discover(&[temp_dir.path().to_path_buf()]).unwrap();

    assert_eq!(discovered.len(), 3);

    // Processing - should continue on error
    let proc_config = ProcessingConfig::new();
    let processor = BatchProcessor::new(proc_config);
    let result = processor.process(&discovered);

    // Verify: Some succeed, some fail
    assert_eq!(result.total, 3);
    assert!(result.success_count() >= 2, "At least 2 valid files");
    assert!(result.failed >= 1, "At least 1 invalid file");
    assert!(!result.is_success(), "Batch should have failures");
    assert!(!result.errors.is_empty(), "Should have error details");
}

#[test]
fn test_batch_in_place_modification() {
    // Setup: Create YAML files with specific content
    let temp_dir = TempDir::new().unwrap();

    let original_content = "key:    value\nnested:  {a: 1,b: 2}\n";
    let file1 = create_yaml_file(&temp_dir, "reformat.yaml", original_content);

    // Read original content
    let before = fs::read_to_string(&file1).unwrap();

    // Discovery
    let config = DiscoveryConfig::new();
    let discovery = FileDiscovery::new(config).unwrap();
    let discovered = discovery.discover(&[temp_dir.path().to_path_buf()]).unwrap();

    // Processing with in_place=true
    let proc_config = ProcessingConfig::new()
        .with_in_place(true)
        .with_indent(2)
        .with_width(80);

    let processor = BatchProcessor::new(proc_config);
    let result = processor.process(&discovered);

    // Verify processing succeeded
    assert_eq!(result.success_count(), 1);

    // Read modified content
    let after = fs::read_to_string(&file1).unwrap();

    // Content should be different (reformatted)
    assert_ne!(before, after, "File should be modified in-place");

    // Verify atomic write - file should still be valid YAML
    let parsed = fast_yaml_core::parser::Parser::parse_str(&after);
    assert!(parsed.is_ok(), "Modified file should still be valid YAML");
}

#[test]
fn test_batch_dry_run_no_modification() {
    // Setup
    let temp_dir = TempDir::new().unwrap();
    let original_content = "key: value\n";
    let file = create_yaml_file(&temp_dir, "test.yaml", original_content);

    // Discovery
    let config = DiscoveryConfig::new();
    let discovery = FileDiscovery::new(config).unwrap();
    let discovered = discovery.discover(&[temp_dir.path().to_path_buf()]).unwrap();

    // Process with dry_run=true and in_place=true
    let proc_config = ProcessingConfig::new()
        .with_in_place(true)
        .with_dry_run(true);

    let processor = BatchProcessor::new(proc_config);
    let result = processor.process(&discovered);

    // Should succeed
    assert_eq!(result.success_count(), 1);

    // File should NOT be modified
    let after = fs::read_to_string(&file).unwrap();
    assert_eq!(original_content, after, "Dry run should not modify files");
}

#[test]
fn test_batch_large_file_mmap_integration() {
    // Create file > 512KB to trigger mmap path
    let temp_dir = TempDir::new().unwrap();

    let large_yaml = "items:\n".to_string()
        + &(0..50_000)
            .map(|i| format!("  - item{i}\n"))
            .collect::<String>();

    let file = create_yaml_file(&temp_dir, "large.yaml", &large_yaml);

    // Verify file size > 512KB
    let metadata = fs::metadata(&file).unwrap();
    assert!(
        metadata.len() > 512 * 1024,
        "File should exceed mmap threshold"
    );

    // Discovery
    let config = DiscoveryConfig::new();
    let discovery = FileDiscovery::new(config).unwrap();
    let discovered = discovery.discover(&[temp_dir.path().to_path_buf()]).unwrap();

    // Process - should use mmap internally
    let proc_config = ProcessingConfig::new();
    let processor = BatchProcessor::new(proc_config);
    let result = processor.process(&discovered);

    // Should succeed
    assert_eq!(result.total, 1);
    assert_eq!(result.success_count(), 1);
}

#[test]
fn test_batch_parallel_workers() {
    // Create multiple files
    let temp_dir = TempDir::new().unwrap();

    for i in 0..20 {
        create_yaml_file(&temp_dir, &format!("file{i}.yaml"), "key: value\n");
    }

    // Discovery
    let config = DiscoveryConfig::new();
    let discovery = FileDiscovery::new(config).unwrap();
    let discovered = discovery.discover(&[temp_dir.path().to_path_buf()]).unwrap();

    assert_eq!(discovered.len(), 20);

    // Process with explicit worker count
    let proc_config = ProcessingConfig::new().with_workers(4);
    let processor = BatchProcessor::new(proc_config);

    let result = processor.process(&discovered);

    // All files should be processed successfully
    assert_eq!(result.total, 20);
    assert_eq!(result.success_count(), 20);
    assert_eq!(result.failed, 0);
}

#[test]
fn test_batch_empty_directory() {
    // Empty directory
    let temp_dir = TempDir::new().unwrap();

    // Discovery should find no files
    let config = DiscoveryConfig::new();
    let discovery = FileDiscovery::new(config).unwrap();
    let discovered = discovery.discover(&[temp_dir.path().to_path_buf()]).unwrap();

    assert_eq!(discovered.len(), 0);

    // Processing empty set should work
    let proc_config = ProcessingConfig::new();
    let processor = BatchProcessor::new(proc_config);
    let result = processor.process(&discovered);

    assert_eq!(result.total, 0);
    assert_eq!(result.success_count(), 0);
    assert_eq!(result.failed, 0);
}
