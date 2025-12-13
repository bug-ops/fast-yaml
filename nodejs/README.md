# @fast-yaml/core

[![npm](https://img.shields.io/npm/v/@fast-yaml/core)](https://www.npmjs.com/package/@fast-yaml/core)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue)](../LICENSE-MIT)
[![Node.js](https://img.shields.io/badge/node-20+-green.svg)](https://nodejs.org/)

**High-performance YAML 1.2.2 parser for Node.js, powered by Rust.**

Drop-in replacement for js-yaml with **5-10x faster** parsing through Rust's `yaml-rust2` library. Full YAML 1.2.2 Core Schema compliance with TypeScript definitions included.

> **YAML 1.2.2 Compliance** — Unlike js-yaml (YAML 1.1 by default), `@fast-yaml/core` follows the modern YAML 1.2.2 specification. This means `yes/no/on/off` are strings, not booleans, and octal numbers require `0o` prefix.

## Installation

```bash
# npm
npm install @fast-yaml/core

# yarn
yarn add @fast-yaml/core

# pnpm
pnpm add @fast-yaml/core
```

**Requirements:** Node.js 20+. TypeScript definitions included.

## Quick Start

```typescript
import { safeLoad, safeDump } from '@fast-yaml/core';

// Parse YAML
const data = safeLoad(`
name: fast-yaml
version: 0.1.0
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
import { safeLoad, safeLoadAll } from '@fast-yaml/core';

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
import { safeDump, safeDumpAll } from '@fast-yaml/core';

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
}
```

### Aliases

For js-yaml compatibility, `load` and `dump` are provided as aliases:

```typescript
import { load, dump } from '@fast-yaml/core';

const data = load('key: value');
const yaml = dump(data);
```

## YAML 1.2.2 Differences

`@fast-yaml/core` implements **YAML 1.2.2 Core Schema**, which differs from js-yaml's default YAML 1.1:

| Feature        | js-yaml (YAML 1.1) | @fast-yaml/core (YAML 1.2.2) |
| -------------- | ------------------ | ---------------------------- |
| `yes/no`       | `true/false`       | `"yes"/"no"` (strings)       |
| `on/off`       | `true/false`       | `"on"/"off"` (strings)       |
| `014` (octal)  | `12`               | `14` (decimal)               |
| `0o14` (octal) | Error              | `12`                         |

### Examples

```typescript
import { safeLoad } from '@fast-yaml/core';

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

| File Size     | js-yaml | @fast-yaml/core | Speedup  |
| ------------- | ------- | --------------- | -------- |
| Small (100B)  | 15 μs   | 5 μs            | **3x**   |
| Medium (2KB)  | 200 μs  | 50 μs           | **4x**   |
| Large (100KB) | 15 ms   | 2 ms            | **7.5x** |

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
| `npm run format`      | Format code with Prettier    |
| `npm run typecheck`   | Run TypeScript type checking |

## Technology Stack

- **YAML Parser**: [yaml-rust2](https://github.com/Ethiraric/yaml-rust2) — Rust YAML 1.2.2 parser
- **Node.js Bindings**: [NAPI-RS](https://napi.rs/) — Zero-cost Node.js bindings
- **Test Framework**: [Vitest](https://vitest.dev/) — Fast test runner

## Related Packages

- [fast-yaml (Python)](https://pypi.org/project/fast-yaml/) — Python bindings for the same Rust core

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](../LICENSE-APACHE))
- MIT license ([LICENSE-MIT](../LICENSE-MIT))

at your option.
