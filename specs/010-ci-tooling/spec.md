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
status: reverse-specified
related:
  - "[[constitution]]"
  - "[[009-limits-security/spec|Limits and security]]"
  - "[[cross-feature-open-questions]]"
---

# Feature: CI tooling (mise toolchain and tasks)

> [!info] Metadata
> **Scope**: `.config/mise/config.toml`, `.config/mise/config.<env>.toml`, the matching lockfiles and the jobs of `.github/workflows/ci.yml` that install tools and run checks. Not product behavior: this spec covers how the repository's checks are installed and run.
> **Baseline**: PR #669 (issue #665), branch `ci-pilot-mise-as-unified-toolc`. Reverse-specified from the implementation as implemented in that PR (HEAD d8c212d plus uncommitted doc and workflow fixes), which is not yet merged. Only the earlier Linux pilot run (run 41335814375) has executed in CI; everything else is unverified and listed in section 8. The adoption decision is tracked in issue #668.

> [!note] Verification marks
> Constitution section 5 reads MUST as verified in code. Requirements that have not run in a real CI job carry `[unverified in real CI]`; everything else is verified by inspection of the files or a local run (`mise install --locked --dry-run`, actionlint, zizmor). Only the Linux pilot run 41335814375 has executed in CI.

## 1. Purpose and value

Before this feature each CI job installed its own tools through a different action (`setup-node`, `setup-python`, `setup-uv`, `pnpm/action-setup`, `taiki-e/install-action`) and repeated the check commands inline, so a contributor could not reproduce a job without reading the workflow. This feature pins the non-Rust tools in one lockfile and defines every check as a named task, so CI and a developer's machine run the same commands with the same tool versions.

### Non-goals

- Replacing rustup. Rust toolchains (stable, nightly, beta, MSRV) are not managed by mise.
- Requiring mise for contributors or users. Plain `cargo`, `pnpm` and `uv` stay supported.
- Changing what the checks verify. A task is a verbatim move of the command CI ran before.
- Release automation.

## 2. User stories

### US-001: Reproduce a CI job locally (P1)

AS A contributor I WANT to run the exact command of a failing CI job SO THAT I can fix it without pushing.

```
GIVEN a failing "Lint (rust)" job
WHEN  I run `mise install --locked && mise run lint:clippy`
THEN  the same cargo invocation as CI runs with RUSTFLAGS="-D warnings"
```

### US-002: Run the pre-PR gate with one command (P1)

AS A contributor I WANT `mise run ci` to run format, lint, doc, test and deny checks sequentially SO THAT I do not have to remember the list.

```
GIVEN a clean checkout with `mise install --locked` done
WHEN  I run `mise run ci`
THEN  fmt:check, lint, doc, test:rust, test:python, test:nodejs and deny run one after another and the first failure stops the run
```

### US-003: Update a tool version safely (P2)

AS A maintainer I WANT a version bump to be a reviewable lockfile diff with checksums SO THAT a changed tool cannot enter CI unnoticed.

```
GIVEN a PR that bumps `node` in .config/mise/config.toml
WHEN  `mise lock --platform linux-x64,linux-arm64,macos-arm64,windows-x64` is rerun
THEN  the .config/mise/mise.lock diff shows the new version and a sha256 per platform
```

### US-004: Test a version leg locally (P2)

AS A contributor I WANT `MISE_ENV=py310 mise run test:python` SO THAT I reproduce the Python 3.10 matrix leg.

```
GIVEN .config/mise/config.py310.toml and mise.py310.lock
WHEN  I run `MISE_ENV=py310 mise install --locked && MISE_ENV=py310 mise run test:python`
THEN  the Python 3.10 pinned in the lock runs the binding tests
```

## 3. Functional requirements

