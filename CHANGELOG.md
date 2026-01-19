# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.0] - 2026-01-19

### Breaking Changes

- **Parallel**: `ParallelConfig` renamed to `Config` with simplified 4-field API
- **Parallel**: Removed `min_chunk_size`, `max_chunk_size`, `max_documents` fields
- **Parallel**: `with_thread_count()` renamed to `with_workers()`
- **CLI**: Batch module removed (functionality preserved, implementation changed)

### Added

- **Parallel**: File-level parallelism with `FileProcessor` struct
  - `parse_files()` for batch validation
  - `format_files()` for dry-run formatting
  - `format_in_place()` for in-place formatting with atomic writes
- **Parallel**: `SmartReader` for automatic mmap/read selection
- **Parallel**: Result types: `BatchResult`, `FileResult`, `FileOutcome`
- **Parallel**: Convenience function `process_files()`
- **Parallel**: New config field `mmap_threshold` for file reading strategy
- **Parallel**: New config field `sequential_threshold` for small input optimization
- **Python**: Batch processing submodule (`fast_yaml._core.batch`)
  - `process_files()` for parallel file validation
  - `format_files()` for dry-run formatting
  - `format_files_in_place()` for in-place formatting
  - `BatchConfig` for configuration
  - `BatchResult` for aggregated results
  - `FileOutcome` enum for per-file outcomes
- **Node.js**: Batch processing functions
  - `processFiles()` for parallel file validation
  - `formatFiles()` for dry-run formatting
  - `formatFilesInPlace()` for in-place formatting
  - `BatchConfig` interface for configuration
  - `BatchResult` interface for results

### Changed

- **CLI**: Batch processing now uses `fast-yaml-parallel` crate directly
- **CLI**: Removed ~2339 lines of duplicate code
- **Parallel**: Unified error type (single `Error` enum for all operations)

### Fixed

- **Security**: Fixed mmap TOCTOU race condition with file locking
- **Security**: Added symlink security checks on Unix platforms
- **Security**: Improved UTF-8 validation for memory-mapped files

### Performance

- **Parallel**: Automatic mmap/read selection reduces syscall overhead
- **Parallel**: Sequential fallback for small files (<4KB) avoids thread overhead
- **Parallel**: Smart file reading with configurable thresholds

### Documentation

- Updated fast-yaml-parallel README with new APIs
- Updated Python and Node.js READMEs with batch processing examples

### Internal

- Workspace tests: 866 passing
- Python tests: 38 batch tests passing
- Node.js tests: 23/25 batch tests passing
- Zero clippy warnings

## [0.4.1] - 2026-01-17

### Added

- **Python**: Parallel dump functionality for multi-document YAML emission
  - `dump_parallel()` function with configurable thread pool
  - Auto-tuning algorithm for optimal thread count based on workload
  - Pre-allocates output buffer to minimize reallocations
- **Python**: Streaming dump API for direct I/O without intermediate string
  - `safe_dump_to()` writes directly to file-like objects
  - Configurable chunk size (default 8KB) for efficient buffer flushing
  - Supports any object with `write()` method (files, StringIO, BytesIO)
- **Python**: Comprehensive type stubs for new parallel and streaming APIs
- **Python**: 34 new tests for streaming functionality (`test_streaming.py`)
- **Core**: Public getter methods for `ParallelConfig` (`thread_count()`, `max_documents()`)
- **Node.js**: Pre-allocation benchmarks to verify linear scaling

### Performance

- **Python**: Parallel dump shows linear scaling with document count
  - Auto-tuning reduces overhead for small workloads (<4 documents)
  - Conservative thread allocation (uses half of CPU cores for small documents)
- **Node.js**: Pre-allocation optimizations maintain linear time complexity
  - Arrays and objects scale linearly with size (no O(n²) growth)

### Fixed

- **Python**: Auto-tune algorithm now handles low CPU count edge cases (macOS CI)
  - Previously panicked with `assertion failed: min <= max` on single-core systems
  - Now ensures `max_threads >= 2` before calling `.clamp()`

### Documentation

- Updated API documentation with new parallel and streaming functions
- Added inline examples for `dump_parallel()` and `safe_dump_to()`
- Documented thread count auto-tuning behavior and thresholds

### Internal

