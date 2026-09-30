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

## Large Integers

Decimal integers beyond the i64 range load as exact Python `int`s and dump back exactly. Loading such a literal
with more digits than `sys.get_int_max_str_digits()` (4300 by default; sign and leading zeros are not counted), or dumping an `int` with more digits,
raises CPython's `ValueError`; raise the limit with `sys.set_int_max_str_digits()`. Literals that fit i64 never
hit this limit. Hex and octal integers are capped at 14284 bits (at most 4300 decimal digits) and are not
subject to the limit. PyYAML reads leading-zero literals differently (`0012` is octal there).

## Parse Limits

Nesting depth and alias expansion are capped by default. Raise or lower the caps with keyword arguments on
`safe_load`, `safe_load_all`, `load`, `load_all`, and the `ParallelConfig`, `LintConfig`, and `BatchConfig`
constructors (each config also has `with_max_depth()` / `with_max_alias_bytes()`; `None` resets to the default).
`LintConfig` also accepts `max_input_bytes` / `with_max_input_bytes()`:

```python
fast_yaml.safe_load(text, max_depth=512, max_alias_bytes=256 * 1024 * 1024)
```

| Option | Default | Range |
|--------|---------|-------|
| `max_depth` | 256 | 1..=512 |
| `max_alias_bytes` | 64 MiB | 1..=1 GiB |
| `max_input_bytes` (`LintConfig` only) | 100 MiB | 1..=1 GiB |

Out-of-range values raise `ValueError`; non-integers (including `bool`) raise `TypeError`.
Depth 512 needs about 1 MiB of thread stack (up to 983 KiB measured in release builds) and can abort the process on stacks of 512 KiB or less; the default of 256 is safe.
The dumper keeps a fixed depth of 256, so data parsed deeper may fail to dump.
The alias budget is per stream, so parallel and batch runs can use up to workers x budget.
`max_input_bytes` bounds linting work on oversized input; the source is already in memory when checked, so it is not a memory bound.

## Numeric Keys

YAML treats `1`, `true` and `1.0` as three different keys, but a Python `dict` or `set` considers them equal.
Instead of silently dropping one, `safe_load` raises `ValueError` with the key's line and column when a mapping or `!!set` holds such keys (`parse_parallel` raises the same error without a position); repeating a key of the same type is still an ordinary duplicate (last value wins).
This differs from PyYAML, which keeps only one of them.
`.nan` keys collapse to a single entry, as in the Rust core.

```python
fast_yaml.safe_load("1: a\ntrue: b\n")
# ValueError: YAML parse error: bool key true is distinct in YAML but equal as a
#   Python dict key to a key of type int at line 2, column 1
```

## Features

- **YAML 1.2.2 compliant** — Full Core Schema support
- **Fast** — 5-10x faster than PyYAML
- **PyYAML compatible** — Drop-in replacement with `load`, `dump`, `Loader`, `Dumper` classes
- **Linter** — Rich diagnostics with line/column tracking; supports inline `# fy: disable` / `disable-line` / `disable-file` (and `# yamllint ...`) suppression comments
- **Parallel processing** — Multi-threaded parsing for large files
- **Batch processing** — Process multiple files in parallel
- **Type stubs** — Full IDE support with `.pyi` files

## Linter Configuration

Per-rule severity and options are passed through `rules`; the option keys are the same kebab-case keys as in the `fy` config file, and errors use the same messages as `fy lint --config`:

```python
from fast_yaml._core import lint

config = lint.LintConfig(
    rules={
        "line-length": {"max": 120, "severity": "error"},
        "quoted-strings": {"quote-type": "double", "required": True},
        "document-start": {"present": True},
        "duplicate-key": "warning",
    },
)
diagnostics = lint.lint("a: 1\n", config)

# Unknown rules, option keys, wrong types and invalid severities raise ValueError
lint.LintConfig(rules={"quoted-strings": {"quote-type": "singel"}})
```

Keyword arguments are applied first, then `rules`, then `disabled_rules` (which always wins).
`max_line_length=None` (or `{"line-length": {"max": None}}`) removes the line length limit.

## Batch Processing

Process multiple YAML files in parallel:

```python
from fast_yaml._core import batch

# Parse multiple files
result = batch.process_files(
    [
        "config1.yaml",
        "config2.yaml",
        "config3.yaml",
    ]
)
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
| `max_depth` | 256 | Maximum nesting depth, 1..=512 (`process_files` only) |
| `max_alias_bytes` | 64 MiB | Alias-expansion budget per file, 1..=1 GiB (`process_files` only) |

`format_files` ignores `max_depth` and `max_alias_bytes`; formatter depth is fixed at 256.

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
