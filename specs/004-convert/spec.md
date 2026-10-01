---
aliases:
  - Convert
  - fy convert
tags:
  - sdd
  - spec
  - convert
  - cli
created: 2026-10-01
status: reverse-specified
related:
  - "[[constitution]]"
  - "[[002-format/spec|Format]]"
---

# Feature: Convert (JSON and YAML)

> [!info] Metadata
> **Product version**: fast-yaml 0.6.6 (main, e5e6cfb) plus #557 (key order, escapes, flow keys), #567 (float key text) and #574 (`--max-documents`). **Surface**: `fy convert <yaml|json> [FILE]` (CLI only; bindings expose their own load/dump APIs).
> **Method**: reverse-specified from `crates/fast-yaml-cli/src/commands/convert.rs` and real `fy` runs.

## 1. Purpose and value

Move data between JSON and YAML without surprises. YAML to JSON resolves the YAML 1.2.2 core schema (types, anchors, aliases, merge keys) into plain JSON, and fails loudly where JSON cannot express the value. JSON to YAML produces readable block YAML in which strings that would be misread as other types are quoted, so the data round-trips.

### Goal
`fy convert` produces output that a standard JSON/YAML parser reads back as the same data, in the key order of the input, and refuses (with a clear error) instead of silently changing data.

### Non-goals
- Preserving comments, formatting, or scalar spelling (use `fy format` for YAML to YAML).
- Lossless YAML-only constructs in JSON (binary, timestamps, custom tags, non-scalar keys); they are flattened or rejected as described below.
- Converting several files at once or recursing into directories (one input per call).
- Other formats (TOML, JSON5, NDJSON).

## 2. User stories

### US-001 (P1): YAML to JSON
AS A developer I WANT `fy convert json` SO THAT I can feed YAML config to JSON-only tools.

```
GIVEN input "b: 1\na: [x, 2.5, true, null]\n"
WHEN  I run `fy convert json`
THEN  stdout is
  {
    "b": 1,
    "a": [
      "x",
      2.5,
      true,
      null
    ]
  }
AND   exit code is 0                                  (keys keep the input order)

WHEN  I add --pretty=false
THEN  stdout is {"b":1,"a":["x",2.5,true,null]} followed by a newline
```

### US-002 (P1): JSON to YAML
AS A developer I WANT `fy convert yaml` SO THAT I can turn API payloads into editable config.

```
GIVEN input {"b":1,"a":{"z":[1,2],"y":null}}
WHEN  I run `fy convert yaml`
THEN  stdout is
  b: 1
  a:
    z:
      - 1
      - 2
    y: ~
```
Keys keep the order of the JSON object. A duplicate key keeps its first position and its last value (`{"a":1,"b":2,"a":3}` gives `a: 3`, `b: 2`).

Strings that look like other types are quoted so the type survives:
`{"a":"true","b":"null","c":"","d":"1.5","e":"~","f":" x","g":"a: b","h":"#x"}` gives `a: "true"`, `b: "null"`, `c: ""`, `d: "1.5"`, `e: "~"`, `f: " x"`, `g: "a: b"`, `h: "#x"`. A multi-line string becomes `s: "line1\nline2"`; plain text such as `é` stays plain. U+0085, U+2028 and U+2029 are escaped (`"a\x85b\u2028c"`), and a scalar containing `?` in flow context is quoted.

### US-003 (P1): Multi-document YAML
AS A user of Kubernetes-style streams I WANT every document converted SO THAT none is dropped.

```
GIVEN "a: 1\n---\nb: 2\n"
WHEN  I run `fy convert json --pretty=false`
THEN  stdout is [{"a":1},{"b":2}]   (an array, one element per document)

GIVEN a single document
THEN  stdout is the bare value, not a one-element array
```

A stream ending in an empty document gives `null` for it (`a: 1\n---\n` gives `[{"a":1},null]`). Empty input is an error: `error: Empty YAML document`, exit 1.

### US-004 (P1): Fail instead of corrupting
AS A pipeline owner I WANT unrepresentable values rejected SO THAT bad data never reaches downstream systems.

```
GIVEN "a: .inf\nb: .nan\n"
WHEN  I run `fy convert json`
THEN  stderr is "error: YAML value '.inf' cannot be represented in JSON (JSON does not support infinity/NaN). Consider replacing with a numeric sentinel value."
AND   exit code is 1 and stdout is empty

GIVEN "? [1,2]\n: v\n"
THEN  "Unsupported YAML map key type: only scalar keys (string, number, boolean, null) can be converted to JSON", exit 1

GIVEN "1: a\n\"1\": b\n"
THEN  the error says the string key "1" is distinct in YAML but converts to the same JSON key as the int key, exit 1
```

