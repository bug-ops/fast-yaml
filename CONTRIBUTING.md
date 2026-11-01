# Contributing to fast-yaml

Thank you for your interest in contributing to fast-yaml. This document provides guidelines and instructions for contributing to the project.

## Table of Contents

- [Getting Started](#getting-started)
- [Development Workflow](#development-workflow)
- [Code Style and Quality](#code-style-and-quality)
- [Testing Requirements](#testing-requirements)
- [Python Contributions](#python-contributions)
- [NodeJS Contributions](#nodejs-contributions)
- [Commit Messages](#commit-messages)
- [Pull Request Process](#pull-request-process)
- [Project Structure](#project-structure)
- [Maintainers](#maintainers)

## Getting Started

### Prerequisites

You will need the following tools installed:

**Rust toolchain** (MSRV 1.91, stable for building, nightly for `rustfmt`):
```bash
# Install Rust via rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install required components
rustup component add rustfmt clippy llvm-tools-preview

# Install development tools
cargo install cargo-nextest cargo-llvm-cov cargo-deny cargo-audit cargo-semver-checks
```

**Python toolchain (for Python bindings):**
```bash
# Install uv (fast Python package manager)
curl -LsSf https://astral.sh/uv/install.sh | sh
```

**NodeJS toolchain (for NodeJS bindings):**
```bash
# Install Node.js 22+ and pnpm (via nvm or your preferred method)
# Biome is installed via pnpm in the nodejs/ directory
```

### Cloning the Repository

```bash
git clone https://github.com/bug-ops/fast-yaml.git
cd fast-yaml
```

### Building the Project

**Rust crates (core functionality):**
```bash
# Build all core crates
cargo build

# Build with optimizations
cargo build --release
```

**Note:** FFI crates (`python/` and `nodejs/`) are excluded from default workspace builds and require specialized build tools:

```bash
# Python bindings
cd python
uv sync
uv run maturin develop

# NodeJS bindings
cd nodejs
pnpm install
pnpm run build
```

The Python and Node.js test suites call the `fy` binary for cross-checks, so run `cargo build --bin fy` first. The Node.js panic tests need `pnpm run build:test`.

## Development Workflow

### 1. Create a Feature Branch

```bash
git checkout -b feat/your-feature-name
```

### 2. Make Changes

Follow the code style guidelines below and ensure your changes align with the project's architecture.

### 3. Run Quality Checks

Before committing, run the full quality check pipeline (flags match CI):

```bash
# Format check (uses nightly rustfmt for Edition 2024 features)
cargo +nightly fmt --all -- --check

# Linting
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Tests (use nextest for faster execution)
cargo nextest run --workspace --all-features --exclude fast-yaml --exclude fast-yaml-nodejs --lib --bins

# Documentation check
RUSTFLAGS="-D warnings" RUSTDOCFLAGS="--deny rustdoc::broken_intra_doc_links" cargo doc --no-deps --workspace
```

#### Optional: mise toolchain and tasks

[mise](https://mise.jdx.dev) pins the non-Rust dev tools and exposes the CI checks as named tasks. CI installs its tools with mise and runs the format, lint, test, coverage, audit and build jobs through `mise run <task>`, so a task reproduces the matching CI job locally. The MSRV check runs through the `msrv` task on a rustup toolchain; the release build check and the SARIF tool install stay outside mise. mise is optional for contributors: plain `cargo`, `pnpm` and `uv` commands keep working, and the commands above remain the reference.

Pinned in `.config/mise/config.toml` and `.config/mise/mise.lock`: Node.js, pnpm, Python, uv, cargo-nextest, cargo-deny, cargo-hack, cargo-llvm-cov and cargo-semver-checks. Rust stays on rustup (see [What stays outside mise](#what-stays-outside-mise)).

```bash
mise install --locked     # install exactly the versions and checksums in .config/mise/mise.lock
mise tasks                # list tasks
mise run ci               # local pre-PR gate
mise run lint:clippy      # a single task
```

| Task | Runs | CI job |
|------|------|--------|
| `fmt` | rustfmt (nightly), `ruff format`, `pnpm run format` (rewrites files) | - |
| `fmt:check`, `fmt:check:rust`, `fmt:check:python`, `fmt:check:nodejs` | formatting checks | Lint (rust), Python Quality Checks, NodeJS Quality Checks |
| `lint` | `lint:clippy`, `lint:python`, `lint:nodejs` | - |
| `lint:clippy` | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Lint (rust) |
| `lint:clippy-hack` | clippy over the feature powerset of the non-FFI crates | Clippy Feature Powerset |
| `lint:python` | `uv sync`, mypy (non-blocking), `ruff check` | Python Quality Checks |
| `lint:nodejs` | `pnpm install`, biome lint, `tsc` | NodeJS Quality Checks |
| `audit:nodejs` | `pnpm audit --audit-level=high` | NodeJS Quality Checks |
| `doc` | `cargo doc --workspace --no-deps` with warnings denied | Lint (rust) |
| `test:rust` | build, nextest and doctests, excluding the FFI crates | Test Rust |
| `test:core-no-arena` | clippy, nextest and doctests of `fast-yaml-core` alone | Test Core (no arena) |
| `build:fy`, `build:python`, `build:nodejs` | build the `fy` binary, the maturin extension, the napi test addon | - |
| `test:python`, `test:nodejs` | binding tests (depend on the build tasks) | Test Python, Test NodeJS |
| `release:nodejs` | release build of the Node.js addon | NodeJS Build |
| `coverage:rust`, `coverage:python`, `coverage:nodejs` | coverage runs | Code Coverage, Python Coverage, NodeJS Coverage |
| `deny` | `cargo deny check` | Security Audit |
| `semver` | `cargo semver-checks` of `fast-yaml-core` (non-blocking) | Security Audit |
| `ci` | `fmt:check`, `lint`, `doc`, `test:rust`, `test:python`, `test:nodejs`, `deny`, sequentially | - |

`mise run ci` is the local pre-PR gate. It covers the format, lint, test and security jobs but not coverage, `lint:clippy-hack`, `test:core-no-arena`, `audit:nodejs`, `semver`, `release:nodejs`, the `msrv` task or the SARIF upload; run those tasks individually when your change touches them. It runs tasks sequentially because parallel tasks race on the cargo lock and on concurrent `pnpm install` runs. Set `YAML_TEST_SUITE_PATH` to a `yaml-test-suite` checkout (tag `data-2022-01-17`) before `mise run test:python` to run the full suite as CI does.

##### Version legs

CI tests other Python and Node.js versions through mise environments. `MISE_ENV` selects an extra config file with its own lockfile:

| `MISE_ENV` | Config | Lockfile | Overrides |
|------------|--------|----------|-----------|
| `py310` | `.config/mise/config.py310.toml` | `.config/mise/mise.py310.lock` | Python 3.10 |
| `py314` | `.config/mise/config.py314.toml` | `.config/mise/mise.py314.lock` | Python 3.14 |
| `node22` | `.config/mise/config.node22.toml` | `.config/mise/mise.node22.lock` | Node.js 22 |

Run a leg locally:

```bash
MISE_ENV=py310 mise install --locked
MISE_ENV=py310 mise run test:python
```

Environment names must not contain dots (`py3.10` would write into `.config/mise/mise.lock`). Node.js 24 and Python 3.12 are the defaults in `.config/mise/config.toml`. All mise files live in `.config/mise/`; tasks still run from the repository root.

##### Adding or updating a tool

1. Edit `[tools]` in `.config/mise/config.toml` (or the `.config/mise/config.<env>.toml` file for a version leg). Prefer the `aqua:` backend; use `github:` when aqua has no entry. Avoid the `cargo:` backend: it builds from source and does not lock transitive tools.
2. Regenerate every lockfile for the platforms CI uses:

   ```bash
   P=linux-x64,linux-arm64,macos-arm64,windows-x64
   mise lock --platform $P
   for env in py310 py314 node22; do MISE_ENV=$env mise lock --platform $P; done
   ```

3. Verify with `mise install --locked`; confirm `git diff` touches only the intended lock entries.
4. If CI needs the tool in a job, add it to that job's `install_args` in `.github/workflows/ci.yml`. Jobs set `MISE_TASK_RUN_AUTO_INSTALL=false`, so a tool missing from `install_args` fails the job instead of being installed silently.
5. `jdx/mise-action` steps pin `version: 2026.10.25` (the mise release the lockfiles were generated with). Dependabot does not bump it: when regenerating lockfiles with a newer mise, change all 13 occurrences in `.github/workflows/ci.yml` together and keep them equal to that mise version.

To see what is stale, run `mise outdated`. A tool bump changes `.config/mise/mise.lock` (and the `.config/mise/mise.<env>.lock` files if a leg is affected); `.config/mise/**` is in the CI path filter, so such a PR runs the full pipeline.

##### Adding a task

Add a `[tasks."name"]` table to `.config/mise/config.toml` with a `description` and `run`. Tasks that CI runs must produce the same result as the command they replace. New CI jobs call `mise run <task>` instead of a `setup-*` action followed by raw commands.

##### Security notes

- `.config/mise/mise.lock` pins each tool version with a per-platform sha256 checksum (and GitHub attestation provenance where the release provides it). Always install with `--locked` so a drifting upstream cannot change what you run.
- CI runs the install step with `MISE_SAFE=1` (safe mode). Safe mode disables `mise run`, so it is set on the install step only, never at job level. Whether safe mode is honored by the setup action in real CI is unverified until the first run.
- mise executes tasks and `[env]` values from `.config/mise/config.toml`. Treat that file like a build script: review changes to it, and trust a checkout (`mise trust`) only after you have read the file. Never put secrets in `[env]`; the file is committed. Per-task `env` holds only build flags.
- The setup action runs with `cache: false`, so a poisoned cache cannot supply binaries; only the checksum-verified downloads are used.
- Third-party actions stay pinned to a full commit SHA.

##### What stays outside mise

- Rust toolchains (stable, nightly for rustfmt, beta, and the MSRV 1.91.0) come from rustup and `dtolnay/rust-toolchain`; mise does not manage them.
- `clippy-sarif` and `sarif-fmt` come from `taiki-e/install-action` in the Clippy job: they have no mise registry entry and share one repository with several tag prefixes, which collide in the lockfile.
- The release build check uses rustup only; the MSRV check uses the rustup 1.91.0 toolchain through `mise run msrv`. When the MSRV changes, update `rust-version`, the `msrv` job and the `msrv` task together.

##### Troubleshooting

- `nodejs/pnpm-lock.yaml` shows as modified after a task: the Node.js tasks run `pnpm install --no-frozen-lockfile`, as CI does, and the lockfile drifts. Revert it with `git checkout nodejs/pnpm-lock.yaml` unless you changed dependencies.
- A task hangs on "Blocking waiting for file lock": another cargo process holds the build directory. Do not run cargo-based tasks in parallel; `mise run ci` is sequential for this reason.
- `mise install --locked` fails with a missing lock entry: regenerate the lockfile for all platforms as above.
- mise refuses to run in a checkout: run `mise trust` after reviewing `.config/mise/config.toml`.
- `mise run test:python` skips most of the yaml-test-suite: set `YAML_TEST_SUITE_PATH`.

Behavior changes also update the matching spec in `specs/` (start at `specs/constitution.md`; one `NNN-feature/spec.md` per capability) and the `[Unreleased]` section of `CHANGELOG.md`.

### 4. Commit Changes

See [Commit Messages](#commit-messages) section below.

### 5. Push and Create Pull Request

```bash
git push origin feat/your-feature-name
```

Then create a pull request following the [PR template](.github/pull_request_template.md).

## Code Style and Quality

### Formatting

All Rust code must be formatted with rustfmt using nightly toolchain (required for Edition 2024):

```bash
# Format all code
cargo +nightly fmt --all

# Check formatting without modifying
cargo +nightly fmt --all -- --check
```

### Linting

Code must pass clippy with no warnings:

```bash
# Same invocation as CI
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

### Workspace Lints

The project uses workspace-level lint configuration in root `Cargo.toml`:

- `unsafe_code = "deny"` - Unsafe code denied by default, must be explicitly allowed with `#[allow(unsafe_code)]` and SAFETY documentation
- `missing_docs = "warn"` - Public items should be documented
- Clippy: `all`, `pedantic`, `nursery`, `cargo` warnings enabled

### Documentation

All public APIs must be documented:

```bash
# Build documentation
cargo doc --workspace --no-deps

# Check for documentation warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

# Doc-tests
cargo test --doc --workspace --exclude fast-yaml --exclude fast-yaml-nodejs
```

## Testing Requirements

### Running Tests

**Always use `cargo nextest` instead of `cargo test`** for faster execution and better output:

```bash
# Run all tests (excludes FFI crates)
cargo nextest run --workspace --all-features --exclude fast-yaml --exclude fast-yaml-nodejs --lib --bins

# Include integration tests
cargo nextest run --workspace --exclude fast-yaml --exclude fast-yaml-nodejs

# Run tests with output
cargo nextest run --workspace --exclude fast-yaml --exclude fast-yaml-nodejs --no-capture

# Run specific test
cargo nextest run --workspace --exclude fast-yaml --exclude fast-yaml-nodejs -E 'test(test_name)'

# Run tests for specific crate
cargo nextest run -p fast-yaml-core
```

### Code Coverage

We maintain the following coverage targets:

- **Critical code** (FFI, parsing): ≥80%
- **Business logic**: ≥70%
- **Overall project**: ≥60%

Generate coverage reports:

```bash
# HTML coverage report (excludes FFI crates)
cargo llvm-cov --workspace --exclude fast-yaml --exclude fast-yaml-nodejs --html

# Coverage with nextest (faster)
cargo llvm-cov nextest --workspace --exclude fast-yaml --exclude fast-yaml-nodejs --html

# Terminal summary
cargo llvm-cov --workspace --exclude fast-yaml --exclude fast-yaml-nodejs

# LCOV format for CI
cargo llvm-cov --workspace --exclude fast-yaml --exclude fast-yaml-nodejs --lcov --output-path lcov.info
```

Open the HTML report at `target/llvm-cov/html/index.html`.

### Fuzzing

Fuzz targets (`parse`, `format`, `lint`, `validate_differential`) live in `fuzz/`; run one with `cargo +nightly fuzz run <target>` (requires `cargo-fuzz`). CI also runs them on pushes to `main`.

### Security Auditing

All dependencies must pass security checks:

**Rust dependencies:**
```bash
# Check for known vulnerabilities
cargo audit

# Comprehensive check (vulnerabilities, licenses, bans)
cargo deny check

# Check only advisories
cargo deny check advisories

# Check license compliance
cargo deny check licenses
```

**NodeJS dependencies:**
```bash
cd nodejs

# Check for vulnerabilities
pnpm audit

# Fail on high/critical only
pnpm audit --audit-level=high
```

## Python Contributions

### Setup

```bash
cd python
uv sync
```

### Testing

```bash
# Run tests
uv run pytest tests/ -v

# With coverage
uv run pytest tests/ -v --cov=fast_yaml --cov-report=html
```

### Code Quality

```bash
# Type checking
uv run mypy fast_yaml/

# Linting
uv run ruff check .

# Formatting
uv run ruff format .
```

### Building

```bash
# Development build
uv run maturin develop

# Release build
uv run maturin build --release
```

## NodeJS Contributions

### Setup

```bash
cd nodejs
pnpm install
```

### Testing

```bash
# Run tests
pnpm test

# With coverage
pnpm run test:coverage

# Run benchmarks
pnpm run bench
```

### Code Quality

NodeJS uses Biome for formatting and linting:

```bash
# Format code
pnpm run format

# Check formatting
pnpm run format:check

# Lint code
pnpm run lint

# Format and lint together
pnpm run check

# Type checking
pnpm run typecheck
```

### Building

```bash
# Build native module (release)
pnpm run build

# Build without optimizations (faster)
pnpm run build:debug
```

## Commit Messages

Follow conventional commit format:

```
<type>(<scope>): <subject>

<body>

<footer>
```

**Types:**
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `style`: Code style changes (formatting, etc.)
- `refactor`: Code refactoring
- `perf`: Performance improvements
- `test`: Test additions or changes
- `chore`: Build process or auxiliary tool changes
- `ci`: CI/CD changes

**Scopes:**
- `core`: fast-yaml-core crate
- `linter`: fast-yaml-linter crate
- `parallel`: fast-yaml-parallel crate
- `cli`: fast-yaml-cli crate (`fy`)
- `python`: Python bindings
- `nodejs`: NodeJS bindings
- `ci`: CI/CD configuration

Combine scopes with commas (`fix(linter,cli)`) and mark breaking changes with `!` (`fix(core)!:`).

**Example:**
```
feat(parallel): add multi-threaded YAML processing

Implement document-level parallelism using Rayon for processing
large multi-document YAML streams. Achieves 3-4x speedup on
8-core systems for files with 100+ documents.
```

## Pull Request Process

### Before Submitting

1. Ensure all quality checks pass:
   ```bash
   cargo +nightly fmt --all -- --check && \
   cargo clippy --workspace --all-targets --all-features -- -D warnings && \
   cargo nextest run --workspace --all-features --exclude fast-yaml --exclude fast-yaml-nodejs --lib --bins && \
   RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
   ```

2. Check code coverage meets targets

3. Run security audits:
   ```bash
   cargo deny check
   # If NodeJS changes:
   cd nodejs && pnpm audit
   ```

4. Update documentation if needed

### PR Description

Use the provided [pull request template](.github/pull_request_template.md). Include:

- Summary of changes
- Motivation and context
- Type of change (bug fix, feature, breaking change, etc.)
- Testing evidence
- Quality checklist completion

### Review Process

1. Automated checks must pass (CI/CD pipeline)
2. Code review by maintainers
3. Address review feedback
4. Final approval and merge

### Breaking Changes

If your PR introduces breaking changes:

1. Clearly mark in PR title: `feat!: breaking change description`
2. Document migration path in PR description
3. Update CHANGELOG.md
4. List every breaking change under `[Unreleased]` in CHANGELOG.md

## Project Structure

### Workspace Layout

```
fast-yaml/
├── crates/
│   ├── fast-yaml-cli/       # `fy` binary (parse, format, convert, lint)
│   ├── fast-yaml-core/      # Core YAML parser/emitter
│   ├── fast-yaml-linter/    # Linting engine
│   └── fast-yaml-parallel/  # Multi-threaded processing
├── python/                  # PyO3 Python bindings
├── nodejs/                  # NAPI-RS NodeJS bindings
├── specs/                   # Feature specifications (source of truth for behavior)
├── fuzz/                    # cargo-fuzz targets
├── tests/                   # Integration tests
└── benches/                 # Criterion benchmarks
```

### FFI Crate Exclusion

The `python/` and `nodejs/` directories contain FFI binding crates that:

- Require specialized build tools (maturin for Python, napi for NodeJS)
- Are excluded from default workspace operations
- Must be built separately using their respective toolchains

**Always exclude FFI crates** from workspace commands:
```bash
cargo build --workspace --exclude fast-yaml --exclude fast-yaml-nodejs
cargo nextest run --workspace --exclude fast-yaml --exclude fast-yaml-nodejs
```

### Core Principles

- **Performance First**: Optimize for speed and memory efficiency
- **Minimal Unsafe Code**: `unsafe_code = "deny"` with explicit documentation for FFI boundaries
- **Comprehensive Testing**: Maintain ≥60% overall coverage
- **Clear Documentation**: Document all public APIs
- **Security by Default**: All dependencies audited

## Maintainers

- Coverage uploads need the `CODECOV_TOKEN` repository secret
- Branch protection on `main` should require the `CI Success` check, which gates all other CI jobs in `.github/workflows/ci.yml`

## Getting Help

- Open an issue for bugs or feature requests
- Use GitHub Discussions for questions
- Read the feature specs in [specs/](specs/) for intended behavior and the architecture overview in the workspace layout above

## License

By contributing to fast-yaml, you agree that your contributions will be licensed under both:

- MIT License
- Apache License 2.0

See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE) for details.
