# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.4] - 2025-12-14

### Changed
- Renamed Python package from `fast-yaml` to `fastyaml-rs` (PyPI name conflict)
- Renamed Node.js package from `@fast-yaml/core` to `fastyaml-rs` (npm scope not available)

## [0.1.3] - 2025-12-14

### Fixed
- Fixed Node.js cross-compilation by using zig instead of Docker (avoids Node version mismatch)
- Removed Windows ARM64 Python wheels (cross-compilation not supported by maturin)

## [0.1.2] - 2025-12-14

### Fixed
- Fixed invalid keyword `yaml-1.2` → `yaml12` for crates.io compliance
- Fixed Python sdist build by creating local README.md (maturin doesn't allow `..` paths)
- Fixed Node.js musl/aarch64 Docker builds by using `stable` images with Node 20+

## [0.1.1] - 2025-12-13

### Added
- README.md files for workspace crates (fast-yaml-core, fast-yaml-ffi, fast-yaml-parallel)
- Workspace-level publishing support for `cargo publish --workspace`

### Changed
- Simplified release CI workflow to use single `cargo publish --workspace` command instead of matrix-based individual crate publishing
- Updated minimum supported Rust version (MSRV) to 1.88.0 (required by napi-rs dependency)

### Fixed
- Resolved clippy `collapsible_if` warnings across 8 files using Rust 2024 let chains syntax:
  - `crates/fast-yaml-core/tests/yaml_spec_fixtures.rs`
  - `crates/fast-yaml-linter/src/context.rs`
  - `crates/fast-yaml-linter/src/formatter/text.rs`
  - `crates/fast-yaml-linter/src/rules/duplicate_keys.rs`
  - `crates/fast-yaml-parallel/src/processor.rs`
  - `python/src/lib.rs`
  - `python/src/lint.rs`
  - `python/src/parallel.rs`

## [0.1.0] - 2025-12-10

### Added
- Initial release of fast-yaml workspace with modular architecture
- **fast-yaml-core**: YAML 1.2.2 compliant parser and emitter
  - Zero-copy parsing where possible
  - Support for multi-document YAML streams
  - Core Schema compliance
  - Comprehensive error reporting with source location tracking
- **fast-yaml-linter**: YAML validation and linting engine
  - Rich diagnostic system with line/column tracking
  - Pluggable linting rules architecture
  - Duplicate key detection
  - Invalid anchor/alias detection
  - Human-readable and JSON diagnostic formatters
- **fast-yaml-parallel**: Multi-threaded YAML processing
  - Intelligent document boundary detection
  - Rayon-based parallel processing
  - Order-preserving result aggregation
  - Optimized for large multi-document YAML files
- **fast-yaml-ffi**: Shared FFI utilities for language bindings
  - Type conversion traits
  - FFI-safe error representation
  - Memory management helpers
- **Python bindings** (fast-yaml-python):
  - PyO3-based native extension
  - `safe_load()` and `safe_dump()` functions
  - Linter integration with detailed diagnostics
  - Parallel processing support
  - Type stubs for IDE integration
- **Node.js bindings** (fast-yaml-nodejs):
  - NAPI-RS based native module
  - TypeScript type definitions
  - Full parser, linter, and parallel processing APIs
  - CommonJS and ESM module support

### Infrastructure
- Comprehensive CI/CD pipeline with GitHub Actions
  - Cross-platform testing (Linux, macOS, Windows)
  - Code coverage reporting via codecov
  - Security scanning with cargo-deny
  - Automated dependency updates via Dependabot
- Workspace-based dependency management
- Rust Edition 2024 with MSRV 1.88.0
- Quality control tooling:
  - cargo-nextest for fast test execution
  - cargo-llvm-cov for code coverage
  - cargo-semver-checks for API compatibility
  - cargo-deny for security auditing

### Documentation
- Project architecture documentation (CLAUDE.md)
- Architecture Decision Records (ADRs) in `.local/adr/`
- Comprehensive README with usage examples
- API documentation for all crates
- Python package documentation
- Node.js package documentation

[unreleased]: https://github.com/bug-ops/fast-yaml/compare/v0.1.4...HEAD
[0.1.4]: https://github.com/bug-ops/fast-yaml/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/bug-ops/fast-yaml/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/bug-ops/fast-yaml/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/bug-ops/fast-yaml/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/bug-ops/fast-yaml/releases/tag/v0.1.0
