# fy

[![Crates.io](https://img.shields.io/crates/v/fast-yaml-cli)](https://crates.io/crates/fast-yaml-cli)
[![CI](https://img.shields.io/github/actions/workflow/status/bug-ops/fast-yaml/ci.yml?branch=main)](https://github.com/bug-ops/fast-yaml/actions)
[![License](https://img.shields.io/crates/l/fast-yaml-cli)](LICENSE)
[![MSRV](https://img.shields.io/badge/MSRV-1.91.0-blue)](https://blog.rust-lang.org/)

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

### Verify installation

```bash
fy --version
fy --help
```

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

**Single file:**

```bash
# Format to stdout
fy format messy.yaml

# Custom indentation (1-9 spaces)
fy format --indent 4 --width 100 config.yaml

# Format in-place
fy format -i config.yaml
```

**Batch mode** (directories, globs, multiple files):

```bash
# Format entire directory (recursive)
fy format -i src/

# Format with glob pattern
fy format -i "configs/**/*.yaml"

# Multiple files
fy format -i file1.yaml file2.yaml file3.yaml

# Non-recursive directory formatting
fy format -i --no-recursive configs/

# Parallel processing with 8 workers
fy format -i -j 8 large-project/

# Read file paths from stdin
find . -name "*.yaml" | fy format -i --stdin-files

# Changed YAML files only (deleted and non-YAML entries are errors)
git diff --name-only --diff-filter=d -- '*.yaml' '*.yml' | fy format -i --stdin-files
```

> [!TIP]
> Batch mode automatically detects when processing multiple files, directories, or glob patterns. Use `-j N` to control parallelism (default: auto-detect CPU cores).

**Include/exclude patterns:**

```bash
# Format all YAML except tests
fy format -i --exclude "**/tests/**" configs/

# Format only production configs
fy format -i --include "prod-*.yaml" configs/

# Combine patterns (respects .gitignore by default)
fy format -i --include "*.yaml" --exclude "draft-*" ./
```

**Dry run and output control:**

```bash
# Preview changes without modifying files
fy format -n configs/

# Quiet mode (only show errors)
fy format -i -q large-project/

# Verbose mode (show each file processed)
fy format -i -v configs/
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

# CI reports (files are named by absolute path)
fy lint --format github .      # GitHub Actions annotations
fy lint --format sarif . > results.sarif   # SARIF 2.1.0 for code scanning
fy lint --format parsable .    # path:line:col: [level] message (code)
```

`parsable` prints `info` and `hint` diagnostics as `warning`, like yamllint's two levels. Report formats list files in path order. A file that cannot be parsed or read is reported as a `syntax` error, and the exit code and stderr message stay the same as with `--format text`. GitHub shows at most 10 annotations per level per step. The `github` format writes `file=` as an absolute path, which the runner maps relative to the workspace.

### Parser resource limits

`fy parse`, `fy convert` and `fy lint` accept `--max-depth` (1 to 512, default 256) and
`--max-alias-bytes` (1 to 1GiB, default 64MiB; suffixes `KiB`, `MiB`, `GiB`). Out-of-range values
are rejected. The alias budget is per file, so batch runs can use up to workers x `--max-alias-bytes` of
memory. For `fy convert`, the flags apply to YAML input only. `fy format` accepts `--max-depth`
only (formatting never expands aliases); `--indent` is 1 to 9 and `--width` 20 to 1000.

```bash
# Allow a large alias expansion
fy parse --max-alias-bytes 512MiB big.yaml

# Reject deeper nesting than 64 levels
fy lint --max-depth 64 config.yaml
```

### Input size limit

`fy parse`, `fy convert`, `fy format` and `fy lint` reject any input file, including each file of a
batch run, and stdin larger than `--max-input-bytes` (alias `--max-input-size`; 1 to 1GiB, default 100MiB; suffixes `KiB`,
`MiB`, `GiB`). For `fy lint` the `max-input-bytes` key of `.fast-yaml.yaml` sets the limit when the flag is absent.

```bash
fy parse --max-input-bytes 500MiB huge.yaml
```

## Commands

| Command | Description |
|---------|-------------|
| `parse` | Parse and validate YAML syntax |
| `format` | Format YAML with consistent style |
| `convert` | Convert between YAML and JSON |
| `lint` | Lint YAML with diagnostics |

## Options

### Common Options

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--in-place` | `-i` | Edit file in-place | - |
| `--output` | `-o` | Write to file | stdout |
| `--format` | `-f` | Output format (yaml/json/compact) | yaml |
| `--no-color` | - | Disable colored output | - |
| `--quiet` | `-q` | Suppress non-error output | - |
| `--verbose` | `-v` | Enable verbose output | - |

### Batch Mode Options

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--jobs` | `-j` | Number of parallel workers (0 = auto) | auto-detect |
| `--stdin-files` | - | Read file paths from stdin; a missing path, directory, non-YAML file or line over 4096 bytes is an error | - |
| `--include` | - | Include pattern (glob, case-insensitive) | `*.yaml`, `*.yml` |
| `--exclude` | - | Exclude pattern (glob, case-insensitive) | none |
| `--no-recursive` | - | Disable recursive directory traversal | recursive |
| `--dry-run` | `-n` | Preview changes without modifying; exits 5 if any file would change | - |

> [!NOTE]
> Batch mode activates automatically when processing multiple paths, directories, glob patterns, or when using `--stdin-files`, `--include`, `--exclude`, or `--jobs`. Include and exclude patterns are case-insensitive. A run where every input is filtered out, or a directory holds no YAML files, exits with code 1; an empty `--stdin-files` list exits 0.

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
| 5 | `fy format --dry-run`: at least one file would change (1 takes precedence if any file failed) |

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

- **Startup time**: ~5ms
- **Binary size**: ~1MB (stripped)
- **Batch processing**: Multi-threaded with automatic CPU detection
- **Streaming**: Memory-mapped I/O for files >512KB
- **Full YAML 1.2.2 compliance**

> [!TIP]
> Batch mode provides 3-6x speedup on multi-core systems when processing 100+ files. Use `-j N` to control worker count.

## License

Licensed under [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE) at your option.
