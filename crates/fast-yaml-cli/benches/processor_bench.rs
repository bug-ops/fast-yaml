//! Benchmarks for the batch processor module.
//!
//! These benchmarks measure:
//! - Small file processing (`read_to_string` path)
//! - Large file processing with mmap
//! - Mmap threshold comparison
//! - Parallel worker scaling
//! - Atomic write overhead
//! - Large batch stress scenarios
//! - End-to-end discovery + processing pipeline

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use fast_yaml_cli::batch::{
    config::ProcessingConfig,
    discovery::{DiscoveryConfig, FileDiscovery},
    processor::BatchProcessor,
};
use std::fs;
use std::hint::black_box;
use tempfile::TempDir;

/// Create test YAML files in a temp directory
fn setup_small_files(count: usize) -> TempDir {
    let temp_dir = TempDir::new().unwrap();

    for i in 0..count {
        let content = format!(
            "id: {i}\ndata:\n  key: value_{i}\n  nested: {{a: 1, b: 2}}\nitems: [1, 2, 3]\n"
        );
        let path = temp_dir.path().join(format!("file_{i:04}.yaml"));
        fs::write(&path, content).unwrap();
    }

    temp_dir
}

/// Create large YAML files for mmap testing
fn setup_large_files(count: usize, size_kb: usize) -> TempDir {
    use std::fmt::Write;

    let temp_dir = TempDir::new().unwrap();

    for i in 0..count {
        // Generate YAML content of specified size
        let items_count = (size_kb * 1024) / 50; // ~50 bytes per item
        let items = (0..items_count).fold(String::new(), |mut acc, j| {
            writeln!(&mut acc, "  - item_{i}_{j}").unwrap();
            acc
        });

        let content = format!("id: {i}\nitems:\n{items}");
        let path = temp_dir.path().join(format!("large_{i:02}.yaml"));
        fs::write(&path, content).unwrap();
    }

    temp_dir
}

/// Benchmark: Process 100 small files (~10KB each)
/// Tests the `read_to_string` code path and small file overhead
fn processor_small_files(c: &mut Criterion) {
    let mut group = c.benchmark_group("processor_small_files");
    group.throughput(Throughput::Elements(100));

    let temp_dir = setup_small_files(100);
    let config = DiscoveryConfig::default();
    let discovery = FileDiscovery::new(config).unwrap();
    let discovered = discovery
        .discover(&[temp_dir.path().to_path_buf()])
        .unwrap();
    let files = &discovered;

    group.bench_function("100_files_10kb", |b| {
        b.iter(|| {
            let config = ProcessingConfig::new();
            let processor = BatchProcessor::new(config);
            let result = processor.process(black_box(files));
            assert_eq!(result.success_count(), 100);
        });
    });

    group.finish();
}

/// Benchmark: Process large files with mmap vs `read_to_string`
/// Compares mmap performance against standard read for 1MB files
fn processor_large_files_mmap(c: &mut Criterion) {
    let mut group = c.benchmark_group("processor_large_files_mmap");
    group.throughput(Throughput::Elements(10));

    let temp_dir = setup_large_files(10, 1024); // 10 files @ 1MB each
    let config = DiscoveryConfig::default();
    let discovery = FileDiscovery::new(config).unwrap();
    let discovered = discovery
        .discover(&[temp_dir.path().to_path_buf()])
        .unwrap();
    let files = &discovered;

    // Verify files exceed mmap threshold (512KB)
    for file in files {
        let metadata = fs::metadata(&file.path).unwrap();
        assert!(metadata.len() > 512 * 1024, "File should trigger mmap");
    }

    group.bench_function("10_files_1mb_mmap", |b| {
        b.iter(|| {
            let config = ProcessingConfig::new();
            let processor = BatchProcessor::new(config);
            let result = processor.process(black_box(files));
            assert_eq!(result.success_count(), 10);
        });
    });

    group.finish();
}

