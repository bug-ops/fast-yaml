# YAML 1.2.2 Specification Test Fixtures

This directory contains YAML test files based on examples from the [YAML 1.2.2 specification](https://yaml.org/spec/1.2.2/).

## Specification Examples (Chapter 2)

| File | Description |
|------|-------------|
| `2.01-sequence-of-scalars.yaml` | Basic sequence (list) |
| `2.02-mapping-scalars-to-scalars.yaml` | Basic mapping (dict) with comments |
| `2.03-mapping-scalars-to-sequences.yaml` | Nested sequences in mapping |
| `2.04-sequence-of-mappings.yaml` | List of dictionaries |
| `2.05-sequence-of-sequences.yaml` | Flow style nested sequences |
| `2.06-mapping-of-mappings.yaml` | Flow style nested mappings |
| `2.07-two-documents.yaml` | Multi-document stream |
| `2.08-play-by-play.yaml` | Documents with `...` end marker |
| `2.09-single-document-comments.yaml` | Comments in document |
| `2.10-anchors-and-aliases.yaml` | `&anchor` and `*alias` usage |
| `2.11-mapping-between-sequences.yaml` | Complex keys (sequences as keys) |
| `2.12-compact-nested-mapping.yaml` | Compact nested structure |
| `2.13-literal-block-scalar.yaml` | Literal `\|` block scalar |
| `2.14-folded-block-scalar.yaml` | Folded `>` block scalar |
| `2.15-folded-indented-lines.yaml` | Folded with preserved indentation |
| `2.16-indentation-determines-scope.yaml` | Mixed block scalars |
| `2.17-quoted-scalars.yaml` | Single and double quoted strings |
| `2.18-multi-line-flow-scalars.yaml` | Multiline plain and quoted |
| `2.19-integers.yaml` | Integer formats (decimal, octal, hex) |
| `2.20-floating-point.yaml` | Float formats including `.inf`, `.nan` |
| `2.21-miscellaneous.yaml` | Null, booleans, quoted string |
| `2.22-timestamps.yaml` | Date/time formats |
| `2.23-explicit-tags.yaml` | `!!str`, `!!binary`, custom tags |
| `2.24-global-tags.yaml` | `%TAG` directive and custom tags |
| `2.25-unordered-sets.yaml` | `!!set` type |
| `2.26-ordered-mappings.yaml` | `!!omap` type |
| `2.27-invoice.yaml` | Full example with anchors, aliases, blocks |
| `2.28-log-file.yaml` | Multi-document log with various features |

## Core Schema Tests

| File | Description |
|------|-------------|
| `core-schema-booleans.yaml` | YAML 1.2.2 boolean handling (`yes/no` are strings) |
| `core-schema-null.yaml` | Null representations (`~`, `null`, empty) |
| `core-schema-integers.yaml` | Integer formats (0o for octal, 0x for hex) |
| `core-schema-floats.yaml` | Float formats including special values |

## Feature Tests

| File | Description |
|------|-------------|
| `block-scalars.yaml` | `\|`, `>`, chomping indicators (`-`, `+`) |
| `flow-collections.yaml` | `[]` and `{}` JSON-like syntax |
| `anchors-aliases.yaml` | Anchors, aliases, merge key (`<<`) |
| `string-quoting.yaml` | Plain, single, double quoted strings |
| `complex-keys.yaml` | Sequences/mappings as keys |
| `edge-cases.yaml` | Empty values, special chars, deep nesting |
| `directives.yaml` | `%YAML` version directive |
| `empty-documents.yaml` | Empty and minimal documents |

## Usage

These files can be used for:

1. **Parser testing** - Verify correct parsing of all YAML features
2. **Linter testing** - Test linting rules against various YAML patterns
3. **Roundtrip testing** - Load → dump → load should preserve semantics
4. **Performance benchmarking** - Use larger examples for benchmarks

## YAML 1.2.2 vs 1.1 Key Differences

- `yes`, `no`, `on`, `off` are **strings** (not booleans)
- Octal numbers require `0o` prefix (`0o14` not `014`)
- Only `true`/`false` are boolean values
