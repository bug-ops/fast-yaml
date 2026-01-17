//! Benchmarks comparing standard vs arena allocation for streaming formatter.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use fast_yaml_core::EmitterConfig;
use fast_yaml_core::streaming::{format_streaming, is_streaming_suitable};
use std::hint::black_box;

#[cfg(feature = "arena")]
use fast_yaml_core::streaming::format_streaming_arena;

fn generate_yaml(size: usize) -> String {
    (0..size)
        .map(|i| format!("key{i}: value{i}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn benchmark_streaming(c: &mut Criterion) {
    let sizes = [100, 500, 1000, 5000, 10000, 50000];
    let mut group = c.benchmark_group("streaming_formatter");

    for size in sizes {
        let yaml = generate_yaml(size);
        let config = EmitterConfig::default();

        // Only benchmark if streaming is suitable
        if !is_streaming_suitable(&yaml) {
            continue;
        }

        group.bench_with_input(BenchmarkId::new("standard", size), &yaml, |b, input| {
            b.iter(|| format_streaming(black_box(input), &config));
        });

        #[cfg(feature = "arena")]
        group.bench_with_input(BenchmarkId::new("arena", size), &yaml, |b, input| {
            b.iter(|| format_streaming_arena(black_box(input), &config));
        });
    }

    group.finish();
}

fn benchmark_with_anchors(c: &mut Criterion) {
    let mut group = c.benchmark_group("streaming_with_anchors");

    // Generate YAML with anchors and aliases
    let yaml: String = (0..100)
        .map(|i| {
            if i < 10 {
                format!("anchor{i}: &a{i} value{i}")
            } else {
                format!("ref{i}: *a{}", i % 10)
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let config = EmitterConfig::default();

    group.bench_function("standard", |b| {
        b.iter(|| format_streaming(black_box(&yaml), &config));
    });

    #[cfg(feature = "arena")]
    group.bench_function("arena", |b| {
        b.iter(|| format_streaming_arena(black_box(&yaml), &config));
    });

    group.finish();
}

fn benchmark_deeply_nested(c: &mut Criterion) {
    use std::fmt::Write;

    let mut group = c.benchmark_group("streaming_deeply_nested");

    // Generate deeply nested YAML
    let mut yaml = String::new();
    for i in 0..15 {
        let indent = "  ".repeat(i);
        writeln!(yaml, "{indent}level{i}:").unwrap();
    }
    let indent = "  ".repeat(15);
    for i in 0..100 {
        writeln!(yaml, "{indent}key{i}: value{i}").unwrap();
    }

    let config = EmitterConfig::default();

    group.bench_function("standard", |b| {
        b.iter(|| format_streaming(black_box(&yaml), &config));
    });

    #[cfg(feature = "arena")]
    group.bench_function("arena", |b| {
        b.iter(|| format_streaming_arena(black_box(&yaml), &config));
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_streaming,
    benchmark_with_anchors,
    benchmark_deeply_nested
);
criterion_main!(benches);
