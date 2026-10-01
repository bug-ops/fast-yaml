---
aliases:
  - Limits and Security
  - Resource limits
tags:
  - sdd
  - spec
  - security
  - limits
created: 2026-10-01
status: reverse-specified
related:
  - "[[constitution]]"
  - "[[001-parse-validate/spec|Parse and validate]]"
---

# Feature: Resource limits and input safety

> [!info] Metadata
> **Scope**: `fast_yaml_core::limits`, `NormalizedInput` / `encoding`, file reading and writing safety in `fast-yaml-parallel` and the CLI, limit options on every surface.
> **Baseline**: v0.6.6, commit e5e6cfb, plus the fixes #574 (document limit in core), #531 (no mmap), #366, #577, #571 (bounded config reads) and #569.

## 1. Purpose and value

fast-yaml is meant to be run on untrusted YAML (CI on pull requests, servers, editors). YAML has well-known amplification attacks (billion-laughs aliases, deep nesting, giant flow collections, document floods). This feature guarantees that for any input, memory, stack and time stay bounded by user-visible, range-checked limits, that rejection is a normal error with a position and a hint (never a crash or OOM), and that file I/O does not follow attackers into special files or leave torn outputs.

### Non-goals

- Not a sandbox for the host application: no CPU-time limit, no per-batch aggregate memory ceiling.
- No path allow-listing or `..` filtering for files read; the path trust boundary is the caller's.
- No authentication, network or secret handling (the product has none).
- No "unlimited" setting: zero is invalid for every limit, so it can never mean "off".

## 2. User stories

### US-001 Reject alias bombs and deep nesting predictably (P1)

AS A maintainer parsing third-party YAML I WANT amplification rejected with a clear error SO THAT my CI runner is not exhausted.

```
GIVEN amp.yaml: a: &a [1,2,3,4] / b: &b [*a,*a,*a,*a] / c: [*b,*b,*b,*b]
WHEN  I run `fy parse --max-alias-bytes 1KiB amp.yaml`
THEN  exit 1:
      error: Failed to parse YAML
        caused by[0] YAML resource limit exceeded at line 2, column 17: alias expansion exceeds 1024 bytes
        hint: raise with --max-alias-bytes

GIVEN nest.yaml: "a:\n  b: 1\n  c:\n    - x\n" (depth 3)
WHEN  `fy parse --max-depth 2 nest.yaml`
THEN  exit 1: "YAML resource limit exceeded at line 4, column 5: nesting depth exceeds 2" + "hint: raise with --max-depth"
WHEN  `fy parse --max-depth 3 nest.yaml` THEN valid, exit 0
```

### US-002 Bound input size before reading (P1)

```
GIVEN ok.yaml (24 bytes)
WHEN  `fy parse --max-input-bytes 5 ok.yaml`
THEN  exit 1: "input size 24 bytes exceeds maximum allowed 5 bytes" + "hint: raise with --max-input-bytes or the max-input-bytes config key"

GIVEN 300 bytes on stdin
WHEN  `fy parse --max-input-bytes 100`
THEN  exit 1: "Failed to read from stdin ... input size 101 bytes exceeds maximum allowed 100 bytes"
      (stdin is read at most limit+1 bytes)

WHEN  `fy parse --max-input-bytes 0 ok.yaml`
THEN  exit 2: "invalid value '0' for '--max-input-bytes <BYTES>': must be between 1 and 1073741824, got 0"
```

### US-003 Bound parser lookahead memory (P1)

AS A user linting large JSON-like files I WANT lookahead bounded SO THAT one flow collection cannot make the parser allocate hundreds of times the input.

```
GIVEN fl.yaml "[a, [b, [c]]]"
WHEN  `fy parse --max-scan-ahead 8 fl.yaml`
THEN  exit 1: "YAML resource limit exceeded at line 1, column 1: parser lookahead exceeds 8 characters past the last node: ... raise the scan-ahead limit"
      + "hint: raise with --max-scan-ahead or the max-scan-ahead config key"
```

### US-004 Flow nesting has a hard ceiling (P2)

```
GIVEN a root flow collection of 256+ nested "[" and --max-depth 512
WHEN  `fy parse --max-depth 512 fl2.yaml`
THEN  exit 1: "flow collection nesting exceeds the scanner limit of 255 levels, which the depth limit cannot raise"
```

### US-005 Limits are validated, typed and consistent on every surface (P1)

