---
aliases:
  - Lint
  - fy lint
tags:
  - sdd
  - spec
  - lint
created: 2026-10-01
status: reverse-specified
related:
  - "[[constitution]]"
  - "[[plan]]"
---

# Feature: Lint (`fy lint` and `fast-yaml-linter`)

> [!info] Basis
> Reverse-specified from v0.6.6 (commit e5e6cfb). Behaviour in sections 2-8 was confirmed by running `target/debug/fy` and reading the code. Deviations from the desired behaviour are listed only in section 10. Supersedes the in-flight specs `001-inline-lint-directives` and `002-lint-ci-output-formats`, whose scope is implemented and folded in here. Sections below also reflect the fixes #571-#575, #578, #579 and #569 made after that baseline (config `extends` and `ignore-from-file`, yamllint parity, single-pass memory, `-o`), and by the batch #588 (yamllint parity of `indentation`, `quoted-strings`, `key-ordering`, `empty-values`; per-rule `ignore`; yamllint rule names and `enable` semantics; `locale`; lazy diagnostic excerpts; JSON input-error reports).

## 1. Purpose and value

YAML that parses can still be wrong or inconsistent: duplicate keys, `yes` read as a boolean, `0755` read as an octal, mixed spacing, long lines. The linter reports these as located, severity-graded diagnostics, in forms a human (rustc-style text), a script (JSON) and a CI system (GitHub annotations, SARIF, `parsable`) can consume. It follows [yamllint](https://yamllint.readthedocs.io/) vocabulary (rule names, options, `extends: default|relaxed`, `# yamllint disable` comments) so existing configuration carries over, and it is fast enough to lint thousands of files in parallel (see [[005-batch-parallel/spec]]).

Scope: the `fy lint` subcommand and the `fast-yaml-linter` crate (library API, 25 built-in rules, config file, inline directives, report formats). Python and Node.js expose the library API only (see [[007-python-api/spec]], [[008-nodejs-api/spec]]).

## 2. User stories

### US-1 (P1): Lint one file or stdin and see what is wrong

AS A developer I WANT located diagnostics with source context SO THAT I can fix problems quickly.

```
GIVEN a.yaml = "name: a\nname: b\nkey:   value   \nlist:\n- 1\n"
WHEN  fy lint --no-config a.yaml
THEN  stdout starts with
      error[duplicate-key]: duplicate key 'name' (first defined at line 1)
        --> input:2:1
      ... then info[key-ordering], warning[colons], hint[trailing-whitespace],
      and ends with "1 errors, 1 warnings"
AND   exit code is 2 (at least one diagnostic of severity error)
```

```
GIVEN "name: ok\n"
WHEN  fy lint --no-config ok.yaml
THEN  stdout is empty, exit code 0

GIVEN a file whose only findings are warnings, info or hints (a:   1)
WHEN  fy lint --no-config w.yaml
THEN  the diagnostic is printed and the exit code is 0
```

### US-2 (P1): Fail CI on errors, annotate pull requests

AS A CI maintainer I WANT machine formats SO THAT findings appear as annotations and in code scanning.

```
WHEN fy lint --no-config --format parsable a.yaml
THEN /abs/path/a.yaml:2:1: [error] duplicate key 'name' (first defined at line 1) (duplicate-key)
     /abs/path/a.yaml:3:1: [warning] key 'key' should be ordered before 'name' (line 1) (key-ordering)
     ...  (one line per diagnostic; info and hint print as [warning])

WHEN fy lint --no-config --format github a.yaml
THEN ::error file=/abs/path/a.yaml,line=2,col=1,endLine=2,endColumn=5,title=duplicate-key::duplicate key 'name' (first defined at line 1)
     ::notice file=...,line=3,col=1,...,title=key-ordering::...        (info and hint -> notice)

WHEN fy lint --no-config --format sarif ok.yaml
THEN a SARIF 2.1.0 log with tool.driver.name "fast-yaml-linter", "rules": [] and "results": []
```

### US-3 (P1): Configure rules per project

AS A project owner I WANT a `.fast-yaml.yaml` file SO THAT the team shares one rule set.

```
GIVEN .fast-yaml.yaml = "extends: default\nrules:\n  line-length: {max: 20}\n  document-start: disable\n  truthy: {level: warning}\n"
AND   c.yaml = "key: a very long line that goes over twenty chars\nflag: yes\n"
WHEN  fy lint c.yaml            (run in that directory)
THEN  stderr: "using config file: <abs>/.fast-yaml.yaml"
AND   stdout: error[line-length] "line exceeds maximum length of 20 characters (current: 49)" and
      warning[truthy] "found non-standard truthy value 'yes' (use true or false)"
AND   exit code 2

WHEN  fy lint --no-config c.yaml
THEN  line-length is not reported (default limit is 80)
```

### US-4 (P2): Suppress a finding locally

AS A developer I WANT inline directives SO THAT a justified exception does not require a global rule change.

```
GIVEN "# fy: disable colons\na:   1\n# fy: enable colons\nb:   2\n"
WHEN  fy lint --no-config --format parsable d.yaml
THEN  only line 4 reports colons (d.yaml:4:3 ... (colons))

GIVEN "# fy: disable-file\na:   1\nb: [ 1 ]\n"
THEN  no output, exit 0

GIVEN "# fy: frobnicate\na: 1\n"
THEN  d.yaml:1:1: [warning] unknown lint directive verb `frobnicate` (lint-directive)
```

### US-5 (P2): Lint many files deterministically

AS A CI maintainer I WANT directory and glob arguments, `--include/--exclude`, `--stdin-files` SO THAT I lint a repo or a git diff in one run, with stable output order.

```
GIVEN d/a.yaml (from US-1) and d/ok.yaml
WHEN  fy lint --no-config --format parsable d
THEN  lines are d/a.yaml:2:1 ... in file-path order, each file's diagnostics sorted by (line, column)

WHEN  printf 'ok.yaml\nw.yaml\n' | fy lint --no-config --stdin-files --format parsable
THEN  w.yaml:1:3: [warning] too many spaces after colon (expected at most 1, found 3) (colons)
```

### US-6 (P2): Use the linter as a library

AS A Rust/Python/Node developer I WANT `Linter::with_all_rules().lint(src)` returning `Vec<Diagnostic>` SO THAT I can embed linting (editors, pre-commit, services).

```
GIVEN "!!set {a: 1, b}\n"
WHEN  Linter::with_all_rules().lint(..)
THEN  Ok(diagnostics) contains code "set-values"
```

### US-7 (P3): Syntax errors are reported, not hidden

```
GIVEN "a: [1,\n"
WHEN  fy lint --no-config --format parsable bad.yaml
THEN  bad.yaml:2:1: [error] YAML syntax error: while parsing a node, did not find expected node content (syntax)
AND   stderr repeats "error: Failed to lint YAML", exit code 1
```

### US-8 (P2): Share a rule set across projects

AS A platform maintainer I WANT `extends: <file>` and `ignore-from-file` SO THAT teams inherit a base config and one ignore list.

```
GIVEN base.yaml = "rules:\n  line-length: {max: 20}\n  truthy: {level: warning}\n"
AND   team.yaml = "extends: base.yaml\nignore-from-file: .lintignore\nrules:\n  key-ordering: {ignored-keys: ['^name$']}\n"
AND   .lintignore = "skip.yaml\n"
WHEN  fy lint --config team.yaml --format parsable c.yaml skip.yaml     (verified)
THEN  c.yaml reports line-length (max 20) and truthy from base.yaml, and key-ordering except for the key `name`
AND   skip.yaml is not linted (an explicit file matching the ignore patterns is skipped silently), exit 0 (warnings only)
```

### US-9 (P2): Reuse a yamllint config and its exceptions

AS A team migrating from yamllint I WANT yamllint rule names, per-rule `ignore` and `locale` SO THAT my `.yamllint` content loads as is.

```
GIVEN .fast-yaml.yaml = "rules:\n  trailing-spaces: {ignore: [generated/]}\n  indentation: {spaces: 4, indent-sequences: consistent}\nlocale: C\n"
WHEN  fy lint generated/a.yaml src/b.yaml
THEN  generated/a.yaml gets no trailing-whitespace finding, src/b.yaml does
AND   indentation findings read "wrong indentation: expected 4 but found 2"

GIVEN extends: default and rules: {document-start: enable}
THEN  a file without --- is reported at the preset's level (warning), as in yamllint
```

## 3. Functional requirements

### 3.1 Diagnostics and engine

| ID | Requirement | Pri |
|----|-------------|-----|
| FR-001 | WHEN a source is linted THE SYSTEM SHALL return diagnostics each carrying `code` (kebab-case), `severity` (`hint < info < warning < error`), `message`, `span` (start/end with 1-based line, 1-based column, 0-based byte offset) an `excerpt` policy (`SourceLines`: the lines around the span are cut from the source when the diagnostic is rendered; `Omitted`: none) and `suggestions`. The JSON field `context` is rendered from the excerpt policy and is absent for `syntax` and input diagnostics. | must |
| FR-002 | THE SYSTEM SHALL return diagnostics sorted by span start (stable for ties) after inline directives are applied. | must |
| FR-003 | WHEN the source begins with a byte order mark THE SYSTEM SHALL report all locations in BOM-free coordinates. | must |
| FR-004 | WHEN the source is not valid YAML, exceeds a parse limit (depth, alias, scan-ahead, `max-documents`) or `max-input-bytes`, THE SYSTEM SHALL return an error (`LintError`) instead of rule diagnostics; the CLI renders it as a `syntax` diagnostic in `json`, `github`, `sarif`, `parsable` formats (FR-035). Syntax errors are never suppressible by directives. | must |
| FR-005 | WHEN the source is multi-document THE SYSTEM SHALL run value-based rules once per document and text-based rules once over the full input. | must |
| FR-006 | WHEN a diagnostic is built THE SYSTEM SHALL use severity from the config override for the rule if set, else the rule's default severity. | must |
| FR-007 | THE SYSTEM SHALL bound linter work by `ParseLimits` and `max-input-bytes` exactly as the parser does (see [[009-limits-security/spec]]); hostile input SHALL NOT make the scanner buffer more than `max-scan-ahead`, and a stream of more than `max-documents` documents (default 100 000, `--max-documents`) SHALL fail like any other limit. | must |
| FR-008 | THE SYSTEM SHOULD expose a metadata-only `LintRule` trait (`id() -> RuleId`, `name`, `description`, `default_severity`), the `SourceRule` (`check(context, config)`) and `DocumentRule` (`check(context, LintDocument { value, first_line }, config)`) traits, and `Linter::add_rule(Rule) -> Result<_, DuplicateRule>` so that embedders can register custom rules, configured via `LintConfig::custom_rules`; a rule with an id that is already registered SHALL be rejected. | should |
| FR-009 | THE SYSTEM SHALL run every built-in rule over one guarded loader pass of the source (events, comments, document markers, a compact node index and flow ranges are collected together), SHALL NOT drive a raw `saphyr-parser` anywhere in `fast-yaml-linter` outside tests (enforced by `clippy::disallowed_methods`), and SHALL gather only what the enabled rules need. The loader pass SHALL build no value tree (`Parser::validate_normalized_observed`) unless an enabled `DocumentRule` asks for the documents. With default options the peak heap of linting a large root flow collection SHALL stay within 1.25x the peak of loading it, and of a large block file at or below it (`tests/lint_memory.rs`); a diagnostic costs a few hundred bytes, so input that yields a diagnostic per scalar is bounded by a larger factor (see D-18). Source excerpts SHALL NOT be stored per diagnostic: they are cut from the BOM-free source (`LintSource`) when a formatter writes the diagnostic (`Findings`), so memory does not grow with the excerpt size. | must |
| FR-009a | THE SYSTEM SHALL write text and JSON reports through `Formatter::write(&mut dyn io::Write, Findings)`, so a single-file report streams instead of being built as one string. `JsonDiagnostic` fixes the field order (`code`, `severity`, `message`, `span`, `context`, `suggestions`, `file`). | must |

### 3.2 Configuration

| ID | Requirement | Pri |
|----|-------------|-----|
| FR-010 | WHEN `fy lint` runs without `--config` and without `--no-config` THE SYSTEM SHALL search for `.fast-yaml.yaml` then `.fast-yaml.yml` starting at the process working directory and walking up at most 20 parent directories, and use the first one found. | must |
| FR-011 | WHEN `--config FILE` is given THE SYSTEM SHALL load exactly that file and fail (exit 1) if it is missing or invalid. WHEN `--no-config` is given THE SYSTEM SHALL use built-in defaults (FR-016). | must |
| FR-012 | THE SYSTEM SHALL accept only the top-level keys `rules`, `extends`, `ignore`, `ignore-from-file`, `yaml-files`, `locale`, `max-input-bytes`, `max-scan-ahead`, `max-diagnostics`; any other key SHALL fail the load with a message that lists the accepted keys. `locale` SHALL be a string; it is not inherited through `extends` (as in yamllint). | must |
| FR-013 | `extends` SHALL be `default`, `relaxed` (yamllint presets, see 6.1) or the path of another config file, resolved against the directory of the file that names it. The extended file SHALL be loaded first, recursively, and the extending file's rules SHALL be applied over it like over a preset; `max-input-bytes`, `max-scan-ahead` and `ignore` SHALL be inherited when the extending file does not set them, `yaml-files` SHALL NOT. A chain of more than 8 files (the extending file included) or a cycle SHALL fail the load. | must |
| FR-014 | A rule entry SHALL be one of: `enable`, `disable`, a severity (`error`, `warning`, `info`, `hint`), or a mapping with option keys plus `enabled`/`enable`, and `severity` or `level`. An unknown rule name or unknown option SHALL fail the load and name the accepted options. A rule key SHALL be a fast-yaml code or its yamllint name (`trailing-spaces` = `trailing-whitespace`, `key-duplicates` = `duplicate-key`, `anchors` = `invalid-anchor`); both spellings of one rule in one mapping SHALL fail the load. Diagnostics and JSON always carry the fast-yaml code. | must |
| FR-014a | WHEN a config file or a binding `rules` patch gives a mapping or severity entry for a rule THE SYSTEM SHALL start it from yamllint's defaults for that rule (severity `error`, yamllint option defaults) over a missing, disabled or fast-yaml-default entry, and from the inherited values over an entry a preset or parent file configured, as yamllint does. `rules: {x: enable}` SHALL reset options and severity to the rule defaults when the inherited entry is disabled, fast-yaml-default or absent (so `extends: default` plus `document-start: enable` still reports at the preset's level), and only set `enabled = true` over an entry configured by a preset or parent. A patch keeps the current options for keys it does not mention. An inline `# yamllint enable` cannot turn on a rule the config disabled. | must |
| FR-015 | `ignore` (gitignore-style patterns, anchored at the config file's directory) SHALL drop files from linting with exit 0; `yaml-files` SHALL select file names in directory walks. `ignore-from-file` (one file name or a list, relative to the config file's directory) SHALL supply the patterns from the lines of those files instead; it SHALL conflict with `ignore` in the same file. Listing more than 32 different files, or more than 1024 pattern lines in total, SHALL fail the load; a repeated file name is read once. | should |
| FR-015a | A rule entry mapping MAY carry `ignore` (a gitignore-style pattern string or list) or `ignore-from-file`, never both. WHEN a file's canonical path matches, THE SYSTEM SHALL NOT run that rule on it (it also drops out of the scan the rule needed). Patterns are anchored at the directory of the config file that declares them, are inherited through `extends` and are replaced by the same rule's `ignore` in the extending file. Input from stdin is never ignored. Matching uses the canonical path (`CanonicalPath`; symlinks are followed). `Linter::lint_file(source, &CanonicalPath)` is the library entry for it; bindings take an optional `path` and reject per-rule `ignore` in a binding patch because it has no config directory. | should |
| FR-015b | `locale` other than `C`, `POSIX` or `C.UTF-8` (case-insensitive) SHALL fail the load only while `key-ordering` is enabled in the final configuration (after CLI overrides); otherwise it is accepted and inert. `key-ordering` compares by code point. | should |
| FR-016 | WITHOUT a config file all 25 rules SHALL be enabled with fast-yaml option defaults (6.1), which differ from the `default` preset: with no config, `document-start`, `document-end` accept either presence and `truthy` does not check keys. | must |
| FR-017 | CLI flags `--max-line-length N`, `--indent-size N` (1-16), `--allow-duplicate-keys` (`true` disables `duplicate-key`) SHALL override the config file (`--indent-size` sets a fixed `indentation` width; the formatter indent of `fy lint` applies only when the config sets neither `spaces` nor `indent-size` and differs from 2); `--max-input-bytes` and `--max-scan-ahead` SHALL override the same-named keys. | must |
| FR-018 | WHEN a config file is auto-discovered THE SYSTEM SHALL print `using config file: <path>` to stderr. | should |
| FR-019 | THE SYSTEM SHALL read every config file, `extends` target and `ignore-from-file` file through one reader that refuses a path that is not a regular file before opening it (a FIFO or device is never opened), re-checks the opened file, and reads at most 1 MiB (larger fails with `TooLarge`). WHEN an `extends` target fails to load THE SYSTEM SHALL name the target file and SHALL NOT quote its content: errors that could echo text become a generic "malformed" error naming the path. | must |

