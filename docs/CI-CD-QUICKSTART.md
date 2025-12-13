# CI/CD Quick Start Guide

Fast setup guide for the fast-yaml CI/CD system using cargo-make.

## TL;DR

```bash
# Install cargo-make
cargo install cargo-make

# Development workflow
cargo make dev         # Format + lint + test

# Before committing
cargo make pre-commit  # Fast checks

# Before pushing
cargo make pre-push    # Full CI simulation

# View all tasks
cargo make help
```

## Installation (5 minutes)

### 1. Install cargo-make

```bash
cargo install cargo-make
```

### 2. Install Rust nightly (for formatting)

```bash
rustup install nightly
rustup component add rustfmt --toolchain nightly
```

### 3. Install Python tools (using uv - recommended)

```bash
pip install uv
cd python
uv sync
```

**OR** using regular pip:

```bash
cd python
pip install -e ".[dev]"
```

### 4. Verify installation

```bash
cargo make verify-project
```

## Daily Workflows

### Starting Work

```bash
# Pull latest
git pull

# Quick check everything works
cargo make test
```

### Making Changes

```bash
# 1. Make code changes

# 2. Format code
cargo make format

# 3. Run tests
cargo make test

# 4. Check linting
cargo make clippy
```

### Before Committing

```bash
# Run pre-commit checks (format-check + clippy + test)
cargo make pre-commit

# If all passes, commit
git add .
git commit -m "feat: your changes"
```

### Before Pushing

```bash
# Full CI simulation (includes security checks)
cargo make pre-push

# If all passes, push
git push
```

## Common Tasks

### Rust Development

```bash
# Format code
cargo make format-rust

# Lint code
cargo make clippy

# Run tests
cargo make test-rust

# Generate coverage
cargo make coverage-html

# Build documentation
cargo make doc-open
```

### Python Development

```bash
# Build Python extension (debug mode, faster)
cargo make python-build-debug

# Run Python tests
cargo make python-test-verbose

# Type check
cargo make python-typecheck

# Python coverage
cargo make python-coverage
```

### Security Checks

```bash
# Run all security checks
cargo make ci-security

# Check for vulnerabilities
cargo make deny-advisories

# Audit dependencies
cargo make audit
```

### Watch Mode (Continuous Testing)

```bash
# Auto-run tests on file changes
cargo make watch

# Watch specific test pattern
cargo make watch-test -- test_name_pattern
```

## CI/CD Overview

### GitHub Actions Workflow

**Location**: `.github/workflows/ci.yml`

**Triggers**:
- Push to `main` branch
- Pull requests to `main`
- Weekly security audit (Sundays)

**Jobs**:
1. **format** - Format checking (1-2 min, Linux only)
2. **lint** - Clippy + docs (3-5 min, Linux only)
3. **security** - Vulnerability scanning (2-3 min, Linux only)
4. **test-rust** - Cross-platform tests (8-12 min, 4 platforms)
5. **coverage** - Code coverage (5-8 min, Linux only)
6. **msrv** - MSRV check (3-5 min, Linux only)
7. **test-python** - Python tests (6-10 min, 10 combinations)
8. **python-quality** - Type checking + linting (2-3 min, Linux only)
9. **python-coverage** - Python coverage (4-6 min, Linux only)
10. **release-build** - Release validation (8-12 min, main only)

**Total Time**: ~12-15 minutes (parallel execution)

### Test Matrix

**Rust Tests**:
- Linux (stable + beta)
- macOS (stable)
- Windows (stable)

**Python Tests**:
- Linux: Python 3.9, 3.10, 3.11, 3.12
- macOS: Python 3.9, 3.11, 3.12
- Windows: Python 3.9, 3.11, 3.12

### Caching

The CI uses multiple caching strategies:
- **Cargo cache** (Swatinem/rust-cache)
- **sccache** (compilation cache)
- **Python dependencies** (pip/uv cache)

**Result**: 60-80% faster builds on cache hit

## Troubleshooting

### "Format check failed"

```bash
# Fix formatting
cargo make format

# Commit
git commit -am "style: format code"
```

### "Clippy failed"

```bash
# Auto-fix where possible
cargo make clippy-fix

# Check remaining issues
cargo make clippy
```

### "Tests failed"

```bash
# Run tests locally
cargo make test-rust

# Run with verbose output
PYTEST_ARGS="-vv -s" cargo make python-test
```

### "Security audit failed"

```bash
# See details
cargo make deny-advisories

# Update dependencies
cargo update

# Re-check
cargo make deny
```

### "Python build failed"

```bash
# Ensure dependencies installed
cd python
uv sync  # or: pip install -e ".[dev]"

# Rebuild
cargo make python-build-debug
```

## cargo-make Task Reference

### Quick Reference

| Category | Task | Description |
|----------|------|-------------|
| **Dev** | `dev` | Format + lint + test |
| **Dev** | `watch` | Auto-run tests on changes |
| **Format** | `format` | Format all code |
| **Format** | `format-check` | Check formatting |
| **Lint** | `clippy` | Run Clippy |
| **Lint** | `clippy-fix` | Auto-fix Clippy issues |
| **Test** | `test` | Run Rust tests (nextest) |
| **Test** | `test-rust` | All Rust tests |
| **Test** | `python-test` | Run Python tests |
| **Coverage** | `coverage` | Generate lcov report |
| **Coverage** | `coverage-html` | HTML coverage report |
| **Security** | `deny` | cargo-deny checks |
| **Security** | `audit` | Vulnerability audit |
| **Docs** | `doc` | Build documentation |
| **Docs** | `doc-open` | Build + open docs |
| **CI** | `ci-rust` | Rust CI pipeline |
| **CI** | `ci-python` | Python CI pipeline |
| **CI** | `ci-security` | Security checks |
| **CI** | `ci-all` | Complete CI |

