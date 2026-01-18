# fast-yaml-core

[![Crates.io](https://img.shields.io/crates/v/fast-yaml-core)](https://crates.io/crates/fast-yaml-core)
[![docs.rs](https://img.shields.io/docsrs/fast-yaml-core)](https://docs.rs/fast-yaml-core)
[![CI](https://img.shields.io/github/actions/workflow/status/bug-ops/fast-yaml/ci.yml?branch=main)](https://github.com/bug-ops/fast-yaml/actions)
[![MSRV](https://img.shields.io/crates/msrv/fast-yaml-core)](https://github.com/bug-ops/fast-yaml)
[![License](https://img.shields.io/crates/l/fast-yaml-core)](LICENSE-MIT)

Core YAML 1.2.2 parser and emitter for the fast-yaml ecosystem.

> [!NOTE]
> This crate provides three distinct components: **Parser** (YAML → data), **Emitter** (data → YAML), and **Streaming Formatter** (events → YAML, no DOM).

## Components

### Parser

**Purpose**: Deserialize YAML text into Rust data structures (DOM).

**When to use**:
- Need to manipulate YAML data programmatically
- Building APIs that consume YAML config
- Validating YAML structure

**Data flow**: `YAML text → Value (DOM)`

### Emitter

**Purpose**: Serialize Rust data structures back to YAML text.

**When to use**:
- Generating YAML from code
- Config file generation
- Data export

**Data flow**: `Value (DOM) → YAML text`

**Configuration**: `EmitterConfig` allows customizing indent, line width, flow style, etc.

### Streaming Formatter (feature: `streaming`)

**Purpose**: Format YAML directly from parser events without building DOM.

**When to use**:
- Formatting large files (faster, less memory)
- CLI tools (convert, format, validate)
- Processing YAML streams

**Data flow**: `Parser events → YAML text` (zero-copy)

**Advantages**:
- ⚡ 2-3x faster than parse + emit
- 📉 O(1) memory vs O(n) for DOM
- 🎯 Ideal for batch operations

> [!TIP]
> Use **Streaming Formatter** for CLI batch mode formatting. Use **Parser + Emitter** when you need to modify YAML data.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
fast-yaml-core = "0.3"
```

Or with cargo-add:

```bash
cargo add fast-yaml-core
```

> [!IMPORTANT]
> Requires Rust 1.88 or later.

## Usage

### Parser: YAML → Data Structures

```rust
use fast_yaml_core::{Parser, Value};

// Parse single document
let yaml = "name: test\nvalue: 123";
let doc = Parser::parse_str(yaml)?;

// Parse multiple documents
let yaml = "---\nfoo: 1\n---\nbar: 2";
let docs = Parser::parse_all_str(yaml)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

### Emitter: Data Structures → YAML

```rust
use fast_yaml_core::{Emitter, EmitterConfig, Value, Map, ScalarOwned};

// Basic emission
let value = Value::Value(ScalarOwned::String("hello".to_string()));
let yaml = Emitter::emit_str(&value)?;

// Custom configuration
let config = EmitterConfig::new()
    .with_indent(4)
    .with_width(120)
    .with_explicit_start(true);
let emitter = Emitter::new(config);
let yaml = emitter.emit_str(&value)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

### Streaming Formatter: Events → YAML (no DOM)

```rust
#[cfg(feature = "streaming")]
use fast_yaml_core::streaming::{format_yaml, FormatterBackend};

#[cfg(feature = "streaming")]
{
    let yaml = "name: test\nvalue: 123";

    // Format without building DOM (faster, less memory)
    let formatted = format_yaml(yaml)?;

    // Result: properly formatted YAML
    assert!(formatted.contains("name: test"));
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

> [!TIP]
> Enable the `streaming` feature for formatter: `fast-yaml-core = { version = "0.4", features = ["streaming"] }`

## YAML 1.2.2 Compliance

This library implements the YAML 1.2.2 specification with the Core Schema:

| Type | Supported Values |
|------|------------------|
| Null | `~`, `null`, empty |
| Boolean | `true`/`false` (lowercase only per YAML 1.2 Core Schema) |
| Integer | Decimal, `0o` octal, `0x` hex |
| Float | Standard, `.inf`, `-.inf`, `.nan` |
| String | Plain, single/double-quoted, literal (`\|`), folded (`>`) |

> [!NOTE]
> YAML 1.1 booleans (`yes`/`no`/`on`/`off`) are treated as strings per YAML 1.2.2 spec.

## Features

| Feature | Description | Use Case |
|---------|-------------|----------|
| `streaming` | Event-based formatting without DOM | CLI tools, large file processing |
| `arena` | Arena-based memory allocation | High-performance parsing |

```toml
# Enable streaming formatter
fast-yaml-core = { version = "0.4", features = ["streaming"] }

# Enable arena allocation
fast-yaml-core = { version = "0.4", features = ["arena"] }

# Enable both
fast-yaml-core = { version = "0.4", features = ["streaming", "arena"] }
```

> [!TIP]
> The `arena` feature provides 10-15% faster parsing for large documents by reducing allocator overhead.

## Related Crates

This crate is part of the [fast-yaml](https://github.com/bug-ops/fast-yaml) workspace:

- `fast-yaml-linter` — YAML linting with rich diagnostics
- `fast-yaml-parallel` — Multi-threaded YAML processing

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT) at your option.
