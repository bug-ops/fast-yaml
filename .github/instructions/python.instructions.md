---
applyTo:
  - "python/**/*.rs"
  - "python/**/*.py"
  - "python/**/*.pyi"
---

# Python Bindings Instructions

## PyO3 Rules
- Release GIL for CPU-intensive ops: `py.allow_threads(|| ...)`
- Every `#[pyfunction]` needs type stub in `_core.pyi`
- Convert errors with `PyErr::new_err()` or `.into()`
- Handle `None` values explicitly in conversions

## Type Stubs
- Match signatures exactly with implementation
- Use `T | None` for optional, `list[T]` for vectors
- Include docstrings matching Rust docs

## Testing
- Use pytest in `tests/`
- Test error messages are helpful
- Round-trip tests: serialize → deserialize → compare