### US-005 (P2): Anchors, aliases and merge keys
AS A user of DRY YAML I WANT aliases and `<<` expanded SO THAT JSON consumers see plain data.

```
GIVEN "base: &b {k: 1, j: 2}\nd:\n  <<: *b\n  k: 9\n"
WHEN  I run `fy convert json --pretty=false`
THEN  stdout is {"base":{"k":1,"j":2},"d":{"k":9,"j":2}}   (explicit key wins over the merged one, in the merged key's position)

GIVEN "x: &a {k: 1}\ny: *a\n<<: *a\n"
THEN  y is a copy of x, and the root also receives "k": 1 from the merge
```

### US-006 (P2): Write to a file
AS A user I WANT `-o` and `-i` SO THAT conversion lands where I need it.

```
WHEN  I run `fy convert json -o o.json cv.yaml`   THEN o.json holds the JSON, cv.yaml is unchanged
WHEN  I run `fy convert json -i cv.yaml`          THEN cv.yaml itself now contains JSON (same path, no rename)
WHEN  I run `fy convert json -i` with no file     THEN "error: --in-place requires a file argument", exit 1
```

### US-007 (P3): Bounded resource use
```
GIVEN a 300-level nested flow sequence
WHEN  I run `fy convert yaml`            THEN "Failed to parse JSON ... recursion limit exceeded", exit 1
WHEN  I run `fy convert json`            THEN "flow collection nesting exceeds the scanner limit of 255 levels, which the depth limit cannot raise", exit 1

GIVEN an alias bomb
WHEN  I run `fy convert json --max-alias-bytes 100`
THEN  "alias expansion exceeds 100 bytes" with "hint: raise with --max-alias-bytes", exit 1
```

## 3. Functional requirements

### YAML to JSON

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-001 | WHEN the target is `json` THE SYSTEM SHALL parse all documents of the input with the core-schema resolver and string key domain, and print one JSON value (single document) or one JSON array (several documents), followed by a newline. | MUST |
| FR-002 | THE SYSTEM SHALL pretty-print with two-space indent by default and a compact single line with `--pretty=false`. | MUST |
| FR-003 | WHEN a scalar resolves to null, bool, int, or float THE SYSTEM SHALL emit the corresponding JSON type; other scalars (including `!!binary`, timestamps, unknown tags) SHALL be emitted as strings of their text, tags dropped. | MUST |
| FR-004 | WHEN an integer is written in hex (`0x1F`) or octal (`0o17`) THE SYSTEM SHALL emit its decimal value; integers beyond 64 bits SHALL keep all digits. | MUST |
| FR-005 | WHEN a float is `.inf`, `-.inf`, or `.nan` THE SYSTEM SHALL fail with the infinity/NaN error and exit 1 (never emit `null` or a string). | MUST |
| FR-006 | WHEN a mapping key is a null, bool, int, or float scalar THE SYSTEM SHALL convert it to its canonical string (`"null"`, `"true"`, `"1"`); a float key is spelled like ECMAScript `Number` toString (`1.5e21` is `"1.5e+21"`, `.inf` is `"Infinity"`, `.nan` is `"NaN"`, `-0.0` is `"0"`); non-scalar keys SHALL fail. | MUST |
| FR-007 | WHEN two distinct YAML keys convert to the same JSON key (`1` and `"1"`, `true` and `"true"`, `1` and `1.0`) THE SYSTEM SHALL fail; spellings of the same YAML key are not a collision. | MUST |
| FR-008 | WHEN aliases or merge keys (`<<`) appear THE SYSTEM SHALL expand them; explicit keys SHALL override merged keys. | MUST |
| FR-009 | WHEN a `!!set` is converted THE SYSTEM SHALL emit an object whose values are `null`. | SHOULD |
| FR-010 | WHEN the input has no document THE SYSTEM SHALL fail with `Empty YAML document` and exit 1. | MUST |
| FR-011 | WHEN a mapping has duplicate YAML keys THE SYSTEM SHALL keep the first key position and the last value. | MUST (as-is) |
| FR-012 | THE SYSTEM SHALL apply `--max-depth` (1..=512, default 256), `--max-alias-bytes` (default 64 MiB), `--max-documents` (default 100 000), `--max-input-bytes`, and `--max-scan-ahead` to YAML input and report violations as typed errors with the flag in a hint. | MUST |

