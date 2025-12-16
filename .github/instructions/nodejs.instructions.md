---
applyTo:
  - "nodejs/**/*.rs"
  - "nodejs/**/*.ts"
  - "nodejs/**/*.js"
---

# Node.js Bindings Instructions

## NAPI-RS Rules
- Export TypeScript definitions for all public functions
- Use `#[napi(ts_return_type = "Promise<T>")]` for async
- Zero-copy buffer handling where possible
- Don't block event loop with sync operations

## API Design
- camelCase for function names
- Return Promises for async operations
- Error messages should be actionable

## Testing
- Use vitest for tests
- Test error paths, not just happy paths