### 3.1 Tools and lock policy

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-001 | THE SYSTEM SHALL declare Node.js (24), pnpm (10.33.0), Python (3.12), uv, cargo-nextest, cargo-deny, cargo-hack, cargo-llvm-cov and cargo-semver-checks in `.config/mise/config.toml`; uv and the cargo tools are declared as `latest` and become reproducible only through the lockfile (FR-002). | must |
| FR-002 | THE SYSTEM SHALL commit `.config/mise/mise.lock` with a sha256 checksum per tool and platform for `linux-x64`, `linux-arm64`, `macos-arm64` and `windows-x64`. Only the `linux-x64` entries have been installed in CI; the other platforms are checksum-complete but unverified (section 8). | must |
| FR-003 | WHEN a tool is added or a version changes, THE MAINTAINER SHALL regenerate `.config/mise/mise.lock` and every `.config/mise/mise.<env>.lock` with `mise lock --platform linux-x64,linux-arm64,macos-arm64,windows-x64` (with `MISE_ENV` set for the env lockfiles) in the same PR. | must |
| FR-004 | THE SYSTEM SHALL install tools in CI with `--locked` (added by the setup action when a lockfile exists). `[unverified in real CI]` together with `MISE_SAFE` on macOS and Windows runners. | must |
| FR-005 | THE SYSTEM SHALL prefer the `aqua:` backend, then `github:`; it SHALL NOT use the `cargo:` backend (source builds, no lock of transitive tools). `github:` is used for cargo-nextest, cargo-hack and cargo-semver-checks, which aqua lacks. The nextest id carries `[version_prefix=cargo-nextest-]`, and `install_args` MUST repeat the id verbatim (`github:nextest-rs/nextest[version_prefix=cargo-nextest-]`); `install_args` splitting by the setup action is `[unverified in real CI]` beyond the Linux pilot. | must |
| FR-006 | THE SYSTEM SHALL provide the version legs `py310`, `py314` and `node22` as `.config/mise/config.<env>.toml` files, each overriding one tool and having its own lockfile. Environment names SHALL NOT contain dots. Resolution of the env locks on macOS and Windows is `[unverified in real CI]`. | must |

### 3.2 Tasks

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-010 | THE SYSTEM SHALL define a task for every check CI runs through mise: `fmt:check{,:rust,:python,:nodejs}`, `lint{,:clippy,:clippy-hack,:python,:nodejs}`, `audit:nodejs`, `doc`, `test:rust`, `test:core-no-arena`, `build:{fy,python,nodejs}`, `test:{python,nodejs}`, `release:nodejs`, `coverage:{rust,python,nodejs}`, `deny`, `semver`, plus `fmt` and `ci`. | must |
| FR-011 | EACH task SHALL run the same command the replaced inline CI step ran; checks that were non-blocking in CI (`mypy`, `semver`) SHALL remain non-blocking in the task. | must |
| FR-012 | THE `ci` task SHALL run `fmt:check`, `lint`, `doc`, `test:rust`, `test:python`, `test:nodejs` and `deny` sequentially, not through parallel `depends`, because parallel runs race on the cargo lock and on `pnpm install`. | must |
| FR-013 | THE SYSTEM SHALL set `RUSTFLAGS` and `RUSTDOCFLAGS` in the `env` of the task that needs them, not in a global `[env]`. | must |
| FR-014 | THE SYSTEM SHALL NOT declare `sources`/`outputs` on build tasks, so a restored `target/` cannot make a task skip a needed rebuild. | must |
| FR-015 | THE binding test tasks (`test:python`, `test:nodejs`) SHALL set `FY_BIN` to the debug `fy` binary, with `.exe` on Windows `[unverified in real CI]`; the `coverage:*` tasks run on Linux only and set it without `.exe`. | must |
| FR-016 | THE tasks `build:python` and `build:nodejs` SHALL depend on `build:fy`, and the test and coverage tasks SHALL depend only on their own build task, so `fy` and each extension build once and not concurrently. | must |