### 3.3 Inline directives

| ID | Requirement | Pri |
|----|-------------|-----|
| FR-020 | THE SYSTEM SHALL recognise the comments `# fy: disable [rules]`, `# fy: enable [rules]`, `# fy: disable-line [rules]`, `# fy: disable-file`, and the yamllint spellings `# yamllint disable|enable|disable-line|disable-file [rule:NAME ...]`. | must |
| FR-021 | `disable`/`enable` SHALL open/close a block that runs from the directive line to the matching `enable` or end of file; with no rule list they apply to all rules; rule lists accumulate. | must |
| FR-022 | `disable-line` SHALL suppress diagnostics whose span starts on the directive's own line when it trails content, or on the following line when the comment stands alone. | must |
| FR-023 | `disable-file` SHALL suppress all rule diagnostics of the file, but not `syntax` errors. | must |
| FR-024 | Suppression SHALL be line-granular: a diagnostic is suppressed when the line its span starts on is covered. | must |
| FR-025 | Directives SHALL be recognised only in real comments, not inside quoted or block scalars (`a: "# fy: disable-file"` does nothing). | must |
| FR-026 | WHEN a directive names an unknown rule, has an unknown verb, malformed names, or trailing text THE SYSTEM SHALL report a `lint-directive` diagnostic, which no directive can suppress. yamllint aliases `key-duplicates`, `trailing-spaces`, `anchors` SHALL be accepted in directives. | must |

