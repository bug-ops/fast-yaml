---
name: fast-yaml-cli
description: High-performance YAML processor (`fy` binary) for validation, formatting, linting, and bidirectional YAML↔JSON conversion. Use when agents need to parse/validate YAML, format it with consistent indentation, check for lint violations with diagnostic output, or convert between YAML and JSON formats. Supports batch processing with parallel workers, glob patterns, and structured lint output (text, JSON, GitHub annotations, SARIF, parsable).
license: MIT OR Apache-2.0
compatibility: |-
  macOS (x86_64, aarch64), Linux (x86_64, aarch64), Windows (manual binary download).
  `fy` binary available via: `cargo install fast-yaml-cli` (requires Rust toolchain),
  prebuilt binary from GitHub Releases (download + checksum verify for Windows),
  or install script: `curl -fsSL https://raw.githubusercontent.com/bug-ops/fast-yaml/main/scripts/install.sh | sh` (macOS, Linux only).
metadata:
  author: bug-ops
  version: "0.7.0"
---

## Installation

### Via Cargo (if Rust toolchain available)

```bash
cargo install fast-yaml-cli
```

Installs `fy` binary to `~/.cargo/bin`. Verify with `fy --version`.

### Via Install Script (macOS, Linux)

```bash
curl -fsSL https://raw.githubusercontent.com/bug-ops/fast-yaml/main/scripts/install.sh | sh
```

Downloads prebuilt binary from latest GitHub Release, verifies checksum with sha256, and installs to `$FASTYAML_INSTALL_DIR` (default: `~/.local/bin`). Requires `curl`, `tar`, and either `sha256sum` or `shasum`.

**Pinning to a specific version:**
```bash
FASTYAML_VERSION=v0.7.0 curl -fsSL https://raw.githubusercontent.com/bug-ops/fast-yaml/main/scripts/install.sh | sh
```

**Custom install directory:**
```bash
FASTYAML_INSTALL_DIR=/usr/local/bin curl -fsSL https://raw.githubusercontent.com/bug-ops/fast-yaml/main/scripts/install.sh | sh
```

### Manual Download

#### macOS & Linux (Unix)

1. Go to https://github.com/bug-ops/fast-yaml/releases
2. Download the prebuilt `.tar.gz` archive for your OS/arch:
   - macOS x86_64: `fy-v0.7.0-x86_64-apple-darwin.tar.gz`
   - macOS ARM64: `fy-v0.7.0-aarch64-apple-darwin.tar.gz`
   - Linux x86_64 (glibc): `fy-v0.7.0-x86_64-unknown-linux-gnu.tar.gz`
   - Linux x86_64 (musl/Alpine): `fy-v0.7.0-x86_64-unknown-linux-musl.tar.gz`
   - Linux ARM64: `fy-v0.7.0-aarch64-unknown-linux-gnu.tar.gz`
3. Download the corresponding `.sha256` checksum file
4. Verify: `sha256sum -c fy-v0.7.0-*.tar.gz.sha256` (or `shasum -a 256`)
5. Extract: `tar -xzf fy-v0.7.0-*.tar.gz`
6. Move binary to PATH: `mv fy-v0.7.0-*/fy /usr/local/bin/`

#### Windows

