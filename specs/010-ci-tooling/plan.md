---
aliases:
  - CI tooling plan
tags:
  - sdd
  - plan
  - ci
created: 2026-11-01
status: reverse-specified
related:
  - "[[spec]]"
  - "[[constitution]]"
  - "[[009-limits-security/spec|Limits and security]]"
---

# Technical Plan: CI tooling

> [!info] References
> **Spec**: [[spec]]. Reverse-specified from the implementation in PR #669 (issue #665, not yet merged); it is not a proposal. Adoption decision and measurements: issue #668.

## 1. Architecture

### Approach

`.config/mise/config.toml` is the single source for tool versions and check commands. Each CI job runs: checkout, the Rust toolchain step (rustup, unchanged), `rust-cache`, `jdx/mise-action` with `install_args` for the job's tools, then `mise run <task>`. Developers run the same tasks locally. Version legs set `MISE_ENV` (job-level for Python; a step before the setup action for the Node 22 leg) so mise merges `config.<env>.toml` over `config.toml` and reads `mise.<env>.lock`, all in `.config/mise/`.

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
| Build graph | `build:python` and `build:nodejs` depend on `build:fy`; tests depend on their build only | `fy` and each extension build once, never concurrently | Tests depending on several builds |
| Version assertions | Post-setup steps compare the interpreter with the matrix version | A silently ignored `MISE_ENV` would test the wrong version and stay green | Trust `MISE_ENV` |
| Failure reporting | `!cancelled()` on independent steps in Lint (rust) and Python Quality Checks | One push reports every failed class | Fail-fast steps |
| Build tasks | No `sources`/`outputs` | rust-cache restores `target/`; mtime skip could reuse a stale `fy` | Declared sources |

## 2. Job-to-task map

| CI job | Task(s) | `install_args` |
|--------|---------|----------------|
| Lint (rust) | `fmt:check:rust`, `lint:clippy`, `doc` | none (`install: false`; no mise tools) |
| Clippy Feature Powerset | `lint:clippy-hack` | `github:taiki-e/cargo-hack` |
| Security Audit | `deny`, `semver` | `aqua:EmbarkStudios/cargo-deny`, `github:obi1kenobi/cargo-semver-checks` |
| Test Rust | `test:rust` | `github:nextest-rs/nextest[version_prefix=cargo-nextest-]` |
| Test Core (no arena) | `test:core-no-arena` | `github:nextest-rs/nextest[version_prefix=cargo-nextest-]` |
| Code Coverage | `coverage:rust` | `aqua:taiki-e/cargo-llvm-cov`, `github:nextest-rs/nextest[version_prefix=cargo-nextest-]` |
| Python Quality Checks | `lint:python`, `fmt:check:python` | `python uv` |
| NodeJS Quality Checks | `fmt:check:nodejs`, `audit:nodejs`, `lint:nodejs` | `node pnpm` |
| Test Python | `test:python` (`MISE_ENV` py310 / py314) | `python uv` |
| Test NodeJS | `test:nodejs` (`MISE_ENV=node22` on the Node 22 leg; unset otherwise) | `node pnpm` |
| Python / NodeJS Coverage | `coverage:python` / `coverage:nodejs` | per job |
| NodeJS Build | `release:nodejs` | `node pnpm` |
| Check MSRV | `msrv` (toolchain from rustup, `install: false` mise step) | not applicable |
| Release Build Check | unchanged (rustup only) | not applicable |

The workflow has no `mise-ci` job and CI never calls the `ci` task; it is the local aggregate only. `ci-success` ("CI Success") is the only required check (ruleset `master`), so other job names may change; it needs `changes` (must be `success`), `lint`, `clippy-powerset`, `security`, `test-rust`, `test-core-no-arena`, `coverage`, `msrv`, `test-python`, `python-quality`, `python-coverage`, `test-nodejs`, `nodejs-quality` and `nodejs-coverage`. Test jobs wait for `lint` (and for the matching quality job). The `Test Rust (windows-latest)` leg also runs `lint:clippy-windows` (FR-029). Issue #665's proposed layout, lock platforms and `mise-ci` job differ from this implementation: see the deviations table in [[spec]] section 8.