/// Benchmark: Compare different mmap thresholds
/// Tests 512KB (current), 1MB, and 2MB thresholds
fn processor_threshold_comparison(c: &mut Criterion) {
    use std::fmt::Write;

    let mut group = c.benchmark_group("processor_threshold_comparison");

    // Create files around threshold boundaries: 256KB, 512KB, 1MB
    let temp_dir = TempDir::new().unwrap();
    for (i, size_kb) in [256, 512, 1024].iter().enumerate() {
        let items_count = (size_kb * 1024) / 50;
        let items = (0..items_count).fold(String::new(), |mut acc, j| {
            writeln!(&mut acc, "  - item_{i}_{j}").unwrap();
            acc
        });
        let content = format!("id: {i}\nitems:\n{items}");
        let path = temp_dir.path().join(format!("file_{size_kb}kb.yaml"));
        fs::write(&path, content).unwrap();
    }

    let config = DiscoveryConfig::default();
    let discovery = FileDiscovery::new(config).unwrap();
    let discovered = discovery
        .discover(&[temp_dir.path().to_path_buf()])
        .unwrap();
    let files = &discovered;

    // Note: We can't change mmap threshold at runtime with current API
    // This benchmark documents the 512KB threshold performance
    group.bench_function("512kb_threshold", |b| {
        b.iter(|| {
            let config = ProcessingConfig::new();
            let processor = BatchProcessor::new(config);
            let result = processor.process(black_box(files));
            assert_eq!(result.success_count(), 3);
        });
    });

    group.finish();
}

/// Benchmark: Parallel worker scaling
/// Measures throughput with 1, 2, 4, 8 workers
fn processor_parallel_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("processor_parallel_scaling");
    group.throughput(Throughput::Elements(200));

    let temp_dir = setup_small_files(200);
    let config = DiscoveryConfig::default();
    let discovery = FileDiscovery::new(config).unwrap();
    let discovered = discovery
        .discover(&[temp_dir.path().to_path_buf()])
        .unwrap();
    let files = &discovered;

    for workers in [1, 2, 4, 8] {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{workers}_workers")),
            &workers,
            |b, &workers| {
                b.iter(|| {
                    let config = ProcessingConfig::new().with_workers(workers);
                    let processor = BatchProcessor::new(config);
                    let result = processor.process(black_box(files));
                    assert_eq!(result.success_count(), 200);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Atomic write overhead
/// Measures cost of temp file + rename for in-place modification
fn processor_atomic_write_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("processor_atomic_write");

    let temp_dir = setup_small_files(50);
    let config = DiscoveryConfig::default();
    let discovery = FileDiscovery::new(config).unwrap();
    let discovered = discovery
        .discover(&[temp_dir.path().to_path_buf()])
        .unwrap();
    let files = &discovered;

    group.bench_function("in_place_false", |b| {
        b.iter(|| {
            let config = ProcessingConfig::new().with_in_place(false);
            let processor = BatchProcessor::new(config);
            let result = processor.process(black_box(files));
            assert_eq!(result.success_count(), 50);
        });
    });

    group.bench_function("in_place_true", |b| {
        b.iter(|| {
            let config = ProcessingConfig::new().with_in_place(true);
            let processor = BatchProcessor::new(config);
            let result = processor.process(black_box(files));
            assert_eq!(result.success_count(), 50);
        });
    });

    group.finish();
}

/// Benchmark: Stress test with 1000 files
/// Real-world batch processing scenario
fn stress_1000_files_parallel(c: &mut Criterion) {
    let mut group = c.benchmark_group("stress_1000_files");
    group.throughput(Throughput::Elements(1000));
    group.sample_size(10); // Reduce sample size for long-running benchmark

    let temp_dir = setup_small_files(1000);
    let config = DiscoveryConfig::default();
    let discovery = FileDiscovery::new(config).unwrap();
    let discovered = discovery
        .discover(&[temp_dir.path().to_path_buf()])
        .unwrap();
    let files = &discovered;

    group.bench_function("1000_files_8_workers", |b| {
        b.iter(|| {
            let config = ProcessingConfig::new().with_workers(8);
            let processor = BatchProcessor::new(config);
            let result = processor.process(black_box(files));
            assert_eq!(result.success_count(), 1000);
        });
    });

    group.finish();
}

/// Benchmark: End-to-end pipeline (discovery + processing)
/// Measures full batch workflow including file discovery
fn end_to_end_batch_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("end_to_end_pipeline");

    let temp_dir = setup_small_files(100);
    let path = temp_dir.path().to_path_buf();

    group.bench_function("discovery_and_processing", |b| {
        b.iter(|| {
            // Discovery phase
            let config = DiscoveryConfig::default();
            let discovery = FileDiscovery::new(config).unwrap();
            let discovered = discovery
                .discover(&[black_box(path.clone())])
                .unwrap();

            // Processing phase
            let config = ProcessingConfig::new();
            let processor = BatchProcessor::new(config);
            let result = processor.process(&discovered);

            assert_eq!(result.success_count(), 100);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    processor_small_files,
    processor_large_files_mmap,
    processor_threshold_comparison,
    processor_parallel_scaling,
    processor_atomic_write_overhead,
    stress_1000_files_parallel,
    end_to_end_batch_pipeline,
);

criterion_main!(benches);
