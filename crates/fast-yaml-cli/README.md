# fy

[![Crates.io](https://img.shields.io/crates/v/fast-yaml-cli)](https://crates.io/crates/fast-yaml-cli)
[![CI](https://img.shields.io/github/actions/workflow/status/bug-ops/fast-yaml/ci.yml?branch=main)](https://github.com/bug-ops/fast-yaml/actions)
[![License](https://img.shields.io/crates/l/fast-yaml-cli)](LICENSE)
[![MSRV](https://img.shields.io/badge/MSRV-1.88.0-blue)](https://blog.rust-lang.org/)

Fast YAML command-line processor with validation and linting. Built on [fast-yaml](https://github.com/bug-ops/fast-yaml) for high-performance YAML 1.2.2 processing.

## Installation

### From crates.io

```bash
cargo install fast-yaml-cli
```

> [!TIP]
> Use `cargo binstall fast-yaml-cli` for faster installation without compilation.

### From source

```bash
git clone https://github.com/bug-ops/fast-yaml
cd fast-yaml
cargo install --path crates/fast-yaml-cli
```

### Pre-built binaries

Download from [GitHub Releases](https://github.com/bug-ops/fast-yaml/releases/latest):

| Platform | Architecture | Download |
|----------|--------------|----------|
| Linux | x86_64 | [fy-x86_64-unknown-linux-gnu.tar.gz](https://github.com/bug-ops/fast-yaml/releases/latest) |
| Linux | aarch64 | [fy-aarch64-unknown-linux-gnu.tar.gz](https://github.com/bug-ops/fast-yaml/releases/latest) |
| macOS | x86_64 | [fy-x86_64-apple-darwin.tar.gz](https://github.com/bug-ops/fast-yaml/releases/latest) |
| macOS | aarch64 | [fy-aarch64-apple-darwin.tar.gz](https://github.com/bug-ops/fast-yaml/releases/latest) |
| Windows | x86_64 | [fy-x86_64-pc-windows-msvc.zip](https://github.com/bug-ops/fast-yaml/releases/latest) |

## Usage

```bash
fy [OPTIONS] [FILE] <COMMAND>
```

### Parse and validate

```bash
# Parse from file
fy parse config.yaml

# Parse from stdin
echo "name: test" | fy parse

# Show parse statistics
fy parse --stats large.yaml
```

### Format YAML

```bash
# Format to stdout
fy format messy.yaml

# Custom indentation (2-8 spaces)
fy format --indent 4 --width 100 config.yaml

# Format in-place
fy format -i config.yaml
```

### Convert formats

```bash
# YAML to JSON
fy convert json config.yaml > config.json

# JSON to YAML
fy convert yaml data.json > data.yaml

# Compact JSON (no pretty-print)
fy convert json --pretty=false app.yaml
```

### Lint YAML

```bash
# Lint with default rules
fy lint config.yaml

# Custom rules
fy lint --max-line-length 100 --indent-size 2 app.yaml

# JSON output for IDE integration
fy lint --format json config.yaml
```

## Commands

| Command | Description |
|---------|-------------|
| `parse` | Parse and validate YAML syntax |
| `format` | Format YAML with consistent style |
| `convert` | Convert between YAML and JSON |
| `lint` | Lint YAML with diagnostics |

## Options

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--in-place` | `-i` | Edit file in-place | - |
| `--output` | `-o` | Write to file | stdout |
| `--format` | `-f` | Output format (yaml/json/compact) | yaml |
| `--no-color` | - | Disable colored output | - |
| `--quiet` | `-q` | Suppress non-error output | - |
| `--verbose` | `-v` | Enable verbose output | - |

## Features

| Feature | Default | Description |
|---------|---------|-------------|
| `colors` | Yes | Colored terminal output |
| `linter` | Yes | YAML linting capabilities |
| `all` | - | All features enabled |

Build with minimal features:

```bash
cargo build --release --no-default-features
```

> [!NOTE]
> The `linter` feature adds the `lint` command. Without it, only `parse`, `format`, and `convert` are available.

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Parse error |
| 2 | Lint errors found |
| 3 | I/O error |
| 4 | Invalid arguments |

## Examples

### Pipeline usage

```bash
# Validate all YAML files
find . -name "*.yaml" -exec fy parse {} \;

# Format and convert in one pipeline
cat config.yaml | fy format | fy convert json > config.json

# Check YAML before committing
git diff --name-only --cached -- '*.yaml' | xargs -I {} fy lint {}
```

### CI/CD integration

```yaml
# GitHub Actions
- name: Validate YAML
  run: |
    cargo install fast-yaml-cli
    find . -name "*.yaml" -exec fy lint {} \;
```

## Performance

Built on `fast-yaml-core` for optimal performance:

- Startup time: ~5ms
- Binary size: ~1MB (stripped)
- Full YAML 1.2.2 compliance

## License

Licensed under [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE) at your option.
