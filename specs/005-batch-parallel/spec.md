---
aliases:
  - Batch and parallel processing
  - fast-yaml-parallel
tags:
  - sdd
  - spec
  - batch
  - parallel
created: 2026-10-01
status: reverse-specified
related:
  - "[[constitution]]"
  - "[[plan]]"
  - "[[006-cli-contract/spec|CLI contract]]"
---

# Feature: Batch and parallel processing

> [!info] Metadata
> **Scope**: crate `fast-yaml-parallel` (document-level and file-level parallelism) and the CLI batch layer (`fast-yaml-cli`: `discovery`, `invocation`, `format_batch`, `lint_batch`, `reporter`).
> **Baseline**: v0.6.6 on `main` (e5e6cfb). Behaviour below was observed with `target/debug/fy`.

## 1. Purpose and value

Large repositories hold thousands of YAML files, and log or data dumps hold huge multi-document streams. This feature makes `fy format` and `fy lint` scale across cores and gives library users the same engine:

- **File level**: discover many files, process them concurrently, return per-file results and an aggregate.
- **Document level** (library only): split a multi-document stream at document boundaries and parse chunks concurrently, with output identical to sequential parsing.
- **Safety**: every file is bounded by the input-size limit, rewrites are atomic, and results are deterministic regardless of thread count.

### Out of scope (non-goals)

- Auto-fix for lint (`fy lint -i` is rejected).
- Parallel `fy parse` / `fy convert` (they take one input).
- Following symlinked directories during a walk, or any path sandboxing (callers own path trust).
- Cross-process or distributed processing; watch mode; incremental caching.
- Durable (fsync) writes.

## 2. User stories

### US-001: Preview formatting of a whole tree (P1)
AS A maintainer I WANT to see what `fy format` would change across a directory SO THAT I can gate CI without touching files.

```
GIVEN d/ holds a.yaml ("b:   1\na: 2\n"), sub/b.yml (clean), bad.yaml ("k: [1\n"), c.yaml ("# c\nz: 1\n"), t.txt
WHEN  I run `fy format -n --strip-comments d`
THEN  stderr shows "Completed: 4 files in <t>ms" then "1 unchanged", "2 would change", "1 failed"
AND   one `error: <abs path>/bad.yaml: YAML syntax error: ...` line follows
AND   no file is modified and the exit code is 1 (a failure outranks "would change")
```

```
GIVEN the same tree without bad.yaml and without comments
WHEN  I run `fy format -n d`
THEN  nothing is written, the summary lists "would change", exit code is 5
```

### US-002: Rewrite a tree in place (P1)
AS A developer I WANT `fy format -i dir` SO THAT all files are normalised at once.

```
GIVEN d2/ as above
WHEN  I run `fy format -i --strip-comments d2`
THEN  stderr shows "2 formatted", "1 unchanged", "1 failed"; a.yaml now reads "b: 1\na: 2\n"
AND   key order is preserved, failed files are left untouched, exit code is 1
```

```
WHEN  I run `fy format d` (no -i, no -n)
THEN  stderr is "error: use -i to format files in-place or --dry-run to preview changes" and exit code is 1
```

### US-003: Lint many files with ordered output (P1)
AS A CI author I WANT deterministic lint output for a batch SO THAT diffs between runs are meaningful.

```
GIVEN w.yaml (truthy + colons warnings), ok.yaml (clean), dup.yaml ("a: 1\na: 2\n")
WHEN  I run `fy lint -j 2 --format parsable w.yaml ok.yaml dup.yaml`
THEN  stdout lists dup.yaml before w.yaml (sorted by path), each line "<path>:<line>:<col>: [level] message (rule)"
AND   running again with -j 1 or -j 8 produces byte-identical stdout
AND   exit code is 2 because a diagnostic of severity error exists
```

### US-004: Select inputs flexibly (P2)
AS A user I WANT directories, globs, explicit files and stdin lists SO THAT I can target exactly what changed.

