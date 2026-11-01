---
aliases:
  - CI tooling
  - mise toolchain
tags:
  - sdd
  - spec
  - ci
  - tooling
created: 2026-11-01
status: living
related:
  - "[[constitution]]"
  - "[[009-limits-security/spec|Limits and security]]"
---

# Feature: CI tooling (mise toolchain and tasks)

> [!info] Metadata
> **Scope**: `mise.toml`, `mise.<env>.toml`, the matching lockfiles and the jobs of `.github/workflows/ci.yml` that install tools and run checks. Not product behavior: this spec covers how the repository's checks are installed and run.
> **Baseline**: PR #669 (issue #665). Describes the implementation as proposed in that PR; items unverified in a real CI run are listed in section 8.

## 1. Purpose and value

Before this feature each CI job installed its own tools through a different action (`setup-node`, `setup-python`, `setup-uv`, `pnpm/action-setup`, `taiki-e/install-action`) and repeated the check commands inline, so a contributor could not reproduce a job without reading the workflow. This feature pins the non-Rust tools in one lockfile and defines every check as a named task, so CI and a developer's machine run the same commands with the same tool versions.

### Non-goals

- Replacing rustup. Rust toolchains (stable, nightly, beta, MSRV) are not managed by mise.
- Requiring mise for contributors or users. Plain `cargo`, `pnpm` and `uv` stay supported.
- Changing what the checks verify. A task is a verbatim move of the command CI ran before.
- Release automation.

## 2. User stories

### US-001 Reproduce a CI job locally (P1)

AS A contributor I WANT to run the exact command of a failing CI job SO THAT I can fix it without pushing.

```
GIVEN a failing "Lint (rust)" job
WHEN  I run `mise install --locked && mise run lint:clippy`
THEN  the same cargo invocation as CI runs with RUSTFLAGS="-D warnings"
```

### US-002 Run the pre-PR gate with one command (P1)

AS A contributor I WANT `mise run ci` to run format, lint, doc, test and deny checks sequentially SO THAT I do not have to remember the list.

### US-003 Update a tool version safely (P2)

AS A maintainer I WANT a version bump to be a reviewable lockfile diff with checksums SO THAT a changed tool cannot enter CI unnoticed.

### US-004 Test a version leg locally (P2)

AS A contributor I WANT `MISE_ENV=py310 mise run test:python` SO THAT I reproduce the Python 3.10 matrix leg.

## 3. Functional requirements

### 3.1 Tools and lock policy

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-001 | THE SYSTEM SHALL pin Node.js, pnpm, Python, uv, cargo-nextest, cargo-deny, cargo-hack, cargo-llvm-cov and cargo-semver-checks in `mise.toml`. | must |
| FR-002 | THE SYSTEM SHALL commit `mise.lock` with a sha256 checksum per tool and platform for `linux-x64`, `linux-arm64`, `macos-arm64` and `windows-x64`. | must |
| FR-003 | WHEN a tool is added or a version changes, THE MAINTAINER SHALL regenerate `mise.lock` and every `mise.<env>.lock` with `mise lock --platform linux-x64,linux-arm64,macos-arm64,windows-x64` (with `MISE_ENV` set for the env lockfiles) in the same PR. | must |
| FR-004 | THE SYSTEM SHALL install tools in CI with `--locked` (added by the setup action when a lockfile exists). | must |
| FR-005 | THE SYSTEM SHALL prefer the `aqua:` backend, then `github:`; it SHALL NOT use the `cargo:` backend (source builds, no lock of transitive tools). | must |
| FR-006 | THE SYSTEM SHALL provide the version legs `py310`, `py314` and `node22` as `mise.<env>.toml` files, each overriding one tool and having its own lockfile. Environment names SHALL NOT contain dots. | must |

### 3.2 Tasks

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-010 | THE SYSTEM SHALL define a task for every check CI runs through mise: `fmt:check{,:rust,:python,:nodejs}`, `lint{,:clippy,:clippy-hack,:python,:nodejs}`, `audit:nodejs`, `doc`, `test:rust`, `test:core-no-arena`, `build:{fy,python,nodejs}`, `test:{python,nodejs}`, `release:nodejs`, `coverage:{rust,python,nodejs}`, `deny`, `semver`, plus `fmt` and `ci`. | must |
| FR-011 | EACH task SHALL run the same command the replaced inline CI step ran; checks that were non-blocking in CI (`mypy`, `semver`) SHALL remain non-blocking in the task. | must |
| FR-012 | THE `ci` task SHALL run `fmt:check`, `lint`, `doc`, `test:rust`, `test:python`, `test:nodejs` and `deny` sequentially, not through parallel `depends`, because parallel runs race on the cargo lock and on `pnpm install`. | must |
| FR-013 | THE SYSTEM SHALL set `RUSTFLAGS` and `RUSTDOCFLAGS` in the `env` of the task that needs them, not in a global `[env]`. | must |
| FR-014 | THE SYSTEM SHALL NOT declare `sources`/`outputs` on build tasks, so a restored `target/` cannot make a task skip a needed rebuild. | must |
| FR-015 | THE binding test tasks SHALL set `FY_BIN` to the debug `fy` binary, with `.exe` on Windows. | must |

