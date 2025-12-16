# fast-yaml-linter

[![Crates.io](https://img.shields.io/crates/v/fast-yaml-linter)](https://crates.io/crates/fast-yaml-linter)
[![docs.rs](https://img.shields.io/docsrs/fast-yaml-linter)](https://docs.rs/fast-yaml-linter)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue)](../../LICENSE-MIT)

YAML linter with rich diagnostics for the fast-yaml ecosystem.

## Features

- **Precise error locations**: Line, column, and byte offset tracking
- **Rich diagnostics**: Source context with highlighting
- **Pluggable rules**: Extensible rule system
- **Multiple output formats**: Text (rustc-style), JSON
- **Zero-cost abstractions**: Efficient linting without double-parsing

## Rust Usage

```rust
use fast_yaml_linter::{Linter, TextFormatter, Formatter};

let yaml = r#"
name: John
age: 30
"#;

// Create linter with all default rules
let linter = Linter::with_all_rules();

// Run linter
let diagnostics = linter.lint(yaml)?;

// Format output
let formatter = TextFormatter::with_color_auto();
let output = formatter.format(&diagnostics, yaml);
println!("{}", output);
```

## Python Usage

> [!NOTE]
> Python bindings are available through the `fastyaml-rs` package on PyPI.

```python
from fast_yaml._core.lint import lint, Linter, LintConfig, TextFormatter, Severity

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

# Access severity levels
Severity.ERROR    # Critical errors
Severity.WARNING  # Potential issues
Severity.INFO     # Informational
Severity.HINT     # Suggestions
```

## Built-in Rules

| Rule | Severity | Description |
|------|----------|-------------|
| `duplicate-key` | ERROR | Detects duplicate keys in mappings |
| `line-length` | INFO | Enforces maximum line length |

## Configuration

### Rust

```rust
use fast_yaml_linter::{Linter, LintConfig};

let config = LintConfig::new()
    .with_max_line_length(Some(120))
    .with_indent_size(4)
    .with_disabled_rule("line-length");

let linter = Linter::with_config(config);
```

### Python

```python
from fast_yaml._core.lint import LintConfig, Linter

config = LintConfig(
    max_line_length=120,
    indent_size=4,
    require_document_start=False,
    require_document_end=False,
    allow_duplicate_keys=False,
    disabled_rules={"line-length"},
)

linter = Linter(config)
```

## Output Formats

### Text (rustc-style)

```
error[duplicate-key]: duplicate key 'name' found
  --> example.yaml:10:5
   |
10 | name: value
   |       ^^^^^ duplicate key defined here
```

### JSON

```json
[
  {
    "code": "duplicate-key",
    "severity": "error",
    "message": "duplicate key 'name' found",
    "span": {
      "start": { "line": 10, "column": 5, "offset": 145 },
      "end": { "line": 10, "column": 9, "offset": 149 }
    }
  }
]
```

## Cargo Features

| Feature | Description |
|---------|-------------|
| `default` | No additional features |
| `json-output` | Enable JSON formatter |

## Diagnostic Types

The linter provides rich diagnostic information:

```python
# Python
diagnostic.code        # Rule code (e.g., "duplicate-key")
diagnostic.severity    # Severity level
diagnostic.message     # Error message
diagnostic.span        # Location span
diagnostic.span.start  # Start location (line, column, offset)
diagnostic.span.end    # End location
diagnostic.context     # Source context (optional)
diagnostic.suggestions # Fix suggestions (optional)
```

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE))
- MIT license ([LICENSE-MIT](../../LICENSE-MIT))

at your option.