- **Security**: Dual licensing added (MIT OR Apache-2.0)
- **Documentation**: Updated unsafe code usage points in project docs
- All CI checks passing: 912 Rust tests, 344 Python tests, 283 Node.js tests
- Code coverage: 94% maintained

## [0.4.0] - 2026-01-17

### Added

- **CLI**: Unified configuration system for consistent command-line behavior
  - `CommonConfig` aggregates output, formatter, I/O, and parallel configs
  - `OutputConfig` handles verbosity, color detection with NO_COLOR support
  - `ParallelConfig` manages worker threads and mmap thresholds
  - Consistent builder pattern across all configuration types
- **CLI**: Universal `Reporter` for centralized output formatting
  - Zero-copy event design using lifetimes (`ReportEvent`)
  - Proper stdout/stderr stream handling with locking
  - Consistent colored output across all commands
- **Benchmarks**: Comprehensive performance comparison vs google/yamlfmt 0.21.0
  - Single-file benchmarks (small/medium/large files)
  - Batch mode benchmarks (50-1000 files)
  - Reproducible benchmark scripts with hyperfine
  - Results documented in README and benches/comparison/

### Changed

- **CLI**: Refactored all commands to use unified `CommonConfig`
  - `parse`, `format`, `convert`, `lint` commands migrated
  - `format_batch` uses `BatchConfig` composition pattern
- **CLI**: Replaced `BatchFormatConfig` (11 flat fields) with `BatchConfig` composition
  - Composes `CommonConfig`, `DiscoveryConfig`, and batch-specific options
  - Cleaner separation of concerns
- **CLI**: Color detection centralized in `OutputConfig::from_cli()`
  - Automatic detection via `is_terminal` crate
  - Respects `NO_COLOR` environment variable
  - Deleted `should_use_color()` helper (replaced with config method)

### Removed

- **CLI**: Deleted `batch/reporter.rs` (428 lines) — replaced with unified `Reporter`
- **CLI**: Removed ~450 lines of duplicate code through refactoring
  - Eliminated field duplication across config types
  - Removed redundant color handling logic
  - Deleted obsolete constructors

### Performance

- **CLI Batch Mode**: 6-15x faster than yamlfmt on multi-file operations
  - 50 files: **2.40x faster**
  - 200 files: **6.63x faster**
  - 500 files: **15.77x faster** ⚡
  - 1000 files: **13.80x faster** ⚡
- **CLI Single-File**: 1.19-1.80x faster than yamlfmt on small/medium files
  - Small (502 bytes): **1.80x faster**
  - Medium (45 KB): **1.19x faster**
  - Large (460 KB): yamlfmt 2.88x faster (yamlfmt optimized for large files)
- **Streaming**: Phase 2 arena allocator improvements
  - 3-11% performance gains in streaming benchmarks
  - Reduced allocations through bumpalo arena

### Documentation

- **README**: Added comprehensive performance section with benchmark tables
  - CLI single-file vs yamlfmt comparison
  - CLI batch mode performance (key differentiator)
  - Test environment details and reproducibility instructions
- **Benchmarks**: Added `benches/comparison/README.md` with detailed methodology
  - Benchmark configuration and fairness criteria
  - Multi-file corpus descriptions
  - Latest results from Apple M3 Pro (12 cores)
- **Benchmarks**: Added `run_batch_benchmark.sh` for native batch mode testing
  - Compares parallel (-j N) vs sequential (-j 0) processing
  - Demonstrates 6-15x speedup with parallel workers

### Internal

- **CLI**: 100% test coverage on all config modules (common, output, parallel)
- **CLI**: Overall test coverage: 94.38% (exceeds 60% target)
- **CLI**: 912 tests passing, 0 failures
- **CI**: Zero clippy warnings with `-D warnings`
- **Security**: Zero vulnerabilities (cargo audit, cargo deny)
- **Code Quality**: Consistent builder pattern with `#[must_use]` and `const fn`

## [0.3.3] - 2025-01-15

### Breaking Changes

- **Python**: Minimum Python version increased from 3.9 to 3.10

### Added

- **Python**: Added support for Python 3.13 and 3.14

### Changed

- **Dependencies**: Updated all dependencies across ecosystems
  - Python: coverage, maturin, mypy, ruff, pathspec, librt
  - Node.js: Updated devDependencies
- **Documentation**: Refreshed all README files with latest project state
- **CI**: Updated Python test matrix and release builds (3.10-3.14)

## [0.3.2] - 2025-12-30

