---
aliases:
  - Format
  - fy format
tags:
  - sdd
  - spec
  - format
  - cli
created: 2026-10-01
status: reverse-specified
related:
  - "[[constitution]]"
  - "[[plan]]"
  - "[[004-convert/spec|Convert]]"
---

# Feature: Format

> [!info] Metadata
> **Product version**: fast-yaml 0.6.6 (main, e5e6cfb) plus the fixes #581 (`-j`), #574 (`--max-documents`), #580 (anchors after a non-ASCII directive) and #531/#366 (file read and write). **Surfaces**: `fy format`, `fast_yaml_core::Emitter::format*`, `streaming::format_streaming*`, bindings' format APIs.
> **Method**: requirements reverse-specified from code and confirmed by real `fy` runs.

## 1. Purpose and value

Turn any valid YAML 1.2.2 stream into one canonical block-style layout so that diffs are small, output is deterministic, and the result is semantically identical to the input. The formatter works on the parser event stream (not on a loaded `Value`), so it keeps what `Value` would lose: scalar spelling (`yes`, `0x1F`, `~`), anchors, aliases, tags, duplicate keys, directives and document boundaries.

It deliberately does not preserve comments, blank lines, flow style or multi-line scalar layout. Because that loses information, `fy format` refuses to touch a file that has comments unless the user opts in.

### Goal
After `fy format`, the document parses to the same data as before, formatting the output again changes nothing, and no comment is ever lost without an explicit opt-in.

### Non-goals
- Preserving comments, blank lines, or flow style (`{a: 1}` becomes block).
- Wrapping or folding long lines (`--width` is accepted but inert, see section 9).
- Style options beyond indent (no quote-style, key sorting, or `---` forcing flags on the CLI).
- Linting or fixing semantic problems (see lint spec).
- Choosing between JSON/YAML output: `fy format` always emits YAML (the former `-f/--format` flag is removed).

## 2. User stories

### US-001 (P1): Normalize a file to stdout
AS A developer I WANT `fy format file.yaml` to print canonical YAML SO THAT I can review the change before applying it.

```
GIVEN a.yaml containing
  b:   1
  a:
      - x
      -   y: 2
          z: [1,2,3]
      - {k: v}
WHEN  I run `fy format a.yaml`
THEN  stdout is
  b: 1
  a:
    - x
    - y: 2
      z:
        - 1
        - 2
        - 3
    - k: v
AND   the exit code is 0 and a.yaml is unchanged
```

### US-002 (P1): Never silently drop comments
AS A maintainer I WANT formatting to fail when comments would be lost SO THAT I do not erase documentation by accident.

```
GIVEN c.yaml = "# top\na: 1 # trail\n"
WHEN  I run `fy format c.yaml`
THEN  stderr is "error: file contains YAML comments that formatting would strip; use --strip-comments to allow this"
AND   exit code is 1 and nothing is written

WHEN  I run `fy format --strip-comments c.yaml`
THEN  stdout is "a: 1\n" and exit code is 0
```

The same guard applies to `-i`: `fy format -i c.yaml` fails with the same message and leaves the file byte-identical.

### US-003 (P1): Format in place, many files
AS A developer I WANT `fy format -i dir/ other.yaml` to rewrite files in parallel SO THAT a whole repository is normalized with one command.

```
GIVEN dir/x.yaml = "a:   1\n" and dir/y.yml = "b:  2\n"
WHEN  I run `fy format -i dir`
THEN  both files are rewritten, stdout ends with "Completed: 2 files ..." and "  2 formatted", exit code 0

GIVEN a path list where one file contains comments
WHEN  I run `fy format -i dir c.yaml`
THEN  the other files are processed, the summary shows "1 failed", the failure names c.yaml, exit code 1
```

Without `-i` or `--dry-run`, several paths or a directory are an error: `error: use -i to format files in-place or --dry-run to preview changes` (exit 1).

### US-004 (P1): CI check without writing
AS A CI maintainer I WANT `--dry-run` SO THAT a pipeline fails when files are not formatted.

```
GIVEN a.yaml that is not canonical
WHEN  I run `fy format -n a.yaml`
THEN  nothing is written; stdout shows "Completed: 1 file in ..." and "  1 would change"
AND   exit code is 5
```

Exit code is 0 when nothing would change, 1 when any file failed (takes precedence over 5). `--dry-run` also works for stdin and conflicts with `-o` (usage error, exit 2).