### 3.3 CI integration

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-020 | EACH migrated job SHALL install tools with `jdx/mise-action` pinned to a full commit SHA, `cache: false`, `version: 2026.10.25` (the mise release the lockfiles were generated with; Dependabot does not bump it, so change all occurrences together and keep it equal to that release), and an `install_args` list limited to the tools the job uses, or `install: false` when the job needs no mise-managed tool (`Lint (rust)`). | must |
| FR-021 | THE workflow SHALL set `MISE_TASK_RUN_AUTO_INSTALL=false` at workflow level so a tool missing from `install_args` fails the job. | must |
| FR-022 | THE `MISE_SAFE=1` setting SHALL apply to every install step that installs tools and to no other step, because safe mode disables `mise run`; the `install: false` step in `Lint (rust)` does not set it. `[unverified in real CI]` beyond the Linux pilot run. | must |
| FR-023 | WHEN a matrix leg needs a non-default tool version, THE workflow SHALL select it through `MISE_ENV` set from the leg's `mise-env` field, and SHALL leave `MISE_ENV` unset (not empty) for default legs. `test-python` sets it as job env (always non-empty); `test-nodejs` exports it through `$GITHUB_ENV` in a step before the setup action, only when the field is non-empty. The behavior of an empty `MISE_ENV` is `[unverified in real CI]`, which is why it is avoided. | must |
| FR-024 | THE `changes` job SHALL treat `.config/mise/**` as a CI-relevant path. | must |
| FR-025 | NEW CI jobs SHALL call `mise run <task>` instead of a `setup-*` action followed by inline commands. | must |
| FR-026 | THE `CI Success` job (`ci-success`) SHALL remain the only required check and SHALL list every gating job in `needs`; other job names MAY change. | must |
| FR-027 | WHEN a job selects a Python or Node.js version, THE job SHALL assert after setup that the interpreter in use has the matrix version (`uv python find` in `python/`; `node --version`), so an ignored `MISE_ENV` or a uv-downloaded Python fails the job. The assertion steps run through mise shims on Windows, `[unverified in real CI]`. | must |
| FR-028 | THE steps that report an independent failure class (SARIF, Verify Clippy, `doc` in `Lint (rust)`; the format step in `Python Quality Checks`) SHALL run under `if: ${{ !cancelled() }}`, so one push reports every failed class; the job still fails. | should |

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
| `.config/mise/config.toml` | Default tools (Node.js 24, Python 3.12, pnpm, uv, cargo tools) and all tasks. |
| `.config/mise/config.<env>.toml`, `.config/mise/mise.<env>.lock` | Single-tool overrides selected by `MISE_ENV` (`py310`, `py314`, `node22`) with their own locks. |
| `.config/mise/mise.lock` | Per-platform checksums for the default tools. |
| `install_args` | Per-job tool subset passed to the setup action. |

## 5. Edge cases and error handling

- A tool listed in a task but not in the job's `install_args` fails with a missing-tool error (FR-021), not a silent install.
- `pnpm install --no-frozen-lockfile` in the Node.js tasks can modify `nodejs/pnpm-lock.yaml`; this matches the pre-mise CI and is not a task failure.
- `mise run test:python` without `YAML_TEST_SUITE_PATH` runs the binding tests but skips the full yaml-test-suite; CI checks out `yaml/yaml-test-suite` at `data-2022-01-17` in `Test Python` and `Python Coverage`, so the CI leg runs more than the default local run.
- On `test-nodejs` default legs `MISE_ENV` is unset, not empty; an empty value set at job level was avoided because its parsing is unverified.
- A version-leg name with a dot writes into `.config/mise/mise.lock` instead of its own file (documented mise behavior; not tested in this repository).

### Assumptions

- Contributors may not have mise; plain `cargo`, `pnpm` and `uv` stay supported.
- GitHub-hosted runners, including Windows, provide `bash` for the assertion steps.
- The aqua and github registries and their release checksums are trusted; the lockfile detects change, not initial compromise.
- `minimum_release_age` (24 h default) applies only when locking `latest` tools; `latest` pins are refreshed by hand (`mise outdated`).

## 6. Success criteria

| ID | Metric | Target |
|----|--------|--------|
| SC-001 | `mise install --locked` succeeds for the default config and for each of `py310`, `py314`, `node22` on `linux-x64`, `linux-arm64`, `macos-arm64` and `windows-x64`. Verified: `linux-x64` default in the CI pilot; Windows and macOS legs run in the matrix on the first CI run; `linux-arm64` is exercised by no job `[unverified in real CI]`. | 100% |
| SC-002 | Each task's command equals the command of the replaced step in the pre-#669 workflow (`git show 7c225ab:.github/workflows/ci.yml`), per FR-011, and every migrated job is green on main | 100% |
| SC-003 | Median wall time of the CI run over the first 10 green runs on main after merge vs the median of the last 10 green runs before PR #669 (both recorded in #668) | <= baseline + 10% |
| SC-004 | `mise run ci` passes on a clean checkout of any commit whose `Lint (rust)`, `Test *` and `Security Audit` jobs pass (coverage, MSRV, `clippy-hack` and the other omissions of section 8 item 5 are outside the gate) | 100% |

