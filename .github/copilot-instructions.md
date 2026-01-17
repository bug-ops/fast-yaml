# fast-yaml Copilot Instructions

High-performance YAML 1.2.2 parser with Rust core and Python/Node.js bindings.

## Architecture

**Workspace structure** with 4 Rust crates + 2 binding packages:
- `crates/fast-yaml-core/` — Core parser/emitter wrapping yaml-rust2
- `crates/fast-yaml-linter/` — Linting engine with pluggable rules and rich diagnostics
- `crates/fast-yaml-parallel/` — Rayon-based multi-threaded document processing
- `crates/fast-yaml-ffi/` — Shared FFI utilities for bindings
- `python/` — PyO3 bindings (maturin build)
- `nodejs/` — NAPI-RS bindings

**Data flow**: Python/Node.js → FFI bindings → core crates → yaml-rust2

## Critical Commands

```bash
# Format (requires nightly for Edition 2024)
cargo +nightly fmt --all

# Lint (exclude python extension to avoid pyo3 issues)
cargo clippy --workspace --all-targets --exclude fast-yaml -- -D warnings

# Test (ALWAYS use nextest, not cargo test)
cargo nextest run --workspace

# Python development
uv sync && uv run maturin develop
uv run pytest tests/ -v

# Coverage
cargo llvm-cov nextest --workspace --html
```

## Code Conventions

**Error handling**: Use `thiserror` in library crates, `anyhow` in bindings.

**Unsafe code is denied by default**: `unsafe_code = "deny"` in workspace lints. Use `#[allow(unsafe_code)]` only for FFI boundaries (NAPI-RS, memory-mapping) with mandatory SAFETY documentation.

**YAML 1.2.2 compliance**: Unlike PyYAML (1.1), `yes/no/on/off` are strings, not booleans. Octal requires `0o` prefix.

**Type conversions** follow patterns in [python/src/conversion.rs](../python/src/conversion.rs) — convert between Rust `Value` types and Python/JS objects via FFI traits.

## Adding Features

**New lint rule**: Create in `crates/fast-yaml-linter/src/rules/`, implement `LintRule` trait, register in `Linter::with_all_rules()`.

**Python API**: Add function in `python/src/lib.rs` with `#[pyfunction]`, export in `#[pymodule]`, add type stub in `python/fast_yaml/_core.pyi`.

**New crate dependency**: Add to `[workspace.dependencies]` in root `Cargo.toml`, then reference with `dep.workspace = true`.

## Testing Patterns

- Unit tests in `#[cfg(test)]` modules within source files
- Integration tests in `crates/*/tests/` and `tests/`
- YAML spec fixtures in `tests/fixtures/yaml-spec/`
- Python tests use pytest in `tests/test_fast_yaml.py`

## Performance Notes

- Release builds use LTO + `codegen-units=1` for maximum optimization
- Python GIL released during CPU-intensive Rust operations
- Parallel processing splits at `---` document boundaries
- Input size capped at 100MB (`MAX_INPUT_SIZE`) for DoS protection

## Code Review Guidelines

### Review Checklist for PRs

**Pre-merge requirements:**
- [ ] All CI checks passing (format, clippy, tests, coverage, security)
- [ ] Code coverage maintained or improved (minimum 60% overall)
- [ ] Documentation updated (API docs, README, CHANGELOG)
- [ ] Tests added for new functionality
- [ ] No clippy warnings introduced
- [ ] Commit messages follow conventional commits format
- [ ] Breaking changes documented and justified

**Code quality:**
- [ ] Code follows Rust idioms and patterns
- [ ] Error handling is comprehensive and informative
- [ ] No `unwrap()` or `expect()` in library code (only in tests)
- [ ] Public APIs have documentation comments with examples
- [ ] Complex logic has explanatory comments
- [ ] No dead code or unused imports

**Performance:**
- [ ] No unnecessary allocations in hot paths
- [ ] Appropriate use of `&str` vs `String`, `&[T]` vs `Vec<T>`
- [ ] Clone operations are justified
- [ ] Large data structures use references when possible

### Rust-Specific Review Points

**Ownership and lifetimes:**
- Check for unnecessary clones that could be borrows
- Verify lifetime annotations are correct and minimal
- Ensure `'static` lifetimes are truly necessary
- Look for potential iterator chaining instead of collecting intermediate results

**Error handling:**
- Library crates use `thiserror` for custom error types
- Binding crates use `anyhow` with context chains
- Errors include source location information (file, line, column) for diagnostics
- Error messages are actionable and user-friendly
- `Result` types are properly propagated with `?` operator

