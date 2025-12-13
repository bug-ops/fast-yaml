#![allow(missing_docs)]
//! Benchmarks for fast-yaml-parallel performance characteristics.
//!
//! These benchmarks measure:
//! - Parallel overhead vs sequential processing
//! - Scalability across document counts
//! - Thread pool creation vs global pool performance
//! - Large file processing efficiency

use std::fmt::Write;
use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use fast_yaml_parallel::{ParallelConfig, parse_parallel, parse_parallel_with_config};

/// Generate multi-document YAML with specified document count and size per document.
fn generate_yaml_docs(doc_count: usize, bytes_per_doc: usize) -> String {
    let mut yaml = String::with_capacity(doc_count * (bytes_per_doc + 10));

    for i in 0..doc_count {
        yaml.push_str("---\n");
        let _ = writeln!(yaml, "id: {i}");
        yaml.push_str("data:\n");

        // Fill to approximate size
        let remaining = bytes_per_doc.saturating_sub(20);
        let lines = remaining / 20;

        for j in 0..lines {
            let _ = writeln!(yaml, "  field_{j}: value_{j}");
        }
    }

    yaml
}

/// Benchmark: Parallel overhead on small workloads.
///
/// Measures the overhead of using parallel processing on small inputs
/// where sequential would be faster. This validates the fallback threshold.
fn bench_parallel_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("overhead");

    // Very small: should use sequential fallback
    let yaml_small = generate_yaml_docs(2, 50);

    group.bench_function("small_2docs_sequential", |b| {
        let config = ParallelConfig::new().with_thread_count(Some(0));
        b.iter(|| parse_parallel_with_config(black_box(&yaml_small), black_box(&config)));
    });

    group.bench_function("small_2docs_parallel", |b| {
        b.iter(|| parse_parallel(black_box(&yaml_small)));
    });

    // Medium: tests threshold decision
    let yaml_medium = generate_yaml_docs(10, 200);

    group.bench_function("medium_10docs_sequential", |b| {
        let config = ParallelConfig::new().with_thread_count(Some(0));
        b.iter(|| parse_parallel_with_config(black_box(&yaml_medium), black_box(&config)));
    });

    group.bench_function("medium_10docs_parallel", |b| {
        b.iter(|| parse_parallel(black_box(&yaml_medium)));
    });

    group.finish();
}

/// Benchmark: Scalability across document counts.
///
/// Tests how performance scales with increasing document count.
/// Expected: Near-linear speedup up to core count, then diminishing returns.
fn bench_scalability(c: &mut Criterion) {
    let mut group = c.benchmark_group("scalability");

    for doc_count in [10, 50, 100, 500, 1000] {
        let yaml = generate_yaml_docs(doc_count, 100);

        group.bench_with_input(
            BenchmarkId::new("sequential", doc_count),
            &yaml,
            |b, yaml| {
                let config = ParallelConfig::new().with_thread_count(Some(0));
                b.iter(|| parse_parallel_with_config(black_box(yaml), black_box(&config)));
            },
        );

        group.bench_with_input(BenchmarkId::new("parallel", doc_count), &yaml, |b, yaml| {
            b.iter(|| parse_parallel(black_box(yaml)));
        });
    }

    group.finish();
}

/// Benchmark: Thread pool configuration strategies.
///
/// Compares global pool (optimized, no creation overhead) vs custom pool.
fn bench_thread_pool_strategies(c: &mut Criterion) {
    let mut group = c.benchmark_group("thread_pool");

    let yaml = generate_yaml_docs(100, 200);

    group.bench_function("global_pool", |b| {
        // Uses global pool by default
        b.iter(|| parse_parallel(black_box(&yaml)));
    });

    group.bench_function("custom_pool_same_size", |b| {
        // Forces custom pool creation (but same thread count)
        let config = ParallelConfig::new().with_thread_count(Some(num_cpus::get()));
        b.iter(|| parse_parallel_with_config(black_box(&yaml), black_box(&config)));
    });

    group.bench_function("custom_pool_4threads", |b| {
        // Custom pool with 4 threads
        let config = ParallelConfig::new().with_thread_count(Some(4));
        b.iter(|| parse_parallel_with_config(black_box(&yaml), black_box(&config)));
    });

    group.finish();
}

/// Benchmark: Document size variation.
///
/// Tests performance with different document sizes to find optimal chunk size.
fn bench_document_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("document_sizes");

    for bytes_per_doc in [50, 200, 1000, 5000, 20000] {
        let yaml = generate_yaml_docs(100, bytes_per_doc);

        group.bench_with_input(
            BenchmarkId::new("parallel", bytes_per_doc),
            &yaml,
            |b, yaml| {
                b.iter(|| parse_parallel(black_box(yaml)));
            },
        );
    }

    group.finish();
}

/// Benchmark: Large file processing.
///
/// Tests performance on larger files (MB scale) to validate memory efficiency.
fn bench_large_files(c: &mut Criterion) {
    let mut group = c.benchmark_group("large_files");
    group.sample_size(10); // Fewer iterations for large benchmarks

    // 1MB: ~10000 documents × 100 bytes
    let yaml_1mb = generate_yaml_docs(10000, 100);

    group.bench_function("1mb_sequential", |b| {
        let config = ParallelConfig::new().with_thread_count(Some(0));
        b.iter(|| parse_parallel_with_config(black_box(&yaml_1mb), black_box(&config)));
    });

    group.bench_function("1mb_parallel", |b| {
        b.iter(|| parse_parallel(black_box(&yaml_1mb)));
    });

    // 5MB: ~50000 documents × 100 bytes
    let yaml_5mb = generate_yaml_docs(50000, 100);

    group.bench_function("5mb_sequential", |b| {
        let config = ParallelConfig::new().with_thread_count(Some(0));
        b.iter(|| parse_parallel_with_config(black_box(&yaml_5mb), black_box(&config)));
    });

    group.bench_function("5mb_parallel", |b| {
        b.iter(|| parse_parallel(black_box(&yaml_5mb)));
    });

    group.finish();
}

/// Benchmark: Chunking overhead.
///
/// Measures just the chunking algorithm performance (document boundary detection).
fn bench_chunking(c: &mut Criterion) {
    let mut group = c.benchmark_group("chunking");

    for doc_count in [10, 100, 1000, 10000] {
        let yaml = generate_yaml_docs(doc_count, 100);

        group.bench_with_input(BenchmarkId::from_parameter(doc_count), &yaml, |b, yaml| {
            b.iter(|| {
                // Parse with chunking
                let _docs = parse_parallel(black_box(yaml));
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_parallel_overhead,
    bench_scalability,
    bench_thread_pool_strategies,
    bench_document_sizes,
    bench_large_files,
    bench_chunking,
);

criterion_main!(benches);