### Added

- **CLI**: Comprehensive integration test suite (59 tests)
  - Parse, format, convert, lint command tests
  - Global flags and error handling tests
  - Edge cases and special scenarios

### Fixed

- **CLI**: File argument now works after subcommand (intuitive syntax)
  - Before: `fy file.yaml parse` (file before subcommand only)
  - After: `fy parse file.yaml` (both syntaxes work)
- **CLI**: Global flags (`-i`, `-o`, `-q`, `-v`, `--no-color`) now work after subcommands
  - Before: `fy --quiet parse input.yaml` (flags only before subcommand)
  - After: `fy parse --quiet input.yaml` (flags work in either position)
- **CLI**: `--pretty=false` flag now accepts explicit boolean values

### Documentation

- Add crates.io badge for `fast-yaml-cli`
- Add docs.rs badge for `fast-yaml-core`
- Expand CLI section with all commands and examples
- Add `cargo binstall` installation option

## [0.3.1] - 2025-12-29

### Added

- **Node.js**: Comprehensive test suites with 70%+ code coverage (up from 10%)
  - `api-coverage.spec.ts` — 91 tests covering all API functions
  - `edge-cases.spec.ts` — Edge case handling and error conditions
  - `mark.spec.ts` — Mark class for error location tracking
  - `options.spec.ts` — Parser and emitter options
  - `schema.spec.ts` — Schema validation tests
- **Python**: Stream processing tests (`test_streams.py`)
- **CI**: npm audit security check for Node.js dependencies

### Changed

- **Node.js**: Migrated from Prettier to Biome v2.3.10 for formatting and linting
- **Node.js**: Updated devDependencies with Biome replacing Prettier
- **Node.js**: Added biome.json configuration with VCS integration and recommended rules
- **CI**: Updated Node.js versions (20→22 LTS, 22→23 Current)
- **CI**: Fixed codecov flags for proper coverage reporting

### Fixed

- **Node.js**: Test assertions corrected for YAML 1.2.2 compliance
- **Node.js**: Memory-intensive tests optimized to prevent OOM in CI
- **CI**: Python test paths corrected for accurate coverage reporting

### Internal

- Removed unused root pyproject.toml and uv.lock files (Python tooling is in python/ directory)
- CI lint step now enforces quality (removed continue-on-error)
- Vitest configured with sequential execution to prevent memory pressure

## [0.3.0] - 2025-12-29

### Breaking Changes

- **Parser**: Migrated from `yaml-rust2` to `saphyr` as the YAML parser foundation
- **YAML 1.2 Core Schema**: Stricter compliance with YAML 1.2 specification:
  - Only lowercase `true`/`false` are parsed as booleans (not `True`/`False`/`TRUE`/`FALSE`)
  - Only lowercase `null` and `~` are parsed as null (not `Null`/`NULL`)
  - Special float values now emit as `.inf`/`-.inf`/`.nan` (YAML 1.2 compliant)

### Changed

- **Core**: Replaced `yaml-rust2 0.10.x` with `saphyr 0.0.6` for YAML parsing
- **Core**: Updated `Value` type to use `saphyr::YamlOwned` internally
- **Core**: Float values now use `OrderedFloat<f64>` wrapper from saphyr
- **Emitter**: Added `fix_special_floats()` post-processing to ensure YAML 1.2 compliant output
- **Python**: Updated bindings to use saphyr types (`YamlOwned`, `ScalarOwned`, `MappingOwned`)
- **Node.js**: Updated bindings to use saphyr types
- **Docs**: Updated README, CLAUDE.md to reference saphyr instead of yaml-rust2
- **Docs**: Updated Technology Stack section with saphyr 0.0.6

### Fixed

- **Emitter**: Special float values (`inf`, `-inf`, `NaN`) now correctly emit as `.inf`, `-.inf`, `.nan` per YAML 1.2 spec

### Internal

- Updated internal type conversions for saphyr's nested value structure (`YamlOwned::Value(ScalarOwned::*)`)
- Added handling for `YamlOwned::Tagged` and `YamlOwned::Representation` variants
- Updated benchmark code to use saphyr API

## [0.2.0] - 2025-12-27

### Breaking Changes

- **Python**: Minimum Python version increased from 3.8 to 3.9
- **Workspace**: FFI crates (python/nodejs) excluded from default `cargo build`. Use specialized build tools:
  - Python: `uv run maturin develop`
  - Node.js: `npm run build`

