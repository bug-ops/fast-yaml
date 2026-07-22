---
name: fast-yaml-cli
description: High-performance YAML processor (`fy` binary) for validation, formatting, linting, and bidirectional YAML↔JSON conversion. Use when agents need to parse/validate YAML, format it with consistent indentation, check for lint violations with diagnostic output, or convert between YAML and JSON formats. Supports batch processing with parallel workers, glob patterns, and structured output (text or JSON).
license: MIT OR Apache-2.0
compatibility: |-
  macOS (x86_64, aarch64), Linux (x86_64, aarch64), Windows (manual binary download).
  `fy` binary available via: `cargo install fast-yaml-cli` (requires Rust toolchain),
  prebuilt binary from GitHub Releases (download + checksum verify for Windows),
  or install script: `curl -fsSL https://raw.githubusercontent.com/bug-ops/fast-yaml/main/scripts/install.sh | sh` (macOS, Linux only).
metadata:
  author: bug-ops
  version: "0.6.4"
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
FASTYAML_VERSION=v0.6.4 curl -fsSL https://raw.githubusercontent.com/bug-ops/fast-yaml/main/scripts/install.sh | sh
```

**Custom install directory:**
```bash
FASTYAML_INSTALL_DIR=/usr/local/bin curl -fsSL https://raw.githubusercontent.com/bug-ops/fast-yaml/main/scripts/install.sh | sh
```

### Manual Download

#### macOS & Linux (Unix)

1. Go to https://github.com/bug-ops/fast-yaml/releases
2. Download the prebuilt `.tar.gz` archive for your OS/arch:
   - macOS x86_64: `fy-v0.6.4-x86_64-apple-darwin.tar.gz`
   - macOS ARM64: `fy-v0.6.4-aarch64-apple-darwin.tar.gz`
   - Linux x86_64 (glibc): `fy-v0.6.4-x86_64-unknown-linux-gnu.tar.gz`
   - Linux x86_64 (musl/Alpine): `fy-v0.6.4-x86_64-unknown-linux-musl.tar.gz`
   - Linux ARM64: `fy-v0.6.4-aarch64-unknown-linux-gnu.tar.gz`
3. Download the corresponding `.sha256` checksum file
4. Verify: `sha256sum -c fy-v0.6.4-*.tar.gz.sha256` (or `shasum -a 256`)
5. Extract: `tar -xzf fy-v0.6.4-*.tar.gz`
6. Move binary to PATH: `mv fy-v0.6.4-*/fy /usr/local/bin/`

#### Windows

1. Go to https://github.com/bug-ops/fast-yaml/releases
2. Download: `fy-v0.6.4-x86_64-pc-windows-msvc.zip`
3. Download the corresponding `.sha256` checksum file
4. Verify the archive (before extracting) — see "Checksum verification" in [Platform Notes → Windows](#windows) section below
5. Extract: `Expand-Archive -Path fy-v0.6.4-x86_64-pc-windows-msvc.zip -DestinationPath .`
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
| `--output FILE` | `-o` | stdout | Write output to FILE instead of stdout |
| `--in-place` | `-i` | — | Edit file in-place (requires file argument; not supported by `lint`) |
| `--no-color` | — | — | Disable colored output (useful in CI) |
| `--quiet` | `-q` | — | Quiet mode: errors only (no info messages) |
| `--verbose` | `-v` | — | Verbose output (e.g., processing details in batch mode) |

### Top-Level Output Format Flag

The top-level output format flag **must be placed BEFORE the subcommand name** (unlike the above flags):

```bash
fy --format json parse file.yaml    # ✓ correct
fy parse file.yaml --format json    # ✗ error: unexpected argument '--format'
```

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--format FORMAT` | `-f` | `yaml` | Output format for subcommand output: `yaml`, `json`, or `compact` |

**Note:** This is distinct from the `lint` subcommand's own `--format` flag (which selects lint output format as `text` or `json` and must come AFTER `lint`). Both flags share the same name but control different things — the top-level `--format` affects how all subcommands render their output, while `fy lint --format json` specifically selects structured lint diagnostics.

### parse

Parse and validate YAML.

```bash
fy parse [OPTIONS] [FILE]
```

**Arguments:**
- `FILE`: Input file. If omitted, reads from stdin.

**Options:**
- `--stats`: Show parse statistics (key count, max nesting depth).

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

