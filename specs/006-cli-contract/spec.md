---
aliases:
  - CLI contract
  - fy CLI
tags:
  - sdd
  - spec
  - cli
created: 2026-10-01
status: reverse-specified
related:
  - "[[constitution]]"
  - "[[005-batch-parallel/spec|Batch and parallel]]"
---

# Feature: `fy` CLI contract

> [!info] Metadata
> **Scope**: crate `fast-yaml-cli`, binary `fy`, cross-command behaviour (flags, input and output conventions, exit codes, channels, feature flags). Per-command semantics live in the parse, format, convert and lint specs; batch discovery lives in [[005-batch-parallel/spec|005]].
> **Baseline**: v0.6.6 on `main` (e5e6cfb), verified with `target/debug/fy`.

## 1. Purpose and value

`fy` is the scriptable face of fast-yaml. Its value is a **predictable contract**: the same flag means the same thing everywhere, results go to stdout, diagnostics go to stderr, and exit codes let CI and shell scripts branch without parsing text. This spec fixes that contract so the four subcommands (`parse`, `format`, `convert`, `lint`) behave as one tool.

### Out of scope (non-goals)

- Interactive prompts, a TUI, shell completion generation, a daemon or LSP mode.
- Environment-variable configuration other than `NO_COLOR` (no `FY_*`, no `RUST_LOG`).
- A `validate` subcommand (the validation command is `parse`).
- Auto-fix in `lint` (`-i` is rejected).

## 2. User stories

### US-001: Validate YAML from a script (P1)
AS A script author I WANT a stable exit code and quiet success SO THAT I can branch on it.

```
GIVEN ok.yaml contains "a: 1"
WHEN  I run `fy parse ok.yaml`
THEN  stdout is "✓ YAML is valid" and the exit code is 0

GIVEN bad.yaml contains "k: [1"
WHEN  I run `fy parse bad.yaml`
THEN  stderr is
      error: Failed to parse YAML
        caused by[0] YAML syntax error: while parsing a flow sequence, expected ',' or ']' at line 2, column 1
AND   stdout is empty and the exit code is 1
```

### US-002: Pipe through stdin and stdout (P1)
AS A user I WANT filters to compose SO THAT `fy` fits in pipelines.

```
WHEN  I run `printf 'b:   1\n' | fy format`
THEN  stdout is "b: 1" and the exit code is 0
WHEN  I run `printf 'a:   1\n' | fy`        (no subcommand)
THEN  the input is formatted to stdout exactly as `fy format` does
WHEN  I run `echo '{"b":1,"a":[1,2]}' | fy convert yaml`
THEN  stdout is "a:\n  - 1\n  - 2\nb: 1"
```

### US-003: Gate CI on formatting (P1)
AS A CI author I WANT a distinct exit code for "would change" SO THAT a formatting check is not confused with a crash.

```
GIVEN d/a.yaml contains "b:   1\na: 2"
WHEN  I run `fy format -n d/a.yaml`
THEN  stderr shows "Completed: 1 file in <t>ms" and "  1 would change", and the exit code is 5
WHEN  the file is already formatted
THEN  the exit code is 0
```

### US-004: Lint with a severity-aware exit code (P1)
AS A CI author I WANT lint to fail only on errors SO THAT warnings do not break the build.

```
GIVEN w.yaml contains "a: yes\nb:  1" (two warnings: truthy, colons)
WHEN  I run `fy lint w.yaml`
THEN  stdout ends with "0 errors, 2 warnings" and the exit code is 0

GIVEN dup.yaml contains "a: 1\na: 2"
WHEN  I run `fy lint dup.yaml`
THEN  stdout shows "error[duplicate-key]: duplicate key 'a' (first defined at line 1)" and "1 errors, 0 warnings"
AND   the exit code is 2
WHEN  I add -q
THEN  warnings are hidden, errors are still printed, exit code unchanged
```

### US-005: Rewrite a file safely in place (P2)
AS A developer I WANT `-i` to replace a file atomically SO THAT a crash never leaves a half-written file.

```
GIVEN m.yaml contains "a:   1"
WHEN  I run `fy format -i m.yaml`
THEN  nothing is printed, m.yaml now reads "a: 1", exit code 0
WHEN  I run `echo 'a: 1' | fy format -i`
THEN  stderr is "error: --in-place (-i) requires a file argument", exit code 1
WHEN  I run `fy lint -i w.yaml`
THEN  stderr is "error: --in-place is not supported by `fy lint` (auto-fix is not implemented)", exit code 1
```

### US-006: Bound resources from the command line (P2)
AS AN operator I WANT size flags SO THAT untrusted input cannot exhaust memory.

