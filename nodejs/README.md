# @fast-yaml/core

Fast YAML 1.2.2 parser, linter, and parallel processor for Node.js, powered by Rust.

## Status

🚧 **Work in Progress** - Phase 5.1: Foundation (Initial Setup)

This package is currently under active development. The basic infrastructure is in place, but core parsing functionality is not yet implemented.

## Planned Features

- 5-10x faster than js-yaml
- YAML 1.2.2 compliance (Core Schema)
- Built-in linter with rich diagnostics
- Parallel processing for large files
- Full TypeScript support
- Zero dependencies (native module)

## Development

This package uses NAPI-RS to create Node.js bindings for the fast-yaml Rust library.

### Prerequisites

- Node.js >= 18
- Rust >= 1.86.0
- NAPI-RS CLI

### Build

```bash
npm install
npm run build:debug
```

### Test

```bash
npm test
```

## License

MIT OR Apache-2.0