# Validate and output as JSON
fy parse config.yaml -f json
```

### format

Format YAML with consistent style (fixed indentation, line width, key ordering). Comments are NOT preserved by the formatter — use `--strip-comments` to suppress the error if comments are present.

```bash
fy format [OPTIONS] [PATHS]...
```

**Arguments:**
- `PATHS`: Input file(s), directory, or glob pattern. If empty and no `--stdin-files`, reads from stdin.
  - Single file: formats in-place with `-i` or to stdout
  - Directory or glob: batch mode (see below)
  - Multiple paths: batch mode (see below)

**Options:**

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--indent INDENT` | — | `2` | Indentation width: 2–8 spaces |
| `--width WIDTH` | — | `80` | Maximum line width (for formatting decisions) |
| `-j, --jobs JOBS` | — | `0` | Parallel workers: 0 = auto-detect, >0 = explicit count |
| `--stdin-files` | — | — | Read file paths from stdin (one per line) — forces batch mode |
| `--include PATTERN` | — | — | Include files matching glob (can repeat) |
| `--exclude PATTERN` | — | — | Exclude files matching glob (can repeat) |
| `--no-recursive` | — | — | Don't recurse into subdirectories (batch mode only) |
| `-n, --dry-run` | — | — | Show what would be changed without modifying files (batch mode only) |
| `--strip-comments` | — | — | Suppress error if comments are detected (comments are stripped) |

**Modes:**

- **Single file:** `fy format file.yaml` → stdout; `fy format -i file.yaml` → in-place
- **Stdin:** `cat file.yaml | fy format` → stdout
- **Batch (directory/glob/multiple paths/–stdin-files):** processes all matched files in parallel:
  - `fy format dir/` → format all `.yaml`/`.yml` in dir recursively, write in-place
  - `fy format '*.yaml'` → format all YAML in current directory
  - `fy format file1.yaml file2.yaml` → format both files
  - `fy format -i --include '*.yaml' --exclude 'vendor/**' .` → include/exclude patterns with recursion disabled: `fy format --no-recursive --include '*.yaml' .`

**Output:**
- Formatted YAML (preserves structure, reorders keys alphabetically, applies indentation)
- Quiet mode (`-q`) suppresses file-processed messages; only shows errors
- Verbose mode (`-v`) shows processing details

**Gotchas:**
- **Comment handling:** if YAML contains comments, `fy format` exits with error (exit 1) unless `--strip-comments` is passed. Comments are not preserved by the formatter.
- **Key ordering:** formatter reorders keys alphabetically in each mapping

**Examples:**
```bash
# Format single file to stdout
fy format messy.yaml

# Format in-place with 4-space indent
fy format -i --indent 4 config.yaml

# Format all YAML in directory (batch mode)
fy format -i configs/

# Dry-run: show what would change
fy format --dry-run -i configs/

# Format with inclusion/exclusion
fy format -i --include '*.yaml' --exclude 'test/**' .

# Format from stdin
cat raw.yaml | fy format

# Format from file list on stdin
find . -name '*.yaml' | fy format --stdin-files
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

**Output:**
- YAML→JSON: formatted JSON (with `--pretty true`) or compact JSON (with `--pretty false`)
- JSON→YAML: formatted YAML with 2-space indent
- Keys are sorted alphabetically

**Examples:**
```bash
# Convert YAML to JSON (pretty)
fy convert json config.yaml

# Convert YAML to compact JSON
fy convert json --pretty false config.yaml

# Convert JSON to YAML
fy convert yaml data.json

# In-place conversion
fy convert -i json data.yaml  # ⚠️ file.yaml becomes file.json

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
| `--config FILE` | — | auto-discover | Path to `.fast-yaml.yaml` config file |
| `--no-config` | — | — | Disable config file auto-discovery |
| `--max-line-length N` | — | — | Override config file's max line length |
| `--indent-size N` | — | — | Override config file's indent size |
| `--format FORMAT` | — | `text` | Output format: `text` (human-readable) or `json` (structured) |
| `--allow-duplicate-keys [BOOL]` | — | — | Allow duplicate keys (opt-in); `true`/`false` or flag alone for `true` |
| `--include PATTERN` | — | — | Include files matching glob (can repeat) |
| `--exclude PATTERN` | — | — | Exclude files matching glob (can repeat) |
| `--no-recursive` | — | — | Don't recurse into subdirectories |
| `-j, --jobs JOBS` | — | `0` | Parallel workers: 0 = auto-detect |

**Config File Discovery:**

If no `--config` is specified, `fy lint` searches from the input file's directory up the tree for `.fast-yaml.yaml`.

**Output Formats:**

**Text (default):**
```
info[key-ordering]: key 'age' should be ordered before 'name' (line 2)
  --> input:3:1
   |
   1 | ---
   2 | name: Alice
   3 | age: 30
     | 
   4 | active: true
```

