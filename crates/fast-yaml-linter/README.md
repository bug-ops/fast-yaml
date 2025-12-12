# fast-yaml-linter

YAML linter with rich diagnostics for the fast-yaml ecosystem.

## Features

- **Precise error locations**: Line, column, and byte offset tracking
- **Rich diagnostics**: Source context with highlighting
- **Pluggable rules**: Extensible rule system
- **Multiple output formats**: Text (rustc-style), JSON, SARIF
- **Zero-cost abstractions**: Efficient linting without double-parsing

## Usage

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

## Built-in Rules

- **duplicate-key** (ERROR): Detects duplicate keys in mappings
- **invalid-anchor** (ERROR): Detects undefined anchor references
- **indentation** (WARNING): Checks for consistent indentation
- **line-length** (INFO): Enforces maximum line length
- **trailing-whitespace** (HINT): Detects trailing whitespace

## Configuration

```rust
use fast_yaml_linter::{Linter, LintConfig};

let config = LintConfig::new()
    .with_max_line_length(Some(120))
    .with_indent_size(4)
    .with_disabled_rule("line-length");

let linter = Linter::with_config(config);
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

### SARIF

Full SARIF 2.1.0 output for IDE integration (requires `sarif-output` feature).

## Features

- `default`: No additional features
- `json-output`: Enable JSON formatter
- `sarif-output`: Enable SARIF formatter
- `all-formats`: Enable all output formats

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE))
- MIT license ([LICENSE-MIT](../../LICENSE-MIT))

at your option.