### Full Task List

```bash
# See all available tasks
cargo make --list-all-steps

# See task descriptions
cargo make help
```

## Performance Tips

### Speed Up Local Development

1. **Use debug builds for Python during development**:
   ```bash
   cargo make python-build-debug  # Faster than python-build
   ```

2. **Use watch mode for rapid iteration**:
   ```bash
   cargo make watch
   ```

3. **Run only what you need**:
   ```bash
   cargo make test           # Just tests
   cargo make clippy         # Just linting
   cargo make python-test    # Just Python tests
   ```

4. **Skip expensive checks**:
   ```bash
   cargo make test           # Skip coverage during dev
   cargo make ci-rust        # Skip Python checks if not needed
   ```

### Cache Optimization

The CI automatically manages caching, but locally:

```bash
# Clean all caches to troubleshoot
cargo clean
rm -rf target/

# Selective clean
cargo clean -p fast-yaml-core  # Clean specific package
```

## GitHub Repository Setup

### Required Secrets

Add to GitHub repository settings (Settings → Secrets → Actions):

1. **CODECOV_TOKEN**
   - Get from: https://codecov.io/
   - Purpose: Upload coverage reports

### Optional: Branch Protection

Recommended settings for `main` branch:

1. Go to: Settings → Branches → Branch protection rules
2. Add rule for `main`:
   - ✅ Require status checks to pass before merging
   - ✅ Require branches to be up to date before merging
   - Select: `CI Success` (this gates all other checks)
   - ✅ Require linear history
   - ✅ Include administrators

### Optional: Dependabot

Create `.github/dependabot.yml` for automatic dependency updates:

```yaml
version: 2
updates:
  - package-ecosystem: "cargo"
    directory: "/"
    schedule:
      interval: "weekly"
  - package-ecosystem: "pip"
    directory: "/python"
    schedule:
      interval: "weekly"
  - package-ecosystem: "github-actions"
    directory: "/"
    schedule:
      interval: "weekly"
```

## IDE Integration

### VS Code

Add to `.vscode/settings.json`:

```json
{
  "rust-analyzer.checkOnSave.command": "clippy",
  "rust-analyzer.checkOnSave.extraArgs": ["--all-targets"],
  "editor.formatOnSave": true,
  "editor.defaultFormatter": {
    "rust": "rust-lang.rust-analyzer",
    "python": "charliermarsh.ruff"
  }
}
```

Add to `.vscode/tasks.json`:

```json
{
  "version": "2.0.0",
  "tasks": [
    {
      "label": "cargo make dev",
      "type": "shell",
      "command": "cargo make dev",
      "group": {
        "kind": "build",
        "isDefault": true
      }
    },
    {
      "label": "cargo make test",
      "type": "shell",
      "command": "cargo make test",
      "group": "test"
    }
  ]
}
```

### Shell Aliases

Add to `~/.bashrc` or `~/.zshrc`:

```bash
alias cm='cargo make'
alias cmdev='cargo make dev'
alias cmtest='cargo make test'
alias cmpre='cargo make pre-commit'
alias cmci='cargo make ci-all'
```

Then use:
```bash
cm dev      # cargo make dev
cmtest      # cargo make test
cmpre       # cargo make pre-commit
```

## Next Steps

1. **Install cargo-make**: `cargo install cargo-make`
2. **Run verification**: `cargo make verify-project`
3. **Try development workflow**: `cargo make dev`
4. **Set up GitHub secrets**: Add `CODECOV_TOKEN`
5. **Configure branch protection**: Require `CI Success` check
6. **Read detailed docs**:
   - [cargo-make Guide](.local/cargo-make-guide.md)
   - [CI/CD Setup Documentation](.local/ci-cd-setup.md)

## Getting Help

### Documentation

- **cargo-make tasks**: `cargo make help`
- **Task details**: `cargo make --list-all-steps`
- **Detailed guide**: [.local/cargo-make-guide.md](.local/cargo-make-guide.md)
- **CI/CD docs**: [.local/ci-cd-setup.md](.local/ci-cd-setup.md)

### Resources

- [cargo-make repository](https://github.com/sagiegurari/cargo-make)
- [GitHub Actions docs](https://docs.github.com/en/actions)
- [Rust CI guide](https://doc.rust-lang.org/cargo/guide/continuous-integration.html)

### Common Questions

**Q: Why use cargo-make instead of just Makefiles?**
A: cargo-make is cross-platform (works on Windows), Rust-aware, and provides better task dependency management.

**Q: Can I skip CI checks locally?**
A: Yes, but not recommended. Use `cargo make test` for quick checks, `cargo make ci-all` for full validation.

**Q: How do I speed up CI?**
A: CI is already optimized with caching and parallelization. Local speedup: use watch mode and debug builds.

**Q: What if CI fails but works locally?**
A: Test on multiple platforms (Linux, macOS, Windows) and Python versions (3.9-3.12). Use Docker for Linux testing.

**Q: How often should I run `cargo make ci-all`?**
A: Before pushing to main or opening a PR. Use `cargo make dev` during development.

## Summary

**Essential Commands**:
```bash
cargo make dev          # Daily development
cargo make pre-commit   # Before committing
cargo make pre-push     # Before pushing
cargo make help         # View all tasks
```

**Key Files**:
- `Makefile.toml` - Task definitions
- `.github/workflows/ci.yml` - CI automation
- `.local/cargo-make-guide.md` - Detailed task guide
- `.local/ci-cd-setup.md` - CI/CD architecture docs

**Remember**: The same commands work locally and in CI, ensuring consistency and reliability.