AS A binding user I WANT the same ranges and error shape in Python and Node.js.

```
GIVEN Node.js `maxInputBytes: 0` in a batch/parallel option object
THEN  it throws "maxInputBytes must be between 1 and 1073741824, got 0"
GIVEN Node.js `maxInputSize: ...` (removed option)
THEN  it throws "maxInputSize was renamed to maxInputBytes"
GIVEN Rust `MaxDepth::new(513)`
THEN  Err(LimitRangeError { value: 513, min: 1, max: 512 })  // "must be between 1 and 512, got 513"
```

### US-006 Document floods and file I/O safety in batch mode (P2)

AS AN operator of a batch pipeline I WANT a bounded document count and safe file handling.

```
GIVEN three.yaml with 3 documents
WHEN  `fy parse --max-documents 2 three.yaml`                       (verified; same for lint, format, convert)
THEN  exit 1: "YAML resource limit exceeded at line 4, column 1: document count exceeds 2 (document 3)"
      + "hint: raise with --max-documents"
GIVEN the same stream and ParseLimits { max_documents: 2 } in fast-yaml-parallel
THEN  parse_parallel fails while chunking, without materializing documents
GIVEN a FIFO or device path passed as a file
THEN  it is refused without blocking
GIVEN `fy parse /tmp` (a directory)
THEN  exit 1: "path is a directory, not a file"
```

### US-007 In-place writes are crash- and symlink-safe (P2)

AS A user running `fy format -i` or `fy lint -i` I WANT a failed write never to leave a torn file or a stray temp file.

```
GIVEN an in-place rewrite
THEN  content goes to a temp file in the same directory and is renamed over the target;
      permissions are preserved, a planted temp-file symlink is not followed,
      a dangling symlink target is refused, and the temp file is removed on every error path
```

## 3. Functional requirements

### 3.1 Limit types

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-001 | THE SYSTEM SHALL provide range-checked limit types `MaxDepth` (1..=512, default 256), `MaxAliasBytes` (1..=1 GiB, default 64 MiB), `MaxInputBytes` (1..=1 GiB, default 100 MiB), `MaxScanAhead` (1..=1 Gi chars, default 4 Mi chars), `MaxDocuments` (1..=10 000 000, default 100 000). | must |
| FR-002 | WHEN a limit is constructed outside its range THE SYSTEM SHALL return `LimitRangeError{value,min,max}` rendering `must be between {min} and {max}, got {value}`; zero SHALL never mean unlimited. | must |
| FR-003 | THE SYSTEM SHALL make limits of different kinds distinct types (`Bounded<K>` over a sealed `Bounds` marker) so a `MaxInputBytes` cannot be passed as a `MaxAliasBytes`. | must |
| FR-004 | THE SYSTEM SHALL group parse limits in `ParseLimits { max_depth, max_alias_bytes, max_tag_bytes, max_scan_ahead, max_documents }` with `Default` equal to the defaults above and `MaxTagBytes` default 64 MiB. | must |
| FR-005 | THE SYSTEM SHALL provide dump-side limits `MaxOutputBytes` (default 100 MiB) and `MaxDumpNodes` (default 16 Mi) enforced by the bindings' dump functions. | should |