### 3.4 Output formats and exit codes

| ID | Requirement | Pri |
|----|-------------|-----|
| FR-030 | `--format text` (default) SHALL print per diagnostic a header `severity[code]: message`, a location line `--> input:LINE:COL`, a source excerpt with a caret row and optional `= help:` line, then a summary line `N errors, M warnings`. In batch mode each file's block is preceded by `<path>:`. | must |
| FR-031 | `--format json` SHALL print a JSON array of diagnostic objects (`code`, `severity`, `message`, `span`, optional `context`, optional `suggestions`); in multi-file mode each object also has `file` as its last key. No findings SHALL print `[]`. `context` is absent on `syntax` and input diagnostics. | must |
| FR-032 | `--format github` SHALL print one workflow command per diagnostic `::LEVEL file=,line=,col=,endLine=,endColumn=,title=CODE::MESSAGE`; error to `error`, warning to `warning`, info and hint to `notice`; message text SHALL be escaped so a diagnostic cannot inject a second command. | must |
| FR-033 | `--format sarif` SHALL print one SARIF 2.1.0 log (`runs[0].tool.driver` = `fast-yaml-linter` + crate version, `rules` = ids of codes seen, results with `ruleId`, `level` error/warning/note, `physicalLocation` with `file://` URI and region). Output SHALL validate against the SARIF 2.1.0 schema. | must |
| FR-034 | `--format parsable` SHALL print `PATH:LINE:COL: [LEVEL] MESSAGE (CODE)` per diagnostic where LEVEL is `error` for errors and `warning` for all other severities; stdin is named `stdin`; newlines in messages and paths SHALL be flattened to spaces. | must |
| FR-035 | In `json`, `github`, `sarif`, `parsable` formats an unreadable or missing file or a syntax/limit error SHALL still print one `syntax` diagnostic (a one-element array in `json`, exit code unchanged), so these formats never print nothing for a failing input; in batch mode every failing file, including syntax and encoding errors, SHALL appear in every one of these formats. `text` keeps the error on stderr only (D-3). | must |
| FR-036 | `--quiet` SHALL drop all non-error diagnostics before output and exit-code evaluation. | should |
| FR-037 | Exit codes: 0 no error-severity diagnostics; 2 at least one error-severity diagnostic, or a syntax error in batch mode; 1 syntax/limit error for a single input, unreadable or missing path, `--config` failure, or `--in-place`; clap usage errors exit 2. Warnings, info and hints never change the exit code. | must |
| FR-038 | `--in-place` SHALL be rejected for lint with `--in-place is not supported by fy lint (auto-fix is not implemented)`. | must |
| FR-039 | Report formats SHALL name files by absolute path (symlinks resolved) so annotations match regardless of working directory. | should |