```
WHEN  I run `fy --max-input-bytes 3 parse ok.yaml`
THEN  stderr is
      error: Failed to read file: ok.yaml
        caused by[0] input size 5 bytes exceeds maximum allowed 3 bytes
        hint: raise with --max-input-bytes or the max-input-bytes config key
AND   the exit code is 1
WHEN  I run `fy parse --max-input-bytes 1.5MiB ok.yaml`
THEN  clap rejects the value (integer sizes only), exit code 2
```

### US-007: Machine-readable lint output (P2)
AS A CI author I WANT `--format github|sarif|parsable|json` SO THAT results integrate with annotations and code scanning (details in the lint spec).

```
WHEN  I run `fy lint --format github w.yaml`
THEN  stdout has `::warning file=<abs>/w.yaml,line=1,col=4,endLine=1,endColumn=7,title=truthy::found non-standard truthy value 'yes' (use true or false)`
```

## 3. Functional requirements

### 3.1 Invocation

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-001 | THE SYSTEM SHALL accept `fy [GLOBAL-OPTIONS] [SUBCOMMAND [OPTIONS] [ARGS]]` with subcommands `parse`, `format`, `convert`, `lint` (`lint` only with feature `linter`), plus clap's `help`. | must |
| FR-002 | WHEN no subcommand is given, THE SYSTEM SHALL format stdin to stdout with `CommentPolicy::Reject`, as `fy format` would. | must |
| FR-003 | WHEN an unknown subcommand is given (including `validate`), THE SYSTEM SHALL exit 2 with clap's message and a similar-subcommand tip. | must |
| FR-004 | THE SYSTEM SHALL print `fy <version>` for `-V/--version` and help for `-h/--help` and `fy help <sub>`, exit 0. | must |
| FR-005 | THE SYSTEM SHALL accept `-i`, `-o`, `--no-color`, `-q`, `-v`, `--max-input-bytes`, `--max-scan-ahead` before or after the subcommand (global flags). | must |
| FR-006 | WHEN both `--quiet` and `--verbose` are present anywhere in argv, THE SYSTEM SHALL exit 2 with `the argument '--quiet' cannot be used with '--verbose'`. | must |

### 3.2 Which global flag applies to which command

| Flag | parse | format | convert | lint |
|------|-------|--------|---------|------|
| `-i/--in-place` | ignored | rewrite (needs a file; atomic) | rewrite file | error: not supported (exit 1) |
| `-o/--output FILE` | ignored | single input or stdin: write there; conflicts with `-n` | write there | ignored |
| `-q` | no output on success | hides summary unless a file failed | no effect | only error-severity diagnostics shown |
| `-v` | timing line on stderr | no effect | no effect | `File:` and `Lint time:` on stderr (not with `--format json`) |
| `--no-color` | yes | yes | yes | yes |
| `--max-input-bytes` | yes | yes | yes | yes (overrides config key) |
| `--max-scan-ahead` | yes | yes | YAML input only | yes (overrides config key) |

`-f/--format yaml|json|compact` is declared only at top level (`fy -f json parse x` parses, `fy parse x -f json` is a usage error) and has no effect (see section 8).

### 3.3 Input and output conventions

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-010 | WHEN a command accepts a file and none is given, THE SYSTEM SHALL read stdin; invalid UTF-8 SHALL fail with `input is not valid UTF-8` (exit 1). | must |
| FR-011 | WHEN `-o` is `-`, `/dev/stdout` or `/dev/fd/1`, THE SYSTEM SHALL write to stdout; `/dev/stderr` or `/dev/fd/2` to stderr; any other value SHALL be written atomically via `write_atomic`. | must |
| FR-012 | WHEN `-i` is given without a single file argument (stdin input), THE SYSTEM SHALL fail with `--in-place (-i) requires a file argument` (exit 1). | must |
| FR-013 | WHEN `format -i` produces content identical to the file, THE SYSTEM SHALL NOT rewrite the file. | must |
| FR-014 | THE SYSTEM SHALL write command results (formatted YAML/JSON, `✓ YAML is valid`, `--stats`, lint reports) to stdout and errors, summaries, timing, discovery warnings and the `using config file:` notice to stderr. | must |
| FR-015 | WHEN `fy convert` runs, THE SYSTEM SHALL require `<TO>` = `yaml` or `json` (missing: exit 2) and SHALL accept `--pretty=false` for compact JSON; YAML input with several documents SHALL become a JSON array. | must |
| FR-016 | WHEN color is enabled (stderr is a TTY, `NO_COLOR` is unset, `--no-color` absent, feature `colors` on), THE SYSTEM SHALL color errors and lint output; the `NO_COLOR` variable with any value (even empty) SHALL disable it. | must |

### 3.4 Exit codes

