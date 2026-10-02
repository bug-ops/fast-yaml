# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.7.0] - 2026-10-02

### Breaking Changes

#### Core

- `Value` is a resolved enum with a `Set` variant; saphyr leaves the public API (`ScalarOwned`, `Map`, `Array`, `canonicalize`, `scalar_key_text`, `LimitGuard`, `MergeKeyValidator`, `ParseError::scanner` removed), the emitter is iterative and event-driven, and `saphyr` is dropped as a dependency (#412) (#400) (#542) (#546) (#561) (#562) (#582)
- `Mapping` and `Set` equality and hashing ignore entry order; `Value` hashes are keyed per process and must not be persisted (#551) (#582)
- `ParseError` reshaped: `Scanner` becomes `Syntax(SyntaxError)`, `Merge`/`LimitExceeded`/`SetValue`/`Key` carry `at: SourcePosition` and a 0-based `DocumentIndex`, `From<MergeError>` and `From<ScanError>` removed, `relocated` takes a document count (#412) (#504) (#517) (#530) (#548) (#554) (#555) (#556) (#561) (#564) (#620) (#637)
- `ParseLimits` gains `max_scan_ahead` and `max_documents`, `LimitKind` gains `ScanAhead`, `Documents` and `FlowNesting`, `MaxDepth::new`/`MaxAliasBytes::new` return `Result<_, LimitRangeError>`, `has_comments*`/`find_comments` take `MaxScanAhead` (#433) (#563) (#574) (#582) (#595)
- `EmitterConfig`: `indent`/`width` are `Indent` (1..=9) and `Width` (20..=1000) newtypes, `compact` is removed, `max_depth` becomes emit-only `max_emit_depth` (default 512), `streaming::MAX_DEPTH` is removed (#382) (#427) (#546) (#561) (#563) (#582)
- `EmitError` changes: `Emit(String)` replaced by `Format`/`Parse`, `UnsupportedType` becomes `ComplexFlowKey`, `SetAsKey`, `DepthLimitExceeded` and `AnchorLimitExceeded` added; `ParseError`, `EmitError`, `Severity` and other error enums are `#[non_exhaustive]` (#371) (#383) (#411) (#412) (#561)
- Merge-key handling: only a plain untagged `<<` (or a `!!merge` scalar) is a merge key, an invalid `<<` value, repeated `<<` or `!!set` member with a value is an error with its position, and `MergeTarget` requires `reject` (#478) (#481) (#491) (#492) (#494) (#504) (#511) (#554) (#555) (#561) (#564)
- Big integers: `DecimalBigInt` replaced by radix-aware `BigInt`/`IntRadix` (`BigIntRef` borrowed), hex and octal integers beyond `i64` load as exact big integers (#412) (#495) (#521) (#561)
- Input handling: `reject_nul` becomes `NormalizedInput`, `parse_chunk_with_budget` becomes `parse_normalized`, a prefix BOM is no longer content, `format` keeps a leading BOM, `DecodeError::UnsupportedEncoding` is a struct variant, and `EventItem` gains `end` (#333) (#451) (#457) (#525) (#526) (#561) (#576)
- `events` module (`EventStream`, `Event`, `ScalarStyle`, `Tag`) replaces the saphyr-based API; `resolve_scalar`/`core_tag_suffix` take them (#542) (#562)
- The `streaming` feature is removed; the streaming formatter is the only `Emitter::format*` path (#408)

#### Linter

- `Location` fields are private (`line()`, `column()`, `offset()`), line and column use the new `OneBased` type, and `Location::try_new` rejects 0 (#621) (#637)
- `SourceRule::check`/`DocumentRule::check` return `Vec<Finding>`, with `diagnose` for tests; the linter adds code and severity, and `flow_common` helpers no longer take them (#625) (#637)
- `LintRule` is metadata-only with `id() -> RuleId`; rules implement `SourceRule` or `DocumentRule`, `add_rule` rejects duplicate ids, `Linter::lint_value` and the `LintContext` doc-start API are removed, and `syntax`/`diagnostic-limit` are reserved codes (#603) (#609) (#623) (#576)
- `Diagnostic.context` becomes `excerpt: Excerpt`, `Formatter::format` becomes `write(&mut dyn io::Write, Findings)`, `KeyIndex` and `DiagnosticBuilder::build_with_context` are removed, `comment_parser` replaced by `Comment`/`CommentKind`, the `tokenizer` module is private (#563) (#576) (#605)
- Rule options are typed per-rule structs validated at config load; unknown rules, option keys, wrong types and unknown top-level keys are errors, replacing `RuleConfig`/`RuleOption`/`RuleOptions` with `RulesConfig`; `Severity` is `FromStr`/`Deserialize` and `ConfigFileSeverity` is removed (#324) (#327) (#426)
- Config changes: `extends`, `ignore`, `yaml-files`, `locale`, per-rule `ignore`/`ignore-from-file` and `max-scan-ahead` are accepted, `into_lint_config` becomes `into_parts`, `warn_unknown_rules` becomes `unknown_rules`, `RuleSettings` gains `ignore`/`origin`, `LintConfig` gains `parse_limits`/`max_input_bytes`, and `ConfigFileError` gains `Extended`, `ExtendsCycle`, `ExtendsTooDeep`, `NotRegularFile`, `TooLarge`, `UnsupportedLocale`, `Decode` and `LimitNotPositive`/`LimitOutOfRange` (#411) (#436) (#510) (#524) (#538) (#563) (#571) (#582) (#595) (#605)
- Config rule entries start from yamllint defaults (severity `error`), so `line-length: {max: 60}` now exits 2; `rules: {x: enable}` resets options; `undefined-alias` is an unknown rule; `level:` aliases `severity` (#426) (#576) (#605)
- `IndentationOptions` is a port of yamllint's `indentation` (`spaces` replaces `indent_size`, `indent-sequences`, `check-multi-line-strings`; default width `consistent`) (#605) (#626) (#637)
- `quoted-strings` follows yamllint: regex `extra-required`/`extra-allowed` (`PatternList`), skips keys and `!!`-tagged scalars, `only-when-needed` uses YAML 1.1 resolvers (#426) (#538) (#576) (#605)
- Rule behavior: `truthy` drops `y`/`n`, `line-length` lets a one-word line through, `document-start`/`document-end` check every document with `MarkerPresence` (`present: false` forbids `...`), `key-ordering` and `empty-values` follow yamllint (#538) (#576) (#605)
- `duplicate-key` compares resolved values (`99`/`+99` collide) and gains `forbid-duplicated-merge-keys` (on by default) (#576)
- `SarifFormatter` is replaced by typed `ReportFormat::Sarif` (SARIF 2.1.0, absolute `file:` URIs) (#314) (#570)
- `ContextLine` gains `column_offset`/`truncated_end`; context lines are windowed to 120 characters (#454) (#468)
- Every `Span` refers to BOM-free text; map an offset back with `NormalizedInput::original_offset` (#576)
- A `!!set` member with a value is a `set-values` diagnostic (exit 2) instead of a parse error; `LoadOptions` gains `set_values` (#565) (#570)
- Lint parse errors no longer start with "failed to parse YAML:" (#519) (#537)

#### Parallel

- `Config::with_workers` takes `Workers` (`Auto`, `Sequential`, `Fixed(WorkerCount)`), `shared_pool` and `ScanAheadLane::for_policy` take `WorkerCount` (1..=128) (#610) (#622)
- `Error` reshaped: `Utf8` becomes `Decode { path, source }`, `InputTooLarge` is a tuple variant, `DocumentLimitExceeded`/`TooManyDocuments` are replaced by core `LimitKind::Documents`, `Chunking`/`Config` removed, `ThreadPool` wraps the Rayon error, `Format` carries `path` and `EmitError`, `EmptyDocument` added (#334) (#411) (#508) (#509) (#524) (#530) (#537) (#574) (#595)
- Files are read into memory instead of memory-mapped; `mmap_threshold`, `SmartReader`, `FileContent` and `Chunk.index` are removed and the crate forbids `unsafe` (#531) (#552) (#595)
- `format_files`/`format_in_place` take a `CommentPolicy`, `format_files` returns `FormatOutput { formatted, changed }`, `Error::CommentsWouldBeStripped` added (#397)
- `Config` gains `with_key_domain` and `with_max_input_bytes` (replacing `with_max_input_size`) (#508) (#537) (#548) (#564)
- An unindented root block scalar keeps a column-0 `---` as content, changing the document count (#552) (#562)

#### CLI

- `ExitCode::ParseError` is renamed `Failure`; unused `IoError` (3) and `InvalidArgs` (4) are removed; exit codes 0, 1, 2 and 5 are unchanged (#628) (#637)
- `-f/--format` is removed, `-o`/`-i` exist only on subcommands that write, `-j` above 128 is rejected, `--quiet` with `--verbose` exits 2 anywhere, `--max-line-length` accepts `1..=4294967295`, `--indent` accepts 1..=9 and out-of-range values fail instead of being clamped (#330) (#382) (#534) (#610) (#611) (#622) (#634) (#637)
- `using config file: <path>` is printed only with `-v` (#614) (#622)
- Inputs above 100 MiB fail by default (`--max-input-bytes`, alias `--max-input-size`), `fy lint` rejects input above 1 GiB (#342) (#436) (#508) (#510) (#534) (#537)
- `fy format --dry-run` exits 5 when any file would change; `fy format <dir|glob>` without `-i` or `--dry-run` and `fy -i` without a subcommand exit 1 (#397) (#401)
- Missing paths, unmatched or malformed globs, explicit non-YAML files, skipped stdin lines and empty filter results exit 1; bracket-only globs like `f[12].yaml` are literal; `--include`/`--exclude` match case-insensitively (#401) (#402) (#496) (#497) (#498) (#512) (#513) (#514) (#523)
- `fy parse`, `lint` and `convert` exit 1 on an invalid `<<` value (#481) (#494)
- Distinct YAML keys that map to the same JSON key or JavaScript property are an error (#479) (#561)
- `DiscoveryConfig.include_patterns` becomes `include: IncludePatterns`; `fy lint` exits 0 when config `ignore` drops every input (#538)
- `fy lint --format json` prints a one-element `syntax` array for a missing, unreadable or invalid input (#605)
- Core-schema tag and scalar resolution changes: unappliable core tags yield strings, doubled-sign integers are strings, `.5`, `-.5` and `+.inf` load as floats, NUL input is a parse error, `inf`/`NaN` stay strings in `format`, and floats are written as YAML 1.1 readers expect (`1.0e+300`, `-0.5`) (#362) (#390) (#393) (#417) (#450) (#458) (#549) (#564)
- Float mapping keys follow ECMAScript `Number` toString (#567) (#595)
- `fy lint` on a `!!set` member with a value exits 2 with a `set-values` diagnostic (#565) (#570)
- `fy lint` visits `.yamllint` by default and `fy format` does not (#571) (#595)

#### Python

- `Location(line, column, offset)` raises `ValueError` for line or column 0 (#621) (#637)
- `LintConfig()` leaves indentation `consistent` and `LintConfig.indent_size` is `None` while consistent (#626) (#637)
- `safe_load`/`parse_parallel` raise `ValueError` for keys YAML keeps distinct but a dict would merge (`1`, `true`, `1.0`), raise on a `!!set` member with a value, and count canonical decimal digits against the int digit limit (#489) (#511) (#521) (#541) (#555) (#564)
- `parse_parallel` loads `!!set` as `set` and hex and octal integers like `safe_load`; dumping a `set` writes `!!set` (#490) (#521) (#537)
- `max_input_size` becomes `max_input_bytes` (`TypeError` on the old name); `max_line_length` has no 1000 cap; `lint` takes an optional `path` for per-rule `ignore` (#426) (#508) (#537) (#605)
- Native `_core.dump`/`dump_all` are removed; `saphyr-parser` is no longer a dependency (#542) (#562) (#612) (#622)
- Wheels are abi3 (`cp310-abi3`) (#472)

#### Node.js

- `maxChunkSize` is removed, `maxDocuments` is enforced (default 100000), `maxInputSize` becomes `maxInputBytes` (throws on the old name) (#435) (#508) (#530) (#537)
- Numeric options (including `maxLineLength` and `indentSize`) reject negative, fractional, `NaN` and out-of-range values with `InvalidArg` (#434) (#628) (#637)
- `processFiles`, `formatFiles`, `formatFilesInPlace` and `safeDump`/`safeDumpAll` throw instead of returning errors (#395) (#433) (#434)
- `LintConfig.rules` is read under depth and node limits and rejects `BigInt` (#423) (#434)
- Minimum Node.js is 22 (#472)

#### Cross-surface

- Dedicated options win over the `rules` patch on every surface; `rules` accepts every rule option; out-of-range sizes and indents are errors (#327) (#426) (#601) (#622)
- `fy format`, `format_files` and the batch formatters share one formatter: explicit tags preserved, `null` instead of `~`, `--- ` and duplicate keys kept (#408)
- Dumps reject data whose emitted size exceeds 100 MiB or that expands beyond 16 Mi nodes (`MaxOutputBytes`, `MaxDumpNodes`, `DumpBudget`) (#395)
- Emitted text changes: flow null and collection keys, root literal blocks, extra quoting of YAML 1.1 lookalikes, `multiline_strings` literal blocks at any indent, U+0085/U+2028/U+2029 double-quoted (#546) (#560) (#582)
- Flow collections, scalars and comment runs over 4 Mi characters are rejected; raise `max_scan_ahead` (#563) (#582)
- Merged keys come first and a repeated `<<` is an error (#391) (#465) (#475) (#554) (#564)
- Key collision errors carry the later key's line and column (#548) (#564)
- Tags on big integers are dropped, big-integer keys compare by value, a recursive alias is a syntax error (#412) (#561)
- Parse errors end with ` (document N)` from the second document on (#517) (#530)
- Python `safe_load`/`load`/`parse_parallel`/Node.js loaders reject streams over 100000 documents (`--max-documents`) (#529) (#537) (#574) (#595)
- Python/Node.js `format_files_in_place` report untouched files as `Unchanged` (#397)
- MSRV is Rust 1.91 and the workspace resolver is 3 (#399)

### Added

- **Core**: `DocumentIndex`, `SourcePosition::new`, `ParseError::reason` and `ParseError::position` (#314) (#412) (#570) (#621) (#637)
- **Core**: `events` module, `LoadOptions` key domains (`Yaml`, `StringKeys`, `Python`) and `SetValues` policy, `NodeRole`, `MaxDocuments`, public `merge::is_merge_key_scalar`/`is_set_tag` (#508) (#518) (#537) (#542) (#548) (#554) (#562) (#564) (#565) (#570)
- **Core**: `NormalizedInput`, `Emitter::format_normalized`, `CommentScanner`, `Parser::parse_normalized_observed`/`validate_normalized_observed`, `fs::read_regular_file`, `decode_input` with UTF-16/UTF-32 BOM detection (#334) (#412) (#524) (#576) (#605) (#609) (#623)
- **Core**: `StreamBudget`, `MaxOutputBytes`, `MaxDumpNodes`, `MaxTagBytes` and `find_comments`; `Float` keeps JSON number spelling (`1.0e+5`) (#369) (#389) (#395) (#405) (#412) (#437) (#561)
- **Core/CLI/Python/Node.js**: `max_depth`/`max_alias_bytes` options and `--max-depth`/`--max-alias-bytes` on `fy parse`/`convert`/`lint`, `fy format --max-depth` (#427) (#433) (#561)
- **Linter**: `set-values` rule, `quoted-strings` `quote-type: consistent`, `allow-quoted-quotes`, `check-keys`, `extra-required`/`extra-allowed` regexes, `line-length` `allow-non-breakable-words`/`-inline-mappings`, `document-end: {present: false}`, `key-ordering` `ignored-keys`, `invalid-anchor` options (#538) (#570) (#572) (#576) (#602) (#637)
- **Linter**: `extends: default|relaxed` and `ignore-from-file`, per-rule `ignore`, `locale`, yamllint rule-name aliases, `indentation` `spaces`/`indent-sequences`/`check-multi-line-strings` (#538) (#571) (#595) (#605)
- **Linter**: inline suppression directives `# fy: disable|enable|disable-line|disable-file` (and `# yamllint ...`) with a `lint-directive` diagnostic (#437)
- **Linter**: `Finding`, `OneBased`, `Location::try_new`, `Findings`, `LintSource`, `Linter::lint_file`/`lint_source_file`, `RulesConfig::apply_at`/`apply_rule_at` and `LintConfig::matching_path` (#605) (#619) (#621) (#622) (#625) (#637)
- **Linter**: `max_input_bytes`/`maxInputBytes` lint option (1 B to 1 GiB, default 100 MiB) with `LintError::InputTooLarge` (#436) (#510)
- **CLI/Linter**: `fy lint --format github|sarif|parsable`, `--stdin-files`, `--max-diagnostics N` and `max-diagnostics`/`max-input-bytes` config keys (#314) (#508) (#537) (#570) (#576) (#603) (#623)
- **CLI**: `--max-input-bytes` (1 B to 1 GiB, `KiB`/`MiB`/`GiB` suffixes) and `-o /dev/null` to discard output (#342) (#534) (#634) (#637)
- **CLI/Parallel**: `RUST_LOG` enables `tracing` debug events; `fast-yaml-parallel` gains an optional `tracing` feature (#614) (#622)
- **Parallel**: `shared_pool`, `read_file`, `AtomicFile`, `ScanAheadPolicy` (#366) (#531) (#532) (#577) (#595)
- **Node.js**: `safeDump` writes `Set` as `!!set` and `Map` as a mapping, including cross-realm and subclasses (#541) (#547) (#566) (#564)
- **Python/Node.js**: `BatchConfig(max_depth=)`, `formatFiles({ maxDepth })`, `parse_limits` options (#427) (#433)
- **Testing/CI**: `cargo-fuzz` targets, format round-trip proptests, `test-core-no-arena` job, actionlint/zizmor and a fuzz workflow on push to `main`, `FY_BIN` parity jobs, nextest hang timeout (#418) (#422) (#432) (#524) (#561) (#570) (#597) (#635)

### Changed

- **Linter**: rules share one loader pass and `FlowIndex`, cutting `fy lint` memory on long flow lines from 3.5x to about 1.1-1.25x of `fy parse` and roughly 2x on diagnostic-heavy input via lazy excerpts and streamed reports (#573) (#576) (#578) (#579) (#595) (#605)
- **Linter**: the text format prints a one-column caret under zero-width spans; `Linter::lint` finds comments in the loader pass; flow-heavy lint is within noise of baseline (#576) (#600) (#605) (#623)
- **Linter**: `fy lint` builds no value tree unless a custom `DocumentRule` is enabled (#609) (#623)
- **Linter**: `extends` error texts name the file without quoting its content (#571) (#595)
- **Core**: faster `NormalizedInput` scan and no per-float allocation (#558) (#562)
- **CLI**: `fy lint` batch streams results through a bounded window in file order; batch runs scale the default scan-ahead limit per worker and retry at the full limit (#576) (#577) (#595)
- **CLI/Core**: typed internals (`ConfigSource`, `ResolvedCli`, `Verbosity`, `Cursor`, `BatchStats`), `fy` builds without default features and drops `num_cpus`, `is-terminal` and unused dead code (#323) (#330) (#401) (#411) (#612) (#613) (#622)
- **CLI**: batch summary prints `1 file` instead of `1 files` (#401)
- **Parallel/Python**: `Workers::Auto` is capped at 128 threads and honors `RAYON_NUM_THREADS` (#610) (#622)
- **Parallel**: parse errors include the underlying cause (#433)
- **Node.js**: loaders return env-bound values without a lifetime transmute (#330) (#342) (#534)
- **Python**: documented the big-integer digit limit (`sys.get_int_max_str_digits()`) on load and dump (#488) (#510)
- **Build/CI**: Python wheels and test matrices reduced, Actions pinned to SHAs, format/quality jobs gate tests, workflow dependency bumps (#432) (#472) (#535) (#597)
- **Docs**: removed `docs/CI-CD-QUICKSTART.md` and `Makefile.toml`; CI notes moved to `CONTRIBUTING.md` (#596)
- **Dependencies**: saphyr 0.1.0 (later dropped), `ordered-float` 5.5 and Rust minor/patch updates (#294) (#299) (#300) (#304) (#305) (#399)
- **Dependencies**: `napi` 3.14, `napi-build` 2.6, Rust lockfile refresh, Node.js dev tooling (`@biomejs/biome`, `@napi-rs/cli`, `@types/node`) (#645)

### Fixed

#### Core

- `fy format` output stays parseable and faithful: directives of later documents and reserved `%NAME` kept, `%TAG` prefix starting with `#` kept, `...` markers, anchors (including after non-ASCII directives and `&` inside scalars), explicit tags, `|+` keep-chomp, `? ` explicit keys, block scalar indent indicators, folded scalars, empty flow collections and non-default `--indent` (#354) (#367) (#377) (#380) (#383) (#409) (#429) (#430) (#431) (#441) (#447) (#449) (#450) (#580) (#595) (#605) (#636)
- Formatter escaping and quoting: control characters, DEL, C1, U+FEFF, U+FFFE/U+FFFF, flow-style keys, `<<` and raw keys, alias keys over the implicit-key limit, empty root block scalar (#379) (#384) (#412) (#458) (#460) (#484) (#486) (#487)
- `fy format` rejects recursive aliases and cross-document aliases and errors instead of dropping anchors or nesting past limits (#371) (#383) (#389) (#412)
- `Emitter::emit_str` no longer panics on big-integer scalars or emits `tag:yaml.org,2002:!int`; `!!set` tags have no trailing space (#483) (#485) (#490) (#537)
- Scalar resolution is unified: verbatim core tags, `!!int`/`!!bool`/`!!float` on quoted scalars and empty values, `-0x8000000000000000` as `i64::MIN`, equal big-integer spellings collapse to one key (#390) (#394) (#464) (#477) (#480) (#495) (#561)
- Repeated mapping keys keep the first position with the last value; flow nesting beyond 255 levels is a `FlowNesting` error (#522) (#530) (#556) (#564)
- Invalid characters (DEL, C1, U+FFFE/U+FFFF, NUL) are rejected with position and document; UTF-16/UTF-32 input without a BOM is reported as an unsupported encoding (#417) (#458) (#525) (#526) (#561)
- A leading UTF-8 BOM is stripped before parsing, linting and conversion, and a mid-stream or second BOM no longer breaks `fy format` or loses directives (#331) (#451) (#455) (#561)
- Unterminated `%` directive at EOF no longer hangs the parser, bindings or CLI (#403) (#415)
- Merge keys are validated in every document and before later syntax errors or duplicate keys (#501) (#502) (#515) (#516) (#528)
- Python and Node.js `__proto__` keys, `U+0085` and Unicode whitespace key starts, non-2 indent block output and complex keys load and dump correctly; Python `safe_dump(sort_keys=True)` sorts set members (#412) (#561)
- Explicit tags are no longer corrupted by `format_files` in Python, Node.js and `fast-yaml-parallel` (#408)

#### Linter

- Byte, char and column offsets and spans on non-ASCII, CRLF and lone-CR input are correct across rules, with no panics on long columns, non-ASCII text before block scalars, lone quotes in keys or unbalanced `{}`/`[]` (#307) (#347) (#350) (#385) (#416) (#440)
- Quadratic lint time in `key-ordering`, flow tokens, `comments-indentation` and long lines is fixed (#385) (#439) (#443) (#444) (#453) (#467) (#468) (#473)
- Flow tokenizer ignores delimiters inside comments, quoted scalars, verbatim tags and directives; `commas`, `braces`, `brackets`, `colons` and `hyphens` match yamllint on flow collections, plain scalars, empty collections, continuation lines, anchors, tags and trailing comments (#345) (#388) (#442) (#445) (#446) (#452) (#575) (#615) (#616) (#617) (#620) (#630) (#631) (#637) (#440)
- `float-values`, `quoted-strings`, `truthy` and `empty-values` use the core scalar resolver and parser positions, so `.5e`, `+.inf`, `"1"` and empty values are classified correctly and keys are located at their own line (#353) (#393) (#458) (#461) (#473) (#474) (#482)
- `document-start` no longer reports a missing `---` after directives; `truthy` respects `%YAML 1.2`; `comments`, `comments-indentation` and `line-length` follow yamllint (#443) (#572) (#575) (#576) (#606) (#618) (#623)
- `duplicate-key` detects repeats through aliases and merge keys; `indent-size` in a later config layer replaces earlier `spaces`; `braces`/`brackets` honor `min-spaces-inside-empty` alone; `document-start: {present: true}` and `quote-type` typos are no longer ignored (#324) (#426) (#565) (#570) (#626) (#637)

#### CLI

- `fy format`: `--dry-run` compares with the input, never writes and works on stdin; `-i` skips already formatted files; batch mode refuses files with comments unless `--strip-comments`, with parser-based comment detection; BOM is kept (#333) (#348) (#397) (#561)
- `fy lint` honors `-o`, reports missing paths in report formats, prints each error once, checks size before reading, refuses to overwrite an input (also `format -o`/`convert -o`) and stays silent on a closed pipe (#509) (#519) (#537) (#569) (#575) (#595) (#604) (#623)
- `fy convert` keeps key order and integers beyond `i64` exact in both directions (#466) (#476) (#557) (#595)
- `-j N` limits concurrency to N, batch runs share one pool, uppercase extensions such as `UP.YAML` are found, `parse --stats` no longer panics on a closed stdout (#513) (#523) (#532) (#581) (#595) (#628) (#637)
- UTF-16/UTF-32 input, including lint configs, reports an "unsupported encoding" message (#334) (#524)

#### Parallel

- `parse_parallel` matches `parse_all` for chunking, empty documents, BOMs, root block scalars with `---`, document indices and `<<` validation; it shares one alias and `%TAG` budget across chunks (#364) (#365) (#387) (#405) (#406) (#455) (#503) (#520) (#552) (#562) (#574) (#595)
- Known saphyr divergences (column-0 `---` in block scalars, empty block scalar at EOF) are documented and pinned in tests (#407) (#455) (#456) (#469)

#### Python

- `safe_load` validates `<<` with core merge rules and honors explicit core tags; `safe_dump`/`dump_all`/`dump_parallel` accept big integers and `Mapping` objects and split `safe_dump_to` output on character boundaries; `parse_parallel` returns `int` for big integers; key collision errors name the document (#376) (#390) (#392) (#462) (#463) (#476) (#518) (#527) (#530) (#537) (#568) (#595)
- `lint(source, LintConfig())` and `lint(source)` report the same findings (#626) (#637)

#### Node.js

- Rust panics throw a catchable `Error` instead of aborting the process; `safeDump` handles `-0`, `BigInt` and `Set`/`Map` subclasses and `LintConfig.rules` type errors name their cause (#404) (#435) (#557) (#566) (#595)

#### CI

- Fuzz corpus directory is created for every target, `run_ordered` cancel test no longer depends on panic-hook timing, the `label-on-issue` workflow stops re-creating `status:needs-triage`, doctests run in CI (#345) (#413) (#414) (#635) (#636)

### Security

- **Core**: nesting-depth limit (256), alias-expansion budget (64 MiB), `%TAG` prefix cap (64 MiB per stream), nested anchor copies bounded by the alias budget and 24x source size, and `forbid(unsafe_code)` (#342) (#369) (#389) (#412) (#534) (#561)
- **Core/Linter**: config, `extends` and `ignore-from-file` are read as bounded regular files (opened `O_NONBLOCK`) so a FIFO or `/dev/zero` cannot hang or exhaust memory; `ignore-from-file` is capped at 32 files and 1024 lines (#571) (#595) (#605)
- **Parallel/CLI**: `fy format` writes through a secure atomic writer that fsyncs, keeps owner, mode, symlinks and extended attributes and writes hard-linked files in place only for their owner (#364) (#366) (#595) (#605)
- **CLI/Linter/Parallel**: file names are escaped (control characters, bidi overrides, U+2028/2029) in all outputs and messages (#607) (#623)
- **Python/Node.js**: deep, cyclic or alias-bomb `rules` input and dump inputs raise an error instead of crashing, with bounded depth and node budgets (#369) (#395) (#426) (#434)
- **Node.js**: dev dependency `js-yaml` bumped to fix GHSA-r3ph-w7gj-g6xm (#368)

### Documentation

- **Specs**: feature specifications added under `specs/` (#583)

## [0.6.6] - 2026-08-26

### Fixed

- Mark `has_source_unicode_hex_escape` in the quoted-strings lint rule as `const fn` to satisfy `clippy::missing_const_for_fn` under newer clippy ([#289](https://github.com/bug-ops/fast-yaml/pull/289))
- Reformat a `python/README.md` code block to match `ruff` 0.16.4's formatter output ([#292](https://github.com/bug-ops/fast-yaml/pull/292))

### Security

- Tighten the `js-yaml` pnpm override from `^4.3.0` to `>=4.3.1` to fix CVE-2026-59870 (quadratic CPU consumption in `!!omap` resolution), which was resolving to the vulnerable `js-yaml` 4.3.0 ([#285](https://github.com/bug-ops/fast-yaml/pull/285))
- Add a `nanoid` pnpm override (`>=3.3.17`) in Node.js bindings to fix GHSA-2v37-7h3g-55p8 (custom generators can loop indefinitely when size is zero), pulled in transitively through `vitest > vite > postcss > nanoid` ([#285](https://github.com/bug-ops/fast-yaml/pull/285))

### Dependencies

- Bump `napi` 3.11.0 → 3.12.0, `napi-build` 2.3.2 → 2.4.0, and `napi-derive` 3.6.0 → 3.6.2 ([#282](https://github.com/bug-ops/fast-yaml/pull/282))
- Bump `clap` 4.6.4 → 4.6.6, `globset` 0.4.19 → 0.4.20, `ignore` 0.4.31 → 0.4.33, and `pyo3` 0.29.0 → 0.29.2 ([#284](https://github.com/bug-ops/fast-yaml/pull/284))
- Bump `napi` 3.12.0 → 3.12.1, `napi-build` 2.4.0 → 2.4.1, `napi-derive` 3.6.2 → 3.6.3, and `thiserror` 2.0.19 → 2.0.20 ([#286](https://github.com/bug-ops/fast-yaml/pull/286))
- Bump `saphyr` 0.0.11 → 0.0.12 and `saphyr-parser` 0.0.11 → 0.0.12 (combined, since both crates are released in lockstep from the same upstream repository; bumping either alone leaves two mismatched `saphyr_parser` versions in the dependency graph and fails to compile) ([#290](https://github.com/bug-ops/fast-yaml/pull/290))
- Bump Node.js dev dependencies: `@biomejs/biome` ^2.5.5 → ^2.5.10, `@napi-rs/cli` ^3.7.4 → ^3.8.6, `@types/node` ^25.9.5 → ^26.3.0, `@vitest/coverage-v8`/`vitest` ^4.1.10 → ^4.1.11, `typescript` ^6.0.3 → ^7.0.2 ([#292](https://github.com/bug-ops/fast-yaml/pull/292))
- Refresh `python/uv.lock` to pick up latest compatible dev dependency versions ([#292](https://github.com/bug-ops/fast-yaml/pull/292))

## [0.6.5] - 2026-07-27

### Added

- Release pipeline now builds and attaches prebuilt `fy` CLI binaries to every GitHub release (Linux x86_64/aarch64 glibc, Linux x86_64 musl, macOS x86_64/aarch64, Windows x86_64), each packaged with a `.sha256` checksum. Linux aarch64 musl (e.g. Alpine on ARM64) is not yet published — build from source with `cargo install fast-yaml-cli`
- `scripts/install.sh`: POSIX-sh installer that detects the host OS/arch/libc (including musl via `/lib/ld-musl-*` or `ldd --version`), downloads the matching prebuilt `fy` binary, verifies its checksum, and installs it to `~/.local/bin` (or `$FASTYAML_INSTALL_DIR`)
- `skills/fast-yaml-cli/SKILL.md`, an Agent Skill documenting installation and usage of the `fy` CLI for AI coding agents ([#276](https://github.com/bug-ops/fast-yaml/pull/276))

### Security

- Bump `crossbeam-epoch` (transitive, via `ignore` and `rayon-core`) 0.9.18 → 0.9.20 to fix RUSTSEC-2026-0204 (invalid pointer dereference in the `fmt::Pointer` impl) ([#265](https://github.com/bug-ops/fast-yaml/pull/265))
- Bump `js-yaml` (transitive, via `@napi-rs/cli`) to >=4.3.0 via pnpm override to fix GHSA-52cp-r559-cp3m (YAML merge-key chains can force quadratic CPU consumption) ([#276](https://github.com/bug-ops/fast-yaml/pull/276))
- Tighten the `js-yaml` pnpm override from an open-ended `>=4.3.0` to `^4.3.0`, which was resolving to the vulnerable `js-yaml` 5.2.1 and reintroducing GHSA-pm4m-ph32-ghv5 (exponential parsing time in flow collections) ([#280](https://github.com/bug-ops/fast-yaml/pull/280))
- Add a `postcss` pnpm override (`>=8.5.18`) in Node.js bindings to fix GHSA-r28c-9q8g-f849 (path traversal via `sourceMappingURL` auto-loading), pulled in transitively through `vitest > vite > postcss` ([#280](https://github.com/bug-ops/fast-yaml/pull/280))

### Dependencies

- Bump `saphyr` 0.0.9 → 0.0.11 and `saphyr-parser` 0.0.9 → 0.0.11 (combined, since both crates are released in lockstep from the same upstream repository; bumping either alone leaves two mismatched `saphyr_parser` versions in the dependency graph and fails to compile) ([#272](https://github.com/bug-ops/fast-yaml/pull/272))
- Bump `saphyr` 0.0.6 → 0.0.9 and `saphyr-parser` 0.0.6 → 0.0.9 (combined, since both crates are released in lockstep from the same upstream repository) ([#268](https://github.com/bug-ops/fast-yaml/pull/268))
- Bump `napi` 3.9.4 → 3.11.0, `napi-derive` 3.5.7 → 3.6.0 ([#265](https://github.com/bug-ops/fast-yaml/pull/265), [#269](https://github.com/bug-ops/fast-yaml/pull/269), [#275](https://github.com/bug-ops/fast-yaml/pull/275), [#279](https://github.com/bug-ops/fast-yaml/pull/279))
- Bump `ignore` 0.4.26 → 0.4.31 ([#265](https://github.com/bug-ops/fast-yaml/pull/265), [#269](https://github.com/bug-ops/fast-yaml/pull/269), [#275](https://github.com/bug-ops/fast-yaml/pull/275), [#279](https://github.com/bug-ops/fast-yaml/pull/279))
- Bump `clap` 4.6.1 → 4.6.4 ([#275](https://github.com/bug-ops/fast-yaml/pull/275), [#279](https://github.com/bug-ops/fast-yaml/pull/279))
- Bump `memchr` 2.8.2 → 2.8.3 ([#269](https://github.com/bug-ops/fast-yaml/pull/269))
- Bump `anyhow` 1.0.103 → 1.0.104, `globset` 0.4.18 → 0.4.19, `serde` 1.0.228 → 1.0.229, `serde_json` 1.0.150 → 1.0.151, `thiserror` 2.0.18 → 2.0.19 ([#275](https://github.com/bug-ops/fast-yaml/pull/275))
- Bump `glob` 0.3.3 → 0.3.4 ([#279](https://github.com/bug-ops/fast-yaml/pull/279))
- Bump `actions/setup-node` 6 → 7 ([#273](https://github.com/bug-ops/fast-yaml/pull/273))
- Bump `actions/setup-python` 6 → 7 ([#274](https://github.com/bug-ops/fast-yaml/pull/274))
- Bump `actions/labeler` 6 → 7 ([#278](https://github.com/bug-ops/fast-yaml/pull/278))
- Bump `lewagon/wait-on-check-action` 1.8.0 → 1.9.0 ([#264](https://github.com/bug-ops/fast-yaml/pull/264), [#277](https://github.com/bug-ops/fast-yaml/pull/277))

## [0.6.4] - 2026-06-13

### Security

- Bump `vite` 8.0.10 → 8.0.16 in Node.js bindings to fix GHSA-7qr8-wg58-9r72 (`server.fs.deny` bypass on Windows) and GHSA-4vq8-g365-vhgc (NTLMv2 hash disclosure via UNC path handling on Windows) ([#260](https://github.com/bug-ops/fast-yaml/pull/260))

### Dependencies

- Bump `pyo3` 0.28.3 → 0.29.0 ([#256](https://github.com/bug-ops/fast-yaml/pull/256))
- Bump `codecov/codecov-action` 6 → 7 ([#254](https://github.com/bug-ops/fast-yaml/pull/254))
- Bump `ignore` 0.4.25 → 0.4.26 ([#255](https://github.com/bug-ops/fast-yaml/pull/255))
- Bump `memchr` 2.8.0 → 2.8.1 ([#253](https://github.com/bug-ops/fast-yaml/pull/253))

## [0.6.3] - 2026-05-26

### Fixed

- CI: Python Cargo cache key now includes the full patch version (e.g. `3.14.5`) so any patch update to the Python interpreter invalidates the cached build artifacts and prevents linker errors on Windows (`LNK1181: cannot open input file 'python314.lib'`)

### Dependencies

- Bump `rayon` 1.11.0 → 1.12.0 ([#248](https://github.com/bug-ops/fast-yaml/pull/248))
- Bump `clap` 4.6.0 → 4.6.1 ([#248](https://github.com/bug-ops/fast-yaml/pull/248))
- Bump `napi` 3.8.4 → 3.9.0, `napi-derive` 3.5.3 → 3.5.6, `napi-build` 2.3.1 → 2.3.2 ([#248](https://github.com/bug-ops/fast-yaml/pull/248), [#249](https://github.com/bug-ops/fast-yaml/pull/249), [#250](https://github.com/bug-ops/fast-yaml/pull/250))
- Bump `assert_cmd` 2.2.0 → 2.2.2 ([#248](https://github.com/bug-ops/fast-yaml/pull/248), [#250](https://github.com/bug-ops/fast-yaml/pull/250))
- Bump `bumpalo` 3.20.2 → 3.20.3, `serde_json` 1.0.149 → 1.0.150 ([#251](https://github.com/bug-ops/fast-yaml/pull/251))

## [0.6.2] - 2026-04-26

### Added

- yaml-test-suite integration in Python CI: parametrized ~400 test cases against `fast_yaml.safe_load()` / `safe_load_all()`, pinned to `data-2022-01-17` tag ([#228](https://github.com/bug-ops/fast-yaml/issues/228))

### Fixed

- Python binding now converts `!!set` tagged mappings to Python `set` objects instead of `dict` with `None` values, per YAML spec §10.3.3 ([#239](https://github.com/bug-ops/fast-yaml/issues/239))
- `parse_all` / `safe_load_all` now yield one implicit null document for non-empty inputs that contain no explicit documents (whitespace-only, comment-only, bare `---`, bare `...`), matching YAML 1.2 §9.2 and PyYAML parity; empty string `""` continues to return `[]` ([#235](https://github.com/bug-ops/fast-yaml/issues/235))
- Bare `---` with no content now correctly resolves to null instead of `String("")`; empty plain scalar with no tag is treated as implicit null per YAML 1.2 §10.3.2 ([#235](https://github.com/bug-ops/fast-yaml/issues/235))
- Non-specific tag `!` on a scalar (e.g. `x: ! 99`) now forces the failsafe schema and returns a string (`"99"`) instead of applying implicit type resolution; matches YAML 1.2 §6.8.1 / §10.3.2 and PyYAML behaviour ([#238](https://github.com/bug-ops/fast-yaml/issues/238))
- Hex (`0x...`) and octal (`0o...`) integer literals that overflow `i64` are now preserved as strings instead of being silently coerced to float; `is_integer_literal` and the `!!int` tag path now recognise hex/octal prefixes ([#230](https://github.com/bug-ops/fast-yaml/issues/230))
- Large integers exceeding `i64` range are now correctly preserved as Python `int` instead of being coerced to `float` ([#229](https://github.com/bug-ops/fast-yaml/issues/229), closes [#227](https://github.com/bug-ops/fast-yaml/issues/227))
- Linter: resolve `sort_by` and `collapsible_match` Clippy warnings in `linter.rs` and `quoted_strings.rs`

### Dependencies

- Bump `pyo3` ([#218](https://github.com/bug-ops/fast-yaml/pull/218))
- Bump `rand` 0.9.2 → 0.9.4 ([#224](https://github.com/bug-ops/fast-yaml/pull/224))
- Bump `pytest` ([#223](https://github.com/bug-ops/fast-yaml/pull/223))
- Bump `vite` ([#219](https://github.com/bug-ops/fast-yaml/pull/219))
- Bump `actions/github-script` 8 → 9 ([#225](https://github.com/bug-ops/fast-yaml/pull/225))
- Bump `softprops/action-gh-release` 2 → 3 ([#220](https://github.com/bug-ops/fast-yaml/pull/220))
- Bump `dependabot/fetch-metadata` 2 → 3 ([#221](https://github.com/bug-ops/fast-yaml/pull/221))
- Bump `lewagon/wait-on-check-action` 1.6.0 → 1.7.0 ([#222](https://github.com/bug-ops/fast-yaml/pull/222), [#226](https://github.com/bug-ops/fast-yaml/pull/226))

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

[Unreleased]: https://github.com/bug-ops/fast-yaml/compare/v0.7.0...HEAD
[0.7.0]: https://github.com/bug-ops/fast-yaml/compare/v0.6.6...v0.7.0
[0.6.6]: https://github.com/bug-ops/fast-yaml/compare/v0.6.5...v0.6.6
[0.6.5]: https://github.com/bug-ops/fast-yaml/compare/v0.6.4...v0.6.5
[0.6.4]: https://github.com/bug-ops/fast-yaml/compare/v0.6.3...v0.6.4
[0.6.3]: https://github.com/bug-ops/fast-yaml/compare/v0.6.2...v0.6.3
[0.6.2]: https://github.com/bug-ops/fast-yaml/compare/v0.6.1...v0.6.2
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