### JSON to YAML

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-020 | WHEN the target is `yaml` THE SYSTEM SHALL parse exactly one JSON value (after stripping a leading BOM) and emit one block-style YAML document ending in `\n`. | MUST |
| FR-021 | WHEN input has trailing content (second value, NDJSON), a trailing comma, or is empty THE SYSTEM SHALL fail with `Failed to parse JSON` plus position, exit 1. | MUST |
| FR-022 | WHEN a JSON integer fits in 64 bits THE SYSTEM SHALL emit an integer; WHEN larger THE SYSTEM SHALL keep every digit as an integer (`12345678901234567890123`). | MUST |
| FR-023 | WHEN a JSON number is a float THE SYSTEM SHALL keep its float nature (`1.0` stays `1.0`, `-0.0` stays `-0.0`, `1E5` becomes `1.0e+5`); a number outside `f64` range SHALL fail (`Float value out of representable range`). | MUST |
| FR-024 | WHEN a string would resolve to another core-schema type or is syntactically unsafe (`true`, `null`, `~`, `""`, `1.5`, leading space, `: `, leading `#`, key `yes`) THE SYSTEM SHALL quote it; line breaks SHALL be written as `\n` escapes in a double-quoted scalar. | MUST |
| FR-025 | WHEN JSON `null` is a value THE SYSTEM SHALL write `~`; an empty array/object SHALL be `[]` / `{}` (a root `null` is `~`). | MUST |
| FR-026 | WHEN duplicate keys appear in a JSON object THE SYSTEM SHALL keep the first key position and the last value. | MUST (as-is) |
| FR-027 | WHEN JSON nests deeper than the JSON parser's recursion limit (128) THE SYSTEM SHALL fail with `recursion limit exceeded`. | MUST |
| FR-028 | WHEN the YAML emitter writes a flow-style key longer than 1024 characters THE SYSTEM SHALL use the explicit `? key` form, SHALL quote a plain scalar that contains `?` in flow context, and SHALL escape U+0085, U+2028 and U+2029 in double-quoted output (the same emitter backs the Python and Node.js dump functions). | MUST |

### Common

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-030 | THE SYSTEM SHALL read from `FILE` or stdin and write to stdout, `-o FILE`, or (with `-i FILE`) back to the same path. | MUST |
| FR-031 | WHEN `-i` has no file THE SYSTEM SHALL fail before reading. | MUST |
| FR-032 | WHEN conversion fails THE SYSTEM SHALL write nothing to stdout or the target file and exit 1. | MUST |
| FR-033 | WHEN the YAML input starts with a BOM THE SYSTEM SHALL ignore it; output never contains a BOM. | MUST |
| FR-034 | THE SYSTEM SHALL produce deterministic output: object keys keep their input order in both directions (insertion-ordered `serde_json` map, `preserve_order`). | MUST |

## 4. Key entities and types

| Type | Role |
|------|------|
| `ConvertFormat` (`Yaml`, `Json`) | Target selector (clap `ValueEnum`) |
| `Converter` | Holds target, `pretty`, `ParseLimits` |
| `Parser::parse_all_with_options`, `LoadOptions`, `KeyDomain::StringKeys` | Resolve YAML to `Value` for conversion |
| `Value` (`fast-yaml-core`) | Resolved tree: null, bool, int, big int, float, string, sequence, mapping, set |
| `serde_json::Value` | JSON side; `Map` is insertion-ordered (`preserve_order` enabled) |
| `Emitter::emit_str` | YAML writer for JSON input (block style) |
| `ParseLimits` (`MaxDepth`, `MaxAliasBytes`, `MaxInputBytes`, `MaxScanAhead`) | Resource bounds |

## 5. Edge cases and error handling

