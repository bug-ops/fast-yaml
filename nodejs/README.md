# fastyaml-rs

[![npm](https://img.shields.io/npm/v/fastyaml-rs)](https://www.npmjs.com/package/fastyaml-rs)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue)](../LICENSE-MIT)
[![Node.js](https://img.shields.io/badge/node-22+-green.svg)](https://nodejs.org/)

**High-performance YAML 1.2.2 parser for Node.js, powered by Rust.**

Drop-in replacement for js-yaml with **5-10x faster** parsing through Rust's `saphyr` library. Full YAML 1.2.2 Core Schema compliance with TypeScript definitions included.

> **YAML 1.2.2 Compliance** — Unlike js-yaml (YAML 1.1 by default), `fastyaml-rs` follows the modern YAML 1.2.2 specification. This means `yes/no/on/off` are strings, not booleans, and octal numbers require `0o` prefix.

## Installation

```bash
# npm
npm install fastyaml-rs

# yarn
yarn add fastyaml-rs

# pnpm
pnpm add fastyaml-rs
```

**Requirements:** Node.js 22+. TypeScript definitions included.

## Quick Start

```typescript
import { safeLoad, safeDump } from 'fastyaml-rs';

// Parse YAML
const data = safeLoad(`
name: fast-yaml
version: 0.3.3
features:
  - fast
  - safe
  - yaml-1.2.2
`);

console.log(data);
// { name: 'fast-yaml', version: '0.3.1', features: ['fast', 'safe', 'yaml-1.2.2'] }

// Serialize to YAML
const yamlStr = safeDump(data);
console.log(yamlStr);
```

> **Migrating from js-yaml?** The API is compatible — just change your import!

## API Reference

### Parsing

```typescript
import { safeLoad, safeLoadAll } from 'fastyaml-rs';

// Parse single document
const doc = safeLoad('key: value');
// { key: 'value' }

// Parse multiple documents
const docs = safeLoadAll(`
---
first: 1
---
second: 2
`);
// [{ first: 1 }, { second: 2 }]
```

### Serialization

```typescript
import { safeDump, safeDumpAll } from 'fastyaml-rs';

// Dump single document
const yaml = safeDump({ name: 'test', count: 42 });
// 'name: test\ncount: 42\n'

// Dump with options
const sorted = safeDump(data, { sortKeys: true });

// Dump multiple documents
const multiDoc = safeDumpAll([{ a: 1 }, { b: 2 }]);
// '---\na: 1\n---\nb: 2\n'
```

### Options

```typescript
interface DumpOptions {
  sortKeys?: boolean; // Sort object keys alphabetically (default: false)
  allowUnicode?: boolean; // Allow unicode characters (default: true)
  indent?: number; // Indentation width 1-9 (default: 2)
  width?: number; // Line width 20-1000 (default: 80)
  defaultFlowStyle?: boolean; // Force flow style [...], {...} (default: null/block)
  explicitStart?: boolean; // Add '---' document marker (default: false)
}
```

**Example with options:**

```typescript
const yaml = safeDump(data, {
  sortKeys: true,
  indent: 4,
  width: 120,
  explicitStart: true,
});
```

### Aliases

For js-yaml compatibility, `load` and `dump` are provided as aliases:

```typescript
import { load, dump } from 'fastyaml-rs';

const data = load('key: value');
const yaml = dump(data);
```

### Linting

`lint()` honors inline suppression comments such as `# fy: disable-line duplicate-key`, `# fy: disable` / `# fy: enable` and `# fy: disable-file` (also spelled `# yamllint ...`); invalid directives are reported as `lint-directive` diagnostics. See the [directive reference](https://github.com/bug-ops/fast-yaml#inline-lint-directives).

## Batch Processing

Process multiple YAML files in parallel:

```typescript
import { processFiles, formatFilesInPlace, BatchConfig } from 'fastyaml-rs';

// Parse multiple files
const result = processFiles([
  'config1.yaml',
  'config2.yaml',
  'config3.yaml',
]);
console.log(`Processed ${result.total} files, ${result.failed} failed`);

// With configuration
const config: BatchConfig = { workers: 4, indent: 2 };
const result = processFiles(paths, config);
```

### Format Files

```typescript
import { formatFiles, formatFilesInPlace } from 'fastyaml-rs';

// Dry-run: get formatted content without writing
const results = formatFiles(['config.yaml']);
for (const { path, content, error } of results) {
  if (content) {
    console.log(`${path}: ${content.length} bytes`);
  }
}

// In-place: format and write back
const result = formatFilesInPlace(['config.yaml']);
console.log(`Changed ${result.changed} files`);
```

### BatchConfig Options

```typescript
interface BatchConfig {
  workers?: number;           // Worker threads (null = auto)
  mmapThreshold?: number;     // Mmap threshold (default: 512KB)
  maxInputBytes?: number;     // Max file size, 1..1073741824 (default: 100MiB)
  indent?: number;            // Indentation (default: 2)
  width?: number;             // Line width (default: 80)
  sortKeys?: boolean;         // Sort keys (default: false)
  maxDepth?: number;          // Max nesting depth, 1..512 (default: 256); processFiles only
  maxAliasBytes?: number;     // Alias-expansion budget per file, 1..1073741824 (default: 64MiB); processFiles only
}
```

### BatchResult

```typescript
interface BatchResult {
  total: number;              // Total files processed
  success: number;            // Successfully processed
  changed: number;            // Files modified
  failed: number;             // Failed files
  durationMs: number;         // Processing time in ms
  errors: BatchError[];       // Error details
}

interface BatchError {
  path: string;
  message: string;
}
```

## Linting

```typescript
import { lint, Linter, type LintConfig } from 'fastyaml-rs';

const config: LintConfig = {
  maxLineLength: 120,
  rules: {
    'line-length': { max: 100, severity: 'warning' },
    'document-start': { present: true },
    'quoted-strings': { 'quote-type': 'single', required: true },
    'key-ordering': 'disable',
    comments: 'info',
  },
  disabledRules: ['truthy'],
};

for (const d of lint('a: 1\n', config)) {
  console.log(`${d.span.start.line}:${d.span.start.column} ${d.code} ${d.message}`);
}
const linter = new Linter(config);
```

`rules` is the same mapping as `rules:` in the `fy lint --config` file, so option keys are
kebab-case. An entry is a severity (`error`, `warning`, `info`, `hint`, any case), `'enable'`,
`'disable'`, or an object with `enabled`, `severity` and the rule's options (see the option table
in the `fast-yaml-linter` README). Unknown rules, unknown options, wrong types and `null` values
(except `line-length.max`, where `null` removes the limit) throw an `Error` naming the rule and
option.

Fields are applied in order: `maxLineLength`, `indentSize`, `requireDocumentStart`,
`requireDocumentEnd`, `allowDuplicateKeys`, then the `rules` patch, then `disabledRules`, which
wins. Unset `maxLineLength` keeps the rule default; `0` and an `indentSize` outside 1 to 16
throw. `requireDocument*: false` leaves the rule unchanged.

## Parse Limits

`safeLoad`, `safeLoadAll`, `load`, `loadAll`, `parseParallel`, `lint` / `Linter`, and `processFiles` accept two limits (as options, `ParallelConfig`, `LintConfig`, or `BatchConfig`):

```javascript
import { safeLoad } from 'fastyaml-rs';

safeLoad(deepYaml, { maxDepth: 400 });            // default 256, range 1..512
safeLoad(aliasHeavyYaml, { maxAliasBytes: 2 ** 28 }); // default 64MiB, range 1..1GiB
```

- Flow collections (`[]`, `{}`) stop at 255 levels whatever `maxDepth` is; deeper flow nesting throws `flow collection nesting exceeds the scanner limit of 255 levels`.
- Values must be integers within the range; `0`, negatives, fractions, `NaN`, and out-of-range values throw `maxDepth must be between 1 and 512, got N`.
- `maxAliasBytes` is an estimate of alias-expansion cost per call (per file in batch runs); JavaScript objects cost several times the estimate, so keep it modest on memory-constrained hosts.
- `parseParallel` / `parseParallelAsync` also accept `maxDocuments` (integer, 1..10000000, default 100000) and `maxInputBytes`; `processFiles` accepts `maxInputBytes`; all throw the same `... must be between 1 and N, got V` error for invalid values.
- `lint` / `Linter` also accept `maxInputBytes` (integer, 1..1073741824, default 100MiB) and reject larger sources. It bounds linting work on oversized input; the source is already in memory when checked, so it is not a memory bound.
- The calling thread needs about 1 MiB of stack at depth 512 (roughly 980 KiB measured in release); on stacks of 512 KiB or less (for example a worker with `stackSizeMb: 0.5`) the process can abort and the overflow cannot be caught, while the default 256 is safe. `formatFiles` / `formatFilesInPlace` apply `maxDepth` and ignore `maxAliasBytes`; `indent` (1..9) and `width` (20..1000) throw when out of range instead of being clamped.

## YAML 1.2.2 Differences

`fastyaml-rs` implements **YAML 1.2.2 Core Schema**, which differs from js-yaml's default YAML 1.1:

| Feature        | js-yaml (YAML 1.1) | fastyaml-rs (YAML 1.2.2) |
| -------------- | ------------------ | ------------------------ |
| `yes/no`       | `true/false`       | `"yes"/"no"` (strings)   |
| `on/off`       | `true/false`       | `"on"/"off"` (strings)   |
| `014` (octal)  | `12`               | `14` (decimal)           |
| `0o14` (octal) | Error              | `12`                     |

### Examples

```typescript
import { safeLoad } from 'fastyaml-rs';

// Booleans — only true/false
safeLoad('true'); // true
safeLoad('false'); // false
safeLoad('yes'); // "yes" (string!)
safeLoad('no'); // "no" (string!)

// Octal numbers — require 0o prefix
safeLoad('0o14'); // 12 (octal)
safeLoad('014'); // 14 (decimal, NOT octal!)

// Special floats
safeLoad('.inf'); // Infinity
safeLoad('-.inf'); // -Infinity
safeLoad('.nan'); // NaN

// Null values
safeLoad('~'); // null
safeLoad('null'); // null
```

## Supported Types

| YAML Type              | JavaScript Type |
| ---------------------- | --------------- |
| `null`, `~`            | `null`          |
| `true`, `false`        | `boolean`       |
| `123`, `0x1F`, `0o17`  | `number`        |
| `1.23`, `.inf`, `.nan` | `number`        |
| `"string"`, `'string'` | `string`        |
| `[a, b, c]`            | `Array`         |
| `{a: 1, b: 2}`         | `Object`        |
| `!!set {a, b}`         | `{a: null, b: null}` (js-yaml form) |

Floats are written with a dot and a signed exponent so YAML 1.1 readers such as PyYAML read them as floats (`1e300` becomes `1.0e+300`). `safeDump` writes a JavaScript `Set` as a `!!set` and a `Map` as a mapping, so the round trip
through `safeLoad` is one-way (a `!!set` loads as an object). Two `Set` members or `Map` keys that
are the same YAML value (`null` and `undefined`) are an error. `safeLoad` rejects, with the
position, a `!!set` member that has a value, a repeated `<<` key in one mapping, and keys that
differ in YAML but share a property name (`1` and `"1"`).

## Security

Input validation is enforced to prevent denial-of-service attacks:

| Limit          | Default |
| -------------- | ------- |
| Max input size | 100 MB  |

## Performance

Benchmarks on typical YAML workloads show **5-10x speedup** over js-yaml for large files:

| File Size     | js-yaml | fastyaml-rs | Speedup  |
| ------------- | ------- | ----------- | -------- |
| Small (100B)  | 15 μs   | 5 μs        | **3x**   |
| Medium (2KB)  | 200 μs  | 50 μs       | **4x**   |
| Large (100KB) | 15 ms   | 2 ms        | **7.5x** |

Run benchmarks yourself:

```bash
npm run bench
```

## Platform Support

Pre-built binaries are available for:

| Platform      | Architecture |
| ------------- | ------------ |
| Linux (glibc) | x64, ARM64   |
| Linux (musl)  | x64          |
| macOS         | x64, ARM64   |
| Windows       | x64, ARM64   |

## Development

### Prerequisites

- Node.js >= 20
- Rust >= 1.91.0
- NAPI-RS CLI (`npm install -g @napi-rs/cli`)

### Build from Source

```bash
# Clone repository
git clone https://github.com/bug-ops/fast-yaml.git
cd fast-yaml/nodejs

# Install dependencies
npm install

# Build debug version
npm run build:debug

# Build release version
npm run build

# Run tests
npm test

# Run benchmarks
npm run bench
```

### Scripts

| Script                | Description                  |
| --------------------- | ---------------------------- |
| `npm run build`       | Build release native module  |
| `npm run build:debug` | Build debug native module    |
| `npm test`            | Run test suite               |
| `npm run bench`       | Run benchmarks               |
| `npm run format`      | Format code with Biome       |
| `npm run lint`        | Lint code with Biome         |
| `npm run check`       | Format and lint with Biome   |
| `npm run typecheck`   | Run TypeScript type checking |

## Technology Stack

- **YAML Parser**: [saphyr](https://github.com/saphyr-rs/saphyr) — Rust YAML 1.2.2 parser
- **Node.js Bindings**: [NAPI-RS](https://napi.rs/) — Zero-cost Node.js bindings
- **Test Framework**: [Vitest](https://vitest.dev/) — Fast test runner
- **Linter/Formatter**: [Biome](https://biomejs.dev/) — Fast all-in-one toolchain

## Related Packages

- [fastyaml-rs (Python)](https://pypi.org/project/fastyaml-rs/) — Python bindings for the same Rust core

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](../LICENSE-APACHE))
- MIT license ([LICENSE-MIT](../LICENSE-MIT))

at your option.