| Code | Meaning | Produced by |
|------|---------|-------------|
| 0 | Success; nothing to change; lint found no error-severity diagnostic; empty `--stdin-files` list | all |
| 1 | Runtime failure: parse/format/convert failure, I/O, decode, size limit, config load, discovery error, `-i` misuse, single-file or stdin lint syntax error, any failed file in a `format` batch | all |
| 2 | clap usage error (bad flag/value, conflicts, missing `<TO>`) **or** lint found at least one error-severity diagnostic **or** a lint batch had any unreadable/syntax-failing file | clap, lint |
| 5 | `format -n` and at least one file would change and none failed | format |

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-020 | THE SYSTEM SHALL use exit codes exactly as the table above states. | must |
| FR-021 | WHEN a batch contains failures and would-change files, THE SYSTEM SHALL prefer 1 over 5. | must |
| FR-022 | THE SYSTEM SHALL emit errors as `error: <message>` followed by `  caused by[i] <cause>` lines and, when the failing limit can be raised, `  hint: raise with <flag>`. Hints exist for `--max-depth`, `--max-alias-bytes`, `--max-input-bytes` (or config key) and `--max-scan-ahead` (or config key). | must |

### 3.5 Limit flags

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-030 | THE SYSTEM SHALL parse `--max-input-bytes` (1 to 1 GiB, default 100 MiB) and `--max-scan-ahead` (1 to 1 GiB, default 4 MiB) as `<digits>[KiB|MiB|GiB]` (case-sensitive suffix, integers only) into typed newtypes (`MaxInputBytes`, `MaxScanAhead`); out-of-range, fractional, negative or overflowing values SHALL be usage errors (exit 2). | must |
| FR-031 | THE SYSTEM SHALL also expose `--max-depth` (1 to 512, default 256) on parse, format, convert and lint, and `--max-alias-bytes` (1 to 1 GiB, default 64 MiB) on parse, convert and lint. | must |
| FR-032 | WHEN a limit flag and a lint config key both set a limit, THE SYSTEM SHALL let the flag win. | must |

### 3.6 Cargo features

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-040 | THE SYSTEM SHALL ship features `colors` (default; without it output is never colored), `linter` (default; adds `lint`, config files and report formats), `arena` (default; core arena backend) and `all`. | must |
| FR-041 | THE SYSTEM SHALL compile with `--no-default-features` (a `parse`/`format`/`convert`-only binary). | should |

## 4. Key entities and types

| Entity | Description |
|--------|-------------|
| `Cli`, `Command` | clap derive structs; global flags live on `Cli`. |
| `ExitCode` | `Success=0`, `ParseError=1`, `LintErrors=2`, `IoError=3` and `InvalidArgs=4` (defined, never produced), `WouldChange=5`. |
| `Verbosity` | `Quiet`, `Normal`, `Verbose`, resolved from `-q/-v` after parsing. |
| `Target` | `Stdin`, `File`, `Batch` resolved from paths and flags. |
| `EditIntent`, `WriteMode` | Encode `-n`, `-i`, `-o` precedence for format as a type, not booleans. |
| `InputSource`, `OutputWriter` | File or stdin input; stdout/stderr/atomic-file output. |
| `MaxInputBytes`, `MaxScanAhead`, `MaxDepth`, `MaxAliasBytes`, `Indent`, `Width` | Range-checked newtypes from `fast-yaml-core::limits`. |
| `PathError`, `RaiseHint` | Discovery errors and limit hints rendered by `format_error`. |

## 5. Edge cases

| Scenario | Expected behaviour |
|----------|--------------------|
| `fy parse -` | `-` is a file name, not stdin: `failed to read '-': No such file or directory`, exit 1. |
| `fy parse -i ok.yaml` | Flag ignored, validation succeeds, exit 0. |
| `fy parse -o out.txt ok.yaml` | `-o` ignored: no file is created. |
| `fy lint -o out.txt w.yaml` | Report on stdout, no file created. |
| `fy format -n -o x f.yaml` | Usage error: `--dry-run` cannot be used with `--output`, exit 2. |
| `fy format -i -n m.yaml` | `-n` wins; nothing written. |
| `fy convert json -i f.yaml` | Replaces `f.yaml` with JSON under the same name; `-i` silently beats `-o`. |
| `fy format d/a.yaml` where file has comments | `file contains YAML comments that formatting would strip; use --strip-comments to allow this`, exit 1. |
| `fy lint --config nope.yml ok.yaml` | `failed to load config file 'nope.yml'`, exit 1. |
| `fy lint --max-line-length 0` | Usage error (non-zero type), exit 2. |
| `fy lint bad.yaml` (syntax error, single file) | `error: Failed to lint YAML ...`, exit 1; nothing on stdout, even with `--format json`. |
| Empty input, `~`, comment-only | `parse` succeeds. |
| Config `ignore` matches the single linted file | File is not read, empty report, exit 0. |
| Discovered `.fast-yaml.yaml` | `using config file: <path>` printed to stderr on every lint run, even with `-q`. |

