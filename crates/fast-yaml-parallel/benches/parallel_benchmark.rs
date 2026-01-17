#![allow(missing_docs)]
//! Benchmarks for fast-yaml-parallel performance characteristics.
//!
//! These benchmarks measure:
//! - Parallel overhead vs sequential processing
//! - Scalability across document counts
//! - Thread pool creation vs global pool performance
//! - Large file processing efficiency
//! - Multi-file parallel processing (batch operations)

use std::fmt::Write;
use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use fast_yaml_core::Parser;
use fast_yaml_parallel::{ParallelConfig, parse_parallel, parse_parallel_with_config};
use rayon::prelude::*;

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

/// Generate a collection of separate YAML "files" for multi-file benchmarks.
///
/// Simulates a directory of YAML configuration files.
fn generate_yaml_files(file_count: usize, bytes_per_file: usize) -> Vec<String> {
    (0..file_count)
        .map(|i| {
            let mut yaml = String::with_capacity(bytes_per_file + 50);
            let _ = writeln!(yaml, "# File {i}");
            let _ = writeln!(yaml, "metadata:");
            let _ = writeln!(yaml, "  name: config_{i}");
            let _ = writeln!(yaml, "  version: 1.0.{i}");
            yaml.push_str("settings:\n");

            // Fill to approximate size
            let remaining = bytes_per_file.saturating_sub(100);
            let lines = remaining / 25;

            for j in 0..lines {
                let _ = writeln!(yaml, "  option_{j}: value_{j}");
            }

            yaml
        })
        .collect()
}

/// Benchmark: Multi-file parallel processing.
///
/// Compares sequential vs parallel processing of multiple separate YAML files.
/// This is the key differentiator vs yamlfmt which processes files sequentially.
///
/// Expected speedup: Near-linear with core count for I/O-bound workloads.
fn bench_multifile_parallel(c: &mut Criterion) {
    let mut group = c.benchmark_group("multifile");

    for file_count in [10, 50, 100, 500] {
        let files = generate_yaml_files(file_count, 500);

        // Sequential processing (like yamlfmt)
        group.bench_with_input(
            BenchmarkId::new("sequential", file_count),
            &files,
            |b, files| {
                b.iter(|| {
                    files
                        .iter()
                        .map(|f| Parser::parse_str(black_box(f)))
                        .collect::<Vec<_>>()
                });
            },
        );

        // Parallel processing with Rayon
        group.bench_with_input(
            BenchmarkId::new("parallel", file_count),
            &files,
            |b, files| {
                b.iter(|| {
                    files
                        .par_iter()
                        .map(|f| Parser::parse_str(black_box(f)))
                        .collect::<Vec<_>>()
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Multi-file with varying file sizes.
///
/// Tests parallel speedup across different file sizes.
/// Larger files = more CPU work = better parallel efficiency.
fn bench_multifile_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("multifile_sizes");

    let file_count = 100;

    for bytes_per_file in [100, 500, 2000, 10000] {
        let files = generate_yaml_files(file_count, bytes_per_file);
        let label = format!("{file_count}x{bytes_per_file}b");

        group.bench_with_input(
            BenchmarkId::new("sequential", &label),
            &files,
            |b, files| {
                b.iter(|| {
                    files
                        .iter()
                        .map(|f| Parser::parse_str(black_box(f)))
                        .collect::<Vec<_>>()
                });
            },
        );

        group.bench_with_input(BenchmarkId::new("parallel", &label), &files, |b, files| {
            b.iter(|| {
                files
                    .par_iter()
                    .map(|f| Parser::parse_str(black_box(f)))
                    .collect::<Vec<_>>()
            });
        });
    }

    group.finish();
}

/// Benchmark: Multi-file linting simulation.
///
/// Simulates linting a codebase: parse + validate each file.
/// This is the typical CI/CD use case where fast-yaml excels.
fn bench_multifile_lint(c: &mut Criterion) {
    let mut group = c.benchmark_group("multifile_lint");
    group.sample_size(20);

    // Simulate a medium project: 200 YAML files, ~1KB each
    let files = generate_yaml_files(200, 1000);

    group.bench_function("sequential_200files", |b| {
        b.iter(|| {
            files
                .iter()
                .filter_map(|f| Parser::parse_str(f).ok())
                .count()
        });
    });

    group.bench_function("parallel_200files", |b| {
        b.iter(|| {
            files
                .par_iter()
                .filter_map(|f| Parser::parse_str(f).ok())
                .count()
        });
    });

    // Large project: 1000 YAML files
    let large_files = generate_yaml_files(1000, 500);

    group.bench_function("sequential_1000files", |b| {
        b.iter(|| {
            large_files
                .iter()
                .filter_map(|f| Parser::parse_str(f).ok())
                .count()
        });
    });

    group.bench_function("parallel_1000files", |b| {
        b.iter(|| {
            large_files
                .par_iter()
                .filter_map(|f| Parser::parse_str(f).ok())
                .count()
        });
    });

    group.finish();
}

/// Benchmark: Thread count scaling for multi-file.
///
/// Tests how speedup scales with thread count for batch file processing.
fn bench_multifile_thread_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("multifile_threads");

    let files = generate_yaml_files(200, 1000);

    for threads in [1, 2, 4, 8] {
        group.bench_with_input(BenchmarkId::from_parameter(threads), &files, |b, files| {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap();

            b.iter(|| {
                pool.install(|| {
                    files
                        .par_iter()
                        .map(|f| Parser::parse_str(black_box(f)))
                        .collect::<Vec<_>>()
                })
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
    // Multi-file parallel processing benchmarks (key differentiator vs yamlfmt)
    bench_multifile_parallel,
    bench_multifile_sizes,
    bench_multifile_lint,
    bench_multifile_thread_scaling,
);

criterion_main!(benches);
