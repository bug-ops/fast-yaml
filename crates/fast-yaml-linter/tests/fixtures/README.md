# Linter Test Fixtures

This directory contains YAML test fixtures for the fast-yaml-linter integration tests.

## Directory Structure

```
fixtures/
├── valid/           # Files that should pass all linting rules
├── invalid/         # Files with known violations
└── edge_cases/      # Edge case files for special scenarios
```

## Valid Fixtures

These files contain valid YAML that should pass all linting rules with default configuration.

| File | Description |
|------|-------------|
| `valid/simple.yaml` | Simple flat YAML structure with basic types |
| `valid/complex.yaml` | Complex nested structures with arrays and objects |
| `valid/comments.yaml` | Proper comment formatting and placement |

## Invalid Fixtures

These files contain intentional violations for testing specific linting rules.

| File | Rule Tested | Expected Violations |
|------|-------------|---------------------|
| `invalid/duplicate_keys.yaml` | duplicate-keys | 3 (duplicate 'name', 'id', 'port') |
| `invalid/long_lines.yaml` | line-too-long | 3 (lines exceeding 80 chars) |
| `invalid/bad_indentation.yaml` | inconsistent-indentation | 4 (inconsistent spacing) |
| `invalid/trailing_whitespace.yaml` | trailing-whitespace | 5 (lines with trailing spaces) |
| `invalid/empty_values.yaml` | empty-value | 4 (keys with empty values) |
| `invalid/bad_comments.yaml` | comment-spacing/format | 4 (missing spaces in comments) |
| `invalid/octal_values.yaml` | implicit-octal | 3 (values with leading zeros) |

## Edge Cases

These files test edge cases and special YAML features.

| File | Description |
|------|-------------|
| `edge_cases/empty.yaml` | Empty document (only comments) |
| `edge_cases/unicode.yaml` | Unicode characters, emojis, RTL text |
| `edge_cases/multiline.yaml` | Block scalars (literal, folded, various chomping) |

## Usage in Tests

Fixtures are loaded using `include_str!` macro in integration tests:

```rust
#[test]
fn test_fixture_duplicate_keys() {
    let yaml = include_str!("fixtures/invalid/duplicate_keys.yaml");
    let linter = Linter::new(LinterConfig::default());
    let diagnostics = linter.lint(yaml).unwrap();

    let dup_key_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule_id == RuleId::DuplicateKeys)
        .collect();

    assert_eq!(dup_key_errors.len(), 3);
}
```

## Adding New Fixtures

When adding new fixtures:

1. Add descriptive comments at the top indicating what rule is tested
2. Document expected violations count
3. Add inline comments marking specific violations
4. Update this README with the new fixture information
5. Add corresponding test in `tests/fixture_tests.rs`

Example:

```yaml
# Test: new_rule_name
# Expected violations: 2

valid_key: value
invalid_key: bad_value  # violation: reason
another_invalid: oops   # violation: reason
```

## Testing Best Practices

- Each fixture should test a single rule or related set of rules
- Include both positive (should pass) and negative (should fail) cases
- Use realistic YAML content similar to actual use cases
- Document expected behavior clearly in comments
- Keep fixtures focused and minimal while still being meaningful
