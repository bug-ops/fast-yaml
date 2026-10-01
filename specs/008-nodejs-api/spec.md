---
aliases:
  - Node.js API
  - fastyaml-rs Node bindings
tags:
  - sdd
  - spec
  - nodejs
  - bindings
created: 2026-10-01
status: reverse-specified
related:
  - "[[constitution]]"
  - "[[007-python-api/spec]]"
---

# Feature: Node.js API (`fastyaml-rs`)

> [!info] Metadata
> **Package**: npm `fastyaml-rs` (napi-rs, Node >= 22, version 0.6.6), typings in `nodejs/index.d.ts` and `nodejs/lint-rules.d.ts`
> **Sources of truth**: `nodejs/src/*.rs`, `nodejs/index.d.ts`, `nodejs/__test__/*.spec.ts`
> **Verified**: examples were run against a binary built from HEAD (`e5e6cfb`, plus the fixes #574, #557, #566, #567, #531/#532) in a scratch target dir. The checked-in `nodejs/*.node` is stale; rebuild before testing. JSDoc examples that import `@fast-yaml/core` are wrong; the package name is `fastyaml-rs`.

## 1. Purpose and value

Give JavaScript/TypeScript users a js-yaml-shaped API (`safeLoad`, `safeDump`, `load`) backed by the Rust core, plus lint, multi-document parallel parsing and batch file formatting. Value: YAML 1.2.2 semantics and resource limits identical to the `fy` CLI, safe by construction (no custom types), typed options that reject out-of-range input instead of silently clamping.

Exports (runtime): `safeLoad`, `safeLoadAll`, `load`, `loadAll`, `safeDump`, `safeDumpAll`, `lint`, `Linter`, `parseParallel`, `parseParallelAsync`, `processFiles`, `formatFiles`, `formatFilesInPlace`, `Mark`, `Schema`, `Severity`, `FileOutcome`, `version`. Interfaces `LoadOptions`, `DumpOptions`, `LintConfig`, `ParallelConfig`, `BatchConfig`, `BatchResult`, `FormatResult`, `Diagnostic`, `Span`, `Location`, `Suggestion`, `ContextLine`, `DiagnosticContext` are type-only. `lint-rules.d.ts` types the per-rule `rules` patch.

## 2. User stories

### US-001 (P1): Load YAML
AS A Node developer I WANT `safeLoad(string)` SO THAT I can replace js-yaml for plain data.

```
GIVEN  "name: test\nitems: [1, 2.5, true, null]"
WHEN   safeLoad(src)
THEN   { name: 'test', items: [1, 2.5, true, null] }

GIVEN  "a: 1\nb: 2\na: 3"
THEN   { a: 3, b: 2 }   (first key position, last value; same as CLI and Python; load/safeLoad/parseParallel agree)

GIVEN  "[yes, 0o7, 0x1f, ~, 2001-01-01]"
THEN   ['yes', 7, 31, null, '2001-01-01']   (YAML 1.2.2 core schema; timestamps stay strings)

GIVEN  "a: &x [1]\nb: *x"                 THEN  { a: [1], b: [1] }
GIVEN  "a: &a {x: 1}\nb:\n  <<: *a\n  y: 2"   THEN  b is { x: 1, y: 2 }
GIVEN  "" THEN null;  safeLoadAll("") THEN []
GIVEN  "---\na: 1\n---\nb: 2\n"; safeLoadAll THEN [{a:1},{b:2}]
```

### US-002 (P1): Dump JavaScript values
```
GIVEN  { b: 1, a: [1, 2], c: { d: null } }
WHEN   safeDump(v)
THEN   'b: 1\na:\n  - 1\n  - 2\nc:\n  d: ~\n'

GIVEN  { b: 1, a: 2 } with { sortKeys: true }       THEN 'a: 2\nb: 1\n'
GIVEN  { a: [1, 2] } with { defaultFlowStyle: true } THEN '{a: [1, 2]}\n'
GIVEN  { a: { b: 1 } } with { indent: 4 }            THEN 'a:\n    b: 1\n'
GIVEN  safeDumpAll([{a:1},{b:2}])                    THEN 'a: 1\n---\nb: 2\n'
GIVEN  ['yes','null','1','a: b','é'] THEN strings that would resolve to other types are double-quoted; 'é' stays plain
```

### US-003 (P1): Errors and limits are catchable
```
GIVEN  "a: [1,2"
WHEN   safeLoad
THEN   throws Error "YAML parse error: YAML syntax error: while parsing a flow sequence, expected ',' or ']' at line 2, column 1"

GIVEN  safeLoad('[[[[1]]]]', { maxDepth: 2 })   THEN throws "... nesting depth exceeds 2"
GIVEN  safeLoad('a: 1', { maxDepth: 0 })        THEN throws "maxDepth must be between 1 and 512, got 0"
GIVEN  safeLoad('a: 1', { maxDepth: 'x' })      THEN throws (NumberExpected, napi type error)
GIVEN  an object that references itself         THEN safeDump throws "nesting depth exceeds 256 (circular reference?)"
GIVEN  "? [a]\n: 1"                             THEN throws "complex keys ... not supported as JavaScript object keys"
GIVEN  a source containing NUL                  THEN throws "NUL (U+0000) is not allowed in YAML"
```

### US-004 (P2): Lint
```
GIVEN  "key: value\nkey: dup\n"
WHEN   lint(src)
THEN   [{ code: 'duplicate-key', severity: 'Error', message: "duplicate key 'key' (first defined at line 1)", span: { start: { line: 2, ... } }, suggestions: [], context }]

GIVEN  new Linter({ disabledRules: ['line-length'] }).lint("a:   1\n")
THEN   diagnostics contain 'colons' (Warning) and none for line-length
GIVEN  lint(src, { rules: { bogus: 'error' } })  THEN throws "unknown rule 'bogus'"
GIVEN  lint('a: 1\n', { maxInputBytes: 2 })      THEN throws "Linting failed: input size 5 bytes exceeds maximum allowed 2 bytes"
GIVEN  unparseable YAML                          THEN throws "Linting failed: YAML syntax error: ..."
```
Severity values are the strings `'Error' | 'Warning' | 'Info' | 'Hint'`.

### US-005 (P2): Parallel parsing
```
GIVEN  "---\na: 1\n---\nb: 2\n"
WHEN   parseParallel(src)  or  await parseParallelAsync(src)
THEN   [{a:1},{b:2}]  (document order preserved; async runs off the event-loop thread)

GIVEN  parseParallel(src, { maxDocuments: 1 }) with 2 documents
THEN   throws (InvalidArg) "... document count exceeds 1 ..."

GIVEN  safeLoadAll("---\n".repeat(100001))
THEN   throws "document count exceeds 100000"      (every loader has a default cap; safeLoadAll(src, { maxDocuments: 3 }) names "(document 4)")
```

### US-006 (P2): Batch files
```
GIVEN  a valid file and a file containing "a: [1,2"
WHEN   processFiles([ok, bad])
THEN   { total: 2, success: 1, changed: 0, failed: 1, durationMs, errors: [{ path: bad, message: "failed to parse YAML: ..." }] }

GIVEN  processFiles([ok], { maxInputBytes: 3 })
THEN   errors[0].message == "input size 16 bytes exceeds maximum allowed 3 bytes"

GIVEN  formatFiles([file with "b:   1\na: 2\n"])
THEN   [{ path, content: 'b: 1\na: 2\n' }]; a failing file yields { path, error }
```

### US-007 (P3): TypeScript users get complete types
AS A TypeScript user I WANT generated typings for every export and option SO THAT misuse is a compile error.

## 3. Functional requirements

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-001 | WHEN `safeLoad`/`load`/`safeLoadAll`/`loadAll` receive a string, THE SYSTEM SHALL parse it under YAML 1.2.2 core-schema rules identical to the CLI/core. | must |
| FR-002 | `load*` SHALL behave as `safeLoad*`; `LoadOptions.schema` SHALL NOT enable unsafe types (all schemas are safe). | must |
| FR-003 | WHEN YAML contains a `!!set`, THE SYSTEM SHALL return an object with `null` values; WHEN it contains a complex key, SHALL throw. | must |
| FR-004 | WHEN YAML contains unknown tags, THE SYSTEM SHALL NOT execute or construct anything; the node loads as its underlying scalar. | must |
| FR-005 | Limit options (`maxDepth` 1..=512 default 256; `maxAliasBytes` 1..=1 GiB default 64 MiB; `maxScanAhead` 1..=1 GiB default 4 MiB; `maxDocuments` 1..=10 000 000 default 100 000; `maxInputBytes` 1..=1 GiB default 100 MiB where offered) SHALL throw on out-of-range or non-integer values. `maxDocuments` is accepted by `LoadOptions` (`safeLoad*`, `load*`), `LintConfig`, `ParallelConfig` and `BatchConfig`; a stream over the limit throws `document count exceeds N` (plus ` (document K)`); `safeDump*` take none. | must |
| FR-006 | `safeDump*` SHALL accept `sortKeys`, `allowUnicode`, `indent` (1..=9, default 2), `width` (20..=1000), `defaultFlowStyle`, `explicitStart`; invalid values throw `InvalidArg`. | must |
| FR-007 | WHEN dumping strings that resolve to non-strings under YAML 1.1/1.2 (`yes`, `null`, `1`, `a: b`), THE SYSTEM SHALL quote them. | must |
| FR-008 | WHEN a value cannot be represented (function, `Buffer`/typed array, circular reference, depth over 256), THE SYSTEM SHALL throw rather than emit lossy YAML. | must |
| FR-009 | `lint`/`Linter.lint` SHALL accept `LintConfig` (`maxLineLength`, `indentSize`, `requireDocumentStart/End`, `allowDuplicateKeys`, `disabledRules`, `rules`, limits) and return `Diagnostic[]` sorted by location; unknown rule names or rule options in `rules` SHALL throw. | must |
| FR-010 | `Linter.withAllRules()` SHALL create a linter with the full default rule set. | should |
| FR-011 | `parseParallel`/`parseParallelAsync` SHALL preserve document order, run on the shared per-process pool, honor `ParallelConfig` (`threadCount`, `minChunkSize`, `maxInputBytes`, `maxDocuments`, limits) and fall back to sequential parsing for single documents. | must |
| FR-012 | `processFiles`/`formatFiles`/`formatFilesInPlace` SHALL never throw for a per-file failure (including `document count exceeds N`); they SHALL report it in `errors` / per-file `error`. `formatFilesInPlace` SHALL write atomically. Files are read into memory; `BatchConfig.mmapThreshold` no longer exists. An explicit `maxScanAhead` is final (no scaling, no retry). | must |
| FR-013 | `BatchConfig`/`ParallelConfig` renamed option `maxInputSize` SHALL throw with a message pointing to `maxInputBytes`. | must |
| FR-014 | `version()` SHALL return the package version string (`"0.6.6"`). | must |
| FR-015 | The shipped `index.d.ts` SHALL describe the runtime: every option name, nullability, and return element type. | should |
| FR-016 | The package SHALL load a prebuilt platform binary and fail with a clear error where none exists. | must |
| FR-017 | `safeDump*` SHALL dump a `BigInt` as a YAML integer (`5n` is `5`, `2n ** 70n` is `1180591620717411303424`), `-0` as `-0.0`, a `Set` as `!!set` with null members and a `Map` as a mapping; a `Set` or `Map` subclass SHALL be recognized by its built-in brand (also across realms), not by an overridden `Symbol.toStringTag`; an object that only claims the tag SHALL fail with `incompatible receiver`. | must |
| FR-018 | `safeLoad*` SHALL name a float mapping key like `String(number)` (`1e21` is `"1e+21"`, `.inf` is `"Infinity"`, `-0.0` is `"0"`), and key-collision errors SHALL spell it the same way. | must |
| FR-019 | `safeDump*` SHALL escape U+0085, U+2028 and U+2029 in double-quoted output, write a flow-style key longer than 1024 characters as `? key` and quote a scalar containing `?` in flow context, and the output SHALL load back to the same data. | must |

## 4. Key entities

| Entity | Notes |
|--------|-------|
| `LoadOptions` | `schema`, `filename`, `allowDuplicateKeys` (accepted for compatibility, no effect), `maxDepth`, `maxAliasBytes`, `maxScanAhead` |
| `DumpOptions` | `sortKeys`, `allowUnicode` (no effect), `indent`, `width` (validated, no effect), `defaultFlowStyle`, `explicitStart` |
| `LintConfig` + `LintRulesConfig` | Rule patch typed by `lint-rules.d.ts` (kebab-case rule names and option keys, same as the CLI config file) |
| `Diagnostic` | `code`, `severity`, `message`, `span{start,end}{line,column,offset}`, `context?`, `suggestions[]` |
| `BatchConfig` / `BatchResult` / `BatchError` | Result shape `{ total, success, changed, failed, durationMs, errors[] }` |
| `FormatResult` | `{ path, content?, error? }` |
| `Mark` | Constructible position holder (`new Mark(name, line, column)`, `toString()` gives `name:line:column`); not attached to any thrown error |
| Number model | JS `number` is an IEEE double: integers above 2^53 lose precision; integers beyond i64 load as strings |

## 5. Edge cases

| Scenario | Expected behavior |
|----------|-------------------|
| `safeLoad('9007199254740993')` | `9007199254740992` (precision loss); see open questions |
| `safeLoad('9223372036854775808')` | string `'9223372036854775808'` |
| `.inf`, `.nan`, `-.inf` | load as `Infinity`, `NaN`, `-Infinity` (JSON.stringify shows `null`) |
| `safeDump([Infinity, NaN, -0, undefined])` | `- .inf\n- .nan\n- -0.0\n- ~\n` |
| `safeDump(5n)`, `safeDump(2n ** 70n)` | `5\n`, `1180591620717411303424\n` |
| `safeDump(new (class extends Set { get [Symbol.toStringTag]() { return 'X' } })(['a']))` | `!!set\na: ~\n` (brand, not tag) |
| `safeLoad('# only a comment')`, `safeLoad('')` | `null`; `safeLoadAll('')` is `[]` |
| `safeDump(new Date(0))` | `{}\n` (no timestamp support) |
| `safeDump(Buffer.from('x'))` | throws "cannot serialize JavaScript value of type Function" |
| UTF-8 BOM prefix | stripped |
| Options object with unknown keys | ignored (see open questions) |
| `null` for numeric options (`workers`, `width`) | throws `NumberExpected`; omit the key instead |
| 100,001 documents via `safeLoadAll` | throws `document count exceeds 100000`; 100,000 load |
| `a:\t1` | rejected by the core scanner |

## 6. Success criteria

| ID | Metric | Target |
|----|--------|--------|
| SC-001 | `vitest` in `nodejs/` on a fresh `build:test` binary | 100% pass |
| SC-002 | Same fixtures through `safeLoad` and `fy parse -f json` | identical data (except documented number model) |
| SC-003 | Lint diagnostics vs `fy lint --format json` | identical code, line, column, severity |
| SC-004 | Inputs within limits | never abort the Node process; failures are thrown errors |
| SC-005 | Performance vs js-yaml | no contractual figure until a reproducible benchmark exists |

## 7. Non-goals

- js-yaml extension API: custom `Type`/`Schema` construction, `yaml.types`, `DEFAULT_SCHEMA`/`SAFE_SCHEMA` constants, `!!js/*` tags.
- Comment-preserving round trip; event/stream API.
- Browser/WASM target.
- Node < 22.

## 8. Agent boundaries

### Always
- Rebuild the `.node` binary (separate `CARGO_TARGET_DIR` when another build is running) before validating behavior.
- Update `index.d.ts` and `lint-rules.d.ts` with every signature or rule change; keep `lint-rules.d.ts` aligned with the linter registry.
- Reuse core limit types; keep binding code limited to translation.

### Ask first
- Changing the integer representation (`bigint`/string) or error `code` contract.
- Adding async variants or new exports.

### Never
- Add a no-op option without documenting it as such.
- Execute or construct anything from YAML tags.

## 9. Open questions / Known deviations

| Topic | Current behavior (verified) | Question |
|-------|------------------------------|----------|
| Large integers (GAP-NODE-001) | Above 2^53 silently rounded to a double; beyond i64 returned as string | [NEEDS CLARIFICATION] `bigint`, string, or keep `number` with documentation? (OQ-10) **Proposed:** `bigint` for integers outside the safe range. |
| Comment loss in `formatFiles*` (GAP-NODE-011) | Comments are silently stripped, also in place; the CLI refuses without `--strip-comments` | [NEEDS CLARIFICATION] Mirror CLI policy by default? (OQ-03) **Proposed:** mirror the CLI policy now (refuse unless explicitly allowed); preserve comments long term. |
| `width`, `allowUnicode`, `sortKeys` in `BatchConfig` (GAP-NODE-002/012) | Accepted, validated, never applied by emitter/formatter | [NEEDS CLARIFICATION] Implement or remove (OQ-01, OQ-02) **Proposed:** remove the option on every surface (pre-1.0 breaking change allowed); wrapping risks round-trip fidelity. For `sortKeys`: preserve order by default, sorting opt-in. |
| `allowDuplicateKeys`, `schema`, `filename` (GAP-NODE-006) | Accepted, ignored; duplicates always collapse (first position, last value) | Throw for `allowDuplicateKeys: false` (js-yaml-like), or remove the options? (OQ-06) **Proposed:** opt-in strict loader; keep lint as the default reporter. |
| Error `code` and marks (GAP-NODE-005/008) | Loader/limit errors have no `code`; dump/lint use `GenericFailure`/`InvalidArg`; `Mark` is never attached to errors | Define an error contract (class, `code`, `line`, `column`) **Proposed:** raise typed exceptions with marks; one error-code table for Node. |
| `safeDumpAll` leading marker (GAP-NODE-004) | No leading `---` (`'a: 1\n---\nb: 2\n'`); JSDoc/README show one | Fix docs or output |
| `load`/`dump` aliases (GAP-NODE-013) | README mentions `dump`/`dumpAll` and `SAFE_SCHEMA`; neither exists | Add aliases or fix README |
| Uneven limits (GAP-NODE-017) | `safeLoad*` has no `maxInputBytes`; `safeDump*` have no `maxDocuments` | Add the missing keywords |
| Unknown option keys (GAP-NODE-007) | Silently ignored on `LintConfig`, `BatchConfig`, `ParallelConfig` | Reject like `rules` entries? |
| Sync batch API (GAP-NODE-021) | `processFiles`/`formatFiles*` block the event loop | Provide `*Async` variants? |
| Typings drift (GAP-NODE-009/019/020) | `lint-rules.d.ts` lacks `lint-directive`; `parseParallelAsync` typed `Promise<unknown>`; `const enum` exports | Generate typings from the registry, fix types |
| Test coverage (GAP-NODE-010/024) | `edge-cases.spec.ts` (93 tests) excluded from vitest; crate Rust unit tests excluded from CI | Re-enable or split the 100 MB cases |
| Docs drift (GAP-NODE-018) | JSDoc package name `@fast-yaml/core`, README Node version and sample versions stale | Fix docs |
| Performance claims (GAP-NODE-025) | README 5-10x vs js-yaml has no reproducible benchmark | Which claims are contractual? (OQ-12) **Proposed:** latest minor only; add a reproducible benchmark before keeping speed claims. |
| Stale prebuilt artifact | `nodejs/*.node` in the working tree is older than HEAD | Rebuild before any parity check |

## 10. Parity with CLI / core

| Capability | CLI | Core | Node |
|------------|-----|------|------|
| Parse, YAML 1.2.2, dup-key rule | `fy parse` | yes | `safeLoad*` yes (integers: double) |
| Limits | flags + config (`--max-documents` is a flag only) | `ParseLimits` | options on every entry except `safeLoad*` input cap |
| Format | `fy format`, refuses comments by default | emitter | `safeDump` (data), `formatFiles*` (strips comments) |
| Lint output | text, json, github, sarif, parsable | n/a | structured `Diagnostic[]` only (no formatter) |
| Lint config | `.fast-yaml.yaml` file | `LintConfig` | object, no discovery |
| Batch | globs, discovery, `-j` | `fast-yaml-parallel` | explicit paths, `workers` |
| Convert | `fy convert` | n/a | `JSON.stringify`/`JSON.parse` around load/dump |
| Python parity | n/a | n/a | same limits, lint model and batch shape as [[007-python-api/spec]]; differs in numbers, errors (no PyYAML classes) |

## 11. See also

- [[constitution]] — principles
- [[007-python-api/spec]] — sibling binding
- `nodejs/README.md`, `nodejs/__test__/` — fixtures
