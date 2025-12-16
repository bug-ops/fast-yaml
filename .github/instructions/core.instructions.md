---
applyTo:
  - "crates/fast-yaml-core/**"
---

# Core Parser Instructions

## YAML 1.2.2 Compliance
- `yes/no/on/off` are strings, NOT booleans (unlike YAML 1.1)
- Octal numbers require `0o` prefix: `0o14` = 12 decimal
- Use yaml-rust2 for parsing, wrap its types

## Value Types
```rust
pub enum Value {
    Null,
    Bool(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Array(Vec<Value>),
    Hash(IndexMap<String, Value>),
}
```

## Error Types
- Use `ParseError` with source location (line, column, byte offset)
- Include context in error messages
- Implement `std::error::Error` via `thiserror`

## Performance
- Avoid allocations in hot paths
- Use `&str` slices from input where possible
- Consider streaming/lazy parsing for large documents
