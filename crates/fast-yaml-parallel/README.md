# fast-yaml-parallel

[![Crates.io](https://img.shields.io/crates/v/fast-yaml-parallel)](https://crates.io/crates/fast-yaml-parallel)
[![docs.rs](https://img.shields.io/docsrs/fast-yaml-parallel)](https://docs.rs/fast-yaml-parallel)
[![CI](https://img.shields.io/github/actions/workflow/status/bug-ops/fast-yaml/ci.yml?branch=main)](https://github.com/bug-ops/fast-yaml/actions)
[![MSRV](https://img.shields.io/crates/msrv/fast-yaml-parallel)](https://github.com/bug-ops/fast-yaml)
[![License](https://img.shields.io/crates/l/fast-yaml-parallel)](LICENSE-MIT)

Multi-threaded YAML processing with work-stealing parallelism.

## Features

This crate provides two types of parallelism:

| Type | API | Use Case |
|------|-----|----------|
| **Document-level** | `parse_parallel()` | Parse multi-document YAML streams (single file with `---` separators) |
| **File-level** | `FileProcessor` | Process multiple YAML files in parallel |

## Installation

```toml
[dependencies]
fast-yaml-parallel = "0.5"
```

Or with cargo-add:

```bash
cargo add fast-yaml-parallel
```

> [!IMPORTANT]
> Requires Rust 1.88 or later.

## Usage

### Document-Level Parallelism

Parse a single file containing multiple `---` separated documents:

```rust
use fast_yaml_parallel::parse_parallel;

let yaml = "---\nfoo: 1\n---\nbar: 2\n---\nbaz: 3";
let docs = parse_parallel(yaml)?;
assert_eq!(docs.len(), 3);
# Ok::<(), Box<dyn std::error::Error>>(())
```

With custom configuration:

```rust
use fast_yaml_parallel::{parse_parallel_with_config, Config};

let config = Config::new()
    .with_workers(Some(8))              // 8 threads
    .with_sequential_threshold(2048);   // Skip parallelism for small inputs

let yaml = "---\nfoo: 1\n---\nbar: 2";
let docs = parse_parallel_with_config(yaml, &config)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

### File-Level Parallelism

Process multiple YAML files in parallel:

```rust
use std::path::PathBuf;
use fast_yaml_parallel::{FileProcessor, Config, BatchResult};

let files = vec![
    PathBuf::from("config1.yaml"),
    PathBuf::from("config2.yaml"),
    PathBuf::from("config3.yaml"),
];

// Parse all files
let result: BatchResult = FileProcessor::new().parse_files(&files);
println!("Processed {} files, {} failed", result.total, result.failed);

// With custom configuration
let config = Config::new().with_workers(Some(4));
let processor = FileProcessor::with_config(config);
let result = processor.parse_files(&files);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Format files in place:

```rust
use std::path::PathBuf;
use fast_yaml_parallel::FileProcessor;
use fast_yaml_core::emitter::EmitterConfig;

let files = vec![PathBuf::from("config.yaml")];
let emitter_config = EmitterConfig::new().with_indent(2).with_width(80);

let processor = FileProcessor::new();
let result = processor.format_in_place(&files, &emitter_config);

println!("Changed {} files", result.changed);
# Ok::<(), Box<dyn std::error::Error>>(())
```

### Convenience Function

Quick file processing without creating a processor:

```rust
use std::path::PathBuf;
use fast_yaml_parallel::process_files;

let files = vec![PathBuf::from("config.yaml")];
let result = process_files(&files);
assert!(result.is_success());
```

## Configuration

| Option | Default | Description |
|--------|---------|-------------|
| `workers` | Auto (CPU cores) | Number of worker threads. `None` = auto, `Some(0)` = sequential |
| `mmap_threshold` | 512 KB | Use memory-mapped I/O for files larger than this |
| `max_input_size` | 100 MB | Maximum input size (DoS protection) |
| `sequential_threshold` | 4 KB | Skip parallelism for inputs smaller than this |

### Configuration Example

```rust
use fast_yaml_parallel::Config;

let config = Config::new()
    .with_workers(Some(8))              // 8 threads
    .with_mmap_threshold(1024 * 1024)   // 1 MB mmap threshold
    .with_max_input_size(50 * 1024 * 1024) // 50 MB max
    .with_sequential_threshold(8192);   // 8 KB sequential threshold
```

## Result Types

### BatchResult

Aggregated result from file processing:

```rust
pub struct BatchResult {
    pub total: usize,      // Total files processed
    pub success: usize,    // Successfully processed
    pub changed: usize,    // Files modified (for format_in_place)
    pub failed: usize,     // Failed files
    pub duration: Duration, // Processing time
    pub errors: Vec<(PathBuf, Error)>, // Error details
}

impl BatchResult {
    pub fn is_success(&self) -> bool;
    pub fn files_per_second(&self) -> f64;
}
```

### FileOutcome

Outcome for individual file:

```rust
pub enum FileOutcome {
    Success { duration: Duration },
    Changed { duration: Duration },
    Unchanged { duration: Duration },
    Error { error: Error, duration: Duration },
}
```

## Performance

Expected speedup on multi-core systems:

| Cores | Document-Level | File-Level |
|-------|----------------|------------|
| 4 | 3-3.5x | 3-4x |
| 8 | 6-6.5x | 6-8x |
| 16 | 10-12x | 12-15x |

> [!TIP]
> Run benchmarks with `cargo bench -p fast-yaml-parallel` to measure on your hardware.

## Related Crates

This crate is part of the [fast-yaml](https://github.com/bug-ops/fast-yaml) workspace:

- `fast-yaml-core` — Core YAML 1.2.2 parser and emitter
- `fast-yaml-linter` — YAML linting with rich diagnostics

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT) at your option.