### US-005 (P2): Filter pipeline
AS A script author I WANT stdin to stdout formatting SO THAT `fy format` composes in pipes.

```
WHEN  I run `printf 'a:   1\n' | fy format`
THEN  stdout is "a: 1\n"
WHEN  I run `printf 'a:   1\n' | fy format -i`
THEN  stderr is "error: --in-place (-i) requires a file argument", exit 1
```

### US-006 (P2): Choose indentation
AS A team member I WANT `--indent N` SO THAT output matches house style.

```
WHEN  I run `fy format --indent 4 a.yaml`
THEN  nested mappings and sequences are indented 4 spaces per level:
  b: 1
  a:
      - x
      - y: 2
        z:
            - 1
```
`--indent 0` and `--indent 10` are usage errors (`must be between 1 and 9`, exit 2).

### US-007 (P3): Trustworthy on hostile input
AS AN operator I WANT limits and typed errors SO THAT a malformed or huge file cannot crash a batch.

```
WHEN  I run `fy format --max-depth 2 a.yaml` on a 3-level document
THEN  stderr shows "YAML resource limit exceeded at line 4, column 9: nesting depth exceeds 2" with "hint: raise with --max-depth", exit 1
```

## 3. Functional requirements

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-001 | WHEN input is valid YAML THE SYSTEM SHALL write block-style YAML that parses to the same data (same scalars as strings, same anchors/aliases/tags, same document count). | MUST |
| FR-002 | WHEN the output is formatted again THE SYSTEM SHALL return it byte-for-byte unchanged (idempotency). | MUST |
| FR-003 | THE SYSTEM SHALL indent each nesting level by `--indent` spaces (1..=9, default 2); the `- ` dash width is independent of indent. | MUST |
| FR-004 | WHEN the input contains a comment (outside quoted/block scalars, keys, anchors, tags) and `--strip-comments` is absent THE SYSTEM SHALL fail with the comment error and exit 1 without writing; WITH the flag it SHALL drop all comments. | MUST |
| FR-005 | WHEN a scalar is written THE SYSTEM SHALL keep its source spelling and quoting (`yes`, `0x1F`, `01`, `1e3`, `.inf`, `~`, `'plain'`, `"it's"`, `"007"`). | MUST |
| FR-006 | WHEN a value is omitted (`a:`) or a document is empty THE SYSTEM SHALL write `null`; an explicit `~` or `null` keeps its spelling. | MUST |
| FR-007 | WHEN the input has several documents THE SYSTEM SHALL separate them with `---`, keep `%YAML`/`%TAG` directives and a leading `---` that follows a directive, and drop the trailing `...`. Reserved directives (any other `%NAME`) are kept in order with trailing blanks and a trailing comment cut, and the output is idempotent. | MUST |
| FR-008 | WHEN a node has an anchor, alias or tag THE SYSTEM SHALL preserve them (`x: &a` followed by an indented body; `y: *a`; `!!str 5`), also in a document that follows a directive with a non-ASCII name. | MUST |
| FR-009 | WHEN a flow collection appears THE SYSTEM SHALL rewrite it as block style; empty collections stay `[]` / `{}`. | MUST |
| FR-010 | WHEN a sequence or mapping is a mapping key THE SYSTEM SHALL use the explicit `?` form. | MUST |
| FR-011 | WHEN a plain or quoted scalar spans several lines THE SYSTEM SHALL write it as one double-quoted scalar with escapes (`a: "foo bar\nbaz"`); literal `\|` and folded `>` block scalars stay block scalars with normalized indentation. | MUST |
| FR-012 | WHEN a mapping has duplicate keys THE SYSTEM SHALL keep all of them in source order (the formatter does not reject them; that is lint's job). | MUST |
| FR-013 | WHEN the input starts with a BOM THE SYSTEM SHALL keep the BOM through `fy format`; CRLF input SHALL produce LF output. | SHOULD |
| FR-014 | WHEN `--dry-run` is given THE SYSTEM SHALL never write any file or `--output` target, print a one-line-per-category summary, and exit 5 if any file would change, 1 if any failed, else 0. | MUST |
| FR-015 | WHEN `-i` is given without a file path THE SYSTEM SHALL fail with a usage-level error; WHEN no path is given THE SYSTEM SHALL read stdin and write stdout. | MUST |
| FR-016 | WHEN several files are formatted THE SYSTEM SHALL process them in parallel on the shared pool (`-j N` = N threads, 0 = auto), isolate failures per file, and report an aggregate summary with the real elapsed time (also for `--dry-run`); successful files are still written. | MUST |
| FR-017 | WHEN an in-place file is already canonical THE SYSTEM SHALL leave it untouched (no rewrite) and count it as `unchanged`. | MUST |
| FR-018 | WHEN a limit is exceeded (`--max-depth` 1..=512 default 256, `--max-documents` default 100 000, `--max-input-bytes`, `--max-scan-ahead`) THE SYSTEM SHALL return a typed error with the location and a hint naming the flag, never panic or exhaust the stack. | MUST |
| FR-019 | WHEN the input is syntactically invalid THE SYSTEM SHALL fail with "Failed to format YAML" plus the parser message and position, exit 1, writing nothing. | MUST |
| FR-020 | WHEN an alias references an unknown anchor THE SYSTEM SHALL fail instead of emitting a dangling alias. | MUST |
| FR-021 | WHEN a file is rewritten in place THE SYSTEM SHALL write it atomically (temporary file, `fsync`, owner, mode and (on Unix) extended attributes kept, then rename via `write_atomic`), so a failure never leaves a half-written file; a hard-linked file the caller owns is written in place so all links see the result ([[005-batch-parallel/spec]] FR-008). | MUST |
| FR-022 | WHEN the input is empty THE SYSTEM SHALL succeed with empty output. | SHOULD |

## 4. Key entities and types

| Type | Role |
|------|------|
| `EmitterConfig` | Options for emit and format: `indent: Indent`, `width: Width`, `explicit_start`, `default_flow_style`, `multiline_strings`, `max_emit_depth: MaxDepth`, `parse_limits: ParseLimits`. The format pipeline reads only `indent`, `explicit_start` and `parse_limits`. |
| `Indent` (1..=9), `Width` (20..=1000) | Range-checked newtypes (`LimitRangeError` on violation). |
| `Emitter::format*` / `streaming::format_streaming*` | Text to text formatter over parser events (std and optional arena backend). |
| `CommentScanner`, `has_comments*`, `find_comments` | Detect comments during/after parsing using byte ranges. |
| `NormalizedInput` | Input after BOM/encoding normalization shared by parse, format, and comment scan. |
| `CommentPolicy` (`fast-yaml-parallel`) | Refuse vs strip decision used by batch formatting. |
| `EditIntent` (`Preview`, `InPlace`, `Print`), `WriteMode`, `FormatStatus` | CLI-side resolution of what to do with the result. |
| `EmitError` | `Parse(ParseError)` for limits and syntax, plus emit-only variants (`ComplexFlowKey`, `SetAsKey`, `DepthLimitExceeded`). |

## 5. Edge cases and error handling

| Scenario | Expected behavior (verified) |
|----------|------------------------------|
| `a:\nb: ~\nc: null` | `a: null`, `b: ~`, `c: null` |
| `a: 1\n---\n---\n` | `a: 1`, `---`, `null`, `---`, `null` (each empty document is `null`) |
| `? [1,2]\n: v` | `?\n  - 1\n  - 2\n: v` |
| `a: 1\na: 2` | both keys kept |
| `<<: {a: 1}\nb: 2` | `<<:` written as a normal key with block body; no merge is applied |
| `{"a": [1, 2]}` (JSON input) | `"a":\n  - 1\n  - 2` (keys keep their quotes) |
| `%YAML 1.2\n---\na: 1` | directive and `---` retained |
| `a: *x` (unknown anchor) | `found unknown anchor at line 1, column 4`, exit 1 |
| `a: [1` | `while parsing a flow sequence, expected ',' or ']'`, exit 1 |
| Comment inside a quoted scalar or `#` in a key/anchor/tag | not a comment; no guard error |
| Comment plus syntax error in the same file | the format error is reported, not the comment error |
| Missing path / glob with no match / explicit non-YAML file | `error: path does not exist: '...'`, exit 1 |
| Implicit key longer than 1024 chars | parser error (YAML 1.2 limit), not a formatter bug |
| `--indent 0|10`, `--max-depth 513`, `--width 19` | usage error, exit 2 |
| Flow nesting over 255 levels | rejected by the scanner regardless of `--max-depth` |

## 6. Non-functional requirements

| ID | Category | Requirement |
|----|----------|-------------|
| NFR-001 | Safety | Nesting is held on the heap: no recursion-driven stack overflow at depth 512 on the emit/format path. |
| NFR-002 | Determinism | Same input and options produce identical bytes on every run, platform, and thread count. |
| NFR-003 | Performance | Formatting is a single streaming pass over parser events; batch runs scale with `-j`. [NEEDS CLARIFICATION: no reproducible benchmark backs a contractual throughput target] |
| NFR-004 | Memory | Parser read-ahead is bounded by `--max-scan-ahead` (default 4 MiB, roughly 190x per input). |

## 7. Success criteria

| ID | Metric | Target |
|----|--------|--------|
| SC-001 | Idempotency on `tests/fixtures/yaml-spec` and `tests/fixtures/linter` (42 files, `--strip-comments`) | 100% byte-stable on second pass |
| SC-002 | Data equivalence: `fy convert json` of input equals that of output | 100% on the fixture set and fuzz/proptest corpus |
| SC-003 | Comment safety: files with comments formatted without `--strip-comments` | 0 files modified, exit 1 |
| SC-004 | Core test suite (`cargo nextest run -p fast-yaml-core --all-features`) | green (723 tests at 0.6.6) |
| SC-005 | Fuzz target `fuzz/fuzz_targets/format.rs` | no panic, no non-idempotent output |

## 8. Agent boundaries

### Always
- Run `fy format` on a scratch copy when verifying behavior; never `-i` real fixtures.
- Re-run the idempotency check after changing the emitter or formatter.

### Ask first
- Adding a formatter option (quoting, wrapping, sorting) or changing a default.
- Changing which `EmitterConfig` fields the format pipeline honors.
- Making the formatter preserve comments or flow style.

### Never
- Weaken `Indent`/`Width` newtypes into plain integers.
- Make comment stripping the silent default.
- Write to a file before the whole result is computed and verified.

## 9. Open questions / Known deviations

| # | Item | Status |
|---|------|--------|
| 1 | `--width` is validated (20..=1000) but never applied; CLI help, Python stub and Node typings still promise "maximum line width". Verified: a 120-character string stays on one line. (GAP-CORE-EMIT-001, OQ-01) | [NEEDS CLARIFICATION: implement folding, or remove the option from every surface (pre-1.0 breaking change)] **Proposed:** remove the option on every surface (pre-1.0 breaking change allowed); wrapping risks round-trip fidelity. |
| 2 | `fy format -v` prints nothing extra although README says it lists each file. (GAP-CLI-004) | [NEEDS CLARIFICATION: implement or drop from docs] |
| 3 | Format re-parses the input a second time to find comments; with a file that is both commented and invalid/over-limit the format error wins. (GAP-CORE-EMIT-010) | accepted; use `CommentScanner` in the single pass |
| 4 | BOM asymmetry: `Emitter::format*` and `fy format` keep the BOM; `streaming::format_streaming*` drop it. (GAP-CORE-EMIT-009) | [NEEDS CLARIFICATION: converge on one behavior] |
| 5 | Silent normalizations not mentioned in help/README: blank lines dropped, multi-line plain/quoted scalars folded into one double-quoted line, flow to block. (GAP-CORE-EMIT-016) | document in `fy format --help` |
| 6 | `a:\t1` (tab after mapping colon) is rejected by parse and format although YAML 1.2.2 allows a tab there; likely inherited from the parser. (GAP-CORE-EMIT-007, OQ-07) | [NEEDS CLARIFICATION: accepted limitation or parser bug] **Proposed:** document as a known limitation and track upstream. |
| 7 | Bindings' file formatters strip comments silently, unlike the CLI. (GAP-NODE-011, GAP-PY-014, OQ-03) | [NEEDS CLARIFICATION: mirror `CommentPolicy`]. See Python/Node specs **Proposed:** mirror the CLI policy now (refuse unless explicitly allowed); preserve comments long term. |
| 8 | `-i` silently overrides `-o`. (GAP-CLI-007) | see CLI spec |
| 9 | Default depth differs: emit 512, format/parse 256. (GAP-CORE-EMIT-004) | [NEEDS CLARIFICATION: converge or document] |
| 10 | Reserved directives (resolved by #592): `%FOO bar` before `---` is kept (`%ÄÖÜ x`, BOM, CRLF, stdin and multi-document streams included); a bare `%` is a syntax error. `%YAML` and `%TAG` are kept too (the parser emits no directive events, so the formatter re-reads them from the source). | closed |

## 10. See also

- [[plan]] — technical plan
- [[004-convert/spec|Convert]] — JSON/YAML conversion shares the emitter
- [[constitution]] — principles