### 3.3 CI integration

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-020 | EACH migrated job SHALL install tools with `jdx/mise-action` pinned to a full commit SHA, `cache: false`, `version: 2026.10.25` (the mise release the lockfiles were generated with; Dependabot does not bump it, so change all occurrences together and keep it equal to that release), and an `install_args` list limited to the tools the job uses. | must |
| FR-021 | THE workflow SHALL set `MISE_TASK_RUN_AUTO_INSTALL=false` so a tool missing from `install_args` fails the job. | must |
| FR-022 | THE `MISE_SAFE=1` setting SHALL apply to the install step only, because safe mode disables `mise run`. | must |
| FR-023 | WHEN a job needs a different tool version, THE matrix SHALL set `MISE_ENV` from a `mise-env` field. | must |
| FR-024 | THE `changes` job SHALL treat `mise.toml`, `mise.*.toml`, `mise.lock` and `mise.*.lock` as CI-relevant paths. | must |
| FR-025 | NEW CI jobs SHALL call `mise run <task>` instead of a `setup-*` action followed by inline commands. | must |
| FR-026 | THE `CI Success` job (`ci-success`) SHALL remain the only required check and SHALL list every gating job in `needs`; other job names MAY change. | must |

### 3.4 Security

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-030 | THE `[env]` section of any mise config SHALL NOT contain secrets; the files are committed. | must |
| FR-031 | THE workflow SHALL keep `permissions: contents: read` at top level and `persist-credentials: false` on checkout. | must |
| FR-032 | ALL actions SHALL be pinned to a full commit SHA with a version comment. | must |
| FR-033 | THE lockfiles SHALL be reviewed like code: a lock diff that changes a version or checksum not mentioned in the PR is a review blocker. | should |

## 4. Key entities

| Entity | Description |
|--------|-------------|
| `mise.toml` | Default tools (Node.js 24, Python 3.12, pnpm, uv, cargo tools) and all tasks. |
| `mise.<env>.toml`, `mise.<env>.lock` | Single-tool overrides selected by `MISE_ENV` (`py310`, `py314`, `node22`) with their own locks. |
| `mise.lock` | Per-platform checksums for the default tools. |
| `install_args` | Per-job tool subset passed to the setup action. |

## 5. Edge cases and error handling

- A tool listed in a task but not in the job's `install_args` fails with a missing-tool error (FR-021), not a silent install.
- `pnpm install --no-frozen-lockfile` in the Node.js tasks can modify `nodejs/pnpm-lock.yaml`; this matches the pre-mise CI and is not a task failure.
- `mise run test:python` without `YAML_TEST_SUITE_PATH` runs the binding tests but skips the full yaml-test-suite.
- A version-leg name with a dot writes into `mise.lock` instead of its own file.

## 6. Success criteria

| ID | Metric | Target |
|----|--------|--------|
| SC-001 | `mise install --locked` succeeds from a clean checkout on each locked platform | 100% |
| SC-002 | Every migrated CI job passes on main with the same checks as before the migration | 100% |
| SC-003 | Median green main wall time vs the pre-migration baseline (600 s) | no regression beyond noise |
| SC-004 | `mise run ci` on a clean checkout passes whenever CI passes | 100% |

## 7. Agent boundaries

### Always (without asking)
- Regenerate all lockfiles for all four platforms after any tool change.
- Add a task for every new check and call it from CI through `mise run`.
- Update `CONTRIBUTING.md` and this spec when tasks or legs change.

### Ask first
- Adding a tool outside the `aqua:` and `github:` backends.
- Putting any value in `[env]`.
- Setting `MISE_SAFE` at job level or enabling `cache: true`.

### Never
- Commit a lockfile generated for fewer platforms than FR-002.
- Replace rustup toolchains with a mise Rust tool without a spec change.
- Rename `CI Success` or drop a job from its `needs` as a side effect of a tooling change.

## 8. Open questions / Known deviations

| # | Topic | Observed | Question |
|---|-------|----------|----------|
| 1 | Windows legs | `windows-x64` lock entries and the POSIX-style tasks (`cd x && y`) are unverified on a Windows runner; Test Rust, Test Python, Test NodeJS and NodeJS Build run there. | [NEEDS CLARIFICATION: confirm on first CI run, or keep Windows on native actions?] |
| 2 | `MISE_SAFE` in CI | Install under `MISE_SAFE=1` with `--locked` auto-added by the action has passed only on Linux in the pilot. | Drop it from the install step if it fails. |
| 3 | `clippy-sarif`, `sarif-fmt` | Installed by `taiki-e/install-action`: no mise registry entry, and `github:` entries of one repository with different tag prefixes collide in the lockfile. | Revisit when the registry or lock format supports them. |
| 4 | MSRV and Rust toolchains | The MSRV check, release build check and all Rust toolchains stay on rustup / `dtolnay/rust-toolchain`. | Pin Rust in mise only if rustup and mise can share one toolchain definition without drift. |
| 5 | `mise run ci` coverage | The local gate omits coverage, `lint:clippy-hack`, `test:core-no-arena`, `audit:nodejs`, `semver`, `release:nodejs`, MSRV and SARIF. | Add a fuller aggregate task, or accept. |
| 6 | Caches | `cache: false` plus removed `setup-node` pnpm-store and `setup-uv` caches make downloads cold on every run; rust-cache keys for Python legs changed. | Measure; consider caching only checksum-verified downloads. |
| 7 | pnpm lockfile drift | Node.js tasks run `pnpm install --no-frozen-lockfile`, so a task run modifies `nodejs/pnpm-lock.yaml` locally. | Move to `--frozen-lockfile` after the lockfile is reconciled. |
| 8 | Fork PRs | Behavior of the setup action under fork-PR token restrictions is unverified. | Check on the first fork PR. |

## 9. See also

- [[constitution]] — quality gate (principle 9)
- [[010-ci-tooling/plan|Plan]] — job-to-task map and design decisions
- `CONTRIBUTING.md` — contributor workflow