**JSON (with `--format json`):**
```json
[
  {
    "code": "key-ordering",
    "severity": "info",
    "message": "key 'age' should be ordered before 'name' (line 2)",
    "span": {
      "start": { "line": 3, "column": 1, "offset": 16 },
      "end": { "line": 3, "column": 1, "offset": 19 }
    },
    "context": {
      "lines": [
        { "line_number": 1, "content": "---", "highlights": [] },
        { "line_number": 2, "content": "name: Alice", "highlights": [] },
        { "line_number": 3, "content": "age: 30", "highlights": [[1, 1]] }
      ]
    }
  }
]
```

**Lint Severity Levels:**
- `error` — exits with code 2 if any errors found
- `warning` — reported but does not affect exit code
- `info` — style suggestions; does not affect exit code

**Built-in Rules** (examples):
- `key-ordering` — keys should be in alphabetical order
- `line-length` — lines should not exceed max length
- `indentation` — indentation should be consistent
- `duplicate-keys` — duplicate keys are not allowed (unless `--allow-duplicate-keys`)

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

# Exclude test files
fy lint --exclude 'test/**' .
```

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success (parse/format/convert succeed; lint found no errors) |
| `1` | Any error: YAML parsing failure, I/O error (file not found, permission denied), or general application error |
| `2` | Lint found errors (diagnostic violations); also used by clap for malformed CLI invocations (flag syntax errors, unexpected arguments) |

**Note:** Exit codes 3 and 4 are defined in the enum but never constructed — all non-lint errors surface as exit 1 in the current implementation.

**Note on exit code 2:** Clap itself returns exit 2 for malformed CLI invocations (e.g., `fy parse file.yaml --format json` where `--format` is in the wrong position). This collides with `ExitCode::LintErrors` (also 2). Both produce exit 2, but the error message differs: clap prints "unexpected argument", while lint produces structured diagnostics.

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
Get-FileHash -Path fy-v0.6.4-x86_64-pc-windows-msvc.zip -Algorithm SHA256
```
Compare the output hash against the `.sha256` file downloaded from the release. Then extract with `Expand-Archive -Path fy-v0.6.4-x86_64-pc-windows-msvc.zip -DestinationPath .` and move the `fy.exe` binary to your chosen `%PATH%` directory.

### PATH Setup Across Shells

Adding the binary directory to `PATH` is shell-specific:

- **bash/zsh:** `export PATH=$HOME/.local/bin:$PATH` (add to `~/.bashrc` or `~/.zshrc`)
- **fish:** `fish_add_path $HOME/.local/bin` (add to `~/.config/fish/config.fish`)
- **Windows (PowerShell):** Use `setx` (persistent) or `$Env:PATH` assignment (session-only)

After install, verify: `fy --version`

## Behavior Notes

1. **Comment Stripping:** The formatter does NOT preserve comments. If input YAML contains comments, `fy format` exits with error (1) unless `--strip-comments` is passed, which silently removes them.
2. **Key Ordering:** Both formatter and linter enforce alphabetical key ordering by default.
3. **JSON Parsing:** Convert from JSON to YAML works with `fy convert yaml <json-file>`. JSON must be valid; the parser uses `serde_json`.
4. **Parallel Processing:** Batch mode (directory/glob/multi-file) automatically uses available CPUs. Override with `-j N`.
5. **Glob Patterns:** Use standard glob syntax (`*`, `?`, `[a-z]`). Patterns like `src/**/*.yaml` work with `--include`/`--exclude`.
6. **Stdin Piping:** All subcommands support reading from stdin if FILE is omitted (except lint without PATHS reads from stdin).
7. **Color Output:** Colored output is enabled by default (if terminal is a TTY). Disable with `--no-color` (useful in CI/scripts).

## Integration with Agents

**When to use:**

- **YAML validation:** `fy parse file.yaml` — quick syntax check
- **YAML formatting:** `fy format -i *.yaml` — batch-process a project's YAML files
- **JSON↔YAML:** `fy convert yaml data.json` — convert API responses or config formats
- **Linting:** `fy lint --format json config.yaml | jq` — structured lint output for CI pipelines
- **Batch processing:** `fy format --include '*.yaml' --exclude 'vendor/**' .` — format directories with pattern matching
- **Parallel jobs:** `fy format -j 8 configs/` — speed up formatting of large file sets

## Compatibility

- **Rust version requirement:** 1.88.0+ (per `rust-version` in `Cargo.toml`)
- **YAML spec:** YAML 1.2.2 (via `yaml-rust2` and `saphyr-parser`)
- **Platforms:** Linux (x86_64, aarch64), macOS (x86_64, aarch64), Windows (manual binary)
