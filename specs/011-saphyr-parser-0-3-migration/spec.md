---
aliases:
  - saphyr 0.3 migration
tags:
  - sdd
  - spec
  - dependencies
  - parser
created: 2026-11-02
status: in-progress
related:
  - "[[constitution]]"
  - "[[plan]]"
---

# Feature: Migrate to saphyr-parser 0.3.x

> [!info] Metadata
> **Author**: research session (continuous improvement)
> **Branch**: `fix/{issue}-saphyr-parser-0-3` (per `.claude/rules/branching.md`; issue number pending)
> **Priority**: P2 (enhancement, breaking 0.x minor bump)
> **Tracking**: issue #701 (PR for #694-#705 hardening batch). Dependabot PR #691 already merged the `saphyr-parser` 0.3.3 pin and most of the migration (anchor recovery); #701 closes the remaining failures. Decisions of the user: OQ-2 = (a), see FR-005.

## 1. Overview

### Problem Statement

`Cargo.toml` pins `saphyr-parser = "0.2.2"`; 0.3.3 is on crates.io (rust-version 1.85, project MSRV 1.91). Dependabot PR #691 bumps it and is red: it compiles, but `cargo nextest run --workspace --all-features --exclude fast-yaml --exclude fast-yaml-nodejs` runs 3115 tests with 3056 passed and 59 failed (Test Rust on all OSes, Test Python, coverage). The failures are not random: they come from four upstream behavior changes that fy's own code and tests depend on. The bump cannot merge until each is either adapted to or deliberately accepted and documented.