SC-003 baseline of 600 s (range 523-1084 s): median of the last 10 fully green push runs of `ci.yml` on `main` before PR #669, read with `gh run list --workflow ci.yml --branch main` (wall time `createdAt` to `updatedAt`, runs 38398852139 to 41334052624). The working file `.local/testing/mise-baseline.md` is a local, untracked artifact; the figures and the post-migration measurements are recorded in issue #668. SC-001 and SC-002 are targets, not yet measured outside the Linux pilot run.

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

| # | Pri | Topic | Observed | Question |
|---|-----|-------|----------|----------|
| 1 | P2 | Windows legs | `windows-x64` lock entries and the tasks run on Windows (`test:rust`, `test:python`, `test:nodejs`, `release:nodejs` with their `build:*` dependencies) are `[unverified in real CI]`. Their commands use `cd x && y`, which mise runs through the Windows shell. `lint:python` (`( ... \|\| true) &&`) and `audit:nodejs` (a multi-line `{ ...; }` block) are POSIX-only and run on Linux only; `coverage:*` hard-codes `FY_BIN` without `.exe` for the same reason. | [NEEDS CLARIFICATION: confirm on the first CI run, or keep Windows on native actions?] **Proposed:** keep the Windows legs; give a task a per-OS `shell` or `run` only if the first run fails. See [[cross-feature-open-questions]] X-16. |
| 2 | P2 | `MISE_SAFE` in CI | Install under `MISE_SAFE=1` with `--locked` added by the setup action passed in the Linux pilot CI run 41335814375, before the version pin and the assertion steps. It has not run on macOS or Windows hosted runners. CONTRIBUTING.md states the same. | [NEEDS CLARIFICATION: keep `MISE_SAFE` on install steps if the first full run passes?] **Proposed:** keep; drop it from the failing install step only. |
| 3 | P3 | `clippy-sarif`, `sarif-fmt` | Installed by `taiki-e/install-action`: no mise registry entry, and `github:` entries of one repository with different tag prefixes collide in the lockfile. | Revisit when the registry or lock format supports them. |
| 4 | P3 | MSRV and Rust toolchains | The MSRV check, release build check and all Rust toolchains stay on rustup / `dtolnay/rust-toolchain`. | Pin Rust in mise only if rustup and mise can share one toolchain definition without drift. |
| 5 | P3 | `mise run ci` coverage | The local gate omits coverage, `lint:clippy-hack`, `test:core-no-arena`, `audit:nodejs`, `semver`, `release:nodejs`, MSRV and SARIF. | [NEEDS CLARIFICATION: accept, or add a fuller aggregate task?] **Proposed:** accept; CONTRIBUTING.md lists the omissions. |
| 6 | P2 | Caches | `cache: false` plus removed `setup-node` pnpm-store and `setup-uv` caches make downloads cold on every run; rust-cache keys for the Python legs and the powerset changed. | [NEEDS CLARIFICATION: is the cold-download cost acceptable?] **Proposed:** decide from the #668 measurements; cache only checksum-verified downloads if not. |
| 7 | P3 | pnpm lockfile drift | Node.js tasks run `pnpm install --no-frozen-lockfile`, so a task run modifies `nodejs/pnpm-lock.yaml` locally. | Move to `--frozen-lockfile` after the lockfile is reconciled. |
| 8 | P3 | Fork PRs | Behavior of the setup action under fork-PR token restrictions is `[unverified in real CI]`. | [NEEDS CLARIFICATION: check on the first fork PR.] **Proposed:** keep `MISE_SAFE` on installs and `permissions: contents: read`; no change unless it fails. |
| 9 | P1 | Pilot decision | Issue #665 asks for a before/after measurement table (CI time, setup-step count, workflow lines) and an adopt, partial or abandon decision with a follow-up for other repositories. Both are tracked in issue #668 (follow-up of #665). | [NEEDS CLARIFICATION: record the measurements and the decision in #668, and file a rollout issue for other repositories if adopted.] **Proposed:** adopt partially if SC-003 holds and the Windows and macOS legs pass, keeping Rust toolchains on rustup. |
| 10 | P2 | Platforms beyond Linux x64 | `linux-arm64` is installed by no CI job; the `macos-latest` legs (Test Rust, Test Python, Test NodeJS, NodeJS Build) install on `macos-arm64` for the first time in PR #669. | [NEEDS CLARIFICATION: confirm macOS on the first run; keep or drop the `linux-arm64` lock entries?] **Proposed:** keep them for local Linux arm64 development. |
| 11 | P3 | `ci-success` does not need `changes` | Pre-existing: if Detect Changes fails, every job is skipped and `CI Success` can pass. Not changed by this feature. | Add `changes` to `needs` in a separate PR. |
| 12 | P3 | Assertion steps on Windows | The version assertions use `shell: bash` and mise shims; the `uv python find` path format on Windows is `[unverified in real CI]`. | Confirm on the first Windows run. |
| 13 | P3 | Non-blocking failures | `lint:python` ends mypy with `\|\| true` and `semver` ends with `\|\| true`, as the replaced steps were `continue-on-error`; the failure is no longer a separate warning annotation in the log summary. | Accept, or split mypy into its own non-blocking step. |

