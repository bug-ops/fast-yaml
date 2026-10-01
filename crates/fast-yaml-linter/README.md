# fast-yaml-linter

[![Crates.io](https://img.shields.io/crates/v/fast-yaml-linter)](https://crates.io/crates/fast-yaml-linter)
[![docs.rs](https://img.shields.io/docsrs/fast-yaml-linter)](https://docs.rs/fast-yaml-linter)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue)](../../LICENSE-MIT)

YAML linter with rich diagnostics for the fast-yaml ecosystem.

> [!NOTE]
> This crate provides two distinct components: **Linter** (validates YAML against rules) and **Diagnostic Formatters** (render diagnostics for display).

## Components

### Linter

**Purpose**: Validate YAML against configurable rules and generate diagnostics.

**Data flow**: `YAML text → Vec<Diagnostic>`

**Built-in rules** (21 total):
- `duplicate-key` — Detect duplicate keys in mappings
- `line-length` — Enforce maximum line length
- `trailing-whitespace` — Detect trailing whitespace
- And 18 more...

### Diagnostic Formatters

**Purpose**: Convert diagnostics into human-readable or machine-readable formats.

**Data flow**: `Vec<Diagnostic> → Formatted output`

**Available formatters**:
- **TextFormatter** — rustc-style output with colors
- **JsonFormatter** — JSON format for IDE/CI integration
- **ReportFormat** — CI reports naming each file: GitHub Actions annotations, yamllint-style `parsable` lines and SARIF 2.1.0

> [!TIP]
> **Linter vs Formatter**: Linter validates YAML (what's wrong), Formatters display results (how to show it).

## Features

- **Precise error locations**: Line, column, and byte offset tracking
- **Rich diagnostics**: Source context with highlighting
- **Pluggable rules**: Extensible rule system
- **Multiple output formats**: Text (rustc-style), JSON, GitHub annotations, parsable, SARIF
- **No value tree by default**: linting reads one guarded parser pass and builds documents only for custom `DocumentRule`s

## Rust Usage

### Complete Pipeline: YAML → Linter → Diagnostics → Formatter → Output

```rust
use fast_yaml_linter::formatter::Findings;
use fast_yaml_linter::{Linter, TextFormatter, Formatter};

let yaml = r#"
name: John
age: 30
name: duplicate  # Error: duplicate key
"#;

// Step 1: Create linter with rules
let linter = Linter::with_all_rules();

// Step 2: Run linter (YAML → Vec<Diagnostic>)
let source = linter.source(yaml)?;
let diagnostics = linter.lint_source(&source)?;

// Step 3: Format diagnostics (excerpts are cut from the source when printed)
let context = source.context();
let formatter = TextFormatter::with_color_auto();
let output = formatter.format(Findings::FromSource { diagnostics: &diagnostics, source: &context });

// Step 4: Display output
println!("{}", output);
// Output:
// error[duplicate-key]: duplicate key 'name' found
//   --> input:4:1
//    |
//  4 | name: duplicate
//    | ^^^^ duplicate key defined here
# Ok::<(), Box<dyn std::error::Error>>(())
```

### Custom Rules

A rule is metadata (`LintRule`) plus either a `SourceRule`, which reads the source text and the
scan products, or a `DocumentRule`, which walks the value tree of each document. Linting builds
the documents only when an enabled `DocumentRule` is registered.

```rust
use fast_yaml_linter::config::CustomRuleCode;
use fast_yaml_linter::rules::{LintRule, Rule, RuleId, SourceRule};
use fast_yaml_linter::{Diagnostic, LintConfig, LintContext, Linter, Severity};

struct NoTodo(CustomRuleCode);

impl LintRule for NoTodo {
    fn id(&self) -> RuleId<'_> {
        RuleId::Custom(&self.0)
    }
    fn name(&self) -> &str {
        "No TODO"
    }
    fn description(&self) -> &str {
        "Reports TODO markers"
    }
    fn default_severity(&self) -> Severity {
        Severity::Info
    }
}

impl SourceRule for NoTodo {
    fn check(&self, _context: &LintContext, _config: &LintConfig) -> Vec<Diagnostic> {
        Vec::new()
    }
}

let mut linter = Linter::with_all_rules();
linter.add_rule(Rule::Source(Box::new(NoTodo(CustomRuleCode::new("no-todo")?))))?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Python Usage

> [!NOTE]
> Python bindings are available through the `fastyaml-rs` package on PyPI.

```python
from fast_yaml._core.lint import lint, Linter, LintConfig, TextFormatter, Severity

# Quick lint
diagnostics = lint("key: value\nkey: duplicate")

for diag in diagnostics:
    print(f"{diag.severity}: {diag.message}")
    print(f"  at line {diag.span.start.line}, column {diag.span.start.column}")

# Custom configuration
config = LintConfig(
    max_line_length=120,
    indent_size=2,
    allow_duplicate_keys=False,
)
linter = Linter(config)
diagnostics = linter.lint(yaml_source)

# Format output
formatter = TextFormatter(use_colors=True)
print(formatter.format(diagnostics, yaml_source))

# Access severity levels
Severity.ERROR    # Critical errors
Severity.WARNING  # Potential issues
Severity.INFO     # Informational
Severity.HINT     # Suggestions
```

## Built-in Rules

The linter includes 21+ rules covering syntax, style, and best practices:

**Document Structure:**
- `document-start` — Enforce `---` document start marker
- `document-end` — Enforce `...` document end marker
- `new-line-at-end-of-file` — Require newline at EOF

**Keys and Values:**
- `duplicate-keys` — Detect duplicate keys (ERROR)
- `empty-values` — Flag empty values
- `key-ordering` — Enforce alphabetical key ordering

**Formatting:**
- `line-length` — Enforce maximum line length
- `indentation` — Check consistent indentation
- `trailing-whitespace` — Detect trailing whitespace
- `empty-lines` — Control empty line usage
- `new-lines` — Enforce newline rules

**Flow Collections:**
- `braces` — Brace spacing in flow mappings `{a: 1}`
- `brackets` — Bracket spacing in flow sequences `[1, 2]`
- `commas` — Comma placement and spacing
- `colons` — Colon spacing after keys

**Values:**
- `truthy` — Detect ambiguous boolean values (`yes`/`no`)
- `quoted-strings` — Enforce string quoting style
- `float-values` — Validate float formatting
- `octal-values` — Detect octal notation
- `set-values` — `!!set` members must not have values (default error)

**Comments:**
- `comments` — Comment formatting rules
- `comments-indentation` — Comment indentation

**Anchors & Aliases:**
- `invalid-anchors` — Validate anchor/alias usage

**Directives:**
- `lint-directive` — Invalid inline directive (unknown rule or verb, misplaced `disable-file`); config-only, cannot be suppressed by a directive

> [!NOTE]
> All rules are configurable. Disable specific rules via `LintConfig::with_disabled_rule(RuleName::LineLength)`.

## Inline Directives

`Linter::lint` honors suppression comments in the YAML source:

```yaml
# fy: disable-file
```

`disable-file` suppresses the whole file. It must be an own-line comment before any YAML content; blank lines, other comments and `%YAML`/`%TAG` lines may precede it, `---` may not. Block and line scopes:

```yaml
# fy: disable line-length trailing-whitespace
long: value
# fy: enable line-length
a: 1
a: 2  # fy: disable-line duplicate-key
# fy: disable-line duplicate-key
a: 3
```

- Verbs: `disable`, `enable`, `disable-line`, `disable-file`; without rule names they apply to all rules. The `rule:` prefix on names is optional.
- `# yamllint ...` is accepted as an alias; `key-duplicates`, `anchors` and `trailing-spaces` map to `duplicate-key`, `invalid-anchor` + `undefined-alias` and `trailing-whitespace`.
- A block `disable` carries across `---` into following documents.
- Unknown rules, unknown `fy:` verbs, malformed names, trailing text and misplaced `disable-file` produce one `lint-directive` diagnostic per directive comment. Unknown names never widen a directive to all rules.
- `disable` and `enable` must be own-line comments; inline ones are rejected with a warning.
- Matching is by the line where a diagnostic's span starts; syntax errors are never suppressed.

## Configuration

Every built-in rule has typed options. Unknown rules, unknown option keys, wrong value types and
invalid severities are rejected with an error that names the rule and the option.

### Rust

```rust
use std::num::NonZeroUsize;
use fast_yaml_linter::{Linter, LintConfig};
use fast_yaml_linter::config::{IndentSize, RuleName};

let config = LintConfig::new()
    .with_max_line_length(NonZeroUsize::new(120))
    .with_indent_size(IndentSize::try_from(4u64).unwrap())
    .with_disabled_rule(RuleName::KeyOrdering);

let linter = Linter::with_config(config);
```

Rules can also be configured from a YAML mapping, the same form used by `.fast-yaml.yaml`:

```rust
use fast_yaml_linter::config::RulesConfig;

let mut rules = RulesConfig::default();
rules
    .apply(serde_norway::Deserializer::from_str(
        "line-length: {max: 120}\nquoted-strings: {quote-type: single}\nkey-ordering: disable",
    ))
    .unwrap();
```

### Config file

```yaml
rules:
  line-length: {max: 120}          # `max: ~` removes the limit
  document-start: {present: true}  # true | false | required | forbidden | allowed
  quoted-strings:
    quote-type: single             # any | single | double
    required: only-when-needed     # true | false | always | not-required | only-when-needed | never
  braces: {forbid: non-empty}      # false | true | non-empty | all
  key-ordering: disable            # shorthand for `enabled: false`
  comments: warning                # shorthand for `severity: warning`
```

An entry is `null` (no change), a severity (`error`, `warning`, `info`, `hint`, any case),
`enable`, `disable`, or a mapping with `enabled`, `severity` (also spelled `level`, as in yamllint;
not both) and the options below. Options
an entry does not mention keep their current values. A limit of `-1` disables that check.

| Rule | Options (default) |
|------|-------------------|
| `braces`, `brackets` | `forbid` (false), `min-spaces-inside` (0), `max-spaces-inside` (0), `min-spaces-inside-empty` (-1, inherit), `max-spaces-inside-empty` (-1, inherit) |
| `colons` | `max-spaces-before` (0), `max-spaces-after` (1) |
| `commas` | `max-spaces-before` (0), `min-spaces-after` (1), `max-spaces-after` (1) |
| `hyphens` | `max-spaces-after` (1) |
| `comments` | `require-starting-space` (true), `ignore-shebangs` (true), `min-spaces-from-content` (2) |
| `document-start` | `present` (allowed; `true`/`required` flags every document without `---`, `false`/`forbidden` flags every `---`, also one after a `%` directive, `allowed`; a source with no document reports nothing) |
| `document-end` | `present` (allowed; `true`/`required` flags every document not closed by `...`, `false`/`forbidden` flags every `...` at column 0) |
| `empty-lines` | `max` (2), `max-start` (0), `max-end` (0) |
| `empty-values` | `forbid-in-block-mappings` (true), `forbid-in-flow-mappings` (true), `forbid-in-block-sequences` (true) |
| `float-values` | `require-numeral-before-decimal` (true), `forbid-scientific-notation` (false), `forbid-nan` (false), `forbid-inf` (false) |
| `indentation` | `indent-size` (2, 1 to 16) |
| `key-ordering` | `case-sensitive` (true), `ignored-keys` ([] regexes, `re.search` semantics; matching keys are skipped) |
| `line-length` | `max` (80, `null` for no limit), `allow-non-breakable-words` (true), `allow-non-breakable-inline-mappings` (false; implies the previous one) |
| `new-lines` | `type` (unix; unix, dos or platform) |
| `octal-values` | `forbid-implicit-octal` (true), `forbid-explicit-octal` (true) |
| `quoted-strings` | `quote-type` (any), `required` (only-when-needed), `extra-required` ([] regexes; not with `always` or `never`), `extra-allowed` ([] regexes; only with `only-when-needed`), `allow-quoted-quotes` (false), `check-keys` (false: keys are skipped); scalars with a `!!` core tag are skipped; see "Regular expressions" below |
| `truthy` | `allowed-values` (['true', 'false'], quoted; `y`/`n` are not truthy spellings), `check-keys` (false) |
| `duplicate-key` | `forbid-duplicated-merge-keys` (true; the yamllint presets set false). Keys are equal when their resolved values are, so `99` and `+99` collide and `"1"` and `1` do not (yamllint compares the text) |
| `invalid-anchor` | `forbid-duplicated-anchors` (true; yamllint's default is false), `forbid-unused-anchors` (false), `forbid-undeclared-aliases` (only `true`: an undeclared alias is always a parse error) |
| `set-values`, `trailing-whitespace`, `new-line-at-end-of-file`, `comments-indentation` | none |

`min-spaces-inside-empty` and `max-spaces-inside-empty` override the non-empty limits for empty
collections independently of each other. A minimum above the maximum is not rejected; as in
yamllint it flags every collection.

Options that yamllint has but fast-yaml does not implement (for example
`indentation.spaces`) are rejected explicitly instead of being ignored.

#### Regular expressions

`quoted-strings.extra-required` and `extra-allowed` take regular expressions matched with
Python `re.search` semantics (anywhere in the value) against string scalars that are not keys:

- a plain scalar matching `extra-required` is reported as "string should be quoted" under
  `required: false` and `only-when-needed`;
- under `only-when-needed`, quotes that are not needed are kept when the value matches
  `extra-required` or `extra-allowed`, and a plain scalar matching only `extra-allowed` stays unquoted (one also matching `extra-required` is reported).

The syntax is Rust regex, not Python `re`: look-around and backreferences are unsupported, `\Z`
is written `\z`, and `$` does not match before a trailing newline. Each option holds at most 64
patterns of at most 256 bytes, and a pattern that compiles to more than 10 MiB is rejected.
Errors name the rule, the option and the pattern index.

#### Config file (yamllint compatibility)

```yaml
extends: relaxed                 # default | relaxed
ignore: |                        # block string or list, gitignore syntax
  vendor/
  !vendor/keep.yaml
yaml-files: ['*.yaml', '*.yml', '*.yaml.j2']
rules:
  line-length: {max: 120}
  quoted-strings: {quote-type: single}   # enables the rule the preset disabled
```

- `extends` starts from the yamllint `default` or `relaxed` preset, with yamllint's option
  defaults and `error` severity (`warning` where the preset says so). Without `extends` the
  fast-yaml defaults apply. Any other `extends` value is the path of a config file, resolved
  against the directory of the file that names it (yamllint: against the working directory).
  That file is loaded first and may extend another one, up to 8 files deep; a cycle is an
  error. The extending file's rules apply over it like over a preset, and `max-input-bytes`,
  `max-scan-ahead` and `ignore` are inherited unless set again (`yaml-files` is not, as in
  yamllint).
- Under `extends`, a rule the preset disables is enabled again by `enable`, a severity name or a
  mapping without `enabled`, and reports `error` unless a severity is given.
- `extends` is not full yamllint parity: it reproduces the preset's rule set, severities and
  the options fast-yaml has, but not options fast-yaml lacks (`indentation.spaces: consistent`,
  `indentation.indent-sequences`, `indentation.check-multi-line-strings`), so rules that depend on them behave
  differently. Rule semantics also differ from yamllint in places, which the `Preset` rustdoc
  lists (duplicate key equality, YAML 1.2 scalar resolution in `quoted-strings`, the last
  `document-end` position, `.yamllint` outside the default `yaml-files`).
- `ignore` patterns are anchored at the directory of the config file, are case-sensitive, and
  apply to directory walks and explicit paths. `!` re-includes a file. `fy lint` exits 0 when
  `ignore` drops every input.
- `ignore-from-file` takes a file name or a list of file names, relative to the config file's
  directory; their lines are ignore patterns, read when the config is loaded and anchored at
  the config file's directory like `ignore`. It cannot be combined with `ignore`.
- `yaml-files` matches the file name only (`sub/*.j2` matches nothing) and replaces the default
  `*.yaml`/`*.yml` for directory walks and globs. An explicit path is linted when it matches
  the default patterns or `yaml-files` (and is not dropped by `ignore`), and `--include`
  overrides `yaml-files`. `ignore` and `yaml-files` each take at most 1024 lines.
- yamllint rule names that differ are hinted, not accepted: `key-duplicates` is
  `duplicate-key`, `trailing-spaces` is `trailing-whitespace`, `anchors` is `invalid-anchor`.
  `locale` and per-rule `ignore` are rejected explicitly.

Custom rules added with `Linter::add_rule` are configured with
`LintConfig::with_custom_rule(CustomRuleCode, RuleSettings)` and read their severity through
`LintConfig::severity_for(RuleId, default)`.

### Python

```python
from fast_yaml._core.lint import LintConfig, Linter

config = LintConfig(
    max_line_length=120,
    indent_size=4,
    require_document_start=False,
    require_document_end=False,
    allow_duplicate_keys=False,
    disabled_rules={"line-length"},
)

linter = Linter(config)
```

## Output Formats

Each formatter converts `Vec<Diagnostic>` to a specific format:

### TextFormatter (rustc-style, for humans)

```rust
use fast_yaml_linter::formatter::Findings;
use fast_yaml_linter::{Formatter, TextFormatter};

let formatter = TextFormatter::new().with_color(true);
let output = formatter.format(Findings::FromSource { diagnostics: &diagnostics, source: &context });
```

**Output**:
```
error[duplicate-key]: duplicate key 'name' found
  --> example.yaml:10:5
   |
10 | name: value
   |       ^^^^^ duplicate key defined here
```

### JsonFormatter (for IDEs/CI)

```rust
use fast_yaml_linter::formatter::Findings;
use fast_yaml_linter::{Formatter, JsonFormatter};

let formatter = JsonFormatter::new(true);
let json = formatter.format(Findings::FromSource { diagnostics: &diagnostics, source: &context });
```

**Output**:
```json
[
  {
    "code": "duplicate-key",
    "severity": "error",
    "message": "duplicate key 'name' found",
    "span": {
      "start": { "line": 10, "column": 5, "offset": 145 },
      "end": { "line": 10, "column": 9, "offset": 149 }
    }
  }
]
```

### ReportFormat (for CI systems)

```rust
use std::path::Path;
use fast_yaml_linter::formatter::{FileReport, ReportFormat, ReportPath, ReportSource};

let source = ReportSource::File(ReportPath::from_absolute(Path::new("/work/config.yaml"))?);
let report = FileReport { source: &source, diagnostics: &diagnostics };
let annotations = ReportFormat::Github.render(&[report]);
```

Files are always named by absolute path (`ReportPath` strips Windows `\\?\` prefixes and builds RFC 3986 `file:` URIs for SARIF). Inputs that cannot be linted are reported with `syntax_diagnostic` / `input_error_diagnostic` (code `syntax`). GitHub shows at most 10 annotations per level per step, and SARIF rules carry only their `id`.

> [!NOTE]
> JsonFormatter requires the `json-output` feature. `ReportFormat::Sarif` requires the `sarif-output` feature.

## Cargo Features

| Feature | Description |
|---------|-------------|
| `default` | No additional features |
| `json-output` | Enable JSON formatter |

## Diagnostic Types

The linter provides rich diagnostic information:

```python
# Python
diagnostic.code        # Rule code (e.g., "duplicate-key")
diagnostic.severity    # Severity level
diagnostic.message     # Error message
diagnostic.span        # Location span
diagnostic.span.start  # Start location (line, column, offset)
diagnostic.span.end    # End location
diagnostic.context     # Source context (optional)
diagnostic.suggestions # Fix suggestions (optional)
```

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE))
- MIT license ([LICENSE-MIT](../../LICENSE-MIT))

at your option.
