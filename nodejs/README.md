# fastyaml-rs

[![npm](https://img.shields.io/npm/v/fastyaml-rs)](https://www.npmjs.com/package/fastyaml-rs)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue)](../LICENSE-MIT)
[![Node.js](https://img.shields.io/badge/node-20+-green.svg)](https://nodejs.org/)

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

**Requirements:** Node.js 20+. TypeScript definitions included.

## Quick Start

```typescript
import { safeLoad, safeDump } from 'fastyaml-rs';

// Parse YAML
const data = safeLoad(`
name: fast-yaml
version: 0.3.0
features:
  - fast
  - safe
  - yaml-1.2.2
`);

console.log(data);
// { name: 'fast-yaml', version: '0.1.0', features: ['fast', 'safe', 'yaml-1.2.2'] }

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
- Rust >= 1.88.0
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
