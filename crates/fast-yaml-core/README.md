# fast-yaml-core

[![Crates.io](https://img.shields.io/crates/v/fast-yaml-core)](https://crates.io/crates/fast-yaml-core)
[![docs.rs](https://img.shields.io/docsrs/fast-yaml-core)](https://docs.rs/fast-yaml-core)
[![CI](https://img.shields.io/github/actions/workflow/status/bug-ops/fast-yaml/ci.yml?branch=main)](https://github.com/bug-ops/fast-yaml/actions)
[![MSRV](https://img.shields.io/crates/msrv/fast-yaml-core)](https://github.com/bug-ops/fast-yaml)
[![License](https://img.shields.io/crates/l/fast-yaml-core)](LICENSE-MIT)

Core YAML 1.2.2 parser and emitter for the fast-yaml ecosystem.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
fast-yaml-core = "0.1"
```

Or with cargo-add:

```bash
cargo add fast-yaml-core
```

> [!IMPORTANT]
> Requires Rust 1.88 or later.

## Usage

### Parsing YAML

```rust
use fast_yaml_core::Parser;

let yaml = "name: test\nvalue: 123";
let doc = Parser::parse_str(yaml)?;
```

### Emitting YAML

```rust
use fast_yaml_core::{Emitter, Value};

let value = Value::String("hello".to_string());
let yaml = Emitter::emit(&value)?;
```

## YAML 1.2.2 Compliance

This library implements the YAML 1.2.2 specification with the Core Schema:

| Type | Supported Values |
|------|------------------|
| Null | `~`, `null`, `Null`, `NULL`, empty |
| Boolean | `true`/`false` (case-insensitive) |
| Integer | Decimal, `0o` octal, `0x` hex |
| Float | Standard, `.inf`, `-.inf`, `.nan` |
| String | Plain, single/double-quoted, literal (`\|`), folded (`>`) |

> [!NOTE]
> YAML 1.1 booleans (`yes`/`no`/`on`/`off`) are treated as strings per YAML 1.2.2 spec.

## Related Crates

This crate is part of the [fast-yaml](https://github.com/bug-ops/fast-yaml) workspace:

- `fast-yaml-linter` — YAML linting with rich diagnostics
- `fast-yaml-parallel` — Multi-threaded YAML processing
- `fast-yaml-ffi` — FFI utilities for language bindings

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT) at your option.
