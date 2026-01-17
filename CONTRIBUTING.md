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

## Getting Started

### Prerequisites

You will need the following tools installed:

**Rust toolchain:**
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
# Install Node.js 18+ and npm (via nvm or your preferred method)
# Biome is installed via npm in the nodejs/ directory
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
npm install
npm run build
```

## Development Workflow

### 1. Create a Feature Branch

```bash
git checkout -b feature/your-feature-name
```

### 2. Make Changes

Follow the code style guidelines below and ensure your changes align with the project's architecture.

### 3. Run Quality Checks

Before committing, run the full quality check pipeline:

```bash
# Format check (uses nightly rustfmt for Edition 2024 features)
cargo +nightly fmt --all -- --check

# Linting (excludes FFI crates)
cargo clippy --workspace --all-targets --exclude fast-yaml --exclude fast-yaml-nodejs -- -D warnings

# Tests (use nextest for faster execution)
cargo nextest run --workspace --exclude fast-yaml --exclude fast-yaml-nodejs

# Documentation check
cargo doc --workspace --no-deps --exclude fast-yaml --exclude fast-yaml-nodejs
```

### 4. Commit Changes

See [Commit Messages](#commit-messages) section below.

### 5. Push and Create Pull Request

```bash
git push origin feature/your-feature-name
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
# Standard clippy check (excludes FFI crates that need special build tools)
cargo clippy --workspace --all-targets --all-features --exclude fast-yaml --exclude fast-yaml-nodejs -- -D warnings

# Pedantic mode for stricter checks
cargo clippy --workspace --all-targets --exclude fast-yaml --exclude fast-yaml-nodejs -- -D warnings -W clippy::pedantic
```

**Note:** FFI crates (`python/` and `nodejs/`) must be excluded from workspace commands as they require maturin/napi build tools.

### Workspace Lints

The project uses workspace-level lint configuration in root `Cargo.toml`:

- `unsafe_code = "forbid"` - No unsafe code allowed
- `missing_docs = "warn"` - Public items should be documented
- Clippy: `all`, `pedantic`, `nursery`, `cargo` warnings enabled

### Documentation

All public APIs must be documented:

```bash
# Build documentation
cargo doc --workspace --no-deps

# Check for documentation warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --exclude fast-yaml --exclude fast-yaml-nodejs
```

## Testing Requirements

### Running Tests

**Always use `cargo nextest` instead of `cargo test`** for faster execution and better output:

```bash
# Run all tests (excludes FFI crates)
cargo nextest run --workspace --exclude fast-yaml --exclude fast-yaml-nodejs

# Run tests with output
cargo nextest run --workspace --exclude fast-yaml --exclude fast-yaml-nodejs --nocapture

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
npm audit

# Fail on high/critical only
npm audit --audit-level=high

# Auto-fix vulnerabilities
npm audit fix
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
uv run mypy python/fast_yaml/

# Linting
uv run ruff check python/

# Formatting
uv run ruff format python/
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
npm install
```

### Testing

```bash
# Run tests
npm test

# With coverage
npm run test:coverage

# Run benchmarks
npm run bench
```

### Code Quality

NodeJS uses Biome for formatting and linting:

```bash
# Format code
npm run format

# Check formatting
npm run format:check

# Lint code
npm run lint

# Format and lint together
npm run check

# Type checking
npm run typecheck
```

### Building

```bash
# Build native module
npm run build

# Build in release mode
npm run build:release
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
- `ffi`: fast-yaml-ffi crate
- `python`: Python bindings
- `nodejs`: NodeJS bindings
- `ci`: CI/CD configuration

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
   cargo clippy --workspace --all-targets --exclude fast-yaml --exclude fast-yaml-nodejs -- -D warnings && \
   cargo nextest run --workspace --exclude fast-yaml --exclude fast-yaml-nodejs && \
   cargo doc --workspace --no-deps --exclude fast-yaml --exclude fast-yaml-nodejs
   ```

2. Check code coverage meets targets

3. Run security audits:
   ```bash
   cargo deny check
   # If NodeJS changes:
   cd nodejs && npm audit
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
4. Consider deprecation warnings before removal

## Project Structure

### Workspace Layout

```
fast-yaml/
├── crates/
│   ├── fast-yaml-core/      # Core YAML parser/emitter
│   ├── fast-yaml-linter/    # Linting engine
│   ├── fast-yaml-parallel/  # Multi-threaded processing
│   └── fast-yaml-ffi/       # FFI utilities
├── python/                  # PyO3 Python bindings
├── nodejs/                  # NAPI-RS NodeJS bindings
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
cargo clippy --workspace --exclude fast-yaml --exclude fast-yaml-nodejs -- -D warnings
cargo nextest run --workspace --exclude fast-yaml --exclude fast-yaml-nodejs
```

### Core Principles

- **Performance First**: Optimize for speed and memory efficiency
- **Zero Unsafe Code**: `unsafe_code = "forbid"` in workspace lints
- **Comprehensive Testing**: Maintain ≥60% overall coverage
- **Clear Documentation**: Document all public APIs
- **Security by Default**: All dependencies audited

## Getting Help

- Open an issue for bugs or feature requests
- Use GitHub Discussions for questions
- Read the [project documentation](CLAUDE.md) for architecture details
- Check [Architecture Decision Records](.local/adr/) for design rationale

## License

By contributing to fast-yaml, you agree that your contributions will be licensed under both:

- MIT License
- Apache License 2.0

See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE) for details.