### Changed

- **Workspace**: Added `default-members` to exclude FFI crates from default cargo commands
- **Build**: Added `manifest-path` to pyproject.toml for maturin configuration
- **Docs**: Updated documentation with new build commands and `--exclude` flags for workspace operations

### Fixed

- **Build**: `cargo build` no longer fails with Python symbol linking errors

## [0.1.11] - 2025-12-19

### Fixed
- Fixed Python package version in pyproject.toml (was still 0.1.9 in 0.1.10 release)

## [0.1.10] - 2025-12-19

### Added
- **Python**: Full PyYAML-compatible `load()` and `load_all()` functions with optional `Loader` parameter
- **Python**: Full PyYAML-compatible `dump()` and `dump_all()` functions with `Dumper`, `indent`, `width`, `explicit_start` parameters
- **Python**: Loader classes (`SafeLoader`, `FullLoader`, `Loader`) for PyYAML API compatibility
- **Python**: Dumper classes (`SafeDumper`, `Dumper`) for PyYAML API compatibility
- **Python**: Complete type stubs for all new classes and functions in `_core.pyi`
- **Python**: 24 new tests for Dumper classes and dump functions
- **Node.js**: Enhanced `DumpOptions` with `indent`, `width`, `defaultFlowStyle`, `explicitStart` parameters

### Fixed
- **Core**: Multi-document YAML emission now correctly adds trailing newlines between documents
- **Node.js**: Fixed multi-document round-trip parsing that was concatenating values with separators

## [0.1.9] - 2025-12-17

### Fixed
- GitHub Release workflow: fixed checksum generation to work with nested artifact directories

## [0.1.8] - 2025-12-17

### Changed
- Cleaned up release workflow: removed unused artifact organization step

## [0.1.7] - 2025-12-17

### Fixed
- npm publishing: regenerated index.js with correct binary names, removed optionalDependencies
- npm trusted publishing configuration
- Working-directory paths in npm publish job
- Replaced sccache with rust-cache in Python wheel builds

## [0.1.6] - 2025-12-16

### Added
- Copilot code review instructions with path-based rules (`.github/instructions/`)
- Automatic PR and issue labeling via GitHub Actions
- 31 repository labels for categorizing issues and PRs

### Changed
- Configured Trusted Publishing (OIDC) for crates.io, PyPI, and npm
- Updated GitHub Actions to latest versions (checkout@v6, setup-node@v6, setup-python@v6, upload-artifact@v6)
- Updated pytest-cov requirement to >=4.0,<8.0

### Fixed
- Package.json formatting
- Release notes template to use fastyaml-rs package names

## [0.1.5] - 2025-12-14

### Changed
- Release workflow verification with renamed packages

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
- README.md files for workspace crates (fast-yaml-core, fast-yaml-parallel)
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
- **fast-yaml-ffi**: Shared FFI utilities (removed in v0.5.0 - not used by bindings)
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

[unreleased]: https://github.com/bug-ops/fast-yaml/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/bug-ops/fast-yaml/compare/v0.4.1...v0.5.0
[0.4.1]: https://github.com/bug-ops/fast-yaml/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/bug-ops/fast-yaml/compare/v0.3.3...v0.4.0
[0.3.3]: https://github.com/bug-ops/fast-yaml/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/bug-ops/fast-yaml/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/bug-ops/fast-yaml/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/bug-ops/fast-yaml/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/bug-ops/fast-yaml/compare/v0.1.11...v0.2.0
[0.1.11]: https://github.com/bug-ops/fast-yaml/compare/v0.1.10...v0.1.11
[0.1.10]: https://github.com/bug-ops/fast-yaml/compare/v0.1.9...v0.1.10
[0.1.9]: https://github.com/bug-ops/fast-yaml/compare/v0.1.8...v0.1.9
[0.1.8]: https://github.com/bug-ops/fast-yaml/compare/v0.1.7...v0.1.8
[0.1.7]: https://github.com/bug-ops/fast-yaml/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/bug-ops/fast-yaml/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/bug-ops/fast-yaml/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/bug-ops/fast-yaml/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/bug-ops/fast-yaml/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/bug-ops/fast-yaml/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/bug-ops/fast-yaml/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/bug-ops/fast-yaml/releases/tag/v0.1.0