**Clippy compliance:**
- Run `cargo clippy --workspace --all-targets -- -D warnings -W clippy::pedantic`
- Address all warnings or explicitly allow with justification
- Common issues to watch:
  - `clippy::missing_errors_doc` — document error conditions
  - `clippy::must_use_candidate` — add `#[must_use]` where appropriate
  - `clippy::redundant_closure` — use method references
  - `clippy::unnecessary_wraps` — remove `Result` if error path is impossible

**Memory safety:**
- Minimize `unsafe` code (denied by default in workspace lints)
- FFI requires unsafe (NAPI-RS, memory-mapping) - ensure proper encapsulation and SAFETY comments
- All unsafe code must have `#[allow(unsafe_code)]` and SAFETY documentation
- Verify bounds checking on slice operations
- Check for potential panics (`unwrap`, `expect`, indexing)

**Concurrency:**
- Parallel code uses Rayon work-stealing correctly
- No data races (Rust prevents at compile-time, but check logic)
- Shared state is properly synchronized (Mutex, RwLock, atomic types)
- Thread pool sizes are configurable or auto-detected

### Python Bindings Review Points

**PyO3 patterns:**
- All `#[pyfunction]` have corresponding type stubs in `_core.pyi`
- Python exceptions use `PyErr::new_err` or `anyhow` conversion
- GIL released with `py.allow_threads(|| ...)` for CPU-intensive operations
- Python objects converted safely (handle `None`, check types)
- Memory management: no circular references between Rust and Python

**Type stubs accuracy:**
- Stub signatures match actual function implementations
- Generic types properly annotated (`list[str]`, `dict[str, Any]`)
- Optional parameters marked with `Optional[T]` or `T | None`
- Return types accurate (including union types for multiple return paths)
- Docstrings in stubs match Rust documentation

**Python API design:**
- Functions are Pythonic (snake_case, keyword arguments)
- Error messages use Python conventions
- Integration with Python ecosystem (pathlib, typing, etc.)
- Performance comparable to or better than pure Python alternatives

**Testing:**
- Pytest tests cover normal and error cases
- Tests verify error messages are helpful
- Round-trip tests (serialize → deserialize → compare)
- Tests run on all supported Python versions (3.9+)

### Node.js Bindings Review Points

**NAPI-RS patterns:**
- All exports have TypeScript type definitions
- JavaScript exceptions properly converted from Rust errors
- Async functions use `#[napi(ts_return_type = "Promise<T>")]`
- Buffer handling is zero-copy where possible
- Node.js objects converted safely with proper error handling

**TypeScript definitions:**
- Type definitions auto-generated or manually maintained
- JSDoc comments included for API documentation
- Generic types properly constrained
- Union types for variant returns
- Export types for public interfaces

**Node.js API design:**
- Functions are idiomatic JavaScript (camelCase, promises)
- Integration with Node.js ecosystem (streams, events)
- Performance competitive with native JS YAML parsers
- Tree-shakeable exports for bundle size optimization

**Testing:**
- Mocha/Jest tests cover normal and error paths
- Tests verify error messages are actionable
- Round-trip serialization tests
- Tests run on Node.js LTS versions (16+)

### Security Review Requirements

**Input validation:**
- All external input validated before processing
- Size limits enforced (100MB cap via `MAX_INPUT_SIZE`)
- No arbitrary code execution paths
- Safe handling of malicious YAML (billion laughs, anchor bombs)

**Dependency security:**
- Run `cargo audit` to check for known vulnerabilities
- Run `cargo deny check advisories` for comprehensive scanning
- All dependencies from trusted sources (crates.io)
- Minimal dependency tree (avoid bloat)

**License compliance:**
- All dependencies use approved licenses (MIT, Apache-2.0, BSD-3-Clause)
- Run `cargo deny check licenses` before adding dependencies
- Verify dual-licensing is preserved (MIT OR Apache-2.0)

**Secrets and sensitive data:**
- No hardcoded secrets or API keys
- No logging of sensitive data
- Secure handling of file paths (no path traversal)
- No debug output in release builds with sensitive info

**FFI safety:**
- PyO3/NAPI-RS bindings properly encapsulate unsafe code
- No memory leaks across FFI boundary
- Panic handling with `catch_unwind` in FFI entry points
- Validate all pointers and references from foreign code

### Performance Review Considerations

**Benchmarking:**
- Criterion benchmarks exist for critical paths
- Before/after comparisons for optimization PRs
- No performance regressions without justification
- Large file benchmarks (1MB, 10MB, 100MB) pass

**Profiling:**
- Profile with `cargo flamegraph` for hot paths
- Check for unexpected allocations with `dhat`
- Verify parallel speedup with different thread counts
- Memory usage within acceptable bounds

