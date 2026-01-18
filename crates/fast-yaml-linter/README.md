# fast-yaml-linter

[![Crates.io](https://img.shields.io/crates/v/fast-yaml-linter)](https://crates.io/crates/fast-yaml-linter)
[![docs.rs](https://img.shields.io/docsrs/fast-yaml-linter)](https://docs.rs/fast-yaml-linter)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue)](../../LICENSE-MIT)

YAML linter with rich diagnostics for the fast-yaml ecosystem.

> [!NOTE]
> This crate provides two distinct components: **Linter** (validates YAML against rules) and **Diagnostic Formatters** (render diagnostics for display).

## Components

### Linter

**Purpose**: Validate YAML against configurable rules and generate diagnostics.

**Data flow**: `YAML text → Vec<Diagnostic>`

**Built-in rules** (21 total):
- `duplicate-key` — Detect duplicate keys in mappings
- `line-length` — Enforce maximum line length
- `trailing-whitespace` — Detect trailing whitespace
- And 18 more...

### Diagnostic Formatters

**Purpose**: Convert diagnostics into human-readable or machine-readable formats.

**Data flow**: `Vec<Diagnostic> → Formatted output`

**Available formatters**:
- **TextFormatter** — rustc-style output with colors
- **JsonFormatter** — JSON format for IDE/CI integration
- **SarifFormatter** — SARIF format for code analysis tools

> [!TIP]
> **Linter vs Formatter**: Linter validates YAML (what's wrong), Formatters display results (how to show it).

## Features

- **Precise error locations**: Line, column, and byte offset tracking
- **Rich diagnostics**: Source context with highlighting
- **Pluggable rules**: Extensible rule system
- **Multiple output formats**: Text (rustc-style), JSON, SARIF
- **Zero-cost abstractions**: Efficient linting without double-parsing
- **Pre-parsed documents**: `lint_value()` accepts already-parsed YAML to avoid double parsing

## Rust Usage

### Complete Pipeline: YAML → Linter → Diagnostics → Formatter → Output

```rust
use fast_yaml_linter::{Linter, TextFormatter, Formatter};

let yaml = r#"
name: John
age: 30
name: duplicate  # Error: duplicate key
"#;

// Step 1: Create linter with rules
let linter = Linter::with_all_rules();

// Step 2: Run linter (YAML → Vec<Diagnostic>)
let diagnostics = linter.lint(yaml)?;

// Step 3: Format diagnostics (Vec<Diagnostic> → String)
let formatter = TextFormatter::with_color_auto();
let output = formatter.format(&diagnostics, yaml);

// Step 4: Display output
println!("{}", output);
// Output:
// error[duplicate-key]: duplicate key 'name' found
//   --> input:4:1
//    |
//  4 | name: duplicate
//    | ^^^^ duplicate key defined here
# Ok::<(), Box<dyn std::error::Error>>(())
```

### Avoid Double Parsing with `lint_value()`

```rust
use fast_yaml_linter::Linter;
use fast_yaml_core::Parser;

let yaml = "name: test";

// Parse once
let value = Parser::parse_str(yaml)?;

// Lint the pre-parsed value (no re-parsing)
let linter = Linter::with_all_rules();
let diagnostics = linter.lint_value(yaml, &value);
# Ok::<(), Box<dyn std::error::Error>>(())
```

> [!TIP]
> Use `lint_value()` when you already have a parsed document to avoid parsing twice.

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

The linter includes 21+ rules covering syntax, style, and best practices:

**Document Structure:**
- `document-start` — Enforce `---` document start marker
- `document-end` — Enforce `...` document end marker
- `new-line-at-end-of-file` — Require newline at EOF

**Keys and Values:**
- `duplicate-keys` — Detect duplicate keys (ERROR)
- `empty-values` — Flag empty values
- `key-ordering` — Enforce alphabetical key ordering

**Formatting:**
- `line-length` — Enforce maximum line length
- `indentation` — Check consistent indentation
- `trailing-whitespace` — Detect trailing whitespace
- `empty-lines` — Control empty line usage
- `new-lines` — Enforce newline rules

**Flow Collections:**
- `braces` — Brace spacing in flow mappings `{a: 1}`
- `brackets` — Bracket spacing in flow sequences `[1, 2]`
- `commas` — Comma placement and spacing
- `colons` — Colon spacing after keys

**Values:**
- `truthy` — Detect ambiguous boolean values (`yes`/`no`)
- `quoted-strings` — Enforce string quoting style
- `float-values` — Validate float formatting
- `octal-values` — Detect octal notation

**Comments:**
- `comments` — Comment formatting rules
- `comments-indentation` — Comment indentation

**Anchors & Aliases:**
- `invalid-anchors` — Validate anchor/alias usage

> [!NOTE]
> All rules are configurable. Disable specific rules via `LintConfig::with_disabled_rule("rule-name")`.

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

Each formatter converts `Vec<Diagnostic>` to a specific format:

### TextFormatter (rustc-style, for humans)

```rust
use fast_yaml_linter::TextFormatter;

let formatter = TextFormatter::new().with_color(true);
let output = formatter.format(&diagnostics, yaml);
```

**Output**:
```
error[duplicate-key]: duplicate key 'name' found
  --> example.yaml:10:5
   |
10 | name: value
   |       ^^^^^ duplicate key defined here
```

### JsonFormatter (for IDEs/CI)

```rust
use fast_yaml_linter::JsonFormatter;

let formatter = JsonFormatter::new();
let json = formatter.format(&diagnostics, yaml);
```

**Output**:
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

### SarifFormatter (for code analysis tools)

```rust
use fast_yaml_linter::SarifFormatter;

let formatter = SarifFormatter::new();
let sarif = formatter.format(&diagnostics, yaml);
// SARIF 2.1.0 compatible output
```

> [!NOTE]
> JsonFormatter requires the `json-output` feature. SarifFormatter requires `sarif-output` feature.

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