| Scenario | Expected behavior (verified) |
|----------|------------------------------|
| `a: 12345678901234567890123` | JSON `12345678901234567890123` |
| `d: 1e3`, `e: 1.0`, `f: -0` | `1000.0`, `1.0`, `0` |
| `1.5e300`, `1e-7`, `100000000000000000000.0` | `1.5e+300`, `1e-7`, `1e+20` |
| `a: !!binary aGk=`, `b: 2001-01-01`, `c: !custom x` | `"aGk="`, `"2001-01-01"`, `"x"` |
| `!!set {a, b}` | `{"a":null,"b":null}` |
| `---\n...` | `null` |
| `"hi"` JSON input | `hi` |
| `plain` YAML input | `"plain"` |
| JSON `{"a":1,"a":2}` / YAML `a: 1\na: 2` | `a: 2` / `{"a": 2}` (last value wins, first position kept, no warning) |
| YAML keys `.inf`, `.nan`, `1e3`, `0.1` | JSON keys `"Infinity"`, `"NaN"`, `"1000"`, `"0.1"` (verified) |
| JSON `{"a": 1e400}` | `Float value out of representable range: 1e+400`, exit 1 |
| YAML double-quoted string with a UTF-16 surrogate escape (`"\ud83d\ude00"`) | `found invalid Unicode character escape code at line 1, column 4`, exit 1 (YAML escapes must be scalar values) |
| NDJSON or `[1,2]\n[3]` | `trailing characters at line 2 column 1`, exit 1 |
| Syntax error in YAML | `Failed to parse YAML` plus parser message, exit 1 |
| Missing input file | `Failed to read file: ...`, exit 1 |
| `-f json` after `convert` | usage error (`-f` no longer exists), exit 2 |

## 6. Non-functional requirements

| ID | Category | Requirement |
|----|----------|-------------|
| NFR-001 | Safety | Alias expansion, nesting, input size, and scan-ahead are bounded; hostile input yields an error, never an abort. |
| NFR-002 | Determinism | Same input yields identical bytes. |
| NFR-003 | Fidelity | For any JSON text `j` that converts, `convert json (convert yaml j)` is data-equal to `j` (modulo duplicate keys), with the same key order. |

## 7. Success criteria

| ID | Metric | Target |
|----|--------|--------|
| SC-001 | JSON to YAML to JSON round trip on `tests/fixtures` JSON corpus | data-equal, 100% |
| SC-002 | YAML to JSON of every `fy format` output equals that of its input | 100% on the fixture set |
| SC-003 | Unrepresentable inputs (`.inf`, `.nan`, collection keys, key collisions) | 100% rejected with exit 1 and no output |
| SC-004 | Unit tests in `commands/convert.rs` (key coercion, collision, big ints, multi-doc, merge keys) | green |

## 8. Agent boundaries

### Always
- Test both directions and a round trip for any change to number or key handling.
- Keep error messages naming the offending value and the remedy.

### Ask first
- Enabling `serde_json` `preserve_order` or otherwise changing key ordering.
- Changing the multi-document shape (array vs error).
- Mapping non-JSON YAML types (binary, timestamps) to anything other than strings.

### Never
- Emit `NaN`/`Infinity` or turn them into `null` silently.
- Overwrite the input file before the conversion has fully succeeded.

## 9. Open questions / Known deviations

| # | Item | Status |
|---|------|--------|
| 1 | Duplicate keys are first-position, last-value with no warning in both directions. (GAP-CLI-009, OQ-02) | [NEEDS CLARIFICATION: warn or reject under a strict option?] |
| 2 | `convert -i json file.yaml` overwrites the same path with JSON (a `.yaml` file containing JSON); `skills/fast-yaml-cli/SKILL.md` claims a renamed `file.json`. (GAP-CLI-006) | [NEEDS CLARIFICATION: rename, refuse, or fix docs] |
| 3 | `-i` silently overrides `-o` (`convert json -i -o x w.yaml` writes `w.yaml` only). (GAP-CLI-007) | [NEEDS CLARIFICATION: make them conflict in clap] |
| 4 | `-` is not accepted as stdin for the file argument. (GAP-CLI-008) | accept `-` as stdin |
| 5 | JSON to YAML has no `--max-input-bytes`-independent depth control; JSON nesting is capped by `serde_json` at 128 and `--max-depth` applies to YAML input only. | document; [NEEDS CLARIFICATION: align limits] |
| 6 | Non-JSON YAML values are flattened silently: tags dropped, binary and timestamps become strings. | [NEEDS CLARIFICATION: warn under `-v`, or accept] |
| 7 | Integers above 2^53 in JSON output are exact in the CLI, but Node bindings lose precision. (GAP-NODE-001, OQ-10) | see Node spec |

## 10. See also

- [[002-format/spec|Format]] — same emitter, YAML to YAML
- [[constitution]] — principles