Install ids are load-bearing: `MISE_TASK_RUN_AUTO_INSTALL=false` makes a mismatch between `install_args` and the config fail the job. The Python and Node.js assertion steps (`uv python find` in `python/`; `node --version`) follow each setup step in `test-python` and `test-nodejs`.

### Traceability

| FR | Task | CI job | Verification |
|----|------|--------|--------------|
| FR-004, FR-022 | install step | all mise jobs | verified on Linux, macOS and Windows (run 41342007315) |
| FR-006, FR-023 | `test:python`, `test:nodejs` | Test Python, Test NodeJS | `mise install --locked --dry-run` per env locally; legs verified in run 41342007315 (`py310` and `node22` on Linux only) |
| FR-011 | every task | the replaced job | by comparison with the pre-#669 workflow |
| FR-012 | `ci` | none (local only) | local `mise run ci`; not run in CI |
| FR-014, FR-016 | `build:*` | Test Python, Test NodeJS | by inspection |
| FR-015 | `test:python`, `test:nodejs` | Test * (Windows) | verified (run 41342007315) |
| FR-021 | all | all | by inspection; a missing id fails the job |
| FR-027 | assertion steps | Test Python, Test NodeJS | verified on Windows and macOS (run 41342007315) |
| FR-028 | `lint:clippy`, `doc`, `lint:python`, `fmt:check:python` | Lint (rust), Python Quality Checks | by inspection |

## 3. Files

```
.config/mise/
  config.toml          default tools and all tasks
  mise.lock            checksums for the default tools
  config.py310.toml    Python 3.10 leg      mise.py310.lock
  config.py314.toml    Python 3.14 leg      mise.py314.lock
  config.node22.toml   Node.js 22 leg       mise.node22.lock
.github/workflows/ci.yml   jobs call mise-action + mise run
```

## 4. Security

- Actions are pinned to full commit SHAs with version comments; `jdx/mise-action` runs with `cache: false`, so only checksum-verified downloads are used.
- `MISE_SAFE=1` is set on tool-installing steps only, because safe mode disables `mise run`; the workflow keeps `permissions: contents: read` and `persist-credentials: false`.
- The mise config is executable (tasks, `[env]`): no secrets in it, and its changes are reviewed like a build script. Fork-PR behavior is unverified ([[spec]] section 8 item 8).

## 5. Testing strategy and first-run checklist

- Local: `mise install --locked` (default and each `MISE_ENV`), `MISE_ENV=py310 mise run test:python`, `mise run ci`; actionlint and zizmor on `ci.yml`.
- CI: the Assert steps guard the selected interpreter.
- First full CI run: check the Windows legs (Test Rust, Test Python, Test NodeJS, NodeJS Build), the macOS legs, `install_args` with the nextest id, and `MISE_SAFE` on installs; record the result in issue #668. Fork PRs are checked on the first fork PR.

## 6. Rollout and rollback

PR #669 migrates the jobs directly, without a parallel comparison job. Measurements and the adoption decision are tracked in issue #668. Rollback is described in [[spec]] section 8.

## 7. Constitution compliance

The quality gate (constitution principle 9) is unchanged: tasks keep `-D warnings` for clippy, tests and doc, and each task equals the command it replaced.

## 8. Risks

| Risk | Mitigation |
|------|------------|
| Lock drift or a lock diff nobody reads | FR-033: review lock diffs; `--locked` in CI |
| Pinned mise version goes stale (Dependabot does not bump it) | FR-020: change all 13 `version:` occurrences together and keep them equal to the lock-generating release |
| Windows shell differences | Per-task `shell` or `run` only if the first run fails |
| Cold downloads with `cache: false` | Measure in #668 |
| Wrong interpreter on a version leg | Assertion steps (FR-027) |

## 9. Maintenance

Tool bump: edit `.config/mise/config.toml` (or the env file), run `mise lock --platform linux-x64,linux-arm64,macos-arm64,windows-x64` for the default and each `MISE_ENV`, verify `mise install --locked`, and add the tool to `install_args` where a job needs it. `mise outdated` lists stale pins.
