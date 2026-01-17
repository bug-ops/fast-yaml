//! Performance benchmarks for file discovery module.

#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use fast_yaml_cli::batch::discovery::{DiscoveryConfig, FileDiscovery};
use std::fs;
use std::hint::black_box;
use tempfile::TempDir;

/// Create a test directory with specified structure.
fn setup_test_directory(file_count: usize, depth: usize, files_per_dir: usize) -> TempDir {
    let temp = TempDir::new().unwrap();

    if depth == 1 {
        // Flat structure
        for i in 0..file_count {
            let file = temp.path().join(format!("file{i}.yaml"));
            fs::write(&file, format!("key: value{i}")).unwrap();
        }
    } else {
        // Nested structure
        let dirs_per_level = (file_count as f64 / files_per_dir as f64).ceil() as usize;
        create_nested_structure(temp.path(), depth, dirs_per_level, files_per_dir, &mut 0);
    }

    temp
}

fn create_nested_structure(
    base: &std::path::Path,
    depth: usize,
    dirs_per_level: usize,
    files_per_dir: usize,
    counter: &mut usize,
) {
    if depth == 0 {
        return;
    }

    // Create files in current directory
    for i in 0..files_per_dir {
        let file = base.join(format!("file{counter}_{i}.yaml"));
        fs::write(&file, format!("key: value{counter}")).unwrap();
    }

    if depth > 1 {
        // Create subdirectories
        for i in 0..dirs_per_level {
            let dir = base.join(format!("dir{counter}_{i}"));
            fs::create_dir(&dir).unwrap();
            *counter += 1;
            create_nested_structure(&dir, depth - 1, dirs_per_level, files_per_dir, counter);
        }
    }
}

fn bench_discover_small(c: &mut Criterion) {
    let temp = setup_test_directory(10, 1, 10);
    let config = DiscoveryConfig::default();
    let discovery = FileDiscovery::new(config).unwrap();
    let paths = vec![temp.path().to_path_buf()];

    c.bench_function("discover_10_files_flat", |b| {
        b.iter(|| {
            let result = discovery.discover(black_box(&paths)).unwrap();
            black_box(result);
        });
    });
}

fn bench_discover_medium(c: &mut Criterion) {
    let temp = setup_test_directory(100, 3, 10);
    let config = DiscoveryConfig::default();
    let discovery = FileDiscovery::new(config).unwrap();
    let paths = vec![temp.path().to_path_buf()];

    c.bench_function("discover_100_files_nested", |b| {
        b.iter(|| {
            let result = discovery.discover(black_box(&paths)).unwrap();
            black_box(result);
        });
    });
}

fn bench_discover_large(c: &mut Criterion) {
    let temp = setup_test_directory(1000, 4, 10);
    let config = DiscoveryConfig::default();
    let discovery = FileDiscovery::new(config).unwrap();
    let paths = vec![temp.path().to_path_buf()];

    c.bench_function("discover_1000_files_nested", |b| {
        b.iter(|| {
            let result = discovery.discover(black_box(&paths)).unwrap();
            black_box(result);
        });
    });
}

fn bench_discover_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("discover_scaling");

    for file_count in [10, 50, 100, 500, 1000] {
        let temp = setup_test_directory(file_count, 3, 10);
        let config = DiscoveryConfig::default();
        let discovery = FileDiscovery::new(config).unwrap();
        let paths = vec![temp.path().to_path_buf()];

        group.bench_with_input(
            BenchmarkId::from_parameter(file_count),
            &file_count,
            |b, _| {
                b.iter(|| {
                    let result = discovery.discover(black_box(&paths)).unwrap();
                    black_box(result);
                });
            },
        );
    }

    group.finish();
}