**Optimization checklist:**
- [ ] Use `&str` slices instead of `String` where possible
- [ ] Avoid unnecessary `clone()` operations
- [ ] Use iterator adapters instead of intermediate collections
- [ ] Leverage Rayon for embarrassingly parallel work
- [ ] Cache expensive computations
- [ ] Use `SmallVec` or similar for small collections

**Release build verification:**
- LTO enabled in `Cargo.toml` (`lto = "fat"`)
- Codegen units optimized (`codegen-units = 1`)
- Panic behavior set to abort (`panic = "abort"`)
- Strip debug symbols (`strip = true`)

## PR Standards

### Commit Message Conventions

Follow [Conventional Commits](https://www.conventionalcommits.org/) format:

```
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

**Types:**
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation only
- `style`: Formatting, missing semicolons, etc.
- `refactor`: Code restructuring without behavior change
- `perf`: Performance improvement
- `test`: Adding or updating tests
- `chore`: Maintenance tasks (deps, CI, etc.)

**Scopes:**
- `core`: fast-yaml-core crate
- `linter`: fast-yaml-linter crate
- `parallel`: fast-yaml-parallel crate
- `ffi`: fast-yaml-ffi crate
- `python`: Python bindings
- `nodejs`: Node.js bindings
- `ci`: CI/CD workflows
- `deps`: Dependencies

**Examples:**

```
feat(linter): add duplicate key detection rule

Implements a new lint rule that detects duplicate keys in YAML mappings
and provides diagnostic information with line numbers.

Closes #42
```

```
fix(python): handle None values in dict conversion

Previously, None values in Python dicts would panic during conversion.
Now they are correctly mapped to YAML null values.

Fixes #53
```

```
perf(parallel): optimize document chunking algorithm

Reduces memory allocations by 40% by using zero-copy slicing for
document boundary detection.

Benchmark results show 25% speedup on 10MB files.
```

```
docs: update README with installation instructions

BREAKING CHANGE: Minimum Python version increased to 3.9
```

### PR Title Format

**Format:** `<type>[scope]: <description>`

**Examples:**
- `feat(linter): add schema validation rule`
- `fix(python): correct error handling in safe_load`
- `docs: add API reference for parallel processing`
- `chore(deps): update yaml-rust2 to 0.10.5`

### Required Checks Before Merge

**Automated CI checks (all must pass):**
1. **Format check**: `cargo +nightly fmt --all -- --check`
2. **Clippy linting**: `cargo clippy --workspace --all-targets -- -D warnings`
3. **Tests**: `cargo nextest run --workspace` (100% pass rate)
4. **Code coverage**: `cargo llvm-cov nextest` (≥60% coverage)
5. **Security audit**: `cargo deny check advisories` (0 critical vulnerabilities)
6. **License check**: `cargo deny check licenses` (all approved)
7. **Documentation**: `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`
8. **SemVer check**: `cargo semver-checks check-release` (no breaking changes)
9. **Python tests**: `uv run pytest tests/ -v` (if Python code changed)

**Manual review checks:**
- [ ] At least one approving review from maintainer
- [ ] All review comments resolved
- [ ] CHANGELOG.md updated (if user-facing change)
- [ ] README.md updated (if API changed)
- [ ] Migration guide provided (if breaking change)

**Branch requirements:**
- Up-to-date with target branch (main/develop)
- No merge conflicts
- Signed commits (recommended)

### Breaking Change Handling

**Identification:**
- SemVer breaking changes flagged by `cargo semver-checks`
- API signature changes in public modules
- Behavior changes that could affect users
- Dependency MSRV increases

**Documentation:**
- Add `BREAKING CHANGE:` footer in commit message
- Document migration path in CHANGELOG.md
- Update version number appropriately (major version bump)
- Add migration guide in docs/ if necessary

**Example breaking change commit:**

```
feat(core)!: change parse() to return iterator instead of Vec

BREAKING CHANGE: parse() now returns impl Iterator<Item = Document>
instead of Vec<Document> for better memory efficiency.

Migration: Call .collect() to get Vec<Document> if needed:
  let docs: Vec<Document> = parse(input)?.collect();

This reduces memory usage by 50% on large files.

Closes #67
```

**Review requirements for breaking changes:**
- Two approving reviews required
- Discussion in issue/RFC before PR
- Clear justification for breaking change
- Complete migration guide

## Copilot Review Agent Instructions

### Automatic Check Priorities

**High priority (always flag):**
1. **Unsafe code**: Flag any `unsafe` blocks without `#[allow(unsafe_code)]` and SAFETY documentation (denied by workspace lints)
2. **Unwrap/expect in library code**: Suggest `?` operator or `match`
3. **Missing error documentation**: Public functions returning `Result` need `# Errors` section
4. **Missing tests**: New functions without corresponding tests
5. **Hardcoded values**: Magic numbers or strings that should be constants
6. **TODO/FIXME comments**: Should be tracked in issues, not committed
7. **Panics in production code**: `panic!`, `.unwrap()`, indexing without bounds check

**Medium priority (suggest improvements):**
1. **Clone operations**: Could this be a borrow instead?
2. **Allocations in loops**: Can we reserve capacity or use iterators?
3. **Missing documentation**: Public items without doc comments
4. **Long functions**: Functions >50 lines should be refactored
5. **Complex conditionals**: Nested `if` >3 levels deep
6. **Duplicate code**: Similar code blocks that could be extracted

**Low priority (style suggestions):**
1. **Variable naming**: Non-descriptive names like `x`, `tmp`, `data`
2. **Import organization**: Group std, external, internal imports
3. **Trailing whitespace**: Format with `cargo +nightly fmt`
4. **Dead code**: Unused functions or imports

### Common Issues to Flag

**Rust-specific:**
- [ ] Using `.clone()` when borrow would work
- [ ] Using `String` when `&str` would suffice
- [ ] Not using `?` operator for error propagation
- [ ] Missing `#[derive(Debug)]` on public types
- [ ] Not using `#[non_exhaustive]` on enums that might grow
- [ ] Using `Vec::new()` when capacity is known (use `with_capacity`)
- [ ] Ignoring `Result` with `let _ =` (use `#[must_use]`)
- [ ] Synchronous I/O in potentially async contexts

**PyO3 bindings:**
- [ ] Not releasing GIL for CPU-intensive operations
- [ ] Missing type stub for `#[pyfunction]`
- [ ] Type stub signature doesn't match implementation
- [ ] Using `unwrap()` instead of PyO3 error conversion
- [ ] Not handling `None` values in Python→Rust conversion
- [ ] Memory leaks from circular references

**NAPI-RS bindings:**
- [ ] Missing TypeScript type definition
- [ ] Async function without proper `Promise<T>` annotation
- [ ] Not using zero-copy buffer operations
- [ ] Synchronous blocking in Node.js event loop
- [ ] Missing error context in JS exception

**Testing:**
- [ ] Test name not descriptive (`test_1`, `test_basic`)
- [ ] No assertion in test
- [ ] Test only checks happy path, no error cases
- [ ] Missing edge case tests (empty input, max size, etc.)
- [ ] Flaky test with timing dependencies

**Security:**
- [ ] Unbounded recursion (could stack overflow)
- [ ] Unbounded memory allocation (DoS risk)
- [ ] No input size validation
- [ ] Path traversal vulnerability in file operations
- [ ] Deserializing untrusted data without validation

### Patterns to Suggest

**Error handling:**
```rust
// Instead of:
let value = some_function().unwrap();

// Suggest:
let value = some_function()
    .context("Failed to execute some_function")?;
```

**Borrowing:**
```rust
// Instead of:
fn process(input: String) -> String { ... }

// Suggest:
fn process(input: &str) -> String { ... }
```

**Iterator chains:**
```rust
// Instead of:
let mut results = Vec::new();
for item in items {
    if item.is_valid() {
        results.push(item.process());
    }
}

// Suggest:
let results: Vec<_> = items
    .iter()
    .filter(|item| item.is_valid())
    .map(|item| item.process())
    .collect();
```

**Error context:**
```rust
// Instead of:
fs::read_to_string(path)?

// Suggest:
fs::read_to_string(path)
    .with_context(|| format!("Failed to read file: {}", path.display()))?
```

**GIL release in PyO3:**
```rust
// Instead of:
#[pyfunction]
fn expensive_operation(py: Python, input: &str) -> PyResult<String> {
    // Heavy computation
    let result = parse_and_process(input)?;
    Ok(result)
}

// Suggest:
#[pyfunction]
fn expensive_operation(py: Python, input: &str) -> PyResult<String> {
    let input = input.to_string();
    py.allow_threads(|| {
        parse_and_process(&input)
    }).map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
}
```

### Review Response Template

When reviewing, structure feedback as:

```markdown
## Summary
[Brief overview of changes and overall assessment]

## Strengths
- [Positive aspects of the PR]

## Required Changes
- [ ] [Critical issues that must be fixed before merge]

## Suggested Improvements
- [ ] [Optional improvements that would enhance code quality]

## Questions
- [Clarifications needed from the author]

## Security/Performance Notes
[Any security or performance implications]
```