### 3.5 Batch lint

| ID | Requirement | Pri |
|----|-------------|-----|
| FR-040 | WHEN more than one input results (directory, glob, several paths, `--stdin-files`) THE SYSTEM SHALL lint files in parallel (`-j`) but report them in file-path order, regardless of finish order, using bounded memory. | must |
| FR-041 | WHEN a path is missing, a glob matches nothing, an explicit file is not YAML, or a `--stdin-files` line is a directory/over 4096 bytes THE SYSTEM SHALL fail with an error naming it. | must |
| FR-042 | Directory walks SHALL select `*.yaml`, `*.yml` and the hidden file `.yamllint` (a lint target as in yamllint; other hidden entries stay skipped) case-insensitively unless `--include` is given, which replaces the defaults; `fy format` does not visit `.yamllint`; `--exclude` and `--no-recursive` apply (full discovery rules in [[005-batch-parallel/spec]]). | must |

### 3.6 Output destination and closed pipes

| ID | Requirement | Pri |
|----|-------------|-----|
| FR-050 | WHEN `-o FILE` is given THE SYSTEM SHALL write the report (every format, single input or batch) to FILE, streaming into a temp file in FILE's directory that replaces it atomically at the end, so memory does not grow with the report. `-o -`, `/dev/stdout`, `/dev/stderr` select the streams ([[006-cli-contract/spec]] FR-011). WHEN FILE is an input (same canonical path, symlink or hard link) THE SYSTEM SHALL refuse with `--output '<path>' is also an input file; refusing to overwrite it` (exit 1) and leave it untouched. | must |
| FR-051 | WHEN stdout or stderr is closed by the reader (`EPIPE`, `2>&1 \| head -1`) THE SYSTEM SHALL stop writing silently, never panic, and exit with the code the diagnostics imply (0 or 2). | must |

### 3.7 Rule parity with yamllint

| ID | Requirement | Pri |
|----|-------------|-----|
| FR-060 | `key-ordering` SHALL accept `ignored-keys` (regular expressions, `re.search` semantics; a matching key is neither checked nor compared against), SHALL check the first key of a mapping inside a sequence item (`- b: 1`) and the keys of a flow mapping that is the whole root of a document (`--- {b: 1, a: 2}`). | must |
| FR-061 | `comments-indentation` SHALL require a standalone comment to match the indent of the next content line (column 0 at end of file) or of the one before it, a following comment of the same run to match the previous comment, and SHALL NOT check the first comment after a block scalar. | must |
| FR-062 | `invalid-anchor` SHALL accept `forbid-duplicated-anchors` (default true: reports an anchor defined again in a document), `forbid-unused-anchors` (default false: reports an anchor no alias of its document refers to) and `forbid-undeclared-aliases` (only `true`; `false` is rejected because an undeclared alias is a parse error). | must |
| FR-063 | `document-start` and `document-end` SHALL check every document of the stream; `document-start: forbidden` SHALL also flag a `---` that follows a `%` directive; an empty or comment-only source SHALL report no marker. | must |
| FR-064 | `truthy` SHALL report `true`/`false` when `allowed-values` leaves them out, and SHALL NOT report `y`/`n`. | must |
| FR-065 | `quoted-strings` SHALL skip a scalar whose anchor is the token before it and SHALL check a scalar with a verbatim `!<tag:yaml.org,2002:str>` tag. | must |
| FR-066 | `comments` SHALL treat a run of leading `#` as one marker (`## ok` is accepted, as in yamllint). | must |
| FR-067 | `indentation` SHALL port yamllint 1.38's stack machine over PyYAML's token stream rebuilt from the scalar index and the text between scalars: options `spaces` (1-16 or `consistent`), `indent-size` (fast-yaml alias of a fixed `spaces`; `spaces` wins when both are set), `indent-sequences` (`true` default, `false`, `whatever`, `consistent`) and `check-multi-line-strings` (default false). With neither `spaces` nor `indent-size` set the width is `consistent`. Findings are `wrong indentation: expected N but found M`, `... expected at least N ...` and `cannot infer indentation: unexpected token`, spanning the first token of the line; mixed tabs and spaces stay reported. The presets set `spaces: consistent` (`default`) and `indent-sequences: consistent` (`relaxed`). | must |
| FR-068 | `quoted-strings` `only-when-needed` SHALL decide "needed" as yamllint does: empty, loads as a non-string under the PyYAML YAML 1.1 resolvers (bool incl. `yes`/`on`, int incl. `0b`/`0x`/`_`/sexagesimal, float needing a dot and a signed exponent so `1e3` and `NaN` are strings, `<<`, null, timestamp, `=`), contains a flow indicator `,[]{}` inside a flow collection, contains an inner TAB, a double-quoted token with a backslash line continuation, or is not loadable as a plain scalar in block context. Root scalars SHALL NOT be checked. `required: true`/`false` plain-scalar checks use the same resolver. | must |
| FR-069 | `key-ordering` SHALL run over the positioned node index and check keys of every mapping (block, flow, nested, explicit `?`), including `null`, `~` and `<<` keys; it SHALL skip anchored, tagged, empty, alias and collection keys (not recorded either), compare by code point, restart in every collection and document, and span a quoted key from its opening quote. | must |
| FR-070 | `empty-values` SHALL report at the column right after the colon (or hyphen) with a zero-width span, SHALL NOT report an empty value that carries an anchor, and SHALL implement `forbid-in-block-sequences` (`- a\n-\n- b` reports `2:2` "empty value in block sequence"); the `Add explicit 'null'` suggestion is an insertion at the span. The text format SHALL print a one-column caret under a zero-width span. | must |
| FR-071 | `braces` and `brackets` SHALL measure only spaces between an opening delimiter and the next token, and between the previous token and a closing delimiter, on the same line; indentation of a closing delimiter that starts its own line, and trailing space after an opening delimiter, SHALL NOT be reported. | must |
| FR-072 | `colons` SHALL NOT report spaces after a colon that is followed by end of line or by a comment (`push:  # c`). | must |
| FR-073 | `hyphens` SHALL check only the hyphen of a block sequence entry, found in the rebuilt token stream (`indentation` shares it), against the token that follows on the same line; `-item`, a continuation line of a plain scalar and a flow item starting with `-` are not entries. | must |
| FR-074 | `line-length` with `allow-non-breakable-inline-mappings` SHALL exempt a line only when, after the first block mapping start of the line scanned alone, a `:` is followed by a scalar token that has no space from its start to the end of the line (yamllint 1.38 `check_inline_mapping`): a `:` with no value, an anchored or tagged value and a flow value are judged by the next `:` and scalar. | must |
| FR-075 | `truthy` SHALL read a document preceded by a `%YAML 1.2` directive as YAML 1.2 and not report `yes`/`no`/`on`/`off` (any case) in it; the directive SHALL NOT carry into the next document, and any other or absent version keeps the YAML 1.1 spellings. | must |
| FR-076 | WHEN `--max-diagnostics N` (or the `max-diagnostics` key, positive integer, inherited through `extends`; the flag wins) is set, `fy lint` SHALL show the first N diagnostics of each file in span order and replace the rest with one `diagnostic-limit` diagnostic (`N more diagnostics not shown (E errors, W warnings); limit is N per file`) whose severity is the highest among the omitted ones, so an omitted error still reads as an error in every format (JSON element, SARIF result and rule, GitHub `::error`, parsable and text line). The cap applies after `--quiet` and after the exit code is decided, so it never changes the exit code, and it is off by default. It bounds output volume only: the linter and a batch still build the full list, and the text footer counts only the diagnostics shown. `diagnostic-limit` and `syntax` are reserved and cannot be custom rule codes. | should |