1. Go to https://github.com/bug-ops/fast-yaml/releases
2. Download: `fy-v0.7.0-x86_64-pc-windows-msvc.zip`
3. Download the corresponding `.sha256` checksum file
4. Verify the archive (before extracting) — see "Checksum verification" in [Platform Notes → Windows](#windows) section below
5. Extract: `Expand-Archive -Path fy-v0.7.0-x86_64-pc-windows-msvc.zip -DestinationPath .`
6. Move `fy.exe` to your chosen `%PATH%` directory

### From Source

```bash
git clone https://github.com/bug-ops/fast-yaml.git
cd fast-yaml
cargo build -p fast-yaml-cli --release
# Binary at: ./target/release/fy
```

## CLI Reference

### Global Options

All subcommands support these flags, usable before or after the subcommand name:

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--no-color` | — | — | Disable colored output (useful in CI) |
| `--quiet` | `-q` | — | Quiet mode: errors only (no info messages) |
| `--verbose` | `-v` | — | Verbose output (e.g., processing details in batch mode) |
| `--max-input-bytes BYTES` | — | `100MiB` | Maximum size of each input file or stdin (1 to 1GiB; suffixes `KiB`, `MiB`, `GiB`, integers only) |
| `--max-scan-ahead CHARS` | — | `4MiB` | Maximum characters the parser may read past the last reported node (same units); bounds parser memory |

`--quiet` and `--verbose` conflict (exit 2). The former top-level `-f/--format` is removed; the lint report format is `fy lint --format`.

`-o/--output` and `-i/--in-place` are not global: each belongs to the subcommands that write (`format`, `convert`; `lint` has only `-o`) and goes after the subcommand name. `fy parse -o x`, `fy -o x lint` and `fy lint -i` are usage errors (exit 2).

### parse

Parse and validate YAML.

```bash
fy parse [OPTIONS] [FILE]
```

**Arguments:**
- `FILE`: Input file. If omitted, reads from stdin.

**Options:**
- `--stats`: Show parse statistics (key count, max nesting depth).
- `--max-depth N`: Maximum nesting depth of sequences and mappings (1-512, default 256; flow collections stop at 255).
- `--max-alias-bytes BYTES`: Maximum bytes materialized by alias expansion per input (default 64MiB).
- `--max-documents N`: Maximum documents per input stream (1-10000000, default 100000).

`--max-depth` and `--max-documents` are also accepted by `format`, `convert` and `lint`; `--max-alias-bytes` by `convert` and `lint`. When a limit is hit the error ends with a `hint: raise with --<flag>` line.

**Output:**
- Valid YAML: `✓ YAML is valid` (exit 0)
- With `--stats`: validation message + statistics block
- Invalid YAML: error message with parser diagnostics (exit 1)

**Examples:**
```bash
# Parse from stdin
echo "name: Alice" | fy parse

# Parse file with statistics
fy parse config.yaml --stats

# Validate and print the data as JSON
fy convert json config.yaml
```

### format

Format YAML with consistent style (fixed indentation and line width; keys keep their order). Comments are NOT preserved by the formatter — use `--strip-comments` to suppress the error if comments are present.

```bash
fy format [OPTIONS] [PATHS]...
```

**Arguments:**
- `PATHS`: Input file(s), directory, or glob pattern. If empty and no `--stdin-files`, reads from stdin.
  - Single file: formats in-place with `-i` or to stdout
  - Directory or glob: batch mode (see below); needs `-i` or `-n`
  - Multiple paths: batch mode (see below); needs `-i` or `-n`
  - A missing path, a glob that matches nothing, or an explicit non-YAML file is an error (exit 1)

**Options:**

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--indent N` | — | `2` | Indentation width: 1-9 spaces |
| `--width N` | — | `80` | Maximum line width (20-1000) |
| `-j, --jobs N` | — | `0` | Parallel workers: 0 = auto, 1-128 = explicit count (other values are a usage error) |
| `--stdin-files` | — | — | Read file paths from stdin (one per line) — forces batch mode; a missing path, directory or non-YAML file is an error |
| `--include PATTERN` | — | `*.yaml`, `*.yml` | Include files matching glob, case-insensitive (can repeat; replaces the defaults) |
| `--exclude PATTERN` | — | — | Exclude files matching glob, case-insensitive (can repeat) |
| `--no-recursive` | — | — | Don't recurse into subdirectories (batch mode only) |
| `-n, --dry-run` | — | — | Never write; print a summary of what would change (single file, stdin and batch); exit 5 if any file would change |
| `--strip-comments` | — | — | Suppress error if comments are detected (comments are stripped) |
| `-o, --output FILE` | — | stdout | Write the formatted YAML to FILE (single input or stdin); conflicts with `-n` and `-i` |
| `-i, --in-place` | — | — | Rewrite the input file(s) (requires a file argument) |

**Modes:**

- **Single file:** `fy format file.yaml` → stdout; `fy format -i file.yaml` → in-place
- **Stdin:** `cat file.yaml | fy format` → stdout
- **Batch (directory/glob/multiple paths/`--stdin-files`):** processes all matched files in parallel; without `-i` or `-n` it fails with `use -i to format files in-place or --dry-run to preview changes`:
  - `fy format -i dir/` → format all `.yaml`/`.yml` in dir recursively, write in-place
  - `fy format -i '*.yaml'` → format all YAML in current directory
  - `fy format -i file1.yaml file2.yaml` → format both files
  - `fy format -i --include '*.yaml' --exclude 'vendor/**' .` → include/exclude patterns; add `--no-recursive` to stay in the top directory
- **Gate in CI:** `fy format -n configs/` exits 5 when any file would change, 0 when all are formatted

**Output:**
- Formatted YAML (preserves structure and key order, applies indentation); directives, tags, anchors and every document of a stream are kept
- Quiet mode (`-q`) suppresses file-processed messages; only shows errors
- Verbose mode (`-v`) shows processing details

**Gotchas:**
- **Comment handling:** if YAML contains comments, `fy format` exits with error (exit 1) unless `--strip-comments` is passed. Comments are not preserved by the formatter.
- **Merge keys:** `<<` is validated while formatting; an invalid merge is an error with its position

**Examples:**
```bash
# Format single file to stdout
fy format messy.yaml

# Format in-place with 4-space indent
fy format -i --indent 4 config.yaml

# Format all YAML in directory (batch mode)
fy format -i configs/

# Dry-run: show what would change
fy format --dry-run configs/

# Format with inclusion/exclusion
fy format -i --include '*.yaml' --exclude 'test/**' .

# Format from stdin
cat raw.yaml | fy format

# Format from file list on stdin
git diff --name-only --diff-filter=d -- '*.yaml' '*.yml' | fy format -i --stdin-files
```

### convert

Convert between YAML and JSON.

```bash
fy convert [OPTIONS] <TO> [FILE]
```

**Arguments:**
- `TO`: Target format: `yaml` or `json` (required)
- `FILE`: Input file. If omitted, reads from stdin.

**Options:**
- `--pretty [PRETTY]`: Pretty-print JSON output (default: `true`). Set to `false` for compact JSON: `--pretty false`
- `-o, --output FILE`: Write to FILE instead of stdout
- `-i, --in-place`: Replace the input file with the converted content, keeping its name (conflicts with `-o`)
- `--max-depth`, `--max-alias-bytes`, `--max-documents`: parse limits (apply to YAML input only)

**Output:**
- YAML→JSON: formatted JSON (with `--pretty true`) or compact JSON (with `--pretty false`)
- JSON→YAML: formatted YAML with 2-space indent
- Key order is preserved; a multi-document YAML stream becomes a JSON array
- Integers beyond 64 bits (including `0x`/`0o` literals) are kept exact

**Examples:**
```bash
# Convert YAML to JSON (pretty)
fy convert json config.yaml

# Convert YAML to compact JSON
fy convert json --pretty false config.yaml

# Convert JSON to YAML
fy convert yaml data.json

# In-place conversion (the file keeps its name; its content becomes JSON)
fy convert json -i data.yaml

# From stdin
echo '{"name": "Alice"}' | fy convert yaml
```

### lint

Lint YAML with diagnostics and structured reporting. Requires `linter` feature (enabled by default in binary releases).

```bash
fy lint [OPTIONS] [PATHS]...
```

**Arguments:**
- `PATHS`: Input file(s), directory, or glob pattern. If empty, reads from stdin.

**Options:**

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--config FILE` | — | auto-discover | Path to the config file (fails with exit 1 if missing or invalid) |
| `--no-config` | — | — | Disable config file auto-discovery (built-in defaults) |
| `--max-line-length N` | — | — | Override the `line-length` max |
| `--indent-size N` | — | — | Override the `indentation` width (1-16) |
| `--format FORMAT` | — | `text` | Output format: `text`, `json`, `github`, `sarif` or `parsable` |
| `-o, --output FILE` | — | stdout | Write the report to FILE (refused when FILE is an input) |
| `--allow-duplicate-keys [BOOL]` | — | — | `true` disables the `duplicate-key` rule |
| `--max-diagnostics N` | — | — | Show at most N diagnostics per file plus one summary line; output only, the exit code is unaffected |
| `--stdin-files` | — | — | Read file paths from stdin (one per line) |
| `--include PATTERN` | — | `*.yaml`, `*.yml`, `.yamllint` | Include files matching glob, case-insensitive (can repeat; replaces the defaults) |
| `--exclude PATTERN` | — | — | Exclude files matching glob (can repeat) |
| `--no-recursive` | — | — | Don't recurse into subdirectories |
| `-j, --jobs N` | — | `0` | Parallel workers: 0 = auto, 1-128 |
| `--max-depth`, `--max-alias-bytes`, `--max-documents` | — | see `parse` | Parse limits; also `--max-input-bytes` and `--max-scan-ahead` (the flags override the same-named config keys) |

`lint` has `-o` but no `-i` (there is no auto-fix); `fy lint -i` is a usage error.

**Config File Discovery:**

Without `--config` or `--no-config`, `fy lint` looks for `.fast-yaml.yaml` (then `.fast-yaml.yml`) starting at the **current working directory** and walking up at most 20 parent directories. With `-v` the chosen file is announced on stderr (`using config file: <path>`).

**Config File Format (yamllint-compatible):**

```yaml
extends: default            # default | relaxed | path to another config (max 8 files deep)
rules:
  line-length: {max: 120}   # rule with options
  document-start: disable   # enable | disable | error | warning | info | hint
  truthy: {level: warning}  # per-rule severity (also `severity:`)
  trailing-spaces: {ignore: [generated/]}   # per-rule gitignore-style ignore
  indentation: {spaces: 4, indent-sequences: consistent}
ignore: [vendor/, '*.generated.yaml']        # drop files from linting (or ignore-from-file)
yaml-files: ['*.yaml', '*.yml', '.yamllint']
max-input-bytes: 10485760   # bytes, plain integer
max-diagnostics: 50
```

Accepted top-level keys: `rules`, `extends`, `ignore`, `ignore-from-file`, `yaml-files`, `locale`, `max-input-bytes`, `max-scan-ahead`, `max-diagnostics`. Any other key, unknown rule name or unknown option fails the load (exit 1) and names the accepted ones. Rules can be keyed by their fast-yaml code or yamllint name (`trailing-spaces` = `trailing-whitespace`, `key-duplicates` = `duplicate-key`, `anchors` = `invalid-anchor`). Without any config all 25 rules run with fast-yaml defaults, which are not identical to yamllint's `default` preset.

**Inline Directives:**

```yaml
# fy: disable truthy        # block: from here until `# fy: enable truthy` or end of file
a: yes
b: yes  # fy: disable-line truthy
# fy: disable-file          # suppress all rule diagnostics in the file
```

`# yamllint disable|enable|disable-line|disable-file [rule:NAME ...]` is accepted too. A malformed directive or unknown rule name is itself reported as `lint-directive`; syntax errors cannot be suppressed.

**Output Formats:**

**Text (default):**
```
info[key-ordering]: key 'age' should be ordered before 'name' (line 1)
  --> input:2:1
   |
   1 | name: Alice
   2 | age: 30
     | ^^^
   3 | active: yes
```

**JSON (with `--format json`):** an array of diagnostics with `code`, `severity`, `message`, `span` (`start`/`end` with 1-based `line`, `column` and byte `offset`) and `context` (source lines with highlights). A syntax or limit error is reported as a `syntax` element.

```json
[
  {
    "code": "duplicate-key",
    "severity": "error",
    "message": "duplicate key 'a' (first defined at line 1)",
    "span": {
      "start": { "line": 2, "column": 1, "offset": 5 },
      "end": { "line": 2, "column": 2, "offset": 6 }
    },
    "context": { "lines": [ ... ] }
  }
]
```

**Parsable (`--format parsable`):** one `path:line:col: [level] message (code)` line per diagnostic.
```
a.yaml:2:1: [error] duplicate key 'a' (first defined at line 1) (duplicate-key)
```

**GitHub (`--format github`):** workflow commands for inline PR annotations (GitHub caps them per step).
```
::error file=/abs/path/a.yaml,line=2,col=1,endLine=2,endColumn=2,title=duplicate-key::duplicate key 'a' (first defined at line 1)
```

**SARIF (`--format sarif`):** one SARIF 2.1.0 log (`tool.driver.name` = `fast-yaml-linter`) for code scanning upload.

**Lint Severity Levels:**
- `error` — exits with code 2 if any errors found
- `warning`, `info`, `hint` — reported but do not affect the exit code
- Override per rule with `rules: {<rule>: {level: warning}}` in the config file

**Built-in Rules** (25; config keys equal the codes shown in diagnostics):
- Content: `duplicate-key`, `empty-values`, `truthy`, `octal-values`, `float-values`, `quoted-strings`, `key-ordering`, `invalid-anchor`, `set-values`
- Layout: `indentation`, `line-length`, `trailing-whitespace`, `empty-lines`, `new-lines`, `new-line-at-end-of-file`
- Punctuation: `braces`, `brackets`, `colons`, `commas`, `hyphens`
- Comments and documents: `comments`, `comments-indentation`, `document-start`, `document-end`
- Meta: `lint-directive` (malformed inline directives)

**Examples:**
```bash
# Lint single file (text output)
fy lint config.yaml

# Lint with JSON output
fy lint --format json config.yaml | jq .

# Lint directory (batch mode)
fy lint configs/

# Lint with custom config
fy lint --config my-lint-config.yaml config.yaml

# Lint with rule overrides
fy lint --max-line-length 120 config.yaml

# Allow duplicate keys
fy lint --allow-duplicate-keys config.yaml

# CI: annotations on pull requests, or a SARIF file for code scanning
fy lint --format github .
fy lint --format sarif -o results.sarif .

# Cap output volume per file (exit code unaffected)
fy lint --max-diagnostics 20 .

# Lint only files changed in git
git diff --name-only --diff-filter=d -- '*.yaml' '*.yml' | fy lint --stdin-files

# Exclude test files
fy lint --exclude 'test/**' .
```

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success (parse/format/convert succeed; lint found no error-severity diagnostic; `format -n` found nothing to change) |
| `1` | Runtime failure: invalid YAML, I/O error, missing path, size or parse limit exceeded, config load error, `-i` without a file, any failed file in a `format` batch |
| `2` | Lint found at least one error-severity diagnostic (or a batch had an unreadable or syntax-failing file); also clap usage errors (unknown or misplaced flag, invalid value, conflicting flags) |
| `5` | `format --dry-run` found files that formatting would change (and none failed; `1` takes precedence) |

**Note on exit code 2:** clap usage errors and lint errors share it. A usage error prints `error: unexpected argument ...` with a usage line; lint errors print diagnostics.

## Platform Notes

### macOS Gatekeeper

Binaries downloaded via the install script acquire the `com.apple.quarantine` extended attribute. On first run, macOS may block execution with: "cannot be opened because the developer cannot be verified" or similar.

To allow the binary, remove the quarantine attribute:
```bash
xattr -d com.apple.quarantine ~/.local/bin/fy
```

Or, if installed via cargo: `~/.cargo/bin/fy` may also be quarantined depending on how Rust was installed.

The install script does NOT automatically remove this attribute — this is by design, allowing you to inspect the binary before use.

### Linux libc Coverage

Prebuilt CLI binaries support both glibc and musl on x86_64, but glibc only on aarch64:

**x86_64 Linux:**
- **glibc** (standard distros like Ubuntu, Debian, Fedora): `x86_64-unknown-linux-gnu` — available via install script and manual download
- **musl** (Alpine, Void, etc.): `x86_64-unknown-linux-musl` — available via install script and manual download; same installation flow as glibc

**aarch64 (ARM64) Linux:**
- **glibc** (Ubuntu ARM64, Debian ARM64): `aarch64-unknown-linux-gnu` — available via install script and manual download
- **musl**: NOT YET PUBLISHED. Alpine on ARM64 and other musl aarch64 systems must build from source:

```bash
git clone https://github.com/bug-ops/fast-yaml.git
cd fast-yaml
cargo build -p fast-yaml-cli --release --target aarch64-unknown-linux-musl
```

Alternatively, use a glibc-compatible container (e.g., Docker with Ubuntu/Debian ARM64 base image).

### Windows

No install script available (`scripts/install.sh` is POSIX shell, Linux/macOS only).

Installation method: **manual binary download only**. See [Manual Download](#manual-download) section above.

When downloaded, the binary is named `fy.exe`. Add its directory to `%PATH%` via:
- **Command Prompt (cmd.exe):** `setx PATH "%PATH%;C:\path\to\fy"`
- **PowerShell:** `$Env:PATH += ";C:\path\to\fy"` (session-only) or use System Properties → Environment Variables (persistent)

Checksum verification on Windows (required before extracting):
```powershell
Get-FileHash -Path fy-v0.7.0-x86_64-pc-windows-msvc.zip -Algorithm SHA256
```
Compare the output hash against the `.sha256` file downloaded from the release. Then extract with `Expand-Archive -Path fy-v0.7.0-x86_64-pc-windows-msvc.zip -DestinationPath .` and move the `fy.exe` binary to your chosen `%PATH%` directory.

### PATH Setup Across Shells

Adding the binary directory to `PATH` is shell-specific:

- **bash/zsh:** `export PATH=$HOME/.local/bin:$PATH` (add to `~/.bashrc` or `~/.zshrc`)
- **fish:** `fish_add_path $HOME/.local/bin` (add to `~/.config/fish/config.fish`)
- **Windows (PowerShell):** Use `setx` (persistent) or `$Env:PATH` assignment (session-only)

After install, verify: `fy --version`

## Behavior Notes

1. **Comment Stripping:** The formatter does NOT preserve comments. If input YAML contains comments, `fy format` exits with error (1) unless `--strip-comments` is passed, which silently removes them.
2. **Key Ordering:** `fy format` and `fy convert` keep key order. Only the `key-ordering` lint rule (info severity) reports unsorted keys.
3. **JSON Parsing:** Convert from JSON to YAML works with `fy convert yaml <json-file>`. JSON must be valid; the parser uses `serde_json`.
4. **Parallel Processing:** Batch mode (directory/glob/multi-file) automatically uses available CPUs. Override with `-j N` (1-128). Output order is deterministic regardless of `-j`.
5. **Glob Patterns:** Use standard glob syntax (`*`, `?`, `[a-z]`). Patterns like `src/**/*.yaml` work with `--include`/`--exclude`.
6. **Stdin Piping:** All subcommands read stdin when no file is given. `fy` with no subcommand formats stdin.
7. **Color Output:** Colored output is enabled by default (if terminal is a TTY). Disable with `--no-color` (useful in CI/scripts).

## Integration with Agents

**When to use:**

- **YAML validation:** `fy parse file.yaml` — quick syntax check
- **YAML formatting:** `fy format -i configs/` — batch-process a project's YAML files; `fy format -n configs/` as a CI gate (exit 5 when changes are needed)
- **JSON↔YAML:** `fy convert yaml data.json` — convert API responses or config formats
- **Linting:** `fy lint --format json config.yaml | jq` — structured lint output for CI pipelines
- **Batch processing:** `fy format --include '*.yaml' --exclude 'vendor/**' .` — format directories with pattern matching
- **Parallel jobs:** `fy format -j 8 configs/` — speed up formatting of large file sets

## Compatibility

- **Rust version requirement:** 1.91.0+ (per `rust-version` in `Cargo.toml`)
- **YAML spec:** YAML 1.2.2 (via `saphyr-parser`)
- **Platforms:** Linux (x86_64, aarch64), macOS (x86_64, aarch64), Windows (manual binary)