> [!info] Evidence (verified against crate sources in the cargo registry and a scratch copy at 0.3.3)
> - 0.3.x `parse_node` returns node events with `start_mark` taken from the first token of the node, so the span now includes the anchor and tag. 0.2.2 reported the content token's mark.
> - 0.3.3 scanner/parser reject a BOM inside a document with `a byte order mark must not appear inside a document`.
> - 0.3.0 (upstream #143): an empty block scalar at end of input is `""`, not `"\n"`.
> - Upstream CHANGELOG 0.3.1 claims a billion-laughs guard (#109). No such code exists in the 0.3.3 `src/`: the string "expansion" appears only in `CHANGELOG.md`, and `parse_node` still resolves aliases with a plain `anchors.get`. See OQ-1.

### Failure classes (59 tests)

| # | Class | Count (approx.) | Root cause | Decision |
|---|-------|-----------------|------------|----------|
| A | Original anchor names lost (`&defaults` becomes `&anchor1`) | ~40 | Anchor now lies inside the node span; `streaming/anchors.rs` searches the gap before the span | Adapt (FR-001..FR-003) |
| B | Error/diagnostic columns shift from key content to key properties | 3 seen (`merge_check::repeated_merge_key_is_rejected_at_the_second_key`: 3,11 became 3,3; `parser::merge_tag_key_with_invalid_value_is_rejected_at_the_keys_content`: 2,11 became 2,3; plus linter `quoted_strings` parity, golden corpus) | Same span change; positions derived from node spans now start at the properties | Decide per site (FR-004) |
| C | BOM inside a document is a scanner error | ~10 | Stricter, YAML-spec-correct scanner | Policy decision (FR-005..FR-007) |
| D | Empty block scalar at EOF is `""` | 3 | Upstream #143 | Flip pinned expectations (FR-008) |
| E | Residual, to triage after A-D | few | Likely consequences of A/B (`tags_preserved_on_collections`, `keys_up_to_1024_chars_stay_implicit`, `test_format_keeps_empty_flow_collections`, `entries_after_a_dash_keep_their_column...`, `test_streaming_omitted_null_emitted_as_null` all show `&anchor1` in the actual output); `ignore_file_limits::a_megabyte_of_repeated_names_loads_fast` passed in isolation in the scratch copy | Re-run after A; see FR-009 |

### Goal

`saphyr-parser` is at 0.3.3 (or the latest 0.3.x), the full suite is green on every OS and in Python, original anchor names survive formatting, every deliberate behavior change is in `CHANGELOG.md` and the affected `specs/` files, and the interplay with fy's own alias limits is stated.

### Out of Scope

- Fixing upstream bugs fy works around: #586 (flow simple-key staleness guard), #543 (3/4-byte percent escapes), #544 (`BufferedInput`/`StrInput`), column-0 `---` inside a block scalar. The diff shows none fixed; keep the workarounds.
- Replacing `streaming/anchors.rs` with a different design (e.g. a saphyr API that exposes anchor names). 0.3.3 still reports numeric ids only.
- New lint rules or formatter features.
- Filing upstream reports (tracked by #470 follow-up, FR-011).

## 2. User Stories

### US-001: Keep anchor names when formatting
AS A fy user formatting YAML with anchors and aliases
I WANT `fy format` to keep my `&defaults` / `*defaults` names
SO THAT the formatted file diffs cleanly and stays readable

**Acceptance criteria:**
```
GIVEN "base: &defaults\n  a: 1\nuse: *defaults\n"
WHEN  I run `fy format` built against saphyr-parser 0.3.3
THEN  the output contains "&defaults" and "*defaults" and no "&anchor1"
```

### US-002: Know what the dependency bump changed
AS A fy maintainer or downstream user
I WANT every user-visible behavior change from the bump listed in CHANGELOG and the specs
SO THAT upgrades are not surprises

**Acceptance criteria:**
```
GIVEN the migration PR
WHEN  I read CHANGELOG [Unreleased] and specs/001, 002, 003, 009
THEN  BOM handling, empty block scalar value and any changed error positions are described
```

### US-003: Green CI on the bump
AS A maintainer
I WANT dependabot's saphyr bump to pass CI
SO THAT the dependency stays current and later 0.3.x patches merge automatically

**Acceptance criteria:**
```
GIVEN saphyr-parser = "0.3.3"
WHEN  CI runs on Linux, macOS, Windows, Python and coverage
THEN  all required checks pass with zero failing tests
```

## 3. Functional Requirements

| ID | Requirement | Priority |
|----|------------|----------|
| FR-001 | WHEN the streaming formatter receives a node event with an anchor THE SYSTEM SHALL recover the original anchor name from the node's own span (the `&name` token inside `Span`) instead of the gap before it. | must |
| FR-002 | THE SYSTEM SHALL keep the existing name-validity rules (`is_valid_anchor_name`), comment/scalar `&` immunity, and re-emission fallback to a generated `&anchorN` only when no valid name can be recovered. | must |
| FR-003 | WHEN a node has both an anchor and a tag, or an anchor on an empty scalar, an empty flow collection, a key, or a document root THE SYSTEM SHALL still recover the name (all cases in `streaming_format_regressions.rs` and `format_roundtrip.rs` pass unchanged). | must |
| FR-004 | WHEN a position is derived from a node-start span (merge-key errors in `merge_check` and `parser`, linter `quoted_strings` after anchors) THE SYSTEM SHALL keep reporting the position it reported with 0.2.2, by skipping node properties, UNLESS the spec of that rule is deliberately changed (then update the test, the spec and CHANGELOG together). | must |
| FR-005 | THE SYSTEM SHALL follow saphyr-parser 0.3.3 for a BOM that is not the first character of the input (decided OQ-2, option (a), breaking): U+FEFF in a plain scalar, in a comment, in a block scalar (mid-line or at line start) or at column 0 inside a mapping is a scanner error `a byte order mark must not appear inside a document`, identically in `Parser::parse`, `parse_all`, `parse_parallel`, the formatter, the linter and all bindings. A BOM inside a single- or double-quoted scalar and a BOM before `---` stay accepted. | must |
| FR-006 | WHEN the input starts with exactly one BOM THE SYSTEM SHALL keep stripping it (spec 001: "UTF-8 file starting with a BOM is valid"); a second leading BOM is an interior BOM and is rejected by FR-005. | must |
| FR-007 | THE SYSTEM SHALL carry a position on the interior-BOM error and cover it by a test on every surface. The emitter already quotes U+FEFF in scalars (`emitter/scalar.rs`), so fy's own output stays valid. No BOM pre-processing is added to `NormalizedInput`. | must |
| FR-008 | WHEN a block scalar header is followed by end of input THE SYSTEM SHALL report the value `""` (upstream 0.3.0 semantics) for clip and keep, and the pinned expectations in `crates/fast-yaml-parallel/tests/saphyr_divergence.rs` SHALL be updated to it. | must |
| FR-009 | THE SYSTEM SHALL triage every residual failure after FR-001..FR-008 and either fix it, update the test with a documented reason, or file an issue (P0-P4 label). No test may be deleted or weakened without a recorded reason. | must |
| FR-010 | THE SYSTEM SHALL keep fy's own limits authoritative: `MaxAliasBytes`, `AnchorCopies` and depth checks SHALL run exactly as in spec 009 FR-012, and the spec SHALL state that these limits do not rely on any saphyr-parser guard. | must |
| FR-011 | WHEN the migration lands THE SYSTEM SHALL update issue #470: report 2 (empty block scalar at EOF) is resolved upstream and is closed in the migration PR; report 1 (column-0 `---` in `scan_block_scalar`) stays open and the pinned test `marker_after_block_scalar_with_explicit_indent_indicator_errors_only_in_saphyr` keeps the divergence documented. | should |
| FR-012 | THE SYSTEM SHALL keep the workarounds for #586, #543 and #544 and re-verify their guard tests against 0.3.3. | must |
| FR-013 | THE SYSTEM SHALL update `CHANGELOG.md` `[Unreleased]` (one line per breaking change, PR link) and the affected files in `specs/` (001, 002, 003, 009, `cross-feature-open-questions.md`; 001 line 254 already lists "Upgrading saphyr-parser" as a risk). | must |

## 4. Non-Functional Requirements

| ID | Category | Requirement |
|----|----------|-------------|
| NFR-001 | Performance | Anchor recovery stays a single forward pass (O(n)); formatter and parse throughput within 5% of the 0.2.2 baseline on the existing benches. |
| NFR-002 | Security | No weakening of resource limits; hostile-input tests in spec 009 (alias bomb, deep nesting, scan-ahead) pass unmodified. |
| NFR-003 | Compatibility | MSRV 1.91 unchanged; saphyr-parser 0.3.3 requires 1.85. `cargo deny check` passes (new transitive deps, licenses). |
| NFR-004 | Consistency | CLI, Python and Node.js give identical results for anchor names, BOM and empty block scalars (cross-binding check). |
| NFR-005 | Quality gate | `mise run ci` plus `lint:clippy-hack`, `test:core-no-arena`, MSRV job all green. |

## 5. Data Model

No new entities. Touched types:

| Entity | Description | Key Attributes |
|--------|-------------|----------------|
| `AnchorNames` (`streaming/anchors.rs`) | Recovers `&name` for numeric anchor ids | source, forward `ByteCursor`, `prev_end` (may become unnecessary) |
| `SourcePosition` (`position.rs`) | `from_span` / `end_of` conversions | start/end of a saphyr `Span`; may need a "content start" variant |
| `NormalizedInput` | BOM-free text plus offset map | owner of the BOM policy |

## 6. Edge Cases and Error Handling

| Scenario | Expected Behavior |
|----------|-------------------|
| `&a !!map` and `!!map &a` (either property order) | Name `a` recovered; tag preserved |
| Anchor on empty scalar (`k: &a`), empty flow `&a []`, anchored key `&k key: v` | Name recovered; formatting unchanged from 0.2.2 |
| `&` inside plain scalars, quoted scalars, comments, `!<tag:a&b>` | Never mistaken for an anchor (existing tests) |
| Anchor name reused across documents or redefined in one document | Existing uniqueness logic unchanged |
| Multibyte text before the anchor | Byte offsets correct (existing tests) |
| Leading single BOM, double BOM | Single stripped (FR-006); double: error (FR-005) |
| BOM mid-line, at line start inside a document | Error (FR-005); the parser emits a truncated scalar before the error. In quoted scalars and before `---`: accepted |
| `--- \|` then EOF; `--- \|+` then EOF; `--- >` then EOF | Value `""` |
| Block scalar then `---`/`...` marker | Unchanged except where #470 report 1 applies |
| Alias to undefined anchor | Still an error (`found unknown anchor`) |
| Alias bomb | Rejected by fy's `MaxAliasBytes` at the alias, not by saphyr |

## 7. Success Criteria

| ID | Metric | Target |
|----|--------|--------|
| SC-001 | Failing tests in `cargo nextest run --workspace --all-features --exclude fast-yaml --exclude fast-yaml-nodejs` | 0 (from 59) |
| SC-002 | Python and Node.js test suites, MSRV, coverage, `ci-success` | green on PR |
| SC-003 | Tests relaxed or deleted without recorded reason | 0 |
| SC-004 | Manual end-to-end check: `fy format` on a file with 3 named anchors, aliases and merge keys | names preserved byte for byte |
| SC-005 | CHANGELOG and spec entries for each of: BOM, empty block scalar, span semantics (if user-visible) | present |

## 8. Agent Boundaries

### Always (without asking)
- Run `cargo nextest` for the affected crates after each change; run the full gate before the PR.
- Re-run the failing-test list from the scratch run to measure progress.
- Keep tests that pin behavior; change an expectation only with a one-line reason in the commit and CHANGELOG/spec.
- Work in a worktree/branch created via `EnterWorktree`, never on `main`.

### Ask First
- Changing any error position that users or goldens see (FR-004) rather than preserving it.
- Bumping beyond 0.3.3 or adding a dependency.

### Never
- Modify the spec of resource limits to fit the bump.
- Delete or `#[ignore]` a failing test to get green.
- Remove the #586/#543/#544 workarounds.
- Hand-edit `Cargo.lock` or lockfiles under `.config/mise/`.

## 9. Open Questions

> [!question] Open items
> - [NEEDS CLARIFICATION: OQ-1. Does saphyr-parser 0.3.3 contain any alias-expansion guard? Upstream CHANGELOG 0.3.1 says so (#109, PR #120), but grep of the 0.3.3 `src/` finds no such code; the guard is probably in the separate `saphyr` loader crate, which fy does not use. Confirm with an alias-bomb run through `fy parse --max-alias-bytes` and a direct `saphyr-parser` event loop. If a guard exists, state its limit and error text and how it composes with `MaxAliasBytes`/`AnchorCopies`.]
> - Resolved: OQ-2 (BOM policy). Decided by the user: option (a), follow saphyr-parser 0.3.3 as is (breaking; see FR-005). Recorded in CHANGELOG by the batch PR.
> - [NEEDS CLARIFICATION: OQ-3. Error positions for merge-key diagnostics and the linter `quoted_strings` rule (FR-004): keep pointing at the key content (needs a helper that skips properties) or accept the property start? Check yamllint's column for `&a "x"` before deciding, since rule parity is the stated reason for the existing expectation (`rules_parity_588`).]
> - [NEEDS CLARIFICATION: OQ-4. Linter golden corpus (`default_config_matches_golden`, `strict_parity_config_matches_golden`): are the diffs only anchor-position shifts (class B) or also BOM/other? Inspect the golden diff before regenerating.]
> - [NEEDS CLARIFICATION: OQ-5. `ignore_file_limits::a_megabyte_of_repeated_names_loads_fast` failed in the full run but passed alone: load-induced timing flake or a real slowdown from 0.3.x? Re-run on the unmodified 0.2.2 baseline under the same load and compare.]
> - [NEEDS CLARIFICATION: OQ-6. Do the "undocumented changes" in the 0.2.2 to 0.3.3 source diff affect anything beyond classes A-D (e.g. flow mapping state inside compact pairs, upstream #139)? Run the yaml-test-suite harness and update its xfail list (known pitfall: `repr_to_python` is a parallel implementation of `coerce_representation`; verify the xfail list shrinks).]
> - [NEEDS CLARIFICATION: OQ-7. Release: ship as its own 0.x minor/patch release or fold into the next release? Not blocking the migration.]

## 10. See Also

- [[constitution]] — project principles (repo: `specs/constitution.md`)
- [[plan]] — migration steps
- [[MOC-specs]] — all specifications
- Repo specs affected: `specs/001-parse-validate/spec.md` (BOM, error text), `002-format` (anchors, BOM escaping), `003-lint` (positions, goldens), `005-batch-parallel` (parse_all parity), `009-limits-security` (alias limits, workaround row 18), `cross-feature-open-questions.md`
- Issues: dependabot PR #691, #470, #586, #543, #544
- Upstream: `saphyr-rs/saphyr` `parser/CHANGELOG.md` (0.3.0 to 0.3.3)
