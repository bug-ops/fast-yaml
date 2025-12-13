# fast-yaml

[![PyPI](https://img.shields.io/pypi/v/fast-yaml)](https://pypi.org/project/fast-yaml/)
[![Python](https://img.shields.io/pypi/pyversions/fast-yaml)](https://pypi.org/project/fast-yaml/)
[![License](https://img.shields.io/pypi/l/fast-yaml)](https://github.com/bug-ops/fast-yaml/blob/main/LICENSE-MIT)

A fast YAML 1.2.2 parser and linter for Python, powered by Rust.

## Installation

```bash
pip install fast-yaml
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
- **Linter** — Rich diagnostics with line/column tracking
- **Parallel processing** — Multi-threaded parsing for large files
- **Type stubs** — Full IDE support with `.pyi` files

## Documentation

See the [main repository](https://github.com/bug-ops/fast-yaml) for full documentation.

## License

Licensed under either of [Apache License, Version 2.0](../LICENSE-APACHE) or [MIT License](../LICENSE-MIT) at your option.