## 4. Key entities and types

| Entity | Description |
|--------|-------------|
| `Diagnostic` | `{ code: DiagnosticCode, severity: Severity, message: Cow<'static, str>, span: Span, excerpt: Excerpt, suggestions: Vec<Suggestion> }`. Built through `DiagnosticBuilder` (`build()` keeps source lines, `build_without_excerpt()` omits them). `Excerpt` is `SourceLines` or `Omitted` and is not serialized. |
| `DiagnosticCode` | Newtype over the kebab-case code; constants for built-ins plus `syntax` and `undefined-alias`. |
| `Severity` | Enum `Hint < Info < Warning < Error` (`#[non_exhaustive]`). |
| `RuleId<'a>`, `LintRule`, `SourceRule`, `DocumentRule`, `Rule` | `RuleId` is `BuiltIn(RuleName) \| Custom(&CustomRuleCode)`, so settings lookups never parse a string. `LintRule` holds the metadata; `SourceRule` and `DocumentRule` add the `check` of each input kind; `Rule` is `Source(Box<dyn SourceRule>) \| Document(Box<dyn DocumentRule>)`, the unit the registry stores. All 25 built-ins are source rules. |
| `Span`, `Location` | `{ line, column, offset }` start/end. |
| `Linter` | Holds `LintConfig` + `RuleRegistry`; `lint(&str)`, `source(&str) -> LintSource`, `lint_source(&LintSource)`, `lint_file(&str, &CanonicalPath)` and `lint_source_file(&LintSource, &CanonicalPath)` (the last two apply per-rule `ignore`). |
| `LintSource`, `Findings`, `Formatter` | `LintSource` owns the BOM-free text and its line index. `Findings` pairs diagnostics with their excerpts (`FromSource` cuts them lazily, `Given` carries precomputed ones); `Formatter::write(&mut dyn io::Write, Findings)` is the required method, `format(Findings) -> String` is provided. |
| `LintConfig` | `{ rules: RulesConfig, custom_rules, parse_limits: ParseLimits, max_input_bytes: MaxInputBytes }`; `parse_limits` carries depth, alias, tag, scan-ahead and document limits. |
| `RulesConfig` / `RuleSettings<O>` | One typed `RuleSettings { enabled, severity: Option<Severity>, options: O, ignore: Option<RuleIgnore>, origin: EntryOrigin }` per built-in rule (`origin` is `FyDefault` or `Configured` and drives `enable`, FR-014a); `RuleName` enum is the closed set of 25 codes. |
| `Limit` / `EmptyInsideLimit` / `MarkerPresence` / `IndentSize` | Option value types: `Limit::Disabled` (`-1`) or `Max(n)`; `required | forbidden | allowed`; 1-16. `IndentSpaces` is `Fixed(IndentSize)` or `Consistent`, `IndentSequences` is `Indented`, `NotIndented`, `Whatever` or `Consistent`; `CanonicalPath` is a resolved path newtype; `LocaleName` is the top-level `locale`. |
| `ConfigFile` | Loaded `.fast-yaml.yaml` with its `extends` chain resolved: rules, limits, `FileSelection { ignore, yaml_files }`; `ConfigFile::discover`, `load`, `into_parts`, `merge_cli_overrides`. |
| `ConfigFileError`, `TopLevelKey` | Load errors, including `Extended` (names the extending file and the cause), `ExtendsCycle`, `ExtendsTooDeep`, `NotRegularFile`, `TooLarge`; `TopLevelKey` is the closed set of the eight top-level keys (`IgnoreFromFile` and `Locale` included); `UnsupportedLocale` is the locale load error. |
| `SourceScan`, `ScanNeeds`, `FlowIndex` | The one loader pass of a lint run and what it collects; `ScanNeeds` is derived from the enabled rules, `FlowIndex` is built from the scan, `SourceScan::of_source` takes `ParseLimits`. |
| `Preset` | `Default`, `Relaxed`; `rules()` returns the yamllint-equivalent `RulesConfig`. |
| `ReportFormat`, `FileReport`, `ReportPath` | Rendering of `github`/`sarif`/`parsable`; `ReportPath` is an absolute path. |
| `LintError` | `InputTooLarge` and `ParseError` (parse errors include limit and scan-ahead violations). |

## 5. Edge cases

| Scenario | Expected behaviour |
|----------|--------------------|
| Empty file | No diagnostics, exit 0 (verified). |
| Multiple documents (`a: 1\n---\nb:   2\n`) | Locations are file-global (`d.yaml:4:3`). |
| BOM prefix | `stdin:1:3` for `\uFEFFa:   1` (column ignores BOM). |
| Info/hint in `parsable` | Printed as `[warning]`. |
| Several diagnostics at one position | Stable order from registry order (e.g. trailing-whitespace, empty-lines, indentation all at 2:1). |
| A rule's `ignore` matches the linted file | That rule does not run on it; other rules still do. |
| `-` stdin with per-rule `ignore` | Never ignored. |
| Config `ignore` matches the linted file | No output (`[]` in JSON, empty report in other formats), exit 0. |
| `extends` names a FIFO, a directory, an over-1 MiB file or a cycle | Load fails (exit 1) with `not a regular file`, `larger than 1048576 bytes` or an `extends` cycle error naming the files; a FIFO is never opened. |
| `fy lint -o a.yaml a.yaml` (also a hard link or symlink of an input) | Refused, exit 1, `a.yaml` untouched (verified). |
| `fy lint DIR 2>&1 \| head -1` | No panic, no error text; exit 2 when errors were found. |
| `.yamllint` in a linted directory | Linted as YAML like any `*.yaml`; it is not read as configuration. |
| Unknown anchor (`a: *x`) | Parse error: `syntax` diagnostic, exit 1. |
| Diagnostic on a line covered by `disable`, but rule code is `lint-directive` | Still reported. |
| `NUL` character in source | Rejected as a parse error (never silently truncates). |
| `-i` with lint | Rejected (FR-038). |