```
WHEN  I run `fy format -n 'd/*.yaml' --strip-comments`
THEN  the three matching files are processed (the unquoted glob also works when the shell expands it)
WHEN  I run `printf '# list\nok.yaml\n\nclean.yaml\n' | fy format -n --stdin-files`
THEN  both files are processed ("2 unchanged"); blank and `#` lines are ignored
WHEN  I run `fy format -n 'zz/*.yaml'`
THEN  error "glob pattern matched no files: 'zz/*.yaml'", exit 1
WHEN  I run `fy format -n d/a.yaml d/t.txt`
THEN  error "not matched by the include patterns (default: *.yaml, *.yml; see --include): 'd/t.txt'", exit 1
```

### US-005: Parse a huge multi-document stream in parallel from Rust (P2)
AS A library user I WANT `parse_parallel(text)` SO THAT large streams parse faster with the same result as `Parser::parse_all`.

```
GIVEN "---\nfoo: 1\n---\nbar: 2\n---\nbaz: 3"
WHEN  parse_parallel is called
THEN  it returns 3 documents in input order
```

### US-006: Build custom batch operations (P3)
AS A tool author I WANT `FileProcessor::process(paths, f)` SO THAT I can run my own per-file closure with the same reading, limits and result bookkeeping.

## 3. Functional requirements

### 3.1 File-level processing (library)

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-001 | WHEN `FileProcessor::parse_files`, `format_in_place` or `process` receives paths, THE SYSTEM SHALL return one `BatchResult` with `total`, `success`, `changed`, `failed`, `duration` and `errors: Vec<(PathBuf, Error)>`, where `total = success + failed`; `format_files` SHALL instead return `Vec<(PathBuf, Result<FormatOutput>)>` in input order. | must |
| FR-002 | WHEN a file fails (I/O, decode, size, syntax, comment guard, write), THE SYSTEM SHALL record it as `FileOutcome::Error` and continue with the other files. | must |
| FR-003 | WHEN reading a file, THE SYSTEM SHALL reject non-regular files (directory, FIFO, device) before opening, check size against `Config::max_input_bytes` before reading, and cap the read at `max + 1` bytes so a growing file is still rejected. | must |
| FR-004 | WHEN a file is at least `mmap_threshold` bytes (default 512 KiB), THE SYSTEM SHALL memory-map it, falling back to an in-memory read if mapping fails; WHEN smaller, THE SYSTEM SHALL read it into a `String`. | must |
| FR-005 | WHEN a file starts with a UTF-16/UTF-32 BOM or is not valid UTF-8, THE SYSTEM SHALL fail it with `Error::Decode`. A UTF-8 BOM is accepted. | must |
| FR-006 | WHEN `format_files` / `format_in_place` is called with `CommentPolicy::Reject` (the default) and a file contains comments, THE SYSTEM SHALL fail that file with `Error::CommentsWouldBeStripped`; with `CommentPolicy::Strip` it SHALL format and drop comments. | must |
| FR-007 | WHEN `format_in_place` produces output identical to the file's content, THE SYSTEM SHALL NOT write the file and SHALL classify it `Unchanged`; WHEN different, it SHALL write via `write_atomic` and classify `Changed`. | must |
| FR-008 | WHEN `write_atomic` replaces a file, THE SYSTEM SHALL write an `O_EXCL` temp file in the same directory and rename it over the target, keep existing permission bits, resolve a symlink target to its real file, refuse dangling links and non-regular targets, and leave no temp file on failure. | must |
| FR-009 | WHEN the batch is small (fewer than 4 files, or under 1,000,000 bytes total and fewer than 10 files), THE SYSTEM SHALL process sequentially; otherwise it SHALL use rayon `par_iter`. | should |
| FR-010 | THE SYSTEM SHALL return per-file results in input order independent of worker count. | must |

### 3.2 Document-level parsing (library)

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-020 | WHEN `parse_parallel(_with_config)` receives text, THE SYSTEM SHALL normalise it (BOM, line endings), split at document boundaries (`---`, `...`, directives, block-scalar and quoting state aware), parse chunks concurrently and return documents in source order. | must |
| FR-021 | WHEN the input has one chunk, is small (`< sequential_threshold`, default 4096 bytes, and fewer than 4 chunks) or `workers == Some(0)`, THE SYSTEM SHALL parse sequentially. | must |
| FR-022 | WHEN the input exceeds `Config::max_input_bytes` (default 100 MiB), THE SYSTEM SHALL return `Error::InputTooLarge`; WHEN it holds more than `Config::max_documents` (default 100,000) documents, `Error::TooManyDocuments`, checked before parsing. | must |
| FR-023 | WHEN several chunks fail, THE SYSTEM SHALL report the error of the lowest-index chunk, with line marks relocated to the whole input and the document index in the message. | must |
| FR-024 | WHEN `workers = Some(n)` with `n > 0` differs from the global pool size, THE SYSTEM SHALL build a dedicated pool capped at 128 threads; `None` SHALL use the global pool. | should |
| FR-025 | THE SYSTEM SHALL produce the same values as `Parser::parse_all`, except for the documented divergences (block scalar at column 0 cut by `---`, empty block scalar before `---`, error text/position for unterminated quoted/flow scalars). | must |
| FR-026 | WHEN the input is empty, THE SYSTEM SHALL return zero documents; WHEN it is only a BOM, one null document. | should |

### 3.3 Discovery and input selection (CLI)

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-040 | WHEN `format` or `lint` gets exactly one existing non-directory path and no batch flag (`--include`, `--exclude`, `-j N>0`, `--stdin-files`), THE SYSTEM SHALL run in single-file mode; WHEN no path is given, in stdin mode; otherwise in batch mode. | must |
| FR-041 | WHEN batch flags are given without any input, THE SYSTEM SHALL fail with a message naming `--jobs`, `--include`, `--exclude`. | must |
| FR-042 | WHEN a directory is walked, THE SYSTEM SHALL skip hidden entries, honour `.gitignore`/global/exclude files inside git work trees, NOT follow symlinks, recurse to depth 100 (depth 1 with `--no-recursive`) and match file names against `*.yaml`/`*.yml` case-insensitively; `--include` SHALL replace these defaults and match on the file name. | must |
| FR-043 | WHEN `--exclude` globs are given, THE SYSTEM SHALL match them (case-insensitive) against the path as given, with a leading `./` stripped; matching files SHALL be dropped silently. | must |
| FR-044 | WHEN an explicit file path is given in batch mode, THE SYSTEM SHALL require a regular file matching the include patterns, and SHALL follow a symlink to a regular file. | must |
| FR-045 | WHEN a path does not exist and contains `*` or `?`, THE SYSTEM SHALL expand it as a glob (max 100,000 matches, then a `Warning:` and truncation); zero matches SHALL fail with `glob pattern matched no files`. `[` alone is literal. | must |
| FR-046 | WHEN `--stdin-files` is used, THE SYSTEM SHALL read one path per line, trim it, skip blank and `#` lines, and fail on a line over 4096 bytes, over 100,000 lines, a directory, a non-YAML file or a missing file, naming the line number. An empty list SHALL be a successful no-op. It SHALL conflict with positional paths (usage error, exit 2). | must |
| FR-047 | WHEN discovery finishes, THE SYSTEM SHALL de-duplicate by canonical path and report canonical absolute paths. | must |
| FR-048 | WHEN discovery yields no files, THE SYSTEM SHALL fail with `no YAML files found: every input was empty or filtered out by include/exclude patterns`, unless a config `ignore` removed files (lint) or the stdin list was empty. | must |
| FR-049 | WHEN a path is missing, a broken symlink, unreadable or invalid, THE SYSTEM SHALL fail with `path does not exist`, `broken symbolic link`, `permission denied` or `failed to read` naming the path, exit 1. | must |

### 3.4 CLI batch execution

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-060 | WHEN `format` runs in batch mode without `-i` or `-n`, THE SYSTEM SHALL fail before reading files (exit 1). `-n` SHALL win over `-i`; `-n` conflicts with `-o` (usage error, exit 2). | must |
| FR-061 | WHEN a `format` batch ends, THE SYSTEM SHALL print to stderr a blank line, `Completed: N file(s) in X.XXms`, then one line each for non-zero counters among `formatted`, `unchanged`, `would change`, `failed`, then one `error: <path>: <message>` per failed file. `-q` SHALL suppress everything unless a file failed. | must |
| FR-062 | THE SYSTEM SHALL exit a `format` batch with 1 if any file failed, else 5 if `-n` and any file would change, else 0. | must |
| FR-063 | WHEN `lint` runs in batch mode, THE SYSTEM SHALL honour `-j N` with a dedicated pool (`0` or unset means all cores), lint files concurrently, and render the report sorted by file path so output does not depend on scheduling. | must |
| FR-064 | WHEN a file cannot be read or has a syntax error in a lint batch, THE SYSTEM SHALL print `error: '<path>': <message>` to stderr, keep linting the rest and exit 2. | must |
| FR-065 | WHEN lint config contains `ignore` or `yaml-files`, THE SYSTEM SHALL apply them during lint discovery (explicitly named files that match `ignore` are skipped silently). | should |
| FR-066 | THE SYSTEM SHALL apply `--max-input-bytes` and `--max-scan-ahead` to every file of a batch. | must |

## 4. Key entities and types

| Entity | Description |
|--------|-------------|
| `Config` (parallel) | Builder: `workers: Option<usize>`, `mmap_threshold` (512 KiB), `max_input_bytes: MaxInputBytes`, `max_documents: MaxDocuments`, `sequential_threshold` (4096), `parse_limits: ParseLimits`, `key_domain: KeyDomain`. |
| `FileProcessor` | Owns `Config` and a `SmartReader`; entry points `parse_files`, `format_files`, `format_in_place`, `process`. |
| `SmartReader` / `FileContent` | Size-aware reader returning `String` or `Mmap`-backed content. |
| `FileResult`, `FileOutcome` | `Success`, `Changed`, `Unchanged`, `Error` with per-file duration. |
| `BatchResult` | Aggregate counters and `errors`. |
| `CommentPolicy` | `Reject` (default) or `Strip`. |
| `FormatOutput` | `{ formatted: String, changed: bool }`. |
| `Error` | `Parse`, `Io`, `Decode`, `Format`, `EmptyDocument`, `CommentsWouldBeStripped`, `CommentScan`, `Write`, `InputTooLarge`, `TooManyDocuments`, `ThreadPool`. |
| `DiscoveryConfig`, `FileDiscovery`, `Target` (CLI) | Include/exclude, depth, hidden, gitignore, symlink policy; input resolution into stdin / file / batch. |

## 5. Edge cases and error handling

| Scenario | Expected behaviour |
|----------|--------------------|
| `fy format -n -j 2 ok.yaml` | `-j` forces batch mode: summary printed, "1 unchanged", exit 0. |
| Directory of only non-YAML files or empty dir | `no YAML files found: ...`, exit 1. |
| Symlinked directory inside a walked tree | Not followed; if nothing else matches, `no YAML files found`. |
| Explicit symlink to a regular file | Processed. |
| Explicit broken symlink | `broken symbolic link: 'sl/broken.yaml'`, exit 1. |
| Same file passed twice | De-duplicated in the CLI; the library does not de-duplicate (callers must). |
| Empty file via `parse_files` | `Error::EmptyDocument`; whitespace- or comment-only file is a successful null document. |
| File grows between size check and read | Rejected as too large; mmap path re-measures the mapping. |
| Panic in a worker (lint batch) | Re-raised after the pool scope joins; no partial report is printed. |
| Mixed failures and would-change | Exit 1. |
| `--exclude 'sub/**'` for argument `d` | Matches nothing (path-as-given semantics); use `**/sub/**`. |

## 6. Success criteria

| ID | Metric | Target |
|----|--------|--------|
| SC-001 | Output of `fy lint` over a fixed corpus is byte-identical for `-j 1`, `-j 2`, `-j 8` | 100% |
| SC-002 | `parse_parallel` equals `Parser::parse_all` on the corpus in `tests/parse_all_parity.rs` (modulo FR-025 divergences) | 100% |
| SC-003 | A failed file never prevents other files from being processed or reported | 100% |
| SC-004 | `format -i` never leaves a partially written or temp file on failure | 0 leftovers |
| SC-005 | Files over `--max-input-bytes` are rejected without reading more than limit + 1 bytes | always |
| SC-006 | Batch exit code follows FR-062 / FR-064 in every mode | 100% |

## 7. Agent boundaries

### Always (without asking)
- Run `cargo nextest run -p fast-yaml-parallel -p fast-yaml-cli` after changes; keep result ordering deterministic.
- Route every file write through `write_atomic`.

### Ask first
- Changing discovery defaults (hidden files, gitignore, symlink policy, exclude semantics).
- Changing exit codes or the summary layout (scripts parse them).
- Adding a dependency to `fast-yaml-parallel`.

### Never
- Write files directly (non-atomic) or follow symlinked directories silently.
- Weaken the input-size or document-count checks.
- Print machine-readable output to stderr or progress to stdout.

## 8. Open questions / Known deviations

| # | Topic | Observed (verified) | Question |
|---|-------|--------------------|----------|
| 1 | `format -j N` is ignored (P2, GAP-PARALLEL-001) | `FileProcessor` always uses the global rayon pool; `Config::workers` is unused for files; `lint -j` does build its own pool. | [NEEDS CLARIFICATION] Honour `-j` for format (dedicated pool, `0`=auto) or remove it from format? |
| 2 | Dry-run duration (P3, GAP-PARALLEL-002) | `format -n` prints `Completed: 4 files in 0.00ms`; in-place prints the real time. | Fix timing to wrap the whole run? |
| 3 | `max_documents` / `key_domain` not applied by `FileProcessor` (P3, GAP-PARALLEL-003) | A file with more documents than `max_documents` still succeeds in `parse_files`/`format_files`. | [NEEDS CLARIFICATION] Enforce per file (core-level limit) or document as document-level only? (see 009-limits-security) **Proposed:** enforce in core so every surface inherits it. |
| 4 | CLI never uses `parse_parallel` (P3, GAP-PARALLEL-006) | A single huge multi-document file is processed on one thread by `fy`. | Intentional? Bindings do use it. |
| 5 | `--exclude` semantics (P3, GAP-CLI-016) | Matched against the path as given, not relative to the walk root; directories are not matched by name. | [NEEDS CLARIFICATION] Switch to gitignore-style matching relative to each root? |
| 6 | `format` ignores config `ignore`/`yaml-files` (P4, GAP-CLI-027) | Only lint applies them. | Should format honour `.fast-yaml.yaml`? |
| 7 | Empty-input semantics differ (P4, GAP-PARALLEL-005) | `parse_parallel("")` = 0 docs, `parse_files` on empty file = `EmptyDocument`, comment-only = success. | Unify or document as intentional. |
| 8 | mmap TOCTOU (P4, GAP-PARALLEL-007) | A truncation/rewrite during mapping can raise SIGBUS or invalidate the `&str`; no locking exists. CLI single-file and lint batch use a threshold of `u64::MAX` (never mmap); `format` batch uses the 512 KiB default. | Keep as an accepted, documented risk? Raise the format threshold? |
| 9 | Aggregate memory (P3) | Memory is roughly threads x file size x parse budget; no global budget. | [NEEDS CLARIFICATION] Cap concurrent bytes in flight? |
| 10 | `-o` with batch `-i` (P3, GAP-CLI-007) | `fy format -i -o x.yaml ok.yaml clean.yaml` exits 0, rewrites in place and creates no `x.yaml`. | [NEEDS CLARIFICATION] Reject `-i` + `-o` with a usage error? |
| 11 | Fuzz/proptest gap (P3, GAP-PARALLEL-009) | Chunker parity is example-based; no differential fuzz target. | Add `parse_parallel(x) == parse_all(x)` fuzzing. |
| 12 | Batch exit codes (P1, GAP-CLI-017) | A lint syntax error in a batch exits 2, the same file alone exits 1; codes 3/4 are never produced | [NEEDS CLARIFICATION: one uniform table, see CLI contract] **Proposed:** distinct codes for findings, syntax error and usage/IO, applied uniformly, one integration test per code. |
| 13 | Support policy | Windows is not exercised in CI; `SECURITY.md` lists only 0.4.x; speed claims have no benchmark | [NEEDS CLARIFICATION: supported platforms and versions, contractual targets] **Proposed:** latest minor only; add a reproducible benchmark before keeping speed claims. |

## 9. See also

- [[plan]] - technical plan for this feature
- [[006-cli-contract/spec|CLI contract]] - exit codes, flags, channels
- [[constitution]] - principles and spec map
