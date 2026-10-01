---
aliases:
  - Python API
  - fast_yaml Python bindings
tags:
  - sdd
  - spec
  - python
  - bindings
created: 2026-10-01
status: reverse-specified
related:
  - "[[constitution]]"
  - "[[008-nodejs-api/spec]]"
---

# Feature: Python API (`fast_yaml`)

> [!info] Metadata
> **Package**: PyPI `fastyaml-rs`, import name `fast_yaml` (PyO3, abi3, Python >= 3.10, version 0.6.6)
> **Sources of truth**: `python/src/*.rs`, `python/fast_yaml/*.py`, `python/fast_yaml/_core.pyi`, `python/tests/`
> **Verified**: examples below were run against a wheel built from HEAD (`e5e6cfb`, plus the fixes #574, #557, #568, #531/#532) in a scratch venv. The checked-in `python/fast_yaml/_core*.so` is stale (0.6.5); always rebuild before testing.

## 1. Purpose and value

Give Python users a drop-in replacement for PyYAML's `safe_*` family, plus lint, batch file processing and multi-document parallel parsing, all backed by the same Rust core as the `fy` CLI. Value: PyYAML-shaped call sites keep working, YAML 1.2.2 semantics and resource limits are identical to the CLI, and untrusted input is safe by construction (no arbitrary object construction).

Public surface:

| Module | Contents |
|--------|----------|
| `fast_yaml` | `safe_load`, `safe_load_all`, `safe_dump`, `safe_dump_all`, `safe_dump_to`, `load`, `load_all`, `dump`, `dump_all`, loader/dumper classes, exception classes, `Mark`, `__version__` |
| `fast_yaml.lint` | `lint()`, `format_diagnostics()`, `Linter`, `LintConfig`, `Diagnostic`, `Severity`, `Location`, `Span`, `ContextLine`, `DiagnosticContext`, `Suggestion`, `TextFormatter` |
| `fast_yaml.parallel` | `parse_parallel()`, `ParallelConfig` |
| `fast_yaml._core.batch` | `process_files()`, `format_files()`, `format_files_in_place()`, `BatchConfig`, `BatchResult`, `FileResult`, `FileOutcome` (no public wrapper module; see open questions) |
| `fast_yaml._core.parallel` | `dump_parallel()` (also not re-exported by `fast_yaml.parallel`) |

## 2. User stories

### US-001 (P1): Load YAML safely like `yaml.safe_load`
AS A Python developer I WANT `safe_load(text_or_bytes_or_stream)` SO THAT I can replace PyYAML without rewriting call sites.

```
GIVEN  "name: test\nitems: [1, 2.5, true, null]"
WHEN   fast_yaml.safe_load(...) is called
THEN   it returns {'name': 'test', 'items': [1, 2.5, True, None]}

GIVEN  the same document as bytes, io.StringIO, or io.BytesIO
WHEN   safe_load is called
THEN   the result is identical

GIVEN  "a: 1\nb: 2\na: 3"
WHEN   safe_load is called
THEN   {'a': 3, 'b': 2}  (first key position, last value; same as the CLI)

GIVEN  "[yes, 0o7, 0x1f, 1e3, .inf, ~, 2001-01-01]"
WHEN   safe_load is called
THEN   ['yes', 7, 31, 1000.0, inf, None, '2001-01-01']   (YAML 1.2.2 core schema: yes is a string; timestamps stay strings)

GIVEN  "a: &a {x: 1}\nb:\n  <<: *a\n  y: 2"
WHEN   safe_load is called
THEN   {'a': {'x': 1}, 'b': {'x': 1, 'y': 2}}
```

### US-002 (P1): Dump Python data to YAML
AS A Python developer I WANT `safe_dump` / `safe_dump_all` / `safe_dump_to` SO THAT I can emit configs deterministically.

```
GIVEN  {"b": 1, "a": [1, 2], "c": {"d": None}}
WHEN   safe_dump(data)
THEN   'b: 1\na:\n  - 1\n  - 2\nc:\n  d: ~\n'      (insertion order preserved, null as ~, sequences indented)

GIVEN  {"b": 1, "a": 2}
WHEN   safe_dump(data, sort_keys=True)
THEN   'a: 2\nb: 1\n'

GIVEN  {"a": [1, 2]}
WHEN   safe_dump(data, default_flow_style=True)
THEN   '{a: [1, 2]}\n'

GIVEN  two documents
WHEN   safe_dump_all([{"a": 1}, {"b": 2}])
THEN   'a: 1\n---\nb: 2\n'

GIVEN  a text stream
WHEN   safe_dump(data, stream)
THEN   the YAML is written to the stream and None is returned
```

### US-003 (P1): Predictable errors and limits
AS A service author I WANT hostile input to fail with a catchable error and bounded resources SO THAT a request cannot crash or exhaust the process.

```
GIVEN  "a: [1,2"
WHEN   safe_load
THEN   ValueError: "YAML parse error: YAML syntax error: while parsing a flow sequence, expected ',' or ']' at line 2, column 1"

GIVEN  "[[[[1]]]]"
WHEN   safe_load(..., max_depth=2)
THEN   ValueError "... nesting depth exceeds 2"

GIVEN  max_depth=0, or max_depth=-1
THEN   ValueError "max_depth must be between 1 and 512, got 0"

GIVEN  max_depth=True or "x"
THEN   TypeError "max_depth must be an int, not bool"
```

### US-004 (P2): Lint from Python
AS A tool author I WANT `lint.lint(source, config)` returning typed diagnostics SO THAT I can embed fast-yaml's rules in my own tooling.

```
GIVEN  "key: value\nkey: dup\n"
WHEN   lint.lint(source)
THEN   one Diagnostic: code 'duplicate-key', severity.as_str() == 'error',
       message "duplicate key 'key' (first defined at line 1)", span.start.line == 2

GIVEN  LintConfig(rules={"line-length": {"max": 5}}) and a longer line
THEN   a 'line-length' diagnostic is returned

GIVEN  LintConfig(disabled_rules=["colons"]) and "a:   1\n"
THEN   no 'colons' diagnostic

GIVEN  diagnostics for "a:   1\n"
WHEN   format_diagnostics(diags, src, use_colors=False)
THEN   text beginning 'warning[colons]: too many spaces after colon (expected at most 1, found 3)\n  --> input:1:3 ...' ending '0 errors, 1 warnings'

GIVEN  unparseable YAML
WHEN   lint.lint(source)
THEN   ValueError "Linting failed: YAML syntax error: ..."
```

### US-005 (P2): Parallel multi-document parsing
```
GIVEN  "---\na: 1\n---\nb: 2\n"
WHEN   parallel.parse_parallel(source)
THEN   [{'a': 1}, {'b': 2}]   (document order preserved)

GIVEN  ParallelConfig(max_documents=1) and 2 documents
THEN   ValueError "... document count exceeds 1 ..."

GIVEN  safe_load_all("---\na\n" * 5, max_documents=3)
THEN   ValueError "... document count exceeds 3 (document 4)"   (the default is 100 000 on every loader)
```

### US-006 (P2): Batch file processing
```
GIVEN  an existing valid file and a file containing "a: [1,2"
WHEN   batch.process_files([ok, bad])
THEN   result.total == 2, success == 1, failed == 1; result.errors() lists (path, message) for the bad file
       (the call itself does not raise for per-file errors)

GIVEN  an unreadable/nonexistent path
THEN   it is reported in errors(), not raised

GIVEN  a file "b:   1\na: 2\n"
WHEN   batch.format_files([path])
THEN   [(path, 'b: 1\na: 2\n', None)]   (tuple: path, content, error)
```

### US-007 (P3): Typing support
AS A typed-Python user I WANT the package to ship `py.typed` and stubs SO THAT mypy/pyright validate calls.

## 3. Functional requirements

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-001 | WHEN `safe_load`/`load` receive `str`, `bytes` (UTF-8) or an object with `.read()` returning either, THE SYSTEM SHALL decode and parse it identically. | must |
| FR-002 | WHEN the input holds several documents, THE SYSTEM SHALL return them, in order, as an iterator (currently a `list_iterator`, fully parsed up front) from `safe_load_all`/`load_all` in order. | must |
| FR-003 | THE SYSTEM SHALL map YAML to Python types: map to `dict`, seq to `list`, `!!set` to `set`, str/int/float/bool/None; ints beyond i64 SHALL load as exact Python `int`; `.inf`/`.nan` as floats. | must |
| FR-004 | THE SYSTEM SHALL follow the same YAML 1.2.2 core-schema scalar resolution, merge-key handling, duplicate-key rule and error positions as the CLI/core. | must |
| FR-005 | WHEN two YAML keys are distinct in YAML but equal as Python `dict` keys (e.g. `1` and `1.0`), THE SYSTEM SHALL fail with `ValueError` ("float key 1.0 is distinct in YAML but equal as a Python dict key ...") rather than drop data silently; `1` and `'1'` stay distinct keys. | must |
| FR-006 | WHEN YAML uses complex keys (sequence or mapping as key), THE SYSTEM SHALL raise `ValueError` ("not supported as Python dict keys"). | must |
| FR-007 | WHEN unknown or `!!python/*` tags appear, THE SYSTEM SHALL NOT construct objects or execute code; the node loads as its underlying scalar/collection. | must |
| FR-008 | WHEN a limit keyword (`max_depth` 1..=512 default 256; `max_alias_bytes` 1..=1 GiB default 64 MiB; `max_scan_ahead` 1..=1 GiB default 4 MiB; `max_documents` 1..=10 000 000 default 100 000) is out of range, THE SYSTEM SHALL raise `ValueError`; WHEN it is not an `int` (including `bool`), `TypeError`. `max_documents` is accepted by `safe_load`, `safe_load_all`, `load`, `load_all`, `LintConfig`, `ParallelConfig` and `BatchConfig` (a stream over the limit raises `ValueError` with the core text `document count exceeds N`, plus ` (document K)` for the rejected document); `dump*` functions take none. | must |
| FR-009 | THE SYSTEM SHALL cap single-input size at 100 MiB for loaders. | must |
| FR-010 | `safe_dump`/`safe_dump_all` SHALL accept `stream`, `allow_unicode`, `sort_keys`, `indent` (1..=9, default 2), `width` (20..=1000), `explicit_start`, `default_flow_style`; invalid values raise `ValueError`/`TypeError`. | must |
| FR-011 | WHEN dumping strings that YAML 1.1 or 1.2 parsers could read as non-strings (`yes`, `null`, `1`, `a: b`, leading `- `/`#`), THE SYSTEM SHALL quote them; plain `y`, `n` and non-ASCII text (`é`) stay plain. | must |
| FR-012 | WHEN dumped data nests deeper than 256 or is self-referential, THE SYSTEM SHALL raise an error instead of recursing. | must |
| FR-013 | WHEN dumping an unsupported type (`datetime.date`, arbitrary objects), THE SYSTEM SHALL raise `TypeError`. | must |
| FR-014 | `lint.lint` SHALL return diagnostics sorted by location, using all default rules when `config` is `None`; `LintConfig` SHALL accept `max_line_length`, `indent_size`, `require_document_start/end`, `allow_duplicate_keys`, `disabled_rules`, `rules` (per-rule patch, same schema as the CLI config file; yamllint rule names such as `trailing-spaces` are accepted there and in `disabled_rules`; per-rule `ignore` needs the `path` argument and is relative to the process working directory), and the four limits (`max_depth`, `max_alias_bytes`, `max_input_bytes`, `max_scan_ahead`). `lint.lint(source, config=None, *, path=None)` and `Linter.lint(source, path=None)` take an optional `str` or `os.PathLike` naming the file, which per-rule `ignore` is matched against; a path that does not exist is resolved through its parent and an unresolvable parent raises `ValueError`. `Diagnostic.context` is filled from the BOM-free source. | must |
| FR-015 | `lint.Linter(config).lint(source)` SHALL give the same diagnostics as `lint.lint(source, config)`. | must |
| FR-016 | `format_diagnostics` SHALL support `format="text"` and `"json"`; any other value raises `ValueError`. | must |
| FR-017 | `parse_parallel` SHALL preserve document order, honor `ParallelConfig` (`thread_count` <= 128 on the shared per-process pool, `min_chunk_size`, `max_input_bytes`, `max_documents`, limits) and release the GIL while parsing. | must |
| FR-018 | `process_files`, `format_files` and `format_files_in_place` SHALL release the GIL, never raise for a per-file failure (including `document count exceeds N`), and report it per file; `format_files_in_place` writes atomically. Files are read into memory; there is no `mmap_threshold` option. An explicit `max_scan_ahead` in `BatchConfig` is final (no scaling, no retry). | must |
| FR-019 | `BatchConfig` SHALL validate `indent` (1..=9), `width` (20..=1000) and the limits at construction; its builder methods (`with_workers`, `with_indent`, `with_width`, `with_sort_keys`, `with_max_depth`, `with_max_alias_bytes`, `with_max_scan_ahead`, `with_max_documents`) SHALL return a new/updated config. | should |
| FR-020 | THE SYSTEM SHALL ship `py.typed` and a `_core.pyi` whose signatures match the runtime. | should |
| FR-021 | WHEN the wheel is built for abi3, THE SYSTEM SHALL work on every CPython >= 3.10 without a rebuild. | must |
| FR-022 | WHEN a key collision or merge error occurs in the Nth document (N >= 2) of a stream THE SYSTEM SHALL end the `ValueError` message with ` (document N)`; an error in the first document carries no suffix. | must |
| FR-023 | `safe_dump*` SHALL escape U+0085, U+2028 and U+2029 in double-quoted output (`"a\x85b"`), write a flow-style key longer than 1024 characters as `? key`, quote a scalar containing `?` in flow context (`{a: ["x?y", "?"]}`) and dump `-0.0` as `-0.0`; the output SHALL load back to the same data. | must |

## 4. Key entities

| Entity | Notes |
|--------|-------|
| `SafeLoader`, `FullLoader`, `Loader` | Marker classes accepted by `load`/`load_all`; all behave as `SafeLoader` (no object construction). |
| `SafeDumper`, `Dumper` | Marker classes accepted by `dump`/`dump_all`; both behave as `SafeDumper`. |
| `YAMLError` > `MarkedYAMLError` > `ScannerError`/`ParserError`/`ComposerError`/`ConstructorError`; `EmitterError`; `Mark` | PyYAML-named classes exported for import compatibility; see open questions on whether they are raised. |
| `LintConfig`, `Linter`, `Diagnostic{code, severity, message, span, context, suggestions}`, `Severity` (error/warning/info/hint), `Location{line,column,offset}`, `Span{start,end}` | Lint model, mirrors the CLI JSON output. |
| `ParallelConfig` | Thread count, chunk size, doc/size limits, `auto_tune` (affects `dump_parallel` only). |
| `BatchConfig`, `BatchResult{total, success, changed, failed, duration_ms; is_success(), files_per_second(), errors()}`, `FileResult`, `FileOutcome` | Batch model; `errors()` is a method returning `list[tuple[path, message]]`. |

Python version policy: `requires-python >=3.10`; large-int conversion respects `sys.get_int_max_str_digits()`.

## 5. Edge cases

| Scenario | Expected behavior |
|----------|-------------------|
| Empty string | `safe_load("")` returns `None`; `safe_load_all("")` yields nothing |
| Comment-only source | `safe_load("# c")` returns `None`; `safe_load_all("# c")` yields one `None` |
| UTF-8 BOM in text | stripped before parsing |
| NUL character | `ValueError` (not valid YAML) |
| `bytes` input in UTF-16/32 | `UnicodeDecodeError` (wrapper decodes UTF-8 only) |
| Alias bomb | `ValueError` once `max_alias_bytes` is exceeded |
| Hex/octal integer literal wider than 14284 bits | loads as `str` (documented) |
| Decimal integer with more digits than `sys.get_int_max_str_digits()` | `ValueError` |
| `safe_dump(b"x")` | currently `'- 120\n'` (generic iterable); see open questions |
| `lint.lint` on source over `max_input_bytes` | `ValueError` "input size N bytes exceeds maximum allowed M bytes" |
| `format_files` on a nonexistent path | tuple with `error` set, no raise |
| `Location` objects | compare equal by value; currently unhashable |

## 6. Success criteria

| ID | Metric | Target |
|----|--------|--------|
| SC-001 | `pytest python/tests` on a freshly built wheel | 100% pass; includes yaml-test-suite and YAML 1.2.2 compliance tests |
| SC-002 | Same document parsed by `safe_load` and by `fy parse -f json` | identical data for all cases in the shared fixtures |
| SC-003 | Lint diagnostics (code, line, column, severity) vs `fy lint --format json` | identical on shared fixtures |
| SC-004 | Any input within limits | never aborts the interpreter (errors surface as exceptions) |
| SC-005 | Parse throughput vs PyYAML | measured claim only; no contractual figure until a reproducible benchmark exists (see open questions) |

## 7. Non-goals

- PyYAML extension API: `add_constructor`, `add_representer`, custom `Loader` subclasses, `yaml_tag` classes, `!!python/*` object construction.
- Round-trip editing with comment preservation.
- Streaming/event API (`yaml.parse`, `yaml.scan`, `compose`).
- Windows/musl guarantees beyond what CI wheels cover.

## 8. Agent boundaries

### Always
- Rebuild the extension (`maturin develop` in a scratch venv / `CARGO_TARGET_DIR`) before verifying behavior; never trust the checked-in `.so`.
- Keep `_core.pyi`, `fast_yaml/*.py` wrappers and Rust signatures in sync; run mypy on the package.
- Reuse core limit types; Python-side validation is only type/range translation.

### Ask first
- Changing which exceptions are raised (PyYAML classes vs `ValueError`).
- Exposing batch/`dump_parallel` as public modules.
- Adding runtime dependencies.

### Never
- Construct arbitrary Python objects from YAML tags.
- Weaken a limit default for convenience.
- Add silent no-op parameters.

## 9. Open questions / Known deviations

| Topic | Current behavior (verified) | Question |
|-------|------------------------------|----------|
| Exception types (GAP-PY-002) | Every parse/dump error is a plain `ValueError`; `YAMLError` and subclasses exist but are never raised; `Mark` never attached; `isinstance(ValueError(), YAMLError)` is False; stub documents `problem_mark` etc. | [NEEDS CLARIFICATION] Raise typed PyYAML-compatible errors with marks (and make them `ValueError` subclasses for backward compat), or declare `ValueError` the contract and trim stubs/tests? (OQ-09) **Proposed:** raise typed exceptions with marks; one error-code table for Node. |
| `width`, `allow_unicode` (GAP-PY-006) | Validated but ignored by the emitter (`allow_unicode=False` does not escape) | [NEEDS CLARIFICATION] Implement or remove (OQ-01); cross-surface **Proposed:** remove the option on every surface (pre-1.0 breaking change allowed); wrapping risks round-trip fidelity. |
| Batch `sort_keys` (GAP-PY-015) | Accepted, stored, never applied | [NEEDS CLARIFICATION] Implement or remove (OQ-02) **Proposed:** preserve order by default, sorting opt-in. |
| Comment loss in `format_files*` (GAP-PY-014) | Comments are silently stripped, including in place; CLI refuses without `--strip-comments` | [NEEDS CLARIFICATION] Mirror the CLI policy with a `strip_comments` option, default refuse? (OQ-03) **Proposed:** mirror the CLI policy now (refuse unless explicitly allowed); preserve comments long term. |
| Bytes and datetimes on dump (GAP-PY-003) | `bytes` become an int list, `date`/`datetime` raise `TypeError`; PyYAML emits `!!binary`/timestamps | [NEEDS CLARIFICATION] Policy for `bytes` (reject vs `!!binary`) and timestamps |
| Loader size cap (GAP-PY-021) | `safe_load*` and `load*` have no `max_input_bytes`; fixed 100 MiB | Add keyword? |
| `max_documents` on dump paths | `dump`, `dump_all`, `safe_dump*` take no `max_documents`; `dump_parallel` honors `ParallelConfig.max_documents` ("cannot serialize to YAML: document count exceeds N") | [NEEDS CLARIFICATION: add to the dump functions?] |
| `dump`/`dump_all` wrappers (GAP-PY-005) | Lack `default_flow_style`; stub positional order disagrees with native signature | Fix wrappers and add stubtest |
| Lint formats (GAP-PY-013) | Only `text`/`json`; CLI also has github, sarif, parsable | Expose the CI formats? |
| Public batch module (GAP-PY-008) | Only reachable via `fast_yaml._core.batch`; `dump_parallel` only via `_core.parallel` | Promote to `fast_yaml.batch` / `fast_yaml.parallel.dump_parallel`? |
| Unhashable value types (GAP-PY-010) | `Location`/`Span` define `__eq__` without `__hash__` | Make hashable/frozen |
| Tab after colon (GAP-PY-019) | `safe_load("a:\t1")` raises ValueError (core scanner) | Core decision (OQ-07) **Proposed:** document as a known limitation and track upstream. |
| Duplicate keys and `%YAML` (GAP-core-parse-004/005) | `safe_load` accepts duplicate keys (first position, last value) and `%YAML 2.0` silently | [NEEDS CLARIFICATION: strict option for loaders?] **Proposed:** opt-in strict loader; keep lint as the default reporter. |
| Non-UTF-8 bytes (GAP-PY-012) | UTF-16 bytes raise `UnicodeDecodeError` | Document or sniff BOM |
| Performance claims (GAP-PY-016) | README claims 5-10x (loader) and 3-6x (parallel); no Python benchmark in repo | Which claims are contractual? (OQ-12) **Proposed:** latest minor only; add a reproducible benchmark before keeping speed claims. |
| Docstring drift (GAP-PY-018) | `help(_core.safe_load)` still says repeated `<<` keeps the last value; actual behavior is an error | Fix docstrings |
| Python versions | CI exercises a subset of 3.10-3.14 | Confirm support policy (OQ-12) **Proposed:** latest minor only; add a reproducible benchmark before keeping speed claims. |
| Stale prebuilt artifact | `python/fast_yaml/_core*.so` in the working tree is older than HEAD | Rebuild before any parity check |

## 10. Parity with CLI / core

| Capability | CLI | Core | Python |
|------------|-----|------|--------|
| Parse, YAML 1.2.2, dup keys first-position/last-value | `fy parse` | yes | `safe_load*` yes |
| Parse limits (depth, alias bytes, scan-ahead, documents) | flags, config (`--max-documents` is a flag only) | `ParseLimits` | keywords on loaders, configs; input cap fixed |
| Format | `fy format` (refuses comments by default) | emitter | `safe_dump` formats data (not text); `format_files*` format text, strip comments |
| Lint formats | text, json, github, sarif, parsable | n/a | text, json |
| Lint config | `.fast-yaml.yaml` | `LintConfig` | `LintConfig` object (no file discovery) |
| Parallel/batch | `-j`, globs, discovery | `fast-yaml-parallel` | explicit path lists, no discovery |
| Convert YAML<->JSON | `fy convert` | n/a | use `json` stdlib on loaded data |
| Exit codes | yes | n/a | exceptions |

## 11. See also

- [[constitution]] — principles (type safety, limits, surface parity)
- [[008-nodejs-api/spec]] — sibling binding
- `python/README.md`, `python/tests/` — behavior fixtures