fn bench_pattern_matching(c: &mut Criterion) {
    let temp = setup_test_directory(100, 1, 100);

    let mut group = c.benchmark_group("pattern_matching");

    // Benchmark with different numbers of patterns
    for pattern_count in [2, 10, 50] {
        let mut patterns = vec!["*.yaml".to_string(), "*.yml".to_string()];

        // Add more patterns
        for i in 2..pattern_count {
            patterns.push(format!("*.test{i}"));
        }

        let config = DiscoveryConfig::default().with_include_patterns(patterns);
        let discovery = FileDiscovery::new(config).unwrap();
        let paths = vec![temp.path().to_path_buf()];

        group.bench_with_input(
            BenchmarkId::from_parameter(pattern_count),
            &pattern_count,
            |b, _| {
                b.iter(|| {
                    let result = discovery.discover(black_box(&paths)).unwrap();
                    black_box(result);
                });
            },
        );
    }

    group.finish();
}

fn bench_deep_nesting(c: &mut Criterion) {
    let mut group = c.benchmark_group("deep_nesting");

    for depth in [2, 5, 10] {
        let temp = setup_test_directory(100, depth, 5);
        let config = DiscoveryConfig::default();
        let discovery = FileDiscovery::new(config).unwrap();
        let paths = vec![temp.path().to_path_buf()];

        group.bench_with_input(BenchmarkId::from_parameter(depth), &depth, |b, _| {
            b.iter(|| {
                let result = discovery.discover(black_box(&paths)).unwrap();
                black_box(result);
            });
        });
    }

    group.finish();
}

fn bench_deduplication(c: &mut Criterion) {
    let temp = setup_test_directory(100, 1, 100);
    let config = DiscoveryConfig::default();
    let discovery = FileDiscovery::new(config).unwrap();

    // Create duplicate paths (same files referenced multiple times)
    let base_path = temp.path().to_path_buf();
    let mut paths = Vec::new();
    for _ in 0..10 {
        paths.push(base_path.clone());
    }

    c.bench_function("deduplication_100_files_10x", |b| {
        b.iter(|| {
            let result = discovery.discover(black_box(&paths)).unwrap();
            black_box(result);
        });
    });
}

fn bench_should_include(c: &mut Criterion) {
    let config = DiscoveryConfig::default();
    let discovery = FileDiscovery::new(config).unwrap();

    c.bench_function("should_include_match", |b| {
        b.iter(|| {
            let result = discovery.should_include(black_box(std::path::Path::new("test.yaml")));
            black_box(result);
        });
    });

    c.bench_function("should_include_no_match", |b| {
        b.iter(|| {
            let result = discovery.should_include(black_box(std::path::Path::new("test.txt")));
            black_box(result);
        });
    });
}

fn bench_exclude_patterns(c: &mut Criterion) {
    let temp = setup_test_directory(100, 3, 10);

    // Create vendor directory
    let vendor = temp.path().join("vendor");
    fs::create_dir(&vendor).unwrap();
    for i in 0..50 {
        fs::write(vendor.join(format!("vendor{i}.yaml")), "data: 1").unwrap();
    }

    let config = DiscoveryConfig::default().with_exclude_patterns(vec!["**/vendor/**".to_string()]);
    let discovery = FileDiscovery::new(config).unwrap();
    let paths = vec![temp.path().to_path_buf()];

    c.bench_function("exclude_vendor_directory", |b| {
        b.iter(|| {
            let result = discovery.discover(black_box(&paths)).unwrap();
            black_box(result);
        });
    });
}

fn bench_glob_initialization(c: &mut Criterion) {
    c.bench_function("globset_build_2_patterns", |b| {
        b.iter(|| {
            let config = DiscoveryConfig::default();
            let discovery = FileDiscovery::new(black_box(config)).unwrap();
            black_box(discovery);
        });
    });

    c.bench_function("globset_build_50_patterns", |b| {
        b.iter(|| {
            let mut patterns = Vec::new();
            for i in 0..50 {
                patterns.push(format!("*.test{i}"));
            }
            let config = DiscoveryConfig::default().with_include_patterns(patterns);
            let discovery = FileDiscovery::new(black_box(config)).unwrap();
            black_box(discovery);
        });
    });
}

criterion_group!(
    benches,
    bench_discover_small,
    bench_discover_medium,
    bench_discover_large,
    bench_discover_scaling,
    bench_pattern_matching,
    bench_deep_nesting,
    bench_deduplication,
    bench_should_include,
    bench_exclude_patterns,
    bench_glob_initialization,
);

criterion_main!(benches);