### Resolution of the open questions of issue #665

| Question in #665 | Resolution |
|---|---|
| Does mise coexist with `dtolnay/rust-toolchain` for nightly and MSRV? | Not attempted: Rust stays on rustup (item 4). |
| `cargo:` or `aqua:` for cargo tools? | `aqua:` then `github:` (FR-005). |
| Is mise-action plus rust-cache faster or slower? | Open: item 6 and SC-003, measured in #668. |
| Do `sources`/`outputs` caches speed up the NAPI and maturin builds? | Not adopted (FR-014): correctness over speed; unmeasured. |
| Do Windows and macOS work with the same config? | macOS and Windows are in the matrix; unverified (items 1 and 10). |

### Rollback

mise is additive for contributors. To revert, restore the pre-#669 workflow steps (`setup-*`, `taiki-e/install-action`, inline commands) and delete `.config/mise/`; the `changes` filter entry `.config/mise/**` goes with it. No product code depends on mise.

### Deviations from issue #665

`python = 3.12` and `node = 24` match the issue.

| Issue #665 proposed | Implemented | Reason |
|---|---|---|
| `mise.toml` and `mise.lock` at the repository root | `.config/mise/config.toml`, `.config/mise/mise.lock` and per-leg files | Keeps the root clean; mise reads `.config/mise/` |
| Lock platforms `linux-x64`, `linux-arm64`, `macos-arm64` | `windows-x64` added | Windows legs run in the CI matrix |
| Parallel `mise-ci` job and at least 10 comparison runs before migrating | No `mise-ci` job; the jobs were migrated directly | User decision; measurement moved to issue #668 |
| Provisional task names | Final names in FR-010 (for example `fmt:check:rust`, `lint:clippy`, `coverage:*`, `build:*`) | Tasks map one to one to CI jobs |
| `sources`/`outputs` on heavy tasks | None (FR-014) | A restored `target/` could skip a needed rebuild |
| `minimum_release_age` left at the default | Not configured in any mise file | Versions come from the lockfile; the setting is irrelevant with `--locked` |
| `cargo:` or `aqua:` backend to be decided | `aqua:` then `github:` (FR-005) | Checksums without source builds |
| Mypy and semver as `continue-on-error` steps | `\|\| true` inside `lint:python` and `semver` (item 13) | Same blocking behavior, less visible |

## 9. See also

- [[constitution]] — quality gate (principle 9)
- [[cross-feature-open-questions]] — X-16 (Windows) and X-17 (adoption decision)
- [[010-ci-tooling/plan|Plan]] — job-to-task map and design decisions
- `CONTRIBUTING.md` — contributor workflow