### 3.2 Enforcement

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-010 | WHEN nesting of block or flow collections exceeds `MaxDepth` THE SYSTEM SHALL fail with `ParseError::LimitExceeded{Depth}` at the offending node, before building the tree. | must |
| FR-011 | WHEN a flow collection nests deeper than 255 levels THE SYSTEM SHALL fail with `LimitExceeded{FlowNesting}` regardless of `MaxDepth`. | must |
| FR-012 | WHEN alias expansion would materialize more than `MaxAliasBytes` (counted per node, 64 bytes each, plus an anchor-copy bound derived from the same budget) THE SYSTEM SHALL fail with `LimitExceeded{AliasBytes|AnchorCopies}` at the alias. | must |
| FR-013 | WHEN `%TAG` prefixes or tags exceed `MaxTagBytes` (with a 64-byte prefix allowance) THE SYSTEM SHALL fail with `LimitExceeded{TagBytes}`. | must |
| FR-014 | WHEN the scanner would read more than `MaxScanAhead` characters past the last reported node (a root or `- ` flow collection, one scalar, or a run of comments) THE SYSTEM SHALL fail with `LimitExceeded{ScanAhead}`; the limit SHALL be enforced by a wrapper around the parser input so it is checked before the scanner buffers. | must |
| FR-015 | THE SYSTEM SHALL share one `StreamBudget` (atomic counters) across chunks of the same stream so splitting a stream cannot multiply the alias or tag budget or the document count; each file in a file batch SHALL have its own budget. | must |
| FR-016 | WHEN input bytes exceed `MaxInputBytes` THE SYSTEM SHALL fail with `InputTooLarge` before parsing; file size SHALL be checked from metadata before reading, reads SHALL be capped at limit+1 bytes, and stdin SHALL be read at most limit+1 bytes. | must |
| FR-017 | WHEN a stream holds more documents than `ParseLimits::max_documents` THE SYSTEM SHALL fail with `LimitExceeded{Documents}` in every loader (core parse, `fy parse|format|convert|lint`, `lint`, `parse_parallel`, `FileProcessor`, Python and Node loaders, lint, batch and parallel APIs), positioned at the start of the first rejected document with the document suffix; `parse_parallel` SHALL check it while chunking, before parsing or allocating per-document state. The count is charged on each document start through the shared `StreamBudget`. | must |
| FR-018 | THE SYSTEM SHALL reject a NUL or other non-printable character with its code point and position before parsing (see [[001-parse-validate/spec|Parse and validate]] FR-004). | must |
| FR-019 | WHEN a batch (`fy lint` or `fy format` over several files, `FileProcessor::format_*`) runs without an explicit scan-ahead limit THE SYSTEM SHALL parse each file under `max(default / workers, 1 MiB)` (`ScanAheadPolicy::Scaled`) and re-run a file rejected with `LimitKind::ScanAhead` at the full default limit, one file at a time (`ScanAheadLane`, per run), so every file's outcome equals a single-file run on any machine. An explicit limit (`--max-scan-ahead`, config key, binding option, `Config::with_parse_limits`) is final (`ScanAheadPolicy::Fixed`): no scaling, no retry. | must |

### 3.3 Error and surface behavior

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-020 | WHEN a limit is exceeded THE SYSTEM SHALL report `YAML resource limit exceeded at line L, column C: {kind}` (document suffix for N >= 2) and the CLI SHALL add a `hint: raise with --max-...` line naming the matching flag. | must |
| FR-021 | THE SYSTEM SHALL map limit failures to exit code 1 and out-of-range flag values to exit code 2 (clap usage error). | must |
| FR-022 | THE CLI SHALL accept `--max-input-bytes` and `--max-scan-ahead` as global options (accepting `KiB`, `MiB`, `GiB` suffixes) and `--max-depth` / `--max-alias-bytes` on `parse`, `convert` and `lint` (`--max-depth` also on `format`) and `--max-documents` (1 to 10 000 000, default 100 000) on `parse`, `format`, `convert` and `lint`; for `lint` the global flags override the `max-input-bytes` / `max-scan-ahead` config keys (there is no config key for the other limits). | must |
| FR-023 | Python and Node.js SHALL validate every limit with the core ranges and the core error text shape; non-integer, negative, zero, `NaN` and over-cap values SHALL be rejected. | must |
| FR-024 | THE SYSTEM SHALL provide `MaxDepth::descend(depth)` and iterative (non-recursive) building and dropping so a depth-512 `Value` neither overflows the stack in the parser nor in drop. | must |

