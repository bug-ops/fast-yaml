# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- yaml-test-suite integration in Python CI: parametrized ~400 test cases against `fast_yaml.safe_load()` / `safe_load_all()`, pinned to `data-2022-01-17` tag ([#228](https://github.com/bug-ops/fast-yaml/issues/228))

### Fixed

- Python binding now converts `!!set` tagged mappings to Python `set` objects instead of `dict` with `None` values, per YAML spec §10.3.3 ([#239](https://github.com/bug-ops/fast-yaml/issues/239))
- `parse_all` / `safe_load_all` now yield one implicit null document for non-empty inputs that contain no explicit documents (whitespace-only, comment-only, bare `---`, bare `...`), matching YAML 1.2 §9.2 and PyYAML parity; empty string `""` continues to return `[]` ([#235](https://github.com/bug-ops/fast-yaml/issues/235))
- Bare `---` with no content now correctly resolves to null instead of `String("")`; empty plain scalar with no tag is treated as implicit null per YAML 1.2 §10.3.2 ([#235](https://github.com/bug-ops/fast-yaml/issues/235))
- Non-specific tag `!` on a scalar (e.g. `x: ! 99`) now forces the failsafe schema and returns a string (`"99"`) instead of applying implicit type resolution; matches YAML 1.2 §6.8.1 / §10.3.2 and PyYAML behaviour ([#238](https://github.com/bug-ops/fast-yaml/issues/238))
- Hex (`0x...`) and octal (`0o...`) integer literals that overflow `i64` are now preserved as strings instead of being silently coerced to float; `is_integer_literal` and the `!!int` tag path now recognise hex/octal prefixes ([#230](https://github.com/bug-ops/fast-yaml/issues/230))
- Large integers exceeding `i64` range are now correctly preserved as Python `int` instead of being coerced to `float` ([#229](https://github.com/bug-ops/fast-yaml/issues/229), closes [#227](https://github.com/bug-ops/fast-yaml/issues/227))

## [0.6.1] - 2026-04-01

### Fixed

- fix(parser): `!!int` tag now coerces float-valued strings to integers via truncation toward zero (e.g. `!!int 3.14` → `3`, `!!int -2.7` → `-2`, `!!int 1.0e2` → `100`); non-finite values (`.nan`, `.inf`) and out-of-range values (e.g. `!!int 1.0e20`) fall through unchanged, consistent with PyYAML convention (#212)
- fix(python): `test_yaml_122_null` now uses `Parser::parse_str()` to test the full fast-yaml pipeline; previously the test used the raw saphyr API and incorrectly asserted that `"Null"` and `"NULL"` were strings rather than null values (#210)
- fix(cli): `fy format -o /dev/stdout`, `-o /dev/stderr`, `-o /dev/fd/1`, `-o /dev/fd/2`, and `-o -` no longer fail with a temp-file error; these paths are now written to directly instead of going through the atomic temp-file-then-rename strategy. (#213)
- fix(parser): explicit YAML tags (`!!int`, `!!float`, `!!bool`, `!!null`, `!!str`) now correctly coerce scalar values, including quoted scalars such as `!!int '42'` (#203)
- fix(parser): YAML merge keys (`<<: *anchor` and `<<: [*a, *b]`) are now resolved during parsing; explicit keys always win over merged keys (#204)
- fix(cli): `fy format` now exits with an error (exit code 1) when the input contains YAML comments, which are silently stripped by the formatter. Pass `--strip-comments` to acknowledge comment loss and proceed. Previously, comments were dropped without any warning or error. (#199)
- fix(nodejs): `safeLoad`, `safeLoadAll`, `load`, `loadAll`, and `parseParallel` now throw a JavaScript exception on error instead of returning an Error object as the resolved value. Root cause was `Unknown<'static>` + `unsafe transmute` pattern bypassing NAPI-RS error propagation; replaced with explicit `env.throw_error()` calls on all error paths (#202)
- fix(linter): `line-length`, `indentation`, `invalid-anchor`, and `trailing-whitespace` rules now respect per-rule severity overrides configured via `LintConfig::with_rule_config`; previously these four rules hardcoded their default severity and ignored any override (#198)
- fix(cli): `fy parse` now accepts empty input, null documents (`~`), and comment-only YAML as valid; previously these returned exit code 1 with "Empty YAML document" — an empty YAML stream is valid per YAML 1.2.2 spec (#200)
- fix(cli): `fy convert json` now coerces null, boolean, and integer YAML map keys to their string representations (e.g. `null` -> `"null"`, `true` -> `"true"`, `42` -> `"42"`) instead of returning an opaque "Map key must be a string" error (#201)

### Added

- **CLI**: `--strip-comments` flag on `fy format` — suppress the new comment-detection error and allow formatting to proceed (comments will still be stripped from the output). (#199)

## [0.6.0] - 2026-03-25

### Added

- **CLI**: `fy lint` now accepts multiple `PATHS...` arguments (files, directories, glob patterns), mirrors the `fy format` batch mode. Supports `--include`/`--exclude` glob filters, `--no-recursive`, and `-j`/`--jobs` for parallel processing. Exit code is non-zero when any file has Error-severity diagnostics. (#165)
- **NodeJS**: `LintConfig.rules` field accepts per-rule severity overrides as a `Record<string, RuleConfig | 'error' | 'warning' | 'info' | 'hint'>`. String shorthand (`'error'`) and object form (`{ severity?, enabled? }`) are both supported. (#171)
- **Python**: `LintConfig(rules=...)` constructor parameter and `LintConfig.with_rule_config(code, severity?, enabled?)` builder method for per-rule severity and enabled overrides. (#171)
- **CLI**: `fy lint` now supports a `--config <path>` flag to load rule configuration from a YAML file. Auto-discovery walks up from the current working directory looking for `.fast-yaml.yaml` or `.fast-yaml.yml` (up to 20 directory levels). Use `--no-config` to disable auto-discovery. (#123)
- **CLI**: `--max-line-length`, `--indent-size`, and `--allow-duplicate-keys` flags on `fy lint` now use `Option<T>` so they only override config file values when explicitly provided; defaults are no longer silently applied over config file settings.
- **Linter**: `ConfigFile` and `ConfigFileError` types in `fast-yaml-linter` for loading and merging `.fast-yaml.yaml` config files into `LintConfig`. Unknown rule names emit a warning to stderr.

### Fixed

- fix(linter): `LintConfig.require_document_start` and `require_document_end` are now wired into `DocumentStartRule` and `DocumentEndRule` respectively; previously these fields were dead and had no effect — setting them to `true` now correctly requires `---`/`...` markers. Added `with_require_document_start` and `with_require_document_end` builder methods to `LintConfig`. (#193)
- fix(python): `ParallelConfig.max_documents` is now enforced — `parse_parallel` and `dump_parallel` return `ValueError` when the parsed document count exceeds the configured limit; previously the limit was validated on construction but silently ignored during parsing (#195)
- `duplicate-key` rule: fix false negative when mapping contains merge-key alias (`<<: *anchor`) — keys after the alias were silently skipped due to `Event::Alias` not advancing the key-tracking state (fixes #188)
- `colons` rule: fix false positive when block mapping key has trailing whitespace but no inline value — spaces after `:` are now only checked when a non-whitespace value follows on the same line (fixes #190)
- fix(linter): `quoted-strings` rule no longer emits "does not need quotes" for double-quoted strings with `\uXXXX`, `\UXXXXXXXX`, or `\xXX` escape sequences; these escapes decode to characters indistinguishable from plain text, requiring raw source inspection (#182)
- fix(linter): `truthy` rule now distinguishes non-standard YAML 1.1-only values (`yes`, `no`, `on`, `off`, `y`, `n`) from non-canonical YAML 1.2.2 booleans (`True`, `TRUE`, `False`, `FALSE`); the latter now emit "non-canonical boolean, use 'true' or 'false'" instead of "non-standard truthy value" (#181)
- fix(cli): `fy lint` now returns an error when `--in-place` / `-i` is passed instead of silently accepting a flag with no effect (#180)
- fix(cli): `fy lint --format json` on multiple files/directories now emits a single valid JSON array where each entry includes a `file` field; previously the output interleaved plain-text path headers between per-file JSON arrays, making it unparseable (#185)
- fix(cli): `fy lint` text formatter no longer prints a `0 errors, 0 warnings` summary when there are no diagnostics; quiet mode (`--quiet`) now produces no output when there are no errors (#186)
- fix(linter): `quoted-strings` rule no longer emits "does not need quotes" for double-quoted strings that contain YAML escape sequences (`\n`, `\t`, `\\`, `\"`, `\uXXXX`, etc.); removing quotes would silently corrupt the value (#175)
- fix(linter): `octal-values` rule no longer fires on octal patterns (`0o\d+`, `0\d+`) found inside YAML comment lines or inline comments (#176)
- fix(linter): `octal-values` diagnostic position now points to the octal value token, not to column 1 of the mapping key (#177)
- fix(linter): `empty-values` rule reported wrong line/column when a key name appeared as a substring of an earlier key (e.g. `a` matched inside `parent`). All three helpers (`find_empty_value_span`, `has_explicit_null_value`, `is_in_flow_mapping`) now use exact boundary matching instead of plain substring search. (#174)
- perf(linter): eliminate O(n²) `LintContext` allocation in multi-document linting by reusing a single pre-built context across all documents instead of calling `LintContext::new(source)` once per document per rule (#169)
- perf(linter): eliminate O(n²) `SourceMapper` allocation in `empty-values` rule by using the shared `SourceContext` from `LintContext` instead of rebuilding it per document; also fixes O(n²) line-offset computation in `find_empty_value_span` to use the pre-built `get_line_offset` index (#169)
- fix(linter): correct `hyphens` rule false positives on list items following non-ASCII (multibyte) characters by using byte-level indexing instead of `chars().nth(offset)` (#161)
- fix(linter): `comments-indentation` rule no longer emits false-positive diagnostics for column-0 comments that follow a nested block; column-0 comments are always valid top-level comments and are skipped unconditionally (#166)
- fix(python): `LintConfig(disabled_rules=...)` now accepts any iterable (list, tuple, set) instead of requiring a set; the argument is converted to a set internally (#168)
- fix(linter): `FlowTokenizer` now uses `char_indices()` instead of `chars().enumerate()` to correctly compute byte offsets for multibyte UTF-8 characters, fixing false positive diagnostics in all token rules (`commas`, `colons`, `braces`, `brackets`, `hyphens`) when YAML contains non-ASCII characters (#167)
- fix(linter): `comments` rule no longer emits false-positive diagnostics for `#` characters inside block scalars (`|` and `>`); block scalar context is now tracked by indentation level (#160)
- fix(linter): `float-values` suggestion for signed leading-dot floats now correctly inserts `0` after the sign character (`-.5` → `-0.5`, `+.5` → `+0.5`) instead of prepending `0` before the sign (#159)
- fix(linter): replace O(n²) `compute_offset` in `quoted-strings` rule with O(1) `SourceContext::get_line_offset` lookup (#147)
- **Python**: `safe_dump_all()` now accepts `indent`, `width`, `explicit_start`, and `default_flow_style` parameters, matching the `safe_dump()` API. (#151)
- fix(linter): implement indentation rule — detect wrong indent size and mixed tabs/spaces (#139)
- **Python**: `safe_load()` now raises `ValueError` with a clear message when YAML contains complex keys (sequences or mappings as mapping keys) instead of a confusing `TypeError` (#144)
- fix(nodejs): `new Linter()` with no args now uses default rules instead of an empty registry (fixes #124)
- fix(python): `Linter()` with no args now uses default rules instead of an empty registry (fixes #135)
- **Python**: `safe_dump()` `indent` and `default_flow_style` parameters now take effect. Previously both were accepted but silently ignored. `indent=N` rescales block-style indentation to N spaces; `default_flow_style=True` renders all mappings and sequences in flow style (`{k: v}` / `[a, b]`). (#127)
- fix(linter): `duplicate-key` rule reported 0-indexed column numbers in JSON output; saphyr `col()` is 0-indexed and now correctly converted to 1-indexed (#131)
- fix(linter): `key-ordering` rule silently skipped nested mapping keys when the parent mapping had more than one top-level key; fixed by interleaving key location with value recursion (#130)
- fix(linter): `enabled: false` in config file did not disable rules; `is_rule_disabled` now checks `rule_configs` in addition to the `disabled_rules` set (#133)
- fix(linter): `float-values` rule now detects signed floats without a leading numeral (`-.5`, `+.5`) in addition to the previously handled bare `.5` case (#138)
- fix(linter): `trailing-whitespace` rule no longer emits false-positive hints on CRLF files; the `\r` from a `\r\n` line ending is now stripped before the whitespace check (#141)
- fix(linter): value-based rules (`key-ordering`, `empty-values`) now check all documents in a multi-document YAML stream; previously only the first document was checked (#142)
- fix(linter): `rules.indentation.indent-size` in config file is now forwarded to `LintConfig::indent_size`; previously the option was stored in `rule_configs` but never applied, so the default 2-space indent was always used (#149)
- fix(linter): `quoted-strings` rule always reported column 1 and wrong byte offset; `make_span` now uses the actual 0-indexed saphyr column converted to 1-indexed, and offset is computed via `SourceContext::get_line_offset` (O(1)) instead of a per-call O(n) scan (#153)
- fix(linter): `key-ordering` rule reported wrong line numbers for documents after the first in multi-document streams; the forward-scan cursor now starts at each document's actual start line instead of always starting at line 1 (#156)
- fix(linter): `DiagnosticBuilder::build` called `SourceContext::new` on every diagnostic, causing O(n²) work when many rules fired; added `build_with_context` method and updated all rule call sites to reuse the pre-built `SourceContext` from `LintContext` (#157)

## [0.5.3] - 2026-03-25

### Added

- **Linter**: `invalid-anchor` rule now detects duplicate anchor definitions (`&name` used more than once in the same document). Reports a `Warning` diagnostic with the location of the duplicate and a reference to the first definition. False positives in comments, quoted strings (including multi-line), and block scalars (`|`/`>`) are suppressed. Document boundaries (`---`) reset the anchor map. (#121)
- Python `safe_dump()` now accepts `explicit_start`, `indent`, `width`, and `default_flow_style` parameters, matching the underlying `_core.safe_dump` and PyYAML API (closes #93)
- NodeJS bindings: `lint()` function, `Linter` class, `LintConfig`, `Diagnostic`, `Severity`, `Span`, `Location`, `ContextLine`, `DiagnosticContext`, `Suggestion` types (closes #61)
- `Linter::with_all_rules_and_config()` method in `fast-yaml-linter` for creating a linter with all default rules and custom configuration

### Fixed

- **Formatter**: `fy format` and `format_streaming` now preserve user-defined anchor names (e.g. `&defaults` stays `&defaults` instead of being renamed to `&anchor1`). The streaming formatter pre-scans the input to extract anchor names before processing events. (#120)
- **CLI**: `fy lint` no longer reports each `duplicate-key` diagnostic twice. `Linter::with_config` already registers all default rules via `with_default_rules()`; the redundant manual `add_rule` calls in `lint.rs` have been removed. (#111)
- **Linter**: `quoted-strings` rule no longer emits false positives when quote characters (`"` or `'`) appear as literal content inside plain (unquoted) YAML scalars. For example, `run: echo "hello"` and `if: ${{ github.event_name == 'push' }}` no longer trigger warnings. The rule was rewritten to use saphyr-parser event-based scalar style detection instead of raw source character scanning. (#113)
- **Linter**: `brackets`, `braces`, and `commas` rules no longer fire false positives on content inside YAML block scalars (`|` literal, `>` folded). Previously, shell scripts and other arbitrary text in `run: |` blocks would trigger spurious diagnostics. The tokenizer now skips all tokens whose byte offset falls inside a block scalar range, detected via saphyr event stream. (#116)
- **Linter**: `hyphens` rule no longer fires a false positive on YAML document separator lines (`---`). Previously the first `-` of `---` was treated as a list-item hyphen, triggering a spurious "missing space after hyphen" warning on every multi-document file. (#114)
- **Linter**: `hyphens`, `colons`, and `commas` rules now report diagnostics at the correct source location instead of always reporting line 1, column 1. The rules now call `source_context.offset_to_location()` to compute the actual line and column from the byte offset. (#114, #115)
- **Linter**: `braces`/`brackets` rules no longer fire on template expressions (Jinja2, GitHub Actions `${{ }}`) inside plain scalar values. Previously the rules scanned raw source text and matched `{`/`[` inside string values, causing false-positive spam on workflow files. The tokenizer now tracks block-context plain scalars and skips flow-syntax characters inside them. (#103)
- **Linter**: `braces`/`brackets` rules now report diagnostics at the correct source location (the `{`/`[` or `}`/`]` token) instead of always reporting line 1, column 1. (#102)
- **Linter**: `duplicate-key` rule now detects duplicate keys at all nesting levels, not only at the top-level mapping. Previously, duplicates inside nested mappings were silently ignored. (#96)
- **Linter**: `duplicate-key` rule no longer emits the same diagnostic twice for a single duplicate key occurrence. The rule was rewritten to use event-based parsing (saphyr-parser) instead of source-text scanning, which also eliminates potential false positives from key names appearing in values or comments. (#97)
- **Linter**: `key-ordering` rule no longer emits N duplicate diagnostics per violation (where N = number of mappings in the document containing the same key name). Each ordering violation now produces exactly one diagnostic, scoped to its own mapping. (#105)
- **Linter**: `quoted-strings` rule no longer flags strings containing glob characters (`*`, `?`, `[`, `]`, `{`, `}`) as unnecessarily quoted. Cron expressions, glob patterns, and template expressions (e.g. `${{ }}`) are now recognized as intentionally quoted. (#107)
- `Emitter::emit_str` and `emit_str_with_config` now always append a trailing newline, consistent with `emit_all_with_config` and POSIX text file convention. Affects `safe_dump`/`safeDump` in Python and NodeJS bindings. (#94)
- `fy format` and `Emitter::format_str` now preserve `%YAML` and `%TAG` directives. Previously they were silently dropped because saphyr does not round-trip directives through its AST. (#95)
- **Linter**: `DuplicateKeysRule` / `SourceMapper` now builds a full inverted key index in a single O(n) pass on first use instead of scanning all source lines for every unique key (O(n²)). `fy lint` performance on large files (Kubernetes manifests, OpenAPI specs) improves from unusable (37s for 10 000 keys) to near-linear. (#100)
- Python/NodeJS bindings now correctly parse `True`/`TRUE`/`False`/`FALSE`/`Null`/`NULL` as bool/null per YAML 1.2.2 Core Schema (fixes #80)
- `batch.format_files` now preserves trailing newline in formatted output (fixes #81)
- `fy convert json` now emits a descriptive error when YAML contains `.inf`, `-.inf`, or `.nan` values that cannot be represented in JSON, instead of the terse `Invalid float value: inf`. (#89)
- `fy convert yaml` now preserves JSON float type for whole-number floats: `1.0` stays `1.0` (not `1`) and `1.23e10` stays `1.23e10` (not `12300000000`). Root cause: `serde_json` in standard mode parses `1.0` as integer-representable, causing it to be stored as `Integer` instead of `FloatingPoint`. Fixed by enabling `arbitrary_precision` feature to obtain the raw JSON token and using `Representation` to pass it through to the YAML emitter verbatim. (#88)
- **Linter**: `Linter::with_config()` now loads all default rules instead of an empty registry. Previously, constructing a `Linter` with a custom config silently disabled all linting rules, causing zero diagnostics regardless of input. Affects Rust, Python, and NodeJS bindings. (#86)
- `fy convert` now correctly handles multi-document YAML streams: all documents are included in a JSON array instead of silently dropping all but the first. Single-document streams continue to produce a plain JSON object. (#87)
- `fy format` no longer adds trailing whitespace to blank lines inside block scalars (`|`, `>`). Previously, blank lines inside a block scalar received the same indentation prefix as non-blank lines, producing `  \n` instead of `\n`. (#85)
- `fy format` no longer converts clip chomp (`|`) to strip chomp (`|-`) on block scalars. The chomp indicator is now derived from the trailing newlines in the scalar value: no trailing newline → strip (`|-`), exactly one → clip (`|`), two or more → keep (`|+`). (#76)
- `fy format` no longer produces extra spaces before inner sequence items in sequence-of-sequences (`-   - item` → `- - item`). (#83)
- `fy format` no longer moves anchors to a separate line ahead of their node. Anchors on mappings and sequences are now emitted inline with their containing prefix. (#84)
- **Core**: Mixed-case YAML 1.2.2 boolean/null variants (`True`, `TRUE`, `False`, `FALSE`, `Null`) are now correctly parsed as `Bool`/`Null` values instead of strings. saphyr only handles lowercase variants natively; the parser now post-processes the value tree to canonicalize the remaining Core Schema variants. (#71)
- **Linter**: `empty-values` rule no longer reports a false positive for values with explicit YAML type tags (`!!null null`, `!!str value`, `!!int 42`, etc.). Any value starting with `!` is now treated as explicitly typed. (#72)
- `fy format` no longer produces trailing spaces on mapping keys whose value is a nested collection
  (`parent: \n` → `parent:\n`). Root cause: the space after `:` was emitted unconditionally; it is
  now deferred and only written when the next event is a scalar value. (#75)
- `fy format` no longer double-indents the first key of a mapping that opens inside a sequence item
  (`-     uses:` → `- uses:`). Root cause: after writing `"- "` for a sequence item, `write_indent`
  was still called for the first mapping key, adding a redundant level of indentation. (#75)
- `fy format` no longer changes float type to integer: `1.0` stays `1.0` (not `1`), `1.23e10` stays `1.23e10` (not `12300000000`). Root cause: streaming formatter now handles all input sizes, preserving the original scalar text representation from the parser. Previously, inputs smaller than 1 KB fell back to DOM-based formatting which lost float precision through Rust's float Display trait.
- `fy format` output now consistently ends with a trailing newline (POSIX convention).
- `fy format` now preserves all documents in multi-document YAML streams (issue #65)
- `DuplicateKeysRule` now fires by default: `LintConfig::default()` sets `allow_duplicate_keys: false`
- Fixed false positives in duplicate key detection — nested keys with same name no longer reported as duplicates
- **Core/CLI**: `fy format` no longer quotes YAML 1.1 boolean-like keys (`on`, `off`, `yes`, `no`).
  In YAML 1.2.2 Core Schema these are plain strings; only `true`, `false`, `null`, and `~` have
  special meaning. The formatter now always uses the streaming path which preserves the original
  scalar style from the parser, instead of the DOM path (saphyr `YamlEmitter`) that incorrectly
  added quotes for YAML 1.1 compatibility. This fixes broken GitHub Actions workflow files after
  `fy format -i`. (#64)
- `fy format <directory>` without `-i` now returns an error instead of silently validating files (#69)
- `fy format --dry-run` now reports "would change: N" instead of "skipped: N" (#69)
- Preserve block scalar styles (literal `|` and folded `>`) in `fy format` (#62)
- **Core**: `fy format` no longer changes the chomp indicator of block scalars. `|` (clip) remains `|` and is not converted to `|-` (strip), preserving the trailing newline in the parsed value. All three chomp variants (`|`, `|-`, `|+`) and their folded equivalents are now round-tripped correctly. (#76)

### Added

- `--allow-duplicate-keys` CLI flag for `fy lint` to opt-in to allowing duplicate keys
- `LintConfig::with_allow_duplicate_keys` builder method

### Changed

- **Core**: `Emitter::format_with_config` now always uses the streaming formatter when the
  `streaming` feature is enabled, regardless of input size. The trailing newline is now always
  emitted for all file sizes (consistent POSIX text-file behavior).

## [0.5.2] - 2026-03-17

### Changed

- Version bump to 0.5.2

## [0.5.1] - 2026-02-20

### Changed

- Updated Rust dependencies (clap minor/patch group)
- Updated PyO3 from 0.27.2 to 0.28.0 with API migration
- Updated Node.js devDependencies and Biome configuration
- Updated Python toolchain dependencies (uv.lock refresh)

### Infrastructure

- Added Dependabot auto-merge workflow for patch and minor updates

## [0.5.0] - 2026-01-19

### Breaking Changes

- **Parallel**: `ParallelConfig` renamed to `Config` with simplified 4-field API
- **Parallel**: Removed `min_chunk_size`, `max_chunk_size`, `max_documents` fields
- **Parallel**: `with_thread_count()` renamed to `with_workers()`
- **CLI**: Batch module removed (functionality preserved, implementation changed)

### Added

- **Parallel**: File-level parallelism with `FileProcessor` struct
  - `parse_files()` for batch validation
  - `format_files()` for dry-run formatting
  - `format_in_place()` for in-place formatting with atomic writes
- **Parallel**: `SmartReader` for automatic mmap/read selection
- **Parallel**: Result types: `BatchResult`, `FileResult`, `FileOutcome`
- **Parallel**: Convenience function `process_files()`
- **Parallel**: New config field `mmap_threshold` for file reading strategy
- **Parallel**: New config field `sequential_threshold` for small input optimization
- **Python**: Batch processing submodule (`fast_yaml._core.batch`)
  - `process_files()` for parallel file validation
  - `format_files()` for dry-run formatting
  - `format_files_in_place()` for in-place formatting
  - `BatchConfig` for configuration
  - `BatchResult` for aggregated results
  - `FileOutcome` enum for per-file outcomes
- **Node.js**: Batch processing functions
  - `processFiles()` for parallel file validation
  - `formatFiles()` for dry-run formatting
  - `formatFilesInPlace()` for in-place formatting
  - `BatchConfig` interface for configuration
  - `BatchResult` interface for results

### Changed

- **CLI**: Batch processing now uses `fast-yaml-parallel` crate directly
- **CLI**: Removed ~2339 lines of duplicate code
- **Parallel**: Unified error type (single `Error` enum for all operations)

### Fixed

- **Security**: Fixed mmap TOCTOU race condition with file locking
- **Security**: Added symlink security checks on Unix platforms
- **Security**: Improved UTF-8 validation for memory-mapped files

### Performance

- **Parallel**: Automatic mmap/read selection reduces syscall overhead
- **Parallel**: Sequential fallback for small files (<4KB) avoids thread overhead
- **Parallel**: Smart file reading with configurable thresholds

### Documentation

- Updated fast-yaml-parallel README with new APIs
- Updated Python and Node.js READMEs with batch processing examples

### Internal

- Workspace tests: 866 passing
- Python tests: 38 batch tests passing
- Node.js tests: 23/25 batch tests passing
- Zero clippy warnings

## [0.4.1] - 2026-01-17

### Added

- **Python**: Parallel dump functionality for multi-document YAML emission
  - `dump_parallel()` function with configurable thread pool
  - Auto-tuning algorithm for optimal thread count based on workload
  - Pre-allocates output buffer to minimize reallocations
- **Python**: Streaming dump API for direct I/O without intermediate string
  - `safe_dump_to()` writes directly to file-like objects
  - Configurable chunk size (default 8KB) for efficient buffer flushing
  - Supports any object with `write()` method (files, StringIO, BytesIO)
- **Python**: Comprehensive type stubs for new parallel and streaming APIs
- **Python**: 34 new tests for streaming functionality (`test_streaming.py`)
- **Core**: Public getter methods for `ParallelConfig` (`thread_count()`, `max_documents()`)
- **Node.js**: Pre-allocation benchmarks to verify linear scaling

### Performance

- **Python**: Parallel dump shows linear scaling with document count
  - Auto-tuning reduces overhead for small workloads (<4 documents)
  - Conservative thread allocation (uses half of CPU cores for small documents)
- **Node.js**: Pre-allocation optimizations maintain linear time complexity
  - Arrays and objects scale linearly with size (no O(n²) growth)

### Fixed

- **Python**: Auto-tune algorithm now handles low CPU count edge cases (macOS CI)
  - Previously panicked with `assertion failed: min <= max` on single-core systems
  - Now ensures `max_threads >= 2` before calling `.clamp()`

### Documentation

- Updated API documentation with new parallel and streaming functions
- Added inline examples for `dump_parallel()` and `safe_dump_to()`
- Documented thread count auto-tuning behavior and thresholds

### Internal

- **Security**: Dual licensing added (MIT OR Apache-2.0)
- **Documentation**: Updated unsafe code usage points in project docs
- All CI checks passing: 912 Rust tests, 344 Python tests, 283 Node.js tests
- Code coverage: 94% maintained

## [0.4.0] - 2026-01-17

### Added

- **CLI**: Unified configuration system for consistent command-line behavior
  - `CommonConfig` aggregates output, formatter, I/O, and parallel configs
  - `OutputConfig` handles verbosity, color detection with NO_COLOR support
  - `ParallelConfig` manages worker threads and mmap thresholds
  - Consistent builder pattern across all configuration types
- **CLI**: Universal `Reporter` for centralized output formatting
  - Zero-copy event design using lifetimes (`ReportEvent`)
  - Proper stdout/stderr stream handling with locking
  - Consistent colored output across all commands
- **Benchmarks**: Comprehensive performance comparison vs google/yamlfmt 0.21.0
  - Single-file benchmarks (small/medium/large files)
  - Batch mode benchmarks (50-1000 files)
  - Reproducible benchmark scripts with hyperfine
  - Results documented in README and benches/comparison/

### Changed

- **CLI**: Refactored all commands to use unified `CommonConfig`
  - `parse`, `format`, `convert`, `lint` commands migrated
  - `format_batch` uses `BatchConfig` composition pattern
- **CLI**: Replaced `BatchFormatConfig` (11 flat fields) with `BatchConfig` composition
  - Composes `CommonConfig`, `DiscoveryConfig`, and batch-specific options
  - Cleaner separation of concerns
- **CLI**: Color detection centralized in `OutputConfig::from_cli()`
  - Automatic detection via `is_terminal` crate
  - Respects `NO_COLOR` environment variable
  - Deleted `should_use_color()` helper (replaced with config method)

### Removed

- **CLI**: Deleted `batch/reporter.rs` (428 lines) — replaced with unified `Reporter`
- **CLI**: Removed ~450 lines of duplicate code through refactoring
  - Eliminated field duplication across config types
  - Removed redundant color handling logic
  - Deleted obsolete constructors

### Performance

- **CLI Batch Mode**: 6-15x faster than yamlfmt on multi-file operations
  - 50 files: **2.40x faster**
  - 200 files: **6.63x faster**
  - 500 files: **15.77x faster** ⚡
  - 1000 files: **13.80x faster** ⚡
- **CLI Single-File**: 1.19-1.80x faster than yamlfmt on small/medium files
  - Small (502 bytes): **1.80x faster**
  - Medium (45 KB): **1.19x faster**
  - Large (460 KB): yamlfmt 2.88x faster (yamlfmt optimized for large files)
- **Streaming**: Phase 2 arena allocator improvements
  - 3-11% performance gains in streaming benchmarks
  - Reduced allocations through bumpalo arena

### Documentation

- **README**: Added comprehensive performance section with benchmark tables
  - CLI single-file vs yamlfmt comparison
  - CLI batch mode performance (key differentiator)
  - Test environment details and reproducibility instructions
- **Benchmarks**: Added `benches/comparison/README.md` with detailed methodology
  - Benchmark configuration and fairness criteria
  - Multi-file corpus descriptions
  - Latest results from Apple M3 Pro (12 cores)
- **Benchmarks**: Added `run_batch_benchmark.sh` for native batch mode testing
  - Compares parallel (-j N) vs sequential (-j 0) processing
  - Demonstrates 6-15x speedup with parallel workers

### Internal

- **CLI**: 100% test coverage on all config modules (common, output, parallel)
- **CLI**: Overall test coverage: 94.38% (exceeds 60% target)
- **CLI**: 912 tests passing, 0 failures
- **CI**: Zero clippy warnings with `-D warnings`
- **Security**: Zero vulnerabilities (cargo audit, cargo deny)
- **Code Quality**: Consistent builder pattern with `#[must_use]` and `const fn`

## [0.3.3] - 2025-01-15

### Breaking Changes

- **Python**: Minimum Python version increased from 3.9 to 3.10

### Added

- **Python**: Added support for Python 3.13 and 3.14

### Changed

- **Dependencies**: Updated all dependencies across ecosystems
  - Python: coverage, maturin, mypy, ruff, pathspec, librt
  - Node.js: Updated devDependencies
- **Documentation**: Refreshed all README files with latest project state
- **CI**: Updated Python test matrix and release builds (3.10-3.14)

## [0.3.2] - 2025-12-30

### Added

- **CLI**: Comprehensive integration test suite (59 tests)
  - Parse, format, convert, lint command tests
  - Global flags and error handling tests
  - Edge cases and special scenarios

### Fixed

- **CLI**: File argument now works after subcommand (intuitive syntax)
  - Before: `fy file.yaml parse` (file before subcommand only)
  - After: `fy parse file.yaml` (both syntaxes work)
- **CLI**: Global flags (`-i`, `-o`, `-q`, `-v`, `--no-color`) now work after subcommands
  - Before: `fy --quiet parse input.yaml` (flags only before subcommand)
  - After: `fy parse --quiet input.yaml` (flags work in either position)
- **CLI**: `--pretty=false` flag now accepts explicit boolean values

### Documentation

- Add crates.io badge for `fast-yaml-cli`
- Add docs.rs badge for `fast-yaml-core`
- Expand CLI section with all commands and examples
- Add `cargo binstall` installation option

## [0.3.1] - 2025-12-29

### Added

- **Node.js**: Comprehensive test suites with 70%+ code coverage (up from 10%)
  - `api-coverage.spec.ts` — 91 tests covering all API functions
  - `edge-cases.spec.ts` — Edge case handling and error conditions
  - `mark.spec.ts` — Mark class for error location tracking
  - `options.spec.ts` — Parser and emitter options
  - `schema.spec.ts` — Schema validation tests
- **Python**: Stream processing tests (`test_streams.py`)
- **CI**: npm audit security check for Node.js dependencies

### Changed

- **Node.js**: Migrated from Prettier to Biome v2.3.10 for formatting and linting
- **Node.js**: Updated devDependencies with Biome replacing Prettier
- **Node.js**: Added biome.json configuration with VCS integration and recommended rules
- **CI**: Updated Node.js versions (20→22 LTS, 22→23 Current)
- **CI**: Fixed codecov flags for proper coverage reporting

### Fixed

- **Node.js**: Test assertions corrected for YAML 1.2.2 compliance
- **Node.js**: Memory-intensive tests optimized to prevent OOM in CI
- **CI**: Python test paths corrected for accurate coverage reporting

### Internal

- Removed unused root pyproject.toml and uv.lock files (Python tooling is in python/ directory)
- CI lint step now enforces quality (removed continue-on-error)
- Vitest configured with sequential execution to prevent memory pressure

## [0.3.0] - 2025-12-29

### Breaking Changes

- **Parser**: Migrated from `yaml-rust2` to `saphyr` as the YAML parser foundation
- **YAML 1.2 Core Schema**: Stricter compliance with YAML 1.2 specification:
  - Only lowercase `true`/`false` are parsed as booleans (not `True`/`False`/`TRUE`/`FALSE`)
  - Only lowercase `null` and `~` are parsed as null (not `Null`/`NULL`)
  - Special float values now emit as `.inf`/`-.inf`/`.nan` (YAML 1.2 compliant)

### Changed

- **Core**: Replaced `yaml-rust2 0.10.x` with `saphyr 0.0.6` for YAML parsing
- **Core**: Updated `Value` type to use `saphyr::YamlOwned` internally
- **Core**: Float values now use `OrderedFloat<f64>` wrapper from saphyr
- **Emitter**: Added `fix_special_floats()` post-processing to ensure YAML 1.2 compliant output
- **Python**: Updated bindings to use saphyr types (`YamlOwned`, `ScalarOwned`, `MappingOwned`)
- **Node.js**: Updated bindings to use saphyr types
- **Docs**: Updated README, CLAUDE.md to reference saphyr instead of yaml-rust2
- **Docs**: Updated Technology Stack section with saphyr 0.0.6

### Fixed

- **Emitter**: Special float values (`inf`, `-inf`, `NaN`) now correctly emit as `.inf`, `-.inf`, `.nan` per YAML 1.2 spec

### Internal

- Updated internal type conversions for saphyr's nested value structure (`YamlOwned::Value(ScalarOwned::*)`)
- Added handling for `YamlOwned::Tagged` and `YamlOwned::Representation` variants
- Updated benchmark code to use saphyr API

## [0.2.0] - 2025-12-27

### Breaking Changes

- **Python**: Minimum Python version increased from 3.8 to 3.9
- **Workspace**: FFI crates (python/nodejs) excluded from default `cargo build`. Use specialized build tools:
  - Python: `uv run maturin develop`
  - Node.js: `npm run build`

### Changed

- **Workspace**: Added `default-members` to exclude FFI crates from default cargo commands
- **Build**: Added `manifest-path` to pyproject.toml for maturin configuration
- **Docs**: Updated documentation with new build commands and `--exclude` flags for workspace operations

### Fixed

- **Build**: `cargo build` no longer fails with Python symbol linking errors

## [0.1.11] - 2025-12-19

### Fixed
- Fixed Python package version in pyproject.toml (was still 0.1.9 in 0.1.10 release)

## [0.1.10] - 2025-12-19

### Added
- **Python**: Full PyYAML-compatible `load()` and `load_all()` functions with optional `Loader` parameter
- **Python**: Full PyYAML-compatible `dump()` and `dump_all()` functions with `Dumper`, `indent`, `width`, `explicit_start` parameters
- **Python**: Loader classes (`SafeLoader`, `FullLoader`, `Loader`) for PyYAML API compatibility
- **Python**: Dumper classes (`SafeDumper`, `Dumper`) for PyYAML API compatibility
- **Python**: Complete type stubs for all new classes and functions in `_core.pyi`
- **Python**: 24 new tests for Dumper classes and dump functions
- **Node.js**: Enhanced `DumpOptions` with `indent`, `width`, `defaultFlowStyle`, `explicitStart` parameters

### Fixed
- **Core**: Multi-document YAML emission now correctly adds trailing newlines between documents
- **Node.js**: Fixed multi-document round-trip parsing that was concatenating values with separators

## [0.1.9] - 2025-12-17

### Fixed
- GitHub Release workflow: fixed checksum generation to work with nested artifact directories

## [0.1.8] - 2025-12-17

### Changed
- Cleaned up release workflow: removed unused artifact organization step

## [0.1.7] - 2025-12-17

### Fixed
- npm publishing: regenerated index.js with correct binary names, removed optionalDependencies
- npm trusted publishing configuration
- Working-directory paths in npm publish job
- Replaced sccache with rust-cache in Python wheel builds

## [0.1.6] - 2025-12-16

### Added
- Copilot code review instructions with path-based rules (`.github/instructions/`)
- Automatic PR and issue labeling via GitHub Actions
- 31 repository labels for categorizing issues and PRs

### Changed
- Configured Trusted Publishing (OIDC) for crates.io, PyPI, and npm
- Updated GitHub Actions to latest versions (checkout@v6, setup-node@v6, setup-python@v6, upload-artifact@v6)
- Updated pytest-cov requirement to >=4.0,<8.0

### Fixed
- Package.json formatting
- Release notes template to use fastyaml-rs package names

## [0.1.5] - 2025-12-14

### Changed
- Release workflow verification with renamed packages

## [0.1.4] - 2025-12-14

### Changed
- Renamed Python package from `fast-yaml` to `fastyaml-rs` (PyPI name conflict)
- Renamed Node.js package from `@fast-yaml/core` to `fastyaml-rs` (npm scope not available)

## [0.1.3] - 2025-12-14

### Fixed
- Fixed Node.js cross-compilation by using zig instead of Docker (avoids Node version mismatch)
- Removed Windows ARM64 Python wheels (cross-compilation not supported by maturin)

## [0.1.2] - 2025-12-14

### Fixed
- Fixed invalid keyword `yaml-1.2` → `yaml12` for crates.io compliance
- Fixed Python sdist build by creating local README.md (maturin doesn't allow `..` paths)
- Fixed Node.js musl/aarch64 Docker builds by using `stable` images with Node 20+

## [0.1.1] - 2025-12-13

### Added
- README.md files for workspace crates (fast-yaml-core, fast-yaml-parallel)
- Workspace-level publishing support for `cargo publish --workspace`

### Changed
- Simplified release CI workflow to use single `cargo publish --workspace` command instead of matrix-based individual crate publishing
- Updated minimum supported Rust version (MSRV) to 1.88.0 (required by napi-rs dependency)

### Fixed
- Resolved clippy `collapsible_if` warnings across 8 files using Rust 2024 let chains syntax:
  - `crates/fast-yaml-core/tests/yaml_spec_fixtures.rs`
  - `crates/fast-yaml-linter/src/context.rs`
  - `crates/fast-yaml-linter/src/formatter/text.rs`
  - `crates/fast-yaml-linter/src/rules/duplicate_keys.rs`
  - `crates/fast-yaml-parallel/src/processor.rs`
  - `python/src/lib.rs`
  - `python/src/lint.rs`
  - `python/src/parallel.rs`

## [0.1.0] - 2025-12-10

### Added
- Initial release of fast-yaml workspace with modular architecture
- **fast-yaml-core**: YAML 1.2.2 compliant parser and emitter
  - Zero-copy parsing where possible
  - Support for multi-document YAML streams
  - Core Schema compliance
  - Comprehensive error reporting with source location tracking
- **fast-yaml-linter**: YAML validation and linting engine
  - Rich diagnostic system with line/column tracking
  - Pluggable linting rules architecture
  - Duplicate key detection
  - Invalid anchor/alias detection
  - Human-readable and JSON diagnostic formatters
- **fast-yaml-parallel**: Multi-threaded YAML processing
  - Intelligent document boundary detection
  - Rayon-based parallel processing
  - Order-preserving result aggregation
  - Optimized for large multi-document YAML files
- **fast-yaml-ffi**: Shared FFI utilities (removed in v0.5.0 - not used by bindings)
- **Python bindings** (fast-yaml-python):
  - PyO3-based native extension
  - `safe_load()` and `safe_dump()` functions
  - Linter integration with detailed diagnostics
  - Parallel processing support
  - Type stubs for IDE integration
- **Node.js bindings** (fast-yaml-nodejs):
  - NAPI-RS based native module
  - TypeScript type definitions
  - Full parser, linter, and parallel processing APIs
  - CommonJS and ESM module support

### Infrastructure
- Comprehensive CI/CD pipeline with GitHub Actions
  - Cross-platform testing (Linux, macOS, Windows)
  - Code coverage reporting via codecov
  - Security scanning with cargo-deny
  - Automated dependency updates via Dependabot
- Workspace-based dependency management
- Rust Edition 2024 with MSRV 1.88.0
- Quality control tooling:
  - cargo-nextest for fast test execution
  - cargo-llvm-cov for code coverage
  - cargo-semver-checks for API compatibility
  - cargo-deny for security auditing

### Documentation
- Project architecture documentation (CLAUDE.md)
- Architecture Decision Records (ADRs) in `.local/adr/`
- Comprehensive README with usage examples
- API documentation for all crates
- Python package documentation
- Node.js package documentation

[Unreleased]: https://github.com/bug-ops/fast-yaml/compare/v0.6.1...HEAD
[0.6.1]: https://github.com/bug-ops/fast-yaml/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/bug-ops/fast-yaml/compare/v0.5.3...v0.6.0
[0.5.3]: https://github.com/bug-ops/fast-yaml/compare/v0.5.2...v0.5.3
[0.5.2]: https://github.com/bug-ops/fast-yaml/compare/v0.5.1...v0.5.2
[0.5.1]: https://github.com/bug-ops/fast-yaml/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/bug-ops/fast-yaml/compare/v0.4.1...v0.5.0
[0.4.1]: https://github.com/bug-ops/fast-yaml/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/bug-ops/fast-yaml/compare/v0.3.3...v0.4.0
[0.3.3]: https://github.com/bug-ops/fast-yaml/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/bug-ops/fast-yaml/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/bug-ops/fast-yaml/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/bug-ops/fast-yaml/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/bug-ops/fast-yaml/compare/v0.1.11...v0.2.0
[0.1.11]: https://github.com/bug-ops/fast-yaml/compare/v0.1.10...v0.1.11
[0.1.10]: https://github.com/bug-ops/fast-yaml/compare/v0.1.9...v0.1.10
[0.1.9]: https://github.com/bug-ops/fast-yaml/compare/v0.1.8...v0.1.9
[0.1.8]: https://github.com/bug-ops/fast-yaml/compare/v0.1.7...v0.1.8
[0.1.7]: https://github.com/bug-ops/fast-yaml/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/bug-ops/fast-yaml/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/bug-ops/fast-yaml/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/bug-ops/fast-yaml/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/bug-ops/fast-yaml/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/bug-ops/fast-yaml/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/bug-ops/fast-yaml/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/bug-ops/fast-yaml/releases/tag/v0.1.0