## 6. Rule catalog

All 25 registered rules (`RuleRegistry::with_default_rules`, config keys = codes). "None" column = fast-yaml options with no config (FR-016); "Default" = yamllint `extends: default` preset (enabled / severity). Severity column is the rule's built-in severity when no override exists. Every example below was run with `fy lint --no-config`.

### 6.1 Catalog

| Code | Severity | None | Default preset | Options (default with no config) |
|------|----------|------|----------------|---------|
| `duplicate-key` | error | on | on, error | `forbid-duplicated-merge-keys` (true; preset false) |
| `line-length` | info | on | on, error | `max` (80, `null` = unlimited), `allow-non-breakable-words` (true), `allow-non-breakable-inline-mappings` (false) |
| `trailing-whitespace` | hint | on | on, error | none |
| `document-start` | warning | on | on, warning | `present: required | forbidden | allowed` (allowed; preset required); checks every document |
| `document-end` | warning | on | off | `present` (allowed; preset required); checks every document |
| `empty-values` | warning | on | off | `forbid-in-block-mappings` / `-flow-mappings` / `-block-sequences` (all true); reported right after the colon or hyphen |
| `new-line-at-end-of-file` | info | on | on, error | none |
| `braces` | warning | on | on, error | `forbid: false | non-empty | all`, `min-spaces-inside` 0, `max-spaces-inside` 0, `min-spaces-inside-empty`, `max-spaces-inside-empty` (inherit) |
| `brackets` | warning | on | on, error | same as `braces` |
| `colons` | warning | on | on, error | `max-spaces-before` 0, `max-spaces-after` 1 |
| `commas` | warning | on | on, error | `max-spaces-before` 0, `min-spaces-after` 1, `max-spaces-after` 1 |
| `hyphens` | warning | on | on, error | `max-spaces-after` 1 |
| `comments` | info | on | on, warning | `require-starting-space` true, `ignore-shebangs` true, `min-spaces-from-content` 2 |
| `comments-indentation` | info | on | on, warning | none |
| `empty-lines` | info | on | on, error | `max` 2, `max-start` 0, `max-end` 0 |
| `new-lines` | warning | on | on, error | `type: unix | dos | platform` (unix) |
| `octal-values` | warning | on | off | `forbid-implicit-octal` true, `forbid-explicit-octal` true |
| `truthy` | warning | on | on, warning | `allowed-values` (`true`, `false`), `check-keys` (false; preset true) |
| `quoted-strings` | warning | on | off | `quote-type: any`, `single` or `double` (`consistent` unsupported), `required: always | only-when-needed | never` (yamllint `true`/`false` accepted; default only-when-needed), `extra-required`, `extra-allowed` (regex lists), `allow-quoted-quotes`, `check-keys` |
| `key-ordering` | info | on | off | `case-sensitive` true, `ignored-keys` (regex list, empty) |
| `float-values` | warning | on | off | `require-numeral-before-decimal` (true; preset false), `forbid-scientific-notation`, `forbid-nan`, `forbid-inf` (all false) |
| `invalid-anchor` | warning | on | on, error | `forbid-duplicated-anchors` (true), `forbid-unused-anchors` (false), `forbid-undeclared-aliases` (`true` only) |
| `indentation` | warning | on | on, error | `spaces` 1-16 or `consistent` (unset = `consistent`), `indent-size` (fixed width, fast-yaml key), `indent-sequences` `true`/`false`/`whatever`/`consistent` (true; relaxed preset `consistent`), `check-multi-line-strings` (false) |
| `set-values` | error | on | on, error | none (`!!set` members must not carry values) |
| `lint-directive` | warning | on | on, no override | none (bad inline directives, see FR-026) |

The `relaxed` preset overlays `default`: `braces`, `brackets` (max 1 space inside), `colons`, `commas`, `empty-lines`, `hyphens`, `indentation`, `line-length` (non-breakable inline mappings allowed) become warnings; `comments`, `comments-indentation`, `document-start`, `truthy` are disabled. A preset spells out every rule, enabled or not, with yamllint's option defaults. A spacing or line-count option set to `-1` (`Limit::Disabled`) skips that check.

### 6.2 Bad / good examples (verified)

| Code | Bad input and output | Good |
|------|---------------------|------|
| `duplicate-key` | `a: 1\nb: 2\na: 3\n` -> `3:1 [error] duplicate key 'a' (first defined at line 1)` | unique keys |
| `line-length` | 93-char `k: aaaa... bb` -> `1:1 line exceeds maximum length of 80 characters (current: 93)` | within limit, or one long word/URL |
| `trailing-whitespace` | `a: 1 \n` -> `1:5 trailing whitespace detected` | no trailing space |
| `document-start` | `a: 1\n` with `{present: required}` -> `1:1 missing document start marker '---'`; `---\na: 1` with `forbidden` -> `document start marker '---' is forbidden` | marker matches option |
| `document-end` | `---\na: 1\n` with `{present: required}` -> `3:1 missing document end marker '...'` | `...` present |
| `empty-values` | `a:\nb: 1\n` -> `1:3 empty value for key 'a'`; `- a\n-\n` -> `2:2 empty value in block sequence`; `a: {b: , c: 1}` -> `empty value for key 'b'` | `a: ~`, `a: ""` |
| `new-line-at-end-of-file` | `a: 1` (no newline) -> `1:5 no newline at end of file` | ends with `\n` |
| `braces` | `a: { b: 1 }\n` -> `1:4` and `1:11 too many spaces inside braces (expected at most 0, found 1)`; `{forbid: true}` + `a: {b: 1}` -> `flow mapping forbidden (forbid: all)` | `{b: 1}` |
| `brackets` | `a: [ 1, 2 ]\n` -> `too many spaces inside brackets` x2 | `[1, 2]` |
| `colons` | `a : 1\n` -> `1:3 too many spaces before colon (expected at most 0, found 1)`; `a:   1` -> `too many spaces after colon (expected at most 1, found 3)` | `a: 1` |
| `commas` | `a: [1,2]\n` -> `1:7 too few spaces after comma (expected at least 1, found 0)` | `[1, 2]` |
| `hyphens` | `a:\n-   1\n` -> `2:2 too many spaces after hyphen (expected at most 1, found 3)` | `- 1` |
| `comments` | `a: 1 # c\n#bad\n` -> `1:6 too few spaces before comment (expected at least 2, found 1)`, `2:1 comment should start with a space after '#'` | `a: 1  # c`, `# ok` |
| `comments-indentation` | `a:\n  b: 1\n    # c\n` -> `3:5 comment indentation does not match surrounding content (expected 2 spaces, found 4)` | comment aligned with next/previous content |
| `empty-lines` | three blank lines between keys -> `2:1 too many consecutive empty lines in document (expected at most 2, found 3)` | up to `max` |
| `new-lines` | `a: 1\r\nb: 2\r\n` -> `1:1 wrong line ending (expected Unix (\n), found DOS (\r\n))` per line | consistent configured style |
| `octal-values` | `a: 0755\nb: 0o7\n` -> `1:4 found implicit octal value '0755' (use quoted string or explicit '0o' prefix)`, `2:4 found explicit octal value '0o7' (...)` | `'0755'` quoted |
| `truthy` | `a: yes\nb: on\n` -> `1:4 found non-standard truthy value 'yes' (use true or false)` | `true`/`false`, or listed in `allowed-values` |
| `quoted-strings` | `a: x` with `{required: true}` (alias of `always`) -> `string should be quoted`; `a: 'x'` with `only-when-needed` -> `string does not need quotes` | consistent with option |
| `key-ordering` | `b: 1\na: 2\n` -> `2:1 key 'a' should be ordered before 'b' (line 1)`; with `ignored-keys: ['^name$']` a key `name` is skipped (verified) | sorted keys |
| `float-values` | `a: .5\n` -> `1:4 float value '.5' should have a numeral before the decimal point (e.g., '0.5')`; with flags: `NaN (not a number) is forbidden`, `Infinity is forbidden`, `scientific notation '1.0e3' is forbidden` | `0.5` |
| `invalid-anchor` | `a: &x 1\nb: &x 2\n` -> `2:4 anchor '&x' is defined multiple times; the earlier definition is shadowed (first defined at line 1)` | unique anchor names |
| `indentation` | `a:\n   b: 1\n` with `{spaces: 2}` -> `2:4 wrong indentation: expected 2 but found 3` | levels at the configured width |
| `set-values` | `!!set\n? a\n: 1\n` -> `2:3 [error] !!set member 'a' has a value; set members are keys only` | `? a` with null value |
| `lint-directive` | `a: 1 # fy: disable nonexistent\n` -> `unknown rule `nonexistent` in lint directive; `disable` and `enable` must be on their own line` | known rule names |