### 3.4 File and path safety

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-030 | WHEN a path is not a regular file (FIFO, device, directory) THE SYSTEM SHALL refuse it without blocking on `open`. | must |
| FR-031 | WHEN a file is read by `fast-yaml-parallel` (`read_file`), THE SYSTEM SHALL check the path is a regular file before opening, check it again on the opened file, check the size from metadata against `MaxInputBytes`, read at most limit+1 bytes into memory and decode it. There is no memory map and no `mmap_threshold`. | must |
| FR-032 | THE SYSTEM SHALL write in-place changes and `-o` outputs through a same-directory temp file and atomic rename (`write_atomic`, streaming `AtomicFile`), `fsync` the data and the directory, preserve permission bits and (on Unix) owner, group and extended attributes (ACLs excepted, see item 13), refuse dangling symlinks and non-regular targets, resist temp-file symlink planting and delete the temp file on every failure path. A hard-linked target SHALL be written in place only when the caller owns it (opened with `O_NOFOLLOW`, identity re-checked against the earlier `stat`); otherwise it SHALL be replaced by rename, which breaks the link. | must |
| FR-033 | THE SYSTEM SHALL NOT follow symlinks during directory discovery by default; an explicitly named symlink to a regular file SHALL be read; a broken symlink SHALL be reported as such. | must |
| FR-034 | THE SYSTEM SHALL cap rayon thread pools at 128 threads (`WorkerCount::MAX`; `-j`, `workers` and `thread_count` above 128 are rejected, `Auto` is capped) and keep one process-wide pool (`shared_pool`), replaced only when a different worker count is requested; `-j N` SHALL limit concurrency to N. | should |
| FR-035 | `fast-yaml-core`, `fast-yaml-linter` and `fast-yaml-parallel` SHALL `forbid(unsafe_code)` (the parallel crate lost its only `mmap` block); the workspace lint level is `deny`, so `unsafe` is possible only at FFI boundaries behind a local `#[allow]` and review. | must |
| FR-036 | THE SYSTEM SHALL read lint config files, `extends` targets and `ignore-from-file` files only as bounded regular files (at most 1 MiB, never opening a FIFO), see [[003-lint/spec]] FR-019. Every such read goes through `fast_yaml_core::fs::read_regular_file`, which checks the path before opening, opens it `O_NONBLOCK` on Unix so a late swap for a FIFO cannot block, re-checks the opened file and caps the read at limit+1. `ignore-from-file` accepts at most 32 different files and 1024 pattern lines, and its errors do not echo file content. | must |

## 4. Key entities

| Entity | Description |
|--------|-------------|
| `Bounded<K>` / `Bounds` | Sealed generic limit of kind `K`, range `1..=K::MAX`; aliases `MaxDepth`, `MaxAliasBytes`, `MaxInputBytes`, `MaxScanAhead`, `MaxDocuments` |
| `MaxTagBytes`, `MaxOutputBytes`, `MaxDumpNodes` | Unvalidated `usize` newtypes (see section 8) |
| `ParseLimits` | Bundle passed to every parse entry point |
| `StreamBudget` | Shared atomic accounting of alias and tag use for one stream |
| `LimitKind` | `Depth`, `AliasBytes`, `AnchorCopies`, `TagBytes`, `Documents`, `ScanAhead`, `FlowNesting`, plus dump-side `OutputBytes`, `DumpNodes` |
| `LimitRangeError`, `InputTooLarge` | Construction and size-check errors |
| `Config` (parallel) | Adds `max_input_bytes`, `parse_limits` (document limit included), `scan_ahead` policy; `max_documents` and `mmap_threshold` are gone |
| `ScanAheadPolicy`, `ScanAheadLane` | `Fixed(MaxScanAhead)` or `Scaled`; the lane holds the first limit and serializes full-limit retries (FR-019) |

## 5. Edge cases

| Scenario | Expected behavior |
|----------|-------------------|
| Limit equals exact size (`--max-depth 3` on depth-3 input) | Accepted; the limit is inclusive |
| Same limit given on both sides of the subcommand | Global flags merge; the value is validated once |
| Alias bomb split across parallel chunks | Budget shared; rejected as for a single stream |
| 150 000 empty documents (`---` lines) in `fy parse` | Rejected: `document count exceeds 100000 (document 100001)`; 100 000 pass |
| File truncated or grown while being read | The read is capped at limit+1 bytes and re-measured; no mapping, so no SIGBUS |
| Large file in a batch | Each file parsed with its own budget; total memory is about files-in-flight x budget; the first attempt uses the scaled scan-ahead limit and a rejected file is retried alone at the full limit |
| `-j 1` batch | One worker, so the scaled scan-ahead limit equals the default and nothing is retried |

## 6. Success criteria

| ID | Metric | Target |
|----|--------|--------|
| SC-001 | Peak memory of scan-ahead shapes (`tests/scan_memory.rs`) | stays under about 190x the configured `max-scan-ahead` |
| SC-002 | Alias-bomb input with default limits | rejected, no OOM, under the 64 MiB budget |
| SC-003 | Depth 512 input | parses and drops without stack overflow |
| SC-004 | Out-of-range limit on any surface | rejected with `must be between {min} and {max}, got {n}` |
| SC-005 | Failure of one file in a batch | the rest are still processed; exit reflects failure |
| SC-006 | Stream of 100 000 / 100 001 documents in parse, lint, format, convert, Python and Node loaders | accepted / rejected identically |
| SC-007 | Batch outcome of a corpus for `-j 1`, 2, 4, 8, 0 (`tests/scan_ahead_lane.rs`, CLI `scan_ahead_scaling`) | identical |

