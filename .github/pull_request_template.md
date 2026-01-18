## Summary

<!-- Provide a brief description of your changes -->

## Type of Change

<!-- Check all that apply -->

- [ ] Bug fix (non-breaking change that fixes an issue)
- [ ] New feature (non-breaking change that adds functionality)
- [ ] Breaking change (fix or feature that would cause existing functionality to not work as expected)
- [ ] Performance improvement
- [ ] Documentation update
- [ ] Code refactoring
- [ ] CI/CD changes
- [ ] Dependency updates
- [ ] Other (please describe):

## Related Issues

<!-- Link to related issues using keywords like "Fixes #123" or "Closes #456" -->

Fixes #

## Motivation and Context

<!-- Why is this change needed? What problem does it solve? -->

## Changes Made

<!-- Describe the changes in detail -->

-
-
-

## Component Impact

<!-- Check all components affected by this PR -->

- [ ] fast-yaml-core (Core parser)
- [ ] fast-yaml-linter (Linter)
- [ ] fast-yaml-parallel (Parallel processing)
- [ ] Python bindings
- [ ] NodeJS bindings
- [ ] CLI tool
- [ ] Documentation
- [ ] CI/CD
- [ ] Tests
- [ ] Other:

## Breaking Changes

<!-- If this is a breaking change, describe what breaks and how to migrate -->

**Breaking:** No / Yes

<!-- If yes, describe: -->
- **What breaks:**
- **Migration path:**
- **Deprecation period:** (if applicable)

## Testing Evidence

### Test Coverage

<!-- Check all that apply -->

- [ ] Added new tests
- [ ] Updated existing tests
- [ ] All tests pass locally
- [ ] Code coverage meets targets (≥60% overall, ≥70% business logic, ≥80% critical)

### Test Results

```bash
# Paste test output here
cargo nextest run --workspace --exclude fast-yaml --exclude fast-yaml-nodejs
```

### Coverage Report

```bash
# Paste coverage summary here
cargo llvm-cov --workspace --exclude fast-yaml --exclude fast-yaml-nodejs
```

## Performance Impact

<!-- Check one -->

- [ ] No performance impact
- [ ] Performance improved (provide benchmarks below)
- [ ] Performance may be affected (provide analysis below)

### Benchmarks (if applicable)

```bash
# Paste benchmark results here
```

## Security Considerations

<!-- Check all that apply -->

- [ ] No security impact
- [ ] Security scan passed (`cargo deny check`)
- [ ] Added security tests
- [ ] Reviewed for common vulnerabilities
- [ ] Updated SECURITY.md (if needed)

### Security Scan Results

```bash
# Rust dependencies
cargo deny check

# NodeJS dependencies (if applicable)
cd nodejs && npm audit
```

## Quality Checklist

<!-- All items must be checked before merging -->

### Code Quality

- [ ] Code follows project style guidelines
- [ ] Formatted with `cargo +nightly fmt --all`
- [ ] Passes `cargo clippy` with no warnings
- [ ] No new compiler warnings
- [ ] Added documentation for public APIs
- [ ] Documentation builds without warnings

### Rust Workspace (if applicable)

- [ ] Excluded FFI crates from workspace commands
- [ ] Used `--exclude fast-yaml --exclude fast-yaml-nodejs` where needed
- [ ] Updated Cargo.toml dependencies if needed
- [ ] Checked for dependency conflicts

### Python Bindings (if applicable)

- [ ] Tests pass: `uv run pytest tests/ -v`
- [ ] Code formatted: `uv run ruff format python/`
- [ ] Linting passes: `uv run ruff check python/`
- [ ] Type checking passes: `uv run mypy python/fast_yaml/`
- [ ] Updated type stubs if needed

### NodeJS Bindings (if applicable)

- [ ] Tests pass: `npm test`
- [ ] Code formatted: `npm run format`
- [ ] Linting passes: `npm run lint`
- [ ] Type checking passes: `npm run typecheck`
- [ ] Updated TypeScript definitions if needed

### Documentation

- [ ] README updated (if needed)
- [ ] CHANGELOG.md updated
- [ ] API documentation updated (if needed)
- [ ] Examples updated (if needed)
- [ ] Migration guide added (if breaking change)

### Testing

- [ ] Unit tests added/updated
- [ ] Integration tests added/updated (if needed)
- [ ] Edge cases covered
- [ ] Error cases tested
- [ ] Coverage targets met

### CI/CD

- [ ] All CI checks pass
- [ ] No new security vulnerabilities
- [ ] License compliance verified
- [ ] SemVer check passes (if applicable)

## Commands Run

<!-- List all quality check commands you ran locally -->

```bash
# Format check
cargo +nightly fmt --all -- --check

# Lint
cargo clippy --workspace --all-targets --exclude fast-yaml --exclude fast-yaml-nodejs -- -D warnings

# Tests
cargo nextest run --workspace --exclude fast-yaml --exclude fast-yaml-nodejs

# Coverage
cargo llvm-cov --workspace --exclude fast-yaml --exclude fast-yaml-nodejs

# Documentation
cargo doc --workspace --no-deps --exclude fast-yaml --exclude fast-yaml-nodejs

# Security
cargo deny check

# Python (if applicable)
cd python
uv run pytest tests/ -v
uv run ruff check python/
uv run mypy python/fast_yaml/

# NodeJS (if applicable)
cd nodejs
npm test
npm run check
```

## Additional Notes

<!-- Any additional information, screenshots, or context -->

## Checklist for Reviewers

<!-- For maintainers reviewing this PR -->

- [ ] Code quality is acceptable
- [ ] Tests are comprehensive
- [ ] Documentation is clear and complete
- [ ] Breaking changes are justified and documented
- [ ] Performance impact is acceptable
- [ ] Security considerations are addressed
- [ ] Follows project architecture and design principles
- [ ] CHANGELOG.md is updated appropriately

## Post-Merge Actions

<!-- Actions to take after merging (if any) -->

- [ ] Update documentation site
- [ ] Announce breaking changes
- [ ] Update examples repository
- [ ] Create follow-up issues
- [ ] Other:

---

**Note:** Please review [CONTRIBUTING.md](../CONTRIBUTING.md) before submitting your PR.