## 6. Success criteria

| ID | Metric | Target |
|----|--------|--------|
| SC-001 | Each exit code in 3.4 has an integration test | 100% of codes produced |
| SC-002 | Machine-readable stdout is never mixed with progress or summary text | 0 violations |
| SC-003 | Any rewrite leaves either the old or the new content, never a partial file | 100% |
| SC-004 | Every value-taking limit flag rejects out-of-range input at parse time | 100% |
| SC-005 | `fy --help` and `fy <sub> --help` describe every accepted flag and its effect | 100% |

## 7. Agent boundaries

### Always (without asking)
- Add or update an integration test in `crates/fast-yaml-cli/tests/` for every CLI behaviour change; update help text.
- Keep stdout for results and stderr for diagnostics.

### Ask first
- Any change to exit codes, flag names, defaults or the summary layout.
- Adding a global flag or an environment variable.

### Never
- Print results to stderr or progress to stdout.
- Write a target file non-atomically.
- Introduce booleans for mutually exclusive modes where an enum fits.

## 8. Open questions / Known deviations

| # | Topic | Observed (verified) | Question |
|---|-------|--------------------|----------|
| 1 | Lint syntax-error exit code (P1, GAP-CLI-017) | Single file/stdin: exit 1. The same file in a batch: exit 2. | [NEEDS CLARIFICATION] One code for all lint syntax errors (2, matching findings) or a separate code? **Proposed:** distinct codes for findings, syntax error and usage/IO, applied uniformly, one integration test per code. |
| 2 | Unused codes 3 and 4 (P2, GAP-CLI-018) | `IoError` and `InvalidArgs` exist in the enum but no path emits them; 2 is shared by usage errors and lint findings. | [NEEDS CLARIFICATION] Implement a distinct usage/IO code or delete 3/4 from enum and docs? **Proposed:** distinct codes for findings, syntax error and usage/IO, applied uniformly, one integration test per code. |
| 3 | Top-level `-f/--format` is dead (P2, GAP-CLI-001) | Accepted, never read; not global. | Remove (pre-1.0, allowed) or implement JSON output for `parse`? |
| 4 | `-o` ignored by `parse` and `lint`, `-i` ignored by `parse` (P2, GAP-CLI-005) | No error, no file. `-i` beats `-o` in convert; `-n` beats `-i` in format (GAP-CLI-007). | [NEEDS CLARIFICATION] Reject with usage errors, or implement `-o` for lint (SARIF to file)? **Proposed:** reject unsupported flags per command with a usage error. |
| 5 | `-` is not stdin (P3, GAP-CLI-008) | `fy parse -` fails with ENOENT although `-o -` means stdout. | Accept `-` as stdin? |
| 6 | `--format json` lint hides syntax failures (P2, GAP-CLI-011) | Failure is stderr-only; stdout empty or `[]`. | Emit a diagnostic object like SARIF does? |
| 7 | `using config file:` always on stderr (P3, GAP-CLI-020) | Printed even with `-q` and machine formats. | Print only with `-v`? |
| 8 | Single-file text lint labels location `input:L:C` (P3, GAP-CLI-015) | Batch prints `<path>:` header; single file does not show the path. | Show the file name in single mode? **Proposed:** paths as given on the command line; file name in every format. |
| 9 | `-v` has no effect on `format`, README says it lists files (P3, GAP-CLI-004) | Verified: only the standard summary. | Implement per-file lines or fix docs. |
| 10 | Broken pipe (P3, GAP-CLI-013) | `fy format big \| head -1` prints `error: Failed to write to stdout`, exit 1. | Treat EPIPE as quiet success? |
| 11 | Cause chain repeats text (P4, GAP-CLI-014) | `caused by[0]` and `[1]` are identical for I/O errors. | Dedupe in `format_error`. |
| 12 | Doc drift (P3, GAP-CLI-002/003/006/021) | Crate README synopsis, `skills/fast-yaml-cli/SKILL.md` (key sorting, `--indent` range, `convert -i` renaming, lint formats) disagree with behaviour. | Refresh docs from this spec. |
| 13 | Single explicit non-YAML file accepted (P4, GAP-CLI-026) | `fy format -n a.txt` works; in a batch it errors on include patterns. | Make consistent? |
| 14 | `--width` | Validated by `fy format` but never applied; help promises wrapping | [NEEDS CLARIFICATION: implement or remove] **Proposed:** remove the option on every surface (pre-1.0 breaking change allowed); wrapping risks round-trip fidelity. |

## 9. See also

- [[005-batch-parallel/spec|Batch and parallel processing]] - discovery, `-j`, summaries
- [[constitution]] - principles and spec map
