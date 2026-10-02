---
aliases:
  - Parse and Validate
  - fy parse
tags:
  - sdd
  - spec
  - parse
  - core
created: 2026-10-01
status: reverse-specified
related:
  - "[[constitution]]"
  - "[[009-limits-security/spec|Limits and security]]"
---

# Feature: Parse and validate YAML 1.2.2

> [!info] Metadata
> **Scope**: `fast-yaml-core` load API (`Parser`, `Value`, scalar resolution, merge keys, sets, key domains, input decoding) and the `fy parse` command.
> **Baseline**: v0.6.6, commit e5e6cfb, plus the fixes #574 (document limit in core), #580 (anchors after a non-ASCII directive) and #567 (float key text). Behavior below was confirmed by running `fy` and reading the code.

## 1. Purpose and value

Everything else in the product (format, lint, convert, bindings) starts with one question: is this text valid YAML, and what data does it denote? This feature is the single answer. It turns bytes into a typed `Value` tree under the YAML 1.2.2 Core Schema, rejects malformed or hostile input with a precise position, and does so identically for every surface (CLI, Rust, Python, Node.js) because they all call the same resolver and loader.

Users get: a fast "is this file valid?" gate for CI and editors, a predictable typing model (no YAML 1.1 surprises such as `yes` becoming a boolean), and errors that point at a line and column.

### Non-goals

- Not a YAML 1.1 loader: `yes/no/on/off`, sexagesimal numbers, `0b`, `1_000` stay strings.
- No timestamp, binary, omap or custom-tag construction; those tags never produce special types.
- No schema validation (JSON Schema, OpenAPI); no `$ref`/include resolution.
- No UTF-16/UTF-32 input; no automatic transcoding.
- No comment or anchor preservation in `Value` (see [[002-format/spec|Format]]).

## 2. User stories

### US-001 Validate a file or stdin (P1)

AS A developer or CI job I WANT `fy parse` to tell me whether input is valid YAML SO THAT I can gate commits and pipelines on its exit code.

```
GIVEN a file ok.yaml containing "name: app\nitems: [1, 2]\n"
WHEN  I run `fy parse ok.yaml`
THEN  stdout is "✓ YAML is valid" and the exit code is 0

GIVEN a file bad.yaml containing "a: [1, 2\nb: 3\n"
WHEN  I run `fy parse bad.yaml`
THEN  stderr is
      error: Failed to parse YAML
        caused by[0] YAML syntax error: illegal placement of ':' indicator at line 2, column 2
      and the exit code is 1

GIVEN `echo 'a: 1' | fy parse`
THEN  stdin is read and the result is "✓ YAML is valid", exit 0

GIVEN an empty file
WHEN  I run `fy parse empty.yaml`
THEN  the result is valid (an empty stream has zero documents), exit 0
```

### US-002 Multi-document streams (P1)

AS A user of Kubernetes-style manifests I WANT every `---` document validated SO THAT an error in document 2 is not hidden.

```
GIVEN "a: 1\n---\nb: [\n"
WHEN  I run `fy parse bad2.yaml`
THEN  exit 1 and the message ends "... at line 4, column 1 (document 2)"
```

A single-document error carries no `(document N)` suffix; the suffix appears for N >= 2.

### US-003 Position-accurate diagnostics (P1)

AS A user I WANT line and column (1-based) in every parse failure SO THAT I can jump to the fault.

```
GIVEN "a: *nope\n"          THEN  "while parsing node, found unknown anchor at line 1, column 4", exit 1
GIVEN "a: \u0000\n" (NUL)   THEN  "NUL (U+0000) is not allowed in YAML at line 1, column 4", exit 1
GIVEN "a: \u0001\n"         THEN  "U+0001 is not allowed in YAML at line 1, column 4", exit 1
```

Positions are measured on the original text; a leading BOM does not shift them.

### US-004 YAML 1.2.2 Core Schema typing (P1)

AS A library user I WANT scalars typed by the Core Schema SO THAT data means the same everywhere. Verified through `fy convert json`:

```
GIVEN  a: yes / b: True / c: tRuE / d: 0o17 / e: -0xFF / f: 017 / h: ~ / i: "7"
       j: 99999999999999999999 / k: !!int "12"
WHEN   I run `fy convert json sc.yaml`
THEN   {"a":"yes","b":true,"c":"tRuE","d":15,"e":-255,"f":17,"h":null,"i":"7",
        "j":99999999999999999999,"k":12}
```

