---
applyTo:
  - "**/*.rs"
  - "**/Cargo.toml"
---

# Rust Code Instructions

## General Rules
- Use `thiserror` for error types in library crates
- Use `anyhow` with context in bindings
- No `unsafe` code - workspace forbids it via `#![forbid(unsafe_code)]`
- No `unwrap()` or `expect()` in library code - use `?` operator
- All public items must have documentation with examples

## Error Handling
```rust
// Good - use ? with context
let value = operation().context("Failed to perform operation")?;

// Bad - panics on error
let value = operation().unwrap();
```

## Ownership
- Prefer `&str` over `String` in function parameters
- Prefer `&[T]` over `Vec<T>` in function parameters
- Use `Cow<'_, str>` when you might or might not need to allocate
- Avoid unnecessary `.clone()` - use references when possible

## Imports
- Group: std → external crates → workspace crates → local modules
- Use `use crate::` for crate-internal imports

## Testing
- Unit tests in `#[cfg(test)]` module at bottom of file
- Use `#[test]` attribute, descriptive names: `test_parse_empty_document`
- Test both success and error cases
