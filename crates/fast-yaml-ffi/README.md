# fast-yaml-ffi

[![Crates.io](https://img.shields.io/crates/v/fast-yaml-ffi)](https://crates.io/crates/fast-yaml-ffi)
[![docs.rs](https://img.shields.io/docsrs/fast-yaml-ffi)](https://docs.rs/fast-yaml-ffi)
[![CI](https://img.shields.io/github/actions/workflow/status/bug-ops/fast-yaml/ci.yml?branch=main)](https://github.com/bug-ops/fast-yaml/actions)
[![MSRV](https://img.shields.io/crates/msrv/fast-yaml-ffi)](https://github.com/bug-ops/fast-yaml)
[![License](https://img.shields.io/crates/l/fast-yaml-ffi)](LICENSE-MIT)

FFI utilities for fast-yaml language bindings.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
fast-yaml-ffi = "0.3"
```

Or with cargo-add:

```bash
cargo add fast-yaml-ffi
```

> [!IMPORTANT]
> Requires Rust 1.88 or later.

## Overview

This crate provides generic traits and utilities for converting between Rust types and foreign types (Python via PyO3, Node.js via NAPI-RS).

### Core Traits

- `ToFfi<T>` — Convert Rust types to foreign types
- `FromFfi<T>` — Convert foreign types to Rust types

## Usage

```rust,ignore
use fast_yaml_ffi::{ToFfi, FromFfi};

// Convert Rust value to Python object
let py_obj = rust_value.to_ffi()?;

// Convert Python object to Rust value
let rust_value = RustType::from_ffi(&py_obj)?;
```

> [!NOTE]
> This crate is primarily used internally by `fast-yaml` (Python) and `fast-yaml-nodejs` bindings.

## Related Crates

This crate is part of the [fast-yaml](https://github.com/bug-ops/fast-yaml) workspace:

- `fast-yaml-core` — Core YAML 1.2.2 parser and emitter
- `fast-yaml-linter` — YAML linting with rich diagnostics
- `fast-yaml-parallel` — Multi-threaded YAML processing

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT) at your option.
