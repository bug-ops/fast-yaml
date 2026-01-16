# fast-yaml

[![CI Status](https://img.shields.io/github/actions/workflow/status/bug-ops/fast-yaml/ci.yml?branch=main)](https://github.com/bug-ops/fast-yaml/actions)
[![Crates.io](https://img.shields.io/crates/v/fast-yaml-cli)](https://crates.io/crates/fast-yaml-cli)
[![docs.rs](https://img.shields.io/docsrs/fast-yaml-core)](https://docs.rs/fast-yaml-core)
[![PyPI](https://img.shields.io/pypi/v/fastyaml-rs)](https://pypi.org/project/fastyaml-rs/)
[![npm](https://img.shields.io/npm/v/fastyaml-rs)](https://www.npmjs.com/package/fastyaml-rs)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue)](LICENSE-MIT)

**High-performance YAML 1.2.2 parser for Python and Node.js, powered by Rust.**

Drop-in replacement for PyYAML and js-yaml with **5-10x faster** parsing. Full YAML 1.2.2 Core Schema compliance, comprehensive linting, and multi-threaded parallel processing.

> [!IMPORTANT]
> **YAML 1.2.2 Compliance** — Unlike PyYAML (YAML 1.1), `fast-yaml` follows the modern YAML 1.2.2 specification. This means `yes/no/on/off` are strings, not booleans.

## Installation

```bash
# Python
pip install fastyaml-rs

# Node.js
npm install fastyaml-rs

# CLI
cargo install fast-yaml-cli
```

<details>
<summary><b>Build from source</b></summary>

> [!WARNING]
> Requires Rust 1.88+, Python 3.10+ or Node.js 20+

```bash
git clone https://github.com/bug-ops/fast-yaml.git
cd fast-yaml

# Python
uv sync && uv run maturin develop

# Node.js
cd nodejs && npm install && npm run build
```

</details>

## Quick Start

### Python

```python
import fast_yaml

data = fast_yaml.safe_load("""
name: fast-yaml
features: [fast, safe, yaml-1.2.2]
""")

yaml_str = fast_yaml.safe_dump(data)
```

> [!TIP]
> Migrating from PyYAML? Just change your import: `import fast_yaml as yaml`

### Node.js

```typescript
import { safeLoad, safeDump } from 'fastyaml-rs';

const data = safeLoad(`name: fast-yaml`);
const yamlStr = safeDump(data);
```

### CLI

```bash
fy parse config.yaml           # Validate syntax
fy format -i config.yaml       # Format in-place
fy convert json config.yaml    # YAML → JSON
fy lint config.yaml            # Lint with diagnostics
```

## Features

- **5-10x Faster** — Rust-powered parsing outperforms PyYAML
- **YAML 1.2.2** — Full Core Schema compliance
- **Drop-in API** — Compatible with PyYAML/js-yaml
- **Linting** — Rich diagnostics with line/column tracking
- **Parallel** — Multi-threaded processing for large files
- **Safe** — Zero `unsafe` code, memory-safe Rust

<details>
<summary><b>Feature details</b></summary>

### Linting

```python
from fast_yaml._core.lint import lint

diagnostics = lint("key: value\nkey: duplicate")
for diag in diagnostics:
    print(f"{diag.severity}: {diag.message} at line {diag.span.start.line}")
```

### Parallel Processing

```python
from fast_yaml._core.parallel import parse_parallel, ParallelConfig

config = ParallelConfig(thread_count=4, max_input_size=100*1024*1024)
docs = parse_parallel(multi_doc_yaml, config)
```

> [!TIP]
> Parallel processing provides 3-6x speedup on 4-8 core systems for multi-document files.

</details>

## Performance

<details>
<summary><b>Benchmark results</b></summary>

### Python API vs PyYAML (Apple M2)

| File Size | PyYAML (pure) | PyYAML + libyaml | fast-yaml | Speedup |
|-----------|---------------|------------------|-----------|---------|
| Small (30B) | 50 μs | 10 μs | 5 μs | **10x / 2x** |
| Medium (2KB) | 2 ms | 400 μs | 150 μs | **13x / 2.7x** |
| Large (500KB) | 500 ms | 100 ms | 35 ms | **14x / 2.9x** |

### CLI vs yamlfmt (Apple M3 Pro)

| File Size | fast-yaml (fy) | yamlfmt | Winner |
|-----------|----------------|---------|--------|
| Small (480B) | **1.8 ms** | 2.9 ms | fast-yaml (1.6x faster) |
| Medium (45KB) | **2.7 ms** | 2.9 ms | fast-yaml (1.1x faster) |
| Large (459KB) | 9.1 ms | **3.0 ms** | yamlfmt (3.0x faster) |

```bash
# Run benchmarks
uv run pytest tests/ -v --benchmark-only
```

</details>

## YAML 1.2.2 Differences

<details>
<summary><b>Differences from PyYAML (YAML 1.1)</b></summary>

| Feature | PyYAML (YAML 1.1) | fast-yaml (YAML 1.2.2) |
|---------|-------------------|------------------------|
| `yes/no` | `True/False` | `"yes"/"no"` (strings) |
| `on/off` | `True/False` | `"on"/"off"` (strings) |
| `014` (octal) | `12` | `14` (decimal) |
| `0o14` (octal) | Error | `12` |

```python
fast_yaml.safe_load("yes")    # "yes" (string, not True!)
fast_yaml.safe_load("0o14")   # 12 (octal)
fast_yaml.safe_load("014")    # 14 (decimal, NOT octal!)
```

</details>

## API Reference

<details>
<summary><b>Loading YAML</b></summary>

```python
# Single document
data = fast_yaml.safe_load(yaml_string)

# Multiple documents
for doc in fast_yaml.safe_load_all(yaml_string):
    print(doc)

# PyYAML-compatible
data = fast_yaml.load(yaml_string, Loader=fast_yaml.SafeLoader)
```

</details>

<details>
<summary><b>Dumping YAML</b></summary>

```python
yaml_str = fast_yaml.safe_dump(data)

# With options
yaml_str = fast_yaml.dump(
    data,
    indent=2,
    width=80,
    explicit_start=True,
    sort_keys=False,
)

# Multiple documents
yaml_str = fast_yaml.safe_dump_all([doc1, doc2, doc3])
```

</details>

<details>
<summary><b>Type mappings</b></summary>

| YAML Type | Python Type |
|-----------|-------------|
| `null`, `~` | `None` |
| `true`, `false` | `bool` |
| `123`, `0x1F`, `0o17` | `int` |
| `1.23`, `.inf`, `.nan` | `float` |
| `"string"`, `'string'` | `str` |
| `[a, b, c]` | `list` |
| `{a: 1, b: 2}` | `dict` |

</details>

## Security

Input validation prevents denial-of-service attacks.

<details>
<summary><b>Security limits</b></summary>

| Limit | Default | Configurable |
|-------|---------|--------------|
| Max input size | 100 MB | Yes (up to 1GB) |
| Max documents | 100,000 | Yes (up to 10M) |
| Max threads | 128 | Yes |

</details>

## Project

<details>
<summary><b>Project structure</b></summary>

```
fast-yaml/
├── crates/
│   ├── fast-yaml-core/     # Core YAML parser/emitter
│   ├── fast-yaml-linter/   # Linting engine
│   ├── fast-yaml-parallel/ # Multi-threaded processing
│   └── fast-yaml-ffi/      # FFI utilities
├── python/                 # PyO3 Python bindings
├── nodejs/                 # NAPI-RS Node.js bindings
└── Cargo.toml             # Workspace manifest
```

</details>

<details>
<summary><b>Technology stack</b></summary>

| Component | Library |
|-----------|---------|
| YAML Parser | [saphyr](https://github.com/saphyr-rs/saphyr) |
| Python Bindings | [PyO3](https://pyo3.rs/) |
| Node.js Bindings | [NAPI-RS](https://napi.rs/) |
| Parallelism | [Rayon](https://github.com/rayon-rs/rayon) |

**Rust 2024 Edition** • **Python 3.10+** • **Node.js 20+**

</details>

## Contributing

Contributions welcome! All PRs must pass CI checks:

```bash
cargo +nightly fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
```

## FAQ

<details>
<summary><b>Why not just use PyYAML?</b></summary>

PyYAML is excellent. Use fast-yaml when you need performance (5-10x faster), YAML 1.2.2 compliance, built-in linting, or parallel processing.

</details>

<details>
<summary><b>Is this a drop-in replacement?</b></summary>

For `safe_*` functions, yes. Just change `import yaml` to `import fast_yaml as yaml`. Note that YAML 1.2.2 has different boolean/octal handling.

</details>

<details>
<summary><b>When should I use parallel processing?</b></summary>

Use `parse_parallel()` for multi-document YAML files (separated by `---`) larger than 1MB with 4+ CPU cores. For single documents, use `safe_load()`.

</details>

## License

Licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.
