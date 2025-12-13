# fast-yaml-parallel

[![Crates.io](https://img.shields.io/crates/v/fast-yaml-parallel)](https://crates.io/crates/fast-yaml-parallel)
[![docs.rs](https://img.shields.io/docsrs/fast-yaml-parallel)](https://docs.rs/fast-yaml-parallel)
[![CI](https://img.shields.io/github/actions/workflow/status/bug-ops/fast-yaml/ci.yml?branch=main)](https://github.com/bug-ops/fast-yaml/actions)
[![MSRV](https://img.shields.io/crates/msrv/fast-yaml-parallel)](https://github.com/bug-ops/fast-yaml)
[![License](https://img.shields.io/crates/l/fast-yaml-parallel)](LICENSE-MIT)

Multi-threaded YAML processing with work-stealing parallelism.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
fast-yaml-parallel = "0.1"
```

Or with cargo-add:

```bash
cargo add fast-yaml-parallel
```

> [!IMPORTANT]
> Requires Rust 1.88 or later.

## Usage

### Basic Usage

```rust
use fast_yaml_parallel::parse_parallel;

let yaml = "---\nfoo: 1\n---\nbar: 2\n---\nbaz: 3";
let docs = parse_parallel(yaml)?;
assert_eq!(docs.len(), 3);
# Ok::<(), Box<dyn std::error::Error>>(())
```

### Custom Configuration

```rust
use fast_yaml_parallel::{parse_parallel_with_config, ParallelConfig};

let config = ParallelConfig::new()
    .with_thread_count(Some(8))
    .with_min_chunk_size(2048);

let yaml = "---\nfoo: 1\n---\nbar: 2";
let docs = parse_parallel_with_config(yaml, &config)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Performance

Expected speedup on multi-document YAML files:

| Cores | Speedup |
|-------|---------|
| 4 | 3-3.5x |
| 8 | 6-6.5x |
| 16 | 10-12x |

> [!TIP]
> Run benchmarks with `cargo bench -p fast-yaml-parallel` to measure performance on your hardware.

## When to Use

**Use parallel processing when:**
- Processing multi-document YAML streams (logs, configs, data dumps)
- Input size > 1MB with multiple documents
- Running on multi-core hardware (4+ cores recommended)

**Use sequential processing when:**
- Single document files
- Small files (<100KB)
- Memory constrained environments

## Configuration Options

| Option | Default | Description |
|--------|---------|-------------|
| `thread_count` | Auto (CPU cores) | Number of worker threads |
| `min_chunk_size` | 4KB | Minimum bytes per chunk |
| `max_chunk_size` | 10MB | Maximum bytes per chunk |
| `max_input_size` | 100MB | Maximum total input size |
| `max_documents` | 100,000 | Maximum documents to parse |

## Related Crates

This crate is part of the [fast-yaml](https://github.com/bug-ops/fast-yaml) workspace:

- `fast-yaml-core` — Core YAML 1.2.2 parser and emitter
- `fast-yaml-linter` — YAML linting with rich diagnostics
- `fast-yaml-ffi` — FFI utilities for language bindings

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT) at your option.