### US-005 Anchors, aliases and merge keys (P2)

AS A config author I WANT `&anchor`, `*alias` and `<<` merge to work as in YAML 1.1 tooling SO THAT existing configs load.

```
GIVEN "base: &b {k: 1}\nd:\n  <<: *b\n  z: 2\n"
WHEN  I run `fy convert json mk.yaml`
THEN  d is {"k":1,"z":2} (no "<<" key remains)

GIVEN "m:\n  <<: 1\n"
WHEN  I run `fy parse mk2.yaml`
THEN  exit 1: "merge key `<<` requires a mapping or a sequence of mappings at line 2, column 3"
```

### US-006 Sets and key domains (P2)

AS A binding or library author I WANT `!!set` loaded as a set and keys mapped into the host language's domain SO THAT YAML 1.2 data fits Python/JS dictionaries without silent collisions.

```
GIVEN "a: !!set {x, y}"             THEN convert json yields {"a":{"x":null,"y":null}}
GIVEN "a: !!set {x, y: 1}"          THEN exit 1: "!!set member has a non-null value, but a set holds members only (write `key:` without a value) at line 1, column 14"
GIVEN keys 1 and "1" with KeyDomain::StringKeys  THEN a Key error names both types
GIVEN keys 1 and "1" with KeyDomain::Yaml        THEN both are kept (distinct keys)
```

### US-007 Stream safety on encodings (P2)

```
GIVEN a UTF-8 file starting with a BOM       THEN valid, exit 0
GIVEN a UTF-16LE file with a BOM             THEN exit 1: "unsupported encoding: input starts with a UTF-16LE byte order mark; only UTF-8 is supported (convert the file to UTF-8, for example with iconv)"
GIVEN CRLF line endings                      THEN valid
```

### US-008 Parse statistics (P3)

AS A user I WANT `fy parse --stats` for a quick shape summary.

```
GIVEN ok.yaml WHEN `fy parse --stats ok.yaml`
THEN  "✓ YAML is valid", blank line, "Statistics:", "  Keys: 2", "  Max depth: 2"
```

`-q` suppresses the success line; `-v` adds `⏱  parse in 0.12ms`.

## 3. Functional requirements

### 3.1 Input handling

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-001 | WHEN input bytes are valid UTF-8 THE SYSTEM SHALL accept them, stripping any leading U+FEFF before parsing. | must |
| FR-002 | WHEN input begins with a UTF-16/UTF-32 byte order mark, or its first bytes match a UTF-16/32 null pattern, THE SYSTEM SHALL return `DecodeError::UnsupportedEncoding` naming the encoding and the evidence. | must |
| FR-003 | WHEN input is not valid UTF-8 THE SYSTEM SHALL return `DecodeError::InvalidUtf8`. | must |
| FR-004 | WHEN the text contains a character outside YAML's c-printable set (including NUL and C0 controls other than tab, LF, CR) THE SYSTEM SHALL reject it with its code point and position before parsing. | must |
| FR-005 | WHEN a file path is a directory or missing THE SYSTEM SHALL fail with a read error and exit 1; `fy parse` of a symlink to a regular file SHALL read the target. | must |

### 3.2 Documents and syntax

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-010 | THE SYSTEM SHALL validate every document of a stream, not only the first. | must |
| FR-011 | WHEN the input is empty THE SYSTEM SHALL return no documents (`parse_all` -> `[]`, `parse_str` -> `None`) and `fy parse` SHALL report valid. | must |
| FR-012 | WHEN a syntax error occurs THE SYSTEM SHALL report `ParseError::Syntax` with 1-based line and column and, for document index >= 2, the index. | must |
| FR-013 | WHEN an alias refers to an undefined anchor THE SYSTEM SHALL fail with a syntax error at the alias. | must |
| FR-014 | THE SYSTEM SHALL support `%TAG` and `%YAML` directives syntactically; `%TAG` handles bound to `tag:yaml.org,2002:` SHALL behave like `!!`. Anchor names SHALL be recovered correctly after a directive whose name is non-ASCII (the position map works by line and column). | must |
| FR-015 | WHEN `fy parse` finishes THE SYSTEM SHALL exit 0 on valid input and 1 on any parse, decode, I/O or limit failure; usage errors (unknown flag, out-of-range limit) SHALL exit 2. | must |

