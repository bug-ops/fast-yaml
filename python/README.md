# fastyaml-rs

[![PyPI](https://img.shields.io/pypi/v/fastyaml-rs)](https://pypi.org/project/fastyaml-rs/)
[![Python](https://img.shields.io/pypi/pyversions/fastyaml-rs)](https://pypi.org/project/fastyaml-rs/)
[![License](https://img.shields.io/pypi/l/fastyaml-rs)](https://github.com/bug-ops/fast-yaml/blob/main/LICENSE-MIT)

A fast YAML 1.2.2 parser and linter for Python, powered by Rust.

> [!IMPORTANT]
> Requires Python 3.10 or later.

## Installation

```bash
pip install fastyaml-rs
```

## Usage

```python
import fast_yaml

# Parse YAML
data = fast_yaml.safe_load("name: test\nvalue: 123")
print(data)  # {'name': 'test', 'value': 123}

# Dump YAML
yaml_str = fast_yaml.safe_dump({"name": "test", "value": 123})
print(yaml_str)  # name: test\nvalue: 123\n
```

## Features

- **YAML 1.2.2 compliant** — Full Core Schema support
- **Fast** — 5-10x faster than PyYAML
- **PyYAML compatible** — Drop-in replacement with `load`, `dump`, `Loader`, `Dumper` classes
- **Linter** — Rich diagnostics with line/column tracking
- **Parallel processing** — Multi-threaded parsing for large files
- **Batch processing** — Process multiple files in parallel
- **Type stubs** — Full IDE support with `.pyi` files

## Batch Processing

Process multiple YAML files in parallel:

```python
from fast_yaml._core import batch

# Parse multiple files
result = batch.process_files([
    "config1.yaml",
    "config2.yaml",
    "config3.yaml",
])
print(f"Processed {result.total} files, {result.failed} failed")

# With configuration
config = batch.BatchConfig(workers=4, indent=2)
result = batch.process_files(paths, config)
```

### Format Files

```python
# Dry-run: get formatted content without writing
results = batch.format_files(["config.yaml"])
for path, content, error in results:
    if content:
        print(f"{path}: {len(content)} bytes")

# In-place: format and write back
result = batch.format_files_in_place(["config.yaml"])
print(f"Changed {result.changed} files")
```

### BatchConfig Options

| Option | Default | Description |
|--------|---------|-------------|
| `workers` | Auto | Number of worker threads |
| `mmap_threshold` | 512 KB | Mmap threshold for large files |
| `max_input_size` | 100 MB | Maximum file size |
| `indent` | 2 | Indentation width |
| `width` | 80 | Line width |
| `sort_keys` | False | Sort dictionary keys |

### BatchResult

```python
result = batch.process_files(paths)
print(f"Total: {result.total}")
print(f"Success: {result.success}")
print(f"Changed: {result.changed}")
print(f"Failed: {result.failed}")
print(f"Duration: {result.duration_ms}ms")
print(f"Files/sec: {result.files_per_second()}")

for path, error in result.errors():
    print(f"Error in {path}: {error}")
```

## Documentation

See the [main repository](https://github.com/bug-ops/fast-yaml) for full documentation.

## License

Licensed under either of [Apache License, Version 2.0](../LICENSE-APACHE) or [MIT License](../LICENSE-MIT) at your option.