Code `undefined-alias` is a recognised name in directives but is not emitted (an undefined alias is a `syntax` error). Code `syntax` is produced only for failed inputs.

## 7. Non-functional requirements

| ID | Category | Requirement |
|----|----------|-------------|
| NFR-001 | Performance | Linting is linear in input size; text-scan rules do not re-scan quadratically; batch lint is bounded-memory (reorder window) regardless of file count. |
| NFR-006 | Memory | One loader pass per lint run, with no value tree (FR-009): the peak heap of linting a 3.9 MB block mapping fell from 98 to 49 MB (load: 57 MB) and a 3.2 MB block sequence of maps from 133 to 56 MB (load: 84 MB), while a root flow collection is unchanged at 1.02x load because the node index dominates; measured RSS of `fy lint` on a 4.17 MB spaced-ints root flow is about 1.11x `fy parse` (3.6x before); diagnostic-heavy input is about 1.7x (was 3.3x) because excerpts are lazy. Known worse cases: D-18. |
| NFR-002 | Determinism | Same input and config yield byte-identical output across runs and across `-j` values. |
| NFR-003 | Safety | No panic on any input within limits; hostile flow collections/anchors respect `max-scan-ahead`. |
| NFR-004 | Compatibility | yamllint rule names, `level`/`severity`, `extends`, `ignore`, `yaml-files` and `# yamllint` directives are accepted; known differences are documented (Preset docs, section 10). |
| NFR-005 | Type safety | Rule names, severities, option values are closed enums/newtypes; invalid config is rejected at load time, not at rule run time. |

## 8. Success criteria

| ID | Metric | Target |
|----|--------|--------|
| SC-001 | Exit-code contract (FR-037) covered by integration tests, one per code | 100% |
| SC-002 | SARIF output validates against the SARIF 2.1.0 schema in CI | pass (`tests/sarif_schema.rs`) |
| SC-003 | Parity subset with yamllint on shared fixtures | no unexplained divergence (`tests/yamllint_parity_tests.rs`) |
| SC-004 | Batch output identical for `-j 1` and `-j 8` on the same tree | byte-identical |
| SC-005 | Every built-in rule has a bad and a good test and an entry in section 6 | 25/25 |
| SC-006 | Lint heap peak on a 1 MiB root flow, all rules, default and `relaxed` options (`tests/lint_memory.rs`); lint output on `tests/fixtures/**` unchanged by refactors (`tests/golden_corpus.rs` snapshots) | <= 1.25x load peak; byte-identical |

## 9. Non-goals

- Auto-fix (`--in-place`): diagnostics are advisory; formatting is `fy format` ([[002-format/spec]]).
- `locale` other than C/POSIX/C.UTF-8 while `key-ordering` is enabled (rejected on load; code-point order only).
- JUnit and other formats beyond text, json, github, sarif, parsable.
- yamllint options not implemented: `quoted-strings.quote-type: consistent`, `indentation.spaces` above 16, `anchors.forbid-undeclared-aliases: false` (rejected with an error, never silently ignored).
- Schema validation (JSON Schema, Kubernetes, OpenAPI).

## 10. Agent boundaries

### Always
- Run `cargo nextest run -p fast-yaml-linter -p fast-yaml-cli` and the SARIF schema test after rule or formatter changes.
- Add a fixture and a bad/good test for every new rule or option, and a row in the catalog above.
- Keep config key names kebab-case and identical to rule codes.

### Ask first
- Changing a built-in rule's default severity or default option (breaking for users' CI).
- Adding a top-level config key or report format.
- Changing exit codes.

### Never
- Make a rule silently ignore an option it accepts (reject or implement).
- Let a directive suppress `syntax` or `lint-directive` diagnostics.
- Print machine-format output and human text on the same stream.

## 11. Open questions / Known deviations

> [!question] Items marked [NEEDS CLARIFICATION] need a product decision before they become requirements. Rows D-1 to D-17 were re-verified at e5e6cfb; D-18 and later at the #588 batch.