### 3.3 Scalar resolution (single implementation, `resolve_scalar`)

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-020 | WHEN a plain scalar is `""`, `~`, `null`, `Null` or `NULL` THE SYSTEM SHALL resolve it to null. | must |
| FR-021 | WHEN a plain scalar is `true/True/TRUE/false/False/FALSE` THE SYSTEM SHALL resolve it to a boolean; every other spelling SHALL be a string. | must |
| FR-022 | WHEN a plain scalar is a decimal, `0x` hex or `0o` octal integer with optional sign THE SYSTEM SHALL resolve it to `Int` if it fits `i64`, else `BigInt`. Leading-zero decimals (`017`) are decimal. | must |
| FR-023 | WHEN a hex/octal literal exceeds 14 284 significant bits THE SYSTEM SHALL keep it a string; decimal literals SHALL NOT be length-capped. | must |
| FR-024 | WHEN a plain scalar matches the YAML 1.2 float grammar or is `.inf`, `-.inf`, `.nan` (three case variants each) THE SYSTEM SHALL resolve it to `Float`. | must |
| FR-025 | WHEN a scalar is quoted or a block scalar without a core tag THE SYSTEM SHALL resolve it to a string. | must |
| FR-026 | WHEN a scalar carries `!!str/!!int/!!float/!!bool/!!null` (shorthand, `%TAG` handle or verbatim) THE SYSTEM SHALL apply it regardless of style and fall back to a string if the text cannot be coerced; `!!int` on finite float text SHALL truncate toward zero inside the `i64` range. | must |
| FR-027 | WHEN a scalar carries the non-specific tag `!` THE SYSTEM SHALL resolve it to a string. | must |
| FR-028 | WHEN a scalar carries a non-core or unsupported core tag (`!custom`, `!!timestamp`, `!!binary`) THE SYSTEM SHALL ignore the tag and type the scalar by its style (see Open questions). | must |
| FR-029 | THE SYSTEM SHALL implement `Float` equality and hashing on normalized value (all NaN equal, `-0.0 == 0.0`) and `BigInt` equality across spellings of the same number. | must |

### 3.4 Collections, merge keys, sets, keys

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-030 | THE SYSTEM SHALL represent loaded data as `Value::{Null, Bool, Int, BigInt, Float, String, Sequence, Mapping, Set}`; `Mapping` and `Set` keep insertion order, but equality and hashing ignore order. | must |
| FR-031 | WHEN a mapping repeats a key THE SYSTEM SHALL keep the first key position and the last value (see Open questions on strictness). | must |
| FR-032 | THE SYSTEM SHALL treat only a plain untagged `<<` or a `!!merge`-tagged scalar as a merge key; `"<<"` and `!!str <<` are ordinary keys. | must |
| FR-033 | WHEN merging THE SYSTEM SHALL merge shallowly, place merged keys before the mapping's own keys, let explicit keys override merged ones in place, give earlier items of `<<: [*a, *b]` precedence, and remove `<<` from the result. | must |
| FR-034 | WHEN a merge value is not a mapping or sequence of mappings THE SYSTEM SHALL return `ParseError::Merge{NotMapping}` at the `<<` key; a `!!set` source SHALL return `SetSource`; two `<<` in one mapping SHALL return `DuplicateKey` unless `DuplicateMergeKeys::LastWins`. | must |
| FR-035 | WHEN a mapping carries a core `!!set` tag THE SYSTEM SHALL load a `Value::Set`; a member with a non-null value SHALL fail with `ParseError::SetValue` unless `SetValues::Ignore`. | must |
| FR-036 | WHEN `KeyDomain::StringKeys` or `KeyDomain::Python` is selected THE SYSTEM SHALL fail with `ParseError::Key` at the later key when two keys of different types share a canonical key text (or, for `Python`, compare equal in Python). `KeyDomain::Yaml` (default) SHALL never collide. | must |
| FR-037 | THE SYSTEM SHALL validate merge values on the event stream before duplicate keys collapse, so the first invalid merge in document order is reported from every entry point. | must |

