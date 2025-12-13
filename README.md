# fast-yaml

[![CI Status](https://img.shields.io/github/actions/workflow/status/bug-ops/fast-yaml/ci.yml?branch=main)](https://github.com/bug-ops/fast-yaml/actions)
[![PyPI](https://img.shields.io/pypi/v/fast-yaml)](https://pypi.org/project/fast-yaml/)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue)](LICENSE-MIT)
[![Rust](https://img.shields.io/badge/rust-1.85+-orange.svg)](https://www.rust-lang.org)
[![Python](https://img.shields.io/badge/python-3.8+-blue.svg)](https://python.org)
[![unsafe forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg)](https://github.com/rust-secure-code/safety-dance/)

**High-performance YAML 1.2.2 parser for Python, powered by Rust.**

Drop-in replacement for PyYAML's `safe_*` functions with **5-10x faster** parsing through Rust's `yaml-rust2` library. Full YAML 1.2.2 Core Schema compliance, comprehensive linting, and multi-threaded parallel processing.

> [!IMPORTANT]
> **YAML 1.2.2 Compliance** — Unlike PyYAML (YAML 1.1), `fast-yaml` follows the modern YAML 1.2.2 specification. This means `yes/no/on/off` are strings, not booleans, and octal numbers require `0o` prefix.

## Features

### Core Parser

- **YAML 1.2.2 Core Schema** — Full specification compliance ([yaml.org/spec/1.2.2](https://yaml.org/spec/1.2.2/))
- **5-10x Faster** — Rust-powered parsing outperforms pure Python PyYAML
- **2-3x Faster** — Even beats PyYAML with libyaml C extension
- **Drop-in Replacement** — Compatible `safe_load`, `safe_dump`, `safe_load_all`, `safe_dump_all`
- **Type-safe** — Full Python type hints with `.pyi` stubs

### Linter

- **Rich Diagnostics** — Precise line, column, and byte offset tracking
- **Multiple Rules** — Duplicate keys, line length, indentation validation
- **Pluggable System** — Extensible rule architecture for custom validation
- **Multiple Formats** — Text and JSON output for IDE integration

### Parallel Processing

- **Multi-threaded Parsing** — Rayon-based parallel document processing
- **Automatic Chunking** — Intelligent document boundary detection
- **Configurable** — Thread count, chunk sizes, and resource limits
- **DoS Protection** — Input size and document count limits

### Architecture

- **Zero `unsafe` Code** — Memory-safe Rust with `#![forbid(unsafe_code)]`
- **Modular Workspace** — Separate crates for core, linter, parallel, and FFI
- **Cross-platform** — Pre-built wheels for Linux, macOS, Windows
- **GIL Release** — Python GIL released during CPU-intensive operations

## Installation

### Python Package

```bash
pip install fast-yaml
```

### Build from Source

> [!WARNING]
> **Requires Rust 1.85+** (2024 edition) and Python 3.8+. Install Rust via [rustup.rs](https://rustup.rs/)

<details>
<summary><b>Using uv (Recommended)</b></summary>

```bash
git clone https://github.com/bug-ops/fast-yaml.git
cd fast-yaml

# Install dependencies and build
uv sync
uv run maturin develop

# Run tests
uv run pytest tests/ -v
cargo nextest run --workspace
```

</details>

<details>
<summary><b>Using pip</b></summary>

```bash
git clone https://github.com/bug-ops/fast-yaml.git
cd fast-yaml

# Create virtual environment
python -m venv .venv
source .venv/bin/activate  # or .venv\Scripts\activate on Windows

# Install build tools
pip install maturin pytest

# Build and install
maturin develop

# Run tests
pytest tests/ -v
cargo nextest run --workspace
```

</details>

## Quick Start

```python
import fast_yaml

# Parse YAML
data = fast_yaml.safe_load("""
name: fast-yaml
version: 0.1.0
features:
  - fast
  - safe
  - yaml-1.2.2
""")

print(data)
# {'name': 'fast-yaml', 'version': '0.1.0', 'features': ['fast', 'safe', 'yaml-1.2.2']}

# Serialize to YAML
yaml_str = fast_yaml.safe_dump(data)
print(yaml_str)
```

> [!TIP]
> **Migrating from PyYAML?** Just change your import: `import fast_yaml as yaml`

## API Reference

### Loading YAML

```python
# Load single document
data = fast_yaml.safe_load(yaml_string)

# Load multiple documents
for doc in fast_yaml.safe_load_all(yaml_string):
    print(doc)
```

### Dumping YAML

```python
# Dump to string
yaml_str = fast_yaml.safe_dump(data)

# With options
yaml_str = fast_yaml.safe_dump(
    data,
    allow_unicode=True,  # allow unicode chars (default)
    sort_keys=False,     # preserve key order (default)
)

# Dump multiple documents
yaml_str = fast_yaml.safe_dump_all([doc1, doc2, doc3])
```

> [!NOTE]
> The `allow_unicode` parameter is accepted for PyYAML API compatibility. yaml-rust2 always outputs unicode characters.

### Parallel Processing

For large multi-document YAML files, use parallel processing:

```python
from fast_yaml._core.parallel import parse_parallel, ParallelConfig

# Parse multi-document YAML in parallel
yaml_content = """
---
doc: 1
---
doc: 2
---
doc: 3
"""

# Default configuration (auto-detect thread count)
docs = parse_parallel(yaml_content)

# Custom configuration
config = ParallelConfig(
    thread_count=4,           # Number of threads (None = auto)
    max_input_size=100*1024*1024,  # 100MB limit
    max_documents=100_000,    # Document count limit
)
docs = parse_parallel(yaml_content, config)
```

> [!TIP]
> Parallel processing provides **3-6x speedup** on 4-8 core systems for files with multiple documents.

### Linting

Validate YAML with rich diagnostics:

```python
from fast_yaml._core.lint import lint, Linter, LintConfig, TextFormatter

# Quick lint
diagnostics = lint("key: value\nkey: duplicate")

for diag in diagnostics:
    print(f"{diag.severity}: {diag.message}")
    print(f"  at line {diag.span.start.line}, column {diag.span.start.column}")

# Custom configuration
config = LintConfig(
    max_line_length=120,
    indent_size=2,
    allow_duplicate_keys=False,
)
linter = Linter(config)
diagnostics = linter.lint(yaml_source)

# Format output
formatter = TextFormatter(use_colors=True)
print(formatter.format(diagnostics, yaml_source))
```

**Available severity levels:**

```python
from fast_yaml._core.lint import Severity

Severity.ERROR    # Critical errors
Severity.WARNING  # Potential issues
Severity.INFO     # Informational
Severity.HINT     # Suggestions
```

## YAML 1.2.2 Differences

`fast-yaml` implements **YAML 1.2.2 Core Schema**, which differs from PyYAML's YAML 1.1:

| Feature | PyYAML (YAML 1.1) | fast-yaml (YAML 1.2.2) |
|---------|-------------------|------------------------|
| `yes/no` | `True/False` | `"yes"/"no"` (strings) |
| `on/off` | `True/False` | `"on"/"off"` (strings) |
| `014` (octal) | `12` | `14` (decimal) |
| `0o14` (octal) | Error | `12` |
| `.inf` | `inf` | `inf` |
| `.nan` | `nan` | `nan` |

### Examples

```python
# Booleans — only true/false
fast_yaml.safe_load("true")   # True
fast_yaml.safe_load("false")  # False
fast_yaml.safe_load("yes")    # "yes" (string!)
fast_yaml.safe_load("no")     # "no" (string!)

# Octal numbers — require 0o prefix
fast_yaml.safe_load("0o14")   # 12 (octal)
fast_yaml.safe_load("014")    # 14 (decimal, NOT octal!)

# Special floats
fast_yaml.safe_load(".inf")   # float('inf')
fast_yaml.safe_load("-.inf")  # float('-inf')
fast_yaml.safe_load(".nan")   # float('nan')

# Null values
fast_yaml.safe_load("~")      # None
fast_yaml.safe_load("null")   # None
```

## Performance

Benchmarks on typical YAML workloads (Apple M2):

| File Size | PyYAML (pure) | PyYAML + libyaml | fast-yaml | Speedup |
|-----------|---------------|------------------|-----------|---------|
| Small (30B) | 50 μs | 10 μs | 5 μs | **10x / 2x** |
| Medium (2KB) | 2 ms | 400 μs | 150 μs | **13x / 2.7x** |
| Large (500KB) | 500 ms | 100 ms | 35 ms | **14x / 2.9x** |

Run benchmarks yourself:

```bash
uv run pytest tests/ -v --benchmark-only
```

## Security

> [!CAUTION]
> Input validation is enforced to prevent denial-of-service attacks.

| Limit | Default | Configurable |
|-------|---------|--------------|
| Max input size | 100 MB | Yes (up to 1GB) |
| Max documents | 100,000 | Yes (up to 10M) |
| Max threads | 128 | Yes |

## Supported Types

| YAML Type | Python Type |
|-----------|-------------|
| `null`, `~` | `None` |
| `true`, `false` | `bool` |
| `123`, `0x1F`, `0o17` | `int` |
| `1.23`, `.inf`, `.nan` | `float` |
| `"string"`, `'string'` | `str` |
| `[a, b, c]` | `list` |
| `{a: 1, b: 2}` | `dict` |

## Project Structure

```
fast-yaml/
├── crates/
│   ├── fast-yaml-core/     # Core YAML parser/emitter
│   ├── fast-yaml-linter/   # Linting engine with diagnostics
│   ├── fast-yaml-parallel/ # Multi-threaded processing
│   └── fast-yaml-ffi/      # FFI utilities for bindings
├── python/                 # PyO3 Python bindings
└── Cargo.toml             # Workspace manifest
```

## Technology Stack

| Component | Library | Version |
|-----------|---------|---------|
| **YAML Parser** | [yaml-rust2](https://github.com/Ethiraric/yaml-rust2) | 0.10 |
| **Python Bindings** | [PyO3](https://pyo3.rs/) | 0.27 |
| **Parallelism** | [Rayon](https://github.com/rayon-rs/rayon) | 1.10 |
| **Error Handling** | [thiserror](https://crates.io/crates/thiserror) | 2.0 |
| **Build Tool** | [maturin](https://maturin.rs/) | 1.7+ |

**Project Metrics**:

- **Language**: Rust 2024 Edition
- **MSRV**: 1.85.0
- **Python**: 3.8+
- **Crates**: 5 (core, linter, parallel, ffi, python)
- **Tests**: 234+ (Rust) + Python test suite

## Current Status

> [!NOTE]
> **Phase 4 Complete!** Full Python bindings for linter and parallel processing are now available.

### ✅ Phase 1: Core Parser (Complete)

- YAML 1.2.2 Core Schema parser
- Python bindings with PyO3
- `safe_load`, `safe_dump`, `safe_load_all`, `safe_dump_all`
- Full type hints and documentation

### ✅ Phase 2: Linter (Complete)

- Rich diagnostic system with source context
- Linting rules (duplicate keys, line length, indentation)
- Multiple output formats (text, JSON)
- Pluggable rule architecture

### ✅ Phase 3: Parallel Processing (Complete)

- Multi-threaded document chunking with Rayon
- Intelligent document boundary detection
- Configurable thread pool and resource limits
- DoS protection (input size, document count limits)

### ✅ Phase 4: Python Integration (Complete)

- Linter Python API with full diagnostics
- Parallel processing Python bindings
- GIL release during CPU-intensive operations
- Comprehensive type stubs

### 💡 Phase 5: NodeJS Bindings (Future)

- NAPI-RS NodeJS bindings
- NPM package with pre-built binaries
- TypeScript type definitions

## Contributing

Contributions welcome!

> [!CAUTION]
> **Quality Standards**: All PRs must pass formatting, linting, and tests. CI enforces these automatically.

```bash
# Quality check pipeline
cargo +nightly fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
cargo deny check
```

## FAQ

<details>
<summary><b>Why not just use PyYAML?</b></summary>

PyYAML is excellent and battle-tested. Use `fast-yaml` when you need:

- **Performance**: 5-10x faster parsing for large files
- **YAML 1.2.2**: Modern spec compliance (PyYAML uses YAML 1.1)
- **Linting**: Built-in validation with rich diagnostics
- **Parallelism**: Multi-threaded processing for large files

</details>

<details>
<summary><b>Is this a drop-in replacement for PyYAML?</b></summary>

For `safe_*` functions, yes. Just change `import yaml` to `import fast_yaml as yaml`.

Note: YAML 1.2.2 has different boolean/octal handling than YAML 1.1.

</details>

<details>
<summary><b>Why Rust instead of C?</b></summary>

- Memory safety without runtime overhead
- No `unsafe` code in the entire codebase
- Modern tooling (cargo, clippy, rustfmt)
- Excellent Python bindings via PyO3

</details>

<details>
<summary><b>When should I use parallel processing?</b></summary>

Use `parse_parallel()` when:

- Processing multi-document YAML files (separated by `---`)
- File size exceeds 1MB
- You have 4+ CPU cores available

For single-document YAML or small files, use `safe_load()`.

</details>

## Acknowledgments

- [yaml-rust2](https://github.com/Ethiraric/yaml-rust2) — Rust YAML parser foundation
- [PyO3](https://pyo3.rs/) — Rust bindings for Python
- [Rayon](https://github.com/rayon-rs/rayon) — Data parallelism library
- [maturin](https://maturin.rs/) — Build tool for Rust Python extensions

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.