## 7. Agent boundaries

### Always
- Add a boundary test (limit-1, limit, limit+1) for any new or changed limit.
- Route new limits through `Bounded<K>` and expose them on every surface in the same change.

### Ask first
- Changing a default or maximum of any limit.
- Adding an unsafe block or a new dependency in a parser path.

### Never
- Add an "unlimited" or zero-means-off value.
- Skip `NormalizedInput` when feeding the parser.
- Weaken the same-directory atomic-write protocol.

## 8. Open questions / Known deviations

| # | Topic | Observed | Question |
|---|-------|----------|----------|
| 2 | Python input limit | Python loaders take no `max_input_bytes` (batch APIs do) (GAP-PY-021) | [NEEDS CLARIFICATION: add to `safe_load*`?] |
| 3 | Unvalidated limits | `MaxTagBytes`, `MaxOutputBytes`, `MaxDumpNodes` accept 0..usize::MAX, unlike the others (GAP-core-parse-008); not user-configurable on CLI | Make them `Bounded` or document |
| 4 | Dump-side limits in parse enum | `LimitKind` mixes parse- and dump-side variants (GAP-core-parse-014) | Split the enum |
| 5 | No output bound in core emitter | Only bindings enforce `MaxOutputBytes` (GAP-CORE-EMIT-011); depth defaults 512 (emit) vs 256 (parse) differ (GAP-CORE-EMIT-004) | [NEEDS CLARIFICATION: bound in core?] |
| 6 | Aggregate memory | No global byte budget across a parallel batch (threads x file size plus budgets) | [NEEDS CLARIFICATION: needed?] |
| 8 | Scan-ahead docs | Help says a value "after a tab" is rejected; only nested (`a:\t[[..]]`) or `-\t[..]` shapes are (GAP-core-parse-003) | Reword help |
| 9 | Support policy | `SECURITY.md` lists only 0.4.x as supported while the crate is 0.6.6 (GAP-core-parse-016 area) | [NEEDS CLARIFICATION: supported-version policy] **Proposed:** latest minor only; add a reproducible benchmark before keeping speed claims. |
| 10 | Windows | Unix-specific hardening is `cfg(unix)`; Windows is not exercised in CI | [NEEDS CLARIFICATION: supported platform?] |
| 11 | Document limit on dump | Python `dump`, `dump_all`, `safe_dump*` and Node `safeDump*` take no `max_documents` (Python `dump_parallel` honors `ParallelConfig.max_documents`); no `max-documents` key in lint config files | [NEEDS CLARIFICATION: add to dump paths and config?] |
| 12 | Scan-ahead aggregate | The 1 MiB floor of `Scaled` makes the first-attempt total `workers` x 1 MiB, above the default beyond 4 workers (up to 128 MiB of look-ahead, about 190 bytes each, at 128 workers) plus one full-limit retry; the lane is per run, so two concurrent runs can each hold a retry. Batch footprint also grows with file count (`WINDOW_PER_WORKER` results for lint) | [NEEDS CLARIFICATION: cap total in-flight bytes instead of per worker?] |
| 13 | `write_atomic` limits | Extended attributes are copied (#587; a denied `com.apple.*` or `security.selinux` attribute is skipped, the old file is opened `O_NOFOLLOW`, `O_NONBLOCK` and `O_NOCTTY`) but ACLs of the replaced file are lost; a caller-owned hard-linked target is rewritten in place (`set_len(0)` then write), not atomically, and a crash can leave it partial; owner/group fall back to the caller's on `EPERM` with group/other bits dropped | accept; document |
| 14 | Config reads | `extends` and `ignore-from-file` accept any absolute path and the pre-open check plus `O_NONBLOCK` keeps a swapped-in FIFO from blocking; `O_NONBLOCK` stays set on the regular-file descriptor (see [[003-lint/spec]] D-23) | accept |

Resolved in this batch and removed: item 1 (`MaxDocuments` is a `ParseLimits` field enforced by core and every surface, #574) and item 7 (mmap removed, #531).

## 9. See also

- [[001-parse-validate/spec|Parse and validate]] — consumer of these limits
- [[005-batch-parallel/spec|Batch and parallel]] — `Config`, discovery, atomic writes
- [[006-cli-contract/spec|CLI contract]] — flag and exit-code contract
- [[constitution]] — principles