| # | Topic | Observed (verified) | Decision needed |
|---|-------|---------------------|-----------------|
| D-1 | Lint syntax-error exit code (GAP-CLI-017) | Single input exits 1, batch exits 2; clap usage errors also 2; `ExitCode::IoError`(3)/`InvalidArgs`(4) exist in code but are not used for lint. | [NEEDS CLARIFICATION: one code per class (findings / syntax / usage-IO) applied uniformly?] **Proposed:** distinct codes for findings, syntax error and usage/IO, applied uniformly, one integration test per code. |
| D-3 | `--format text` on syntax error | Text prints nothing on stdout; only the stderr error, exit 1. `json`, `github`, `sarif`, `parsable` print a `syntax` diagnostic (FR-035). | [NEEDS CLARIFICATION: emit a `syntax` diagnostic in `text` too?] |
| D-4 | Text format file name | Location line is always the literal `input:L:C`; the file name appears only as batch header. JSON has `file` only in multi-file mode. | [NEEDS CLARIFICATION: print path in single-file text, add `file` always] **Proposed:** paths as given on the command line; file name in every format. |
| D-5 | CI paths | `github` and `sarif` print absolute paths, which may not attach to PR files on runners. | [NEEDS CLARIFICATION: repo-relative paths when under CWD or git root?] **Proposed:** paths as given on the command line; file name in every format. |
| D-6 | No-config rule set | With no config all 25 rules run with fast-yaml defaults (e.g. `key-ordering`, `quoted-strings`, `octal-values`), unlike the yamllint `default` preset. | [NEEDS CLARIFICATION: is this the intended default, or should no-config equal `extends: default`?] **Proposed:** default preset closer to yamllint. |
| D-7 | `Linter::new()` / `Default` | Empty registry: `lint` always returns no diagnostics; `with_all_rules()` is the working constructor (`with_config` equals `with_all_rules_and_config`). | [NEEDS CLARIFICATION: make `new()` load rules or remove `Default`] **Proposed:** `new()` registers all rules. |
| D-9 | Text-scan rules vs yamllint | `octal-values` flags `0755` inside a block scalar, `indentation` flags block-scalar content (`a: \|\n   text`), `empty-lines` counts whitespace-only lines. | [NEEDS CLARIFICATION: move to event-based parity, or document as accepted] |
| D-10 | `--allow-duplicate-keys=false` | Only `true` acts; `false` does not re-enable a rule disabled in config. | minor; clarify flag semantics |
| D-11 | `braces`/`brackets` on empty collection | `a: {  }` reports twice (`1:4` and `1:7`) with "inside braces"; yamllint reports once, "inside empty braces". | minor |
| D-12 | Zero-width spans | GitHub `endColumn` equals `col` for a zero-width span (`colons`, `empty-values`); text output prints a one-column caret. | minor |
| D-13 | `using config file` message | Printed on stderr for every auto-discovery, also with `-q` and non-text formats. | [NEEDS CLARIFICATION: only with `--verbose`?] |
| D-14 | Config discovery root | Starts at CWD, not at the linted file's directory (matches yamllint). | confirm intended |
| D-15 | `undefined-alias` | Public constant and directive alias, never emitted. | remove or implement |
| D-16 | Binding formats | Python and Node.js `lint` expose text/json only; no github/sarif/parsable. | cross-surface decision |
| D-17 | Duplicate keys and `%YAML` versions | Core accepts duplicate keys and `%YAML 1.3/2.0` silently; the linter is the only reporter of duplicates | [NEEDS CLARIFICATION: add a strict mode in core, or keep lint as the sole reporter?] **Proposed:** opt-in strict loader; keep lint as the default reporter. |
| D-18 | Lint memory on diagnostic-heavy input | Excerpts are cut lazily (FR-009), so lint RSS on a minified flow (`[1,1,...]`, a `commas` diagnostic per scalar) fell from 3.3x to about 1.7x of parse, and on `[[],[],...]` from 741 to 564 MB; a root flow map with 227k `key-ordering` diagnostics is 1.7x (the remaining cost is the diagnostic struct and message). Wall time on flow-heavy input is 10-35% slower (positioned node index, indentation token port). | accept; profile the extra cost per token |
| D-21 | `quoted-strings` vs yamllint | `only-when-needed` follows yamllint's check (FR-068). Remaining differences: NEL handling in quoted scalars is approximated (only when the token has no escapes); `"\<LS>"` is a syntax error in fast-yaml but valid in PyYAML; `quote-type: consistent` is unsupported. | accept |
| D-23 | Config reader race | Reads open with `O_NONBLOCK` after a pre-open regular-file check and re-check the opened file, so a path swapped for a FIFO cannot block or be read unbounded; `O_NONBLOCK` stays set on the regular-file descriptor and `O_NOCTTY` is not passed. `extends` and `ignore-from-file` accept any absolute path and error text reveals whether it exists. An auto-discovered `.fast-yaml.yaml`/`.yamllint` that is a FIFO is skipped silently. | accept |
| D-25 | Untouched option keys keep fast-yaml defaults | A mapping or severity entry in a config file or a binding `rules` patch starts from yamllint's defaults (FR-014a), but a rule left out of the file keeps fast-yaml's no-config defaults, which differ from yamllint's (FR-016). Per-rule `ignore` in a binding patch is unsupported (no config directory). | accept |
| D-26 | Per-rule `ignore` matching | Patterns are anchored at the declaring config's directory and matched against the canonical path, so a symlink to an ignored file is ignored silently; yamllint matches the path as typed relative to the working directory. Bindings anchor at the process working directory. | accept |
| D-27 | `indentation` on documents saphyr rejects | A document saphyr-parser rejects but PyYAML scans (a quoted scalar or flow continuation below the key's indent) yields a syntax error and no indentation findings. | accept |
| D-28 | Line numbers after raw LS/PS/NEL | Only YAML 1.2 line breaks count, so line numbers after a raw U+2028/U+2029/U+0085 differ from yamllint. Case-insensitive `key-ordering` lowercases per character (final sigma is not special-cased). | accept |
| D-29 | Unsupported yamllint values | `quoted-strings.quote-type: consistent` (yamllint >= 1.35) and `indentation.spaces` above 16 are rejected; locale `c`/`C.utf8` is accepted where yamllint crashes. | follow-up issue |
| D-30 | Option precedence in bindings | CLI `--indent-size` overrides `rules.indentation.spaces` from the config; in the bindings the `rules` patch is applied last and wins over `indent_size`/`indentSize`. | [NEEDS CLARIFICATION: one winner on every surface?] |
| D-32 | Syntax diagnostics have no excerpt | `syntax` and input diagnostics use `Excerpt::Omitted`, so JSON carries no `context` for them (and no `file` key outside batch mode). | accept |
| D-33 | Minor yamllint differences | `colons` "too many spaces after colon" reports column 3 where yamllint reports 5; `[a: ]` is flagged by `empty-values` and `{b, a}` by `key-ordering` where yamllint is silent; a single-file JSON array ends without a trailing newline. | follow-up issue |
| D-34 | Lint wall time | Release lint is 10-35% slower on flow-heavy input (minified 2.4 MB: +24% text, +35% JSON) and flat on block input, in exchange for 26-48% lower RSS. | accept; profile |

Resolved and removed: D-2 (`fy lint -o` writes the report, FR-050), D-31 and X-2 (`format -o` and `convert -o` refuse an input file like `lint -o`, [[006-cli-contract/spec]] FR-043), the `## ok` and `hyphens` items of D-9 (FR-073), and with #588 D-8 (`forbid-in-block-sequences` is implemented, FR-070), D-19 (`key-ordering` covers nested flow and explicit keys, FR-069), D-20 (`empty-values` columns match yamllint, FR-070) D-24 (the `Preset` rustdoc divergence list was refreshed) and D-22 (yamllint rule names are accepted in `rules:`, FR-014). #374 is out of scope; #540 (merge-key validation cost) did not reproduce, so the core change was reverted.

## 12. See also

- [[plan]] — technical plan (existing implementation)
- [[constitution]] — principles
- [[001-parse-validate/spec]] — parse errors and limits behind `syntax` diagnostics
- [[005-batch-parallel/spec]] — discovery and parallel runner
- [[006-cli-contract/spec]] — global flags, exit-code table
