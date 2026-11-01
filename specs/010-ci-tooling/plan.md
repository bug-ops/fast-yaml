---
aliases:
  - CI tooling plan
tags:
  - sdd
  - plan
  - ci
created: 2026-11-01
status: living
related:
  - "[[spec]]"
  - "[[constitution]]"
---

# Technical Plan: CI tooling

> [!info] References
> **Spec**: [[spec]]. Describes the implementation from PR #669 (issue #665); it is not a proposal.

## 1. Architecture

### Approach

`mise.toml` is the single source for tool versions and check commands. Each CI job runs: checkout, the Rust toolchain step (rustup, unchanged), `rust-cache`, `jdx/mise-action` with `install_args` for the job's tools, then `mise run <task>`. Developers run the same tasks locally. Version legs set `MISE_ENV` so mise merges `mise.<env>.toml` over `mise.toml` and reads `mise.<env>.lock`.

### Key design decisions

| Decision | Choice | Rationale | Alternatives |
|----------|--------|-----------|--------------|
| Backends | `aqua:` where the registry has the tool, else `github:` | Checksums and attestations without compiling; aqua lacks cargo-nextest, cargo-hack and cargo-semver-checks | `cargo:` (source builds, top-level-only locking) |
| Rust | Stays on rustup | stable, nightly, beta and MSRV legs; clippy components | mise Rust tool |
| `ci` task | Sequential `run = [{task=...}]` | Parallel `depends` raced `pnpm install` and the cargo lock | Parallel with locks |
| Safe mode | `MISE_SAFE=1` on the install step only | Safe mode disables `mise run` | Job-level env |
| Cache | `cache: false` | Only checksum-verified downloads are used | Action cache |
| Tool subset | `install_args` per job plus `MISE_TASK_RUN_AUTO_INSTALL=false` | Faster jobs; a missing tool fails loudly | Install everything |
| Version legs | `MISE_ENV` files with own lockfile | Each tested Python/Node.js stays lock-pinned | Matrix `setup-*` versions |
| Build tasks | No `sources`/`outputs` | rust-cache restores `target/`; mtime skip could reuse a stale `fy` | Declared sources |

## 2. Job-to-task map

| CI job | Task(s) | `install_args` |
|--------|---------|----------------|
| Lint (rust) | `fmt:check:rust`, `lint:clippy`, `doc` | `install: false` (no mise tools) |
| Clippy Feature Powerset | `lint:clippy-hack` | `cargo-hack` |
| Security Audit | `deny`, `semver` | `cargo-deny`, `cargo-semver-checks` |
| Test Rust | `test:rust` | `nextest` |
| Test Core (no arena) | `test:core-no-arena` | `nextest` |
| Code Coverage | `coverage:rust` | `cargo-llvm-cov`, `nextest` |
| Python Quality Checks | `lint:python`, `fmt:check:python` | `python uv` |
| NodeJS Quality Checks | `fmt:check:nodejs`, `audit:nodejs`, `lint:nodejs` | `node pnpm` |
| Test Python | `test:python` (`MISE_ENV` py310 / py314) | `python uv` |
| Test NodeJS | `test:nodejs` (`MISE_ENV` node22 or empty) | `node pnpm` |
| Python / NodeJS Coverage | `coverage:python` / `coverage:nodejs` | per job |
| NodeJS Build | `release:nodejs` | `node pnpm` |
| Check MSRV, Release Build Check | unchanged (rustup only) | not applicable |

The workflow has no `mise-ci` job and CI never calls the `ci` task; it is the local aggregate only. `ci-success` ("CI Success") is the only required check (ruleset `master`), so other job names may change; it needs `lint`, `clippy-powerset`, `security`, `test-rust`, `test-core-no-arena`, `coverage`, `msrv`, `test-python`, `python-quality`, `python-coverage`, `test-nodejs`, `nodejs-quality` and `nodejs-coverage`. Test jobs wait for `lint` (and for the matching quality job).

## 3. Files

```
mise.toml            default tools and all tasks
mise.lock            checksums for the default tools
mise.py310.toml      Python 3.10 leg      mise.py310.lock
mise.py314.toml      Python 3.14 leg      mise.py314.lock
mise.node22.toml     Node.js 22 leg       mise.node22.lock
.github/workflows/ci.yml   jobs call mise-action + mise run
```

## 4. Maintenance

Tool bump: edit `mise.toml` (or the env file), run `mise lock --platform linux-x64,linux-arm64,macos-arm64,windows-x64` for the default and each `MISE_ENV`, verify `mise install --locked`, and add the tool to `install_args` where a job needs it. `mise outdated` lists stale pins.