### 3.5 API surface

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-040 | THE SYSTEM SHALL provide `Parser::{parse_str, parse_str_with_limits, parse_all, parse_all_with_limits, parse_all_with_budget, parse_all_with_options, parse_normalized, parse_normalized_observed, validate_normalized_observed}` and `decode_input[_owned]`; no entry point takes raw bytes. | must |
| FR-041 | THE SYSTEM SHALL expose a public event stream (`events::EventStream`) so linter and chunked parsing consume the same events as the loader. | must |
| FR-042 | THE SYSTEM SHALL expose `merge_into<T: MergeTarget>` so bindings with their own mapping type get identical merge order. | should |
| FR-043 | `ParseError` SHALL be `#[non_exhaustive]` with variants `Syntax`, `LimitExceeded`, `Merge`, `SetValue`, `Key`, each carrying `at: SourcePosition` (1-based line and column; `Syntax` holds it in `SyntaxError`) and `document: DocumentIndex` (0-based, a type distinct from the 1-based position), and SHALL expose `position()`, `reason()`, `document_index() -> DocumentIndex` (#621). | must |
| FR-044 | `fy parse` SHALL honor `--max-depth`, `--max-alias-bytes`, `--max-documents`, `--max-input-bytes`, `--max-scan-ahead`, `--stats`, `-q`, `-v`, `--no-color` and read stdin when FILE is omitted. | must |
| FR-045 | WHEN a stream holds more documents than `ParseLimits::max_documents` (default 100 000) THE SYSTEM SHALL fail every entry point that loads documents (`parse_all*`, `parse_str*`, events, formatter, parallel chunks sharing a `StreamBudget`) with `ParseError::LimitExceeded{Documents}` at the start of the first rejected document, including empty and comment-only documents ([[009-limits-security/spec]] FR-017). | must |
| FR-046 | WHEN a float is used as a mapping key THE SYSTEM SHALL spell it for key collision and `StringKeys` conversion like ECMAScript `Number` toString (`1e21` is `1e+21`, `100.0` is `100`, `.inf` is `Infinity`, `-.inf` is `-Infinity`, `.nan` is `NaN`, `-0.0` is `0`). | must |
| FR-047 | `Parser::validate_normalized_observed` SHALL report the same error and show `on_event` the same events as `parse_normalized_observed` without building the documents; it skips the tree builder only for `KeyDomain::Yaml`, where the limit guard and merge validator raise every builder error, and loads in full for any other domain. A differential test and the `validate_differential` fuzz target pin the equivalence. | must |

## 4. Key entities

| Entity | Description |
|--------|-------------|
| `Parser` | Namespace of stateless parse functions; every call builds a fresh `StreamBudget` unless one is passed. |
| `Value` | Loaded tree; `Clone + Eq + Hash`; no tag, anchor or style information. |
| `Mapping`, `Set` | Boxed, insertion-ordered, order-insensitive equality; `Mapping::insert` keeps first position. |
| `Float`, `BigInt` | Newtypes over `f64` and canonical decimal text (`IntRadix` remembers the spelling). |
| `ResolvedScalar`, `ScalarStyle` | Borrowed scalar classification; styles `Plain/SingleQuoted/DoubleQuoted/Literal/Folded`. |
| `LoadOptions` | `KeyDomain` x `DuplicateMergeKeys` x `SetValues`; all `#[non_exhaustive]`, defaults `Yaml/Reject/Reject`. |
| `NormalizedInput` | Validated, BOM-stripped text with original-offset mapping. |
| `ParseError`, `SyntaxError`, `SourcePosition`, `DocumentIndex` | Positioned, document-indexed errors; `SourcePosition` counts from 1 and `DocumentIndex` from 0, so the two cannot be swapped. |
| `ParseLimits`, `StreamBudget` | See [[009-limits-security/spec|Limits and security]]. |

## 5. Edge cases

| Scenario | Expected behavior |
|----------|-------------------|
| `a:\t1` (tab after colon) | Rejected: "':' must be followed by a valid YAML whitespace at line 1, column 4" (see Open questions) |
| `%YAML 1.3\n---\na: 1` | Accepted; directive version not interpreted |
| Alias to an anchor defined in an earlier document | Syntax error (anchors are per document) |
| Merge key through alias (`[&k !!merge <<, {*k : 1}]`) | Alias in key position is a merge key; elsewhere it is the plain string |
| `!!set {<<}` | `<<` is an ordinary member |
| `1e400` | `+inf` float; `fy convert json` then fails ("JSON does not support infinity/NaN") |
| `!!int 1e30` | String `"1e30"`, no diagnostic |
| `\r` alone or `\r\n` line breaks | Accepted as line breaks |
| 150 000 empty documents (`---` lines) | Rejected by `fy parse` with `document count exceeds 100000 (document 100001)`; exactly 100 000 pass |

## 6. Success criteria

| ID | Metric | Target |
|----|--------|--------|
| SC-001 | `fy parse` exit code for valid / invalid / usage-error input | 0 / 1 / 2 in 100% of tests |
| SC-002 | Every parse failure carries line and column | 100% (`SourcePosition` is non-optional) |
| SC-003 | Scalar typing identical across `fy`, Rust, Python, Node.js | one shared `resolve_scalar`; cross-binding parity tests pass |
| SC-004 | yaml-test-suite conformance | pass rate does not drop; xfail list only shrinks (Python harness) |
| SC-005 | Malformed-input fuzzing | no panic, no unbounded allocation |

## 7. Agent boundaries

### Always
- Run `cargo nextest run -p fast-yaml-core` and the fixtures under `tests/fixtures/` after parser changes.
- Keep `resolve_scalar` the only place that decides scalar types.
- Verify parser changes against yaml-test-suite xfail list (it must shrink, not grow).

### Ask first
- Changing a default of `LoadOptions` or any scalar rule (breaking for all bindings).
- Upgrading `saphyr-parser` (error texts are passed through and may change).

### Never
- Add unsafe code to the core crate (`#![forbid(unsafe_code)]`).
- Add YAML 1.1 implicit typing.
- Expose saphyr types in the public API.

## 8. Open questions / Known deviations

| # | Topic | Observed | Question |
|---|-------|----------|----------|
| 1 | Duplicate keys | `a: 1\na: 2` loads, `fy parse` says valid; only the linter reports them (GAP-core-parse-004) | [NEEDS CLARIFICATION: should core offer an opt-in strict mode, and should `fy parse` warn?] **Proposed:** opt-in strict loader; keep lint as the default reporter. |
| 2 | `%YAML` versions | `%YAML 2.0` and `%YAML 1.3` accepted silently; `%YAML 1.1` still gets 1.2 typing (-005) | [NEEDS CLARIFICATION: reject major > 1 and warn on minor > 2?] **Proposed:** reject major > 1 and warn on minor > 2 in the same opt-in strict loader. |
| 4 | Tab after colon | `a:\t1` rejected on all surfaces; likely `saphyr-parser` limitation although YAML 1.2.2 allows tab separation | [NEEDS CLARIFICATION: accepted limitation?] **Proposed:** document as a known limitation and track upstream. |
| 5 | Integer grammar | Signed hex/octal and `0O`/`0X` accepted, beyond the 1.2.2 Core Schema; absent from README (-007) | [NEEDS CLARIFICATION: permanent extensions?] |
| 6 | Silent tag loss | Unsupported/custom tags dropped; out-of-range `!!int` becomes a string without diagnostic (-019) | [NEEDS CLARIFICATION: warn, or document as designed?] |
| 7 | `fy parse` ignored flags | Resolved: `-f` is removed and `-o`/`-i` exist only on the subcommands that write, so `fy parse -o out.txt` is a usage error (exit 2) (-010) | closed |
| 8 | `--stats` scope | Counts the first document only (`parse_str_with_limits` returns first doc): `multi.yaml` reports Keys 1, Max depth 1 although document 2 is deeper | Compute over all documents |
| 9 | Error message stability | Syntax reasons are verbatim `saphyr-parser` text; only the flow-nesting text is pinned by a test (-011) | Add golden tests for common messages |
| 10 | File-read error chain | `caused by[0]` and `caused by[1]` repeat the same message (-017) | De-duplicate |
| 11 | `parse_str` cost | Builds every document and returns the first (-018) | Document or add a lazy API |
| 12 | Stale docs | Core README (`0.3`, `parse_all_str`), SKILL.md `fy parse -f json` example (-015, -016) | Fix docs; there is no `fy validate` command |
| 13 | Performance claims | README speed claims have no reproducible benchmark in the repo | [NEEDS CLARIFICATION: which claims are contractual?] **Proposed:** latest minor only; add a reproducible benchmark before keeping speed claims. |

Resolved in this batch and removed: item 3 (`MaxDocuments` is a `ParseLimits` field enforced in core, #574).

## 9. See also

- [[plan]] — technical plan for this feature
- [[009-limits-security/spec|Limits and security]] — depth, alias, scan-ahead, input size
- [[002-format/spec|Format]], [[003-lint/spec|Lint]], [[004-convert/spec|Convert]] — consumers of the parse layer
- [[constitution]] — principles
