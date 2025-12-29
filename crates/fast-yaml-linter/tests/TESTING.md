# Linter Testing Guide

This document describes the testing strategy for the fast-yaml-linter crate.

## Test Structure

```
tests/
├── fixtures/           # Test YAML files
│   ├── valid/         # Files that should pass all rules
│   ├── invalid/       # Files with known violations
│   └── edge_cases/    # Edge cases and special scenarios
├── fixture_tests.rs   # Integration tests using fixtures
└── TESTING.md         # This file
```

## Running Tests

Use `cargo nextest` for faster test execution:

```bash
# Run all linter tests
cargo nextest run -p fast-yaml-linter

# Run specific test
cargo nextest run -p fast-yaml-linter test_invalid_duplicate_keys

# Run with output
cargo nextest run -p fast-yaml-linter --nocapture

# Run only integration tests
cargo nextest run -p fast-yaml-linter --test fixture_tests
```

Traditional cargo test also works:

```bash
# Run all tests
cargo test -p fast-yaml-linter

# Run specific test
cargo test -p fast-yaml-linter test_invalid_duplicate_keys
```

## Test Categories

### Valid Fixtures Tests

Tests that verify valid YAML passes all linting rules:

- `test_valid_simple` - Basic valid YAML structure
- `test_valid_complex` - Nested structures and arrays
- `test_valid_comments` - Proper comment formatting

### Invalid Fixtures Tests

Tests that verify specific rule violations are detected:

- `test_invalid_duplicate_keys` - Duplicate key detection (3 expected)
- `test_invalid_long_lines` - Line length enforcement (3+ expected)
- `test_invalid_bad_indentation` - Indentation consistency (3+ expected)
- `test_invalid_trailing_whitespace` - Trailing whitespace detection (3+ expected)
- `test_invalid_empty_values` - Empty value detection (3+ expected)
- `test_invalid_bad_comments` - Comment formatting (3+ expected)
- `test_invalid_octal_values` - Implicit octal detection (2+ expected)

### Edge Case Tests

Tests for special YAML features and edge cases:

- `test_edge_case_empty` - Empty documents (comments only)
- `test_edge_case_unicode` - Unicode characters, emojis, RTL text
- `test_edge_case_multiline` - Block scalars (literal, folded, chomping)

### Integration Tests

Tests for linter configuration and behavior:

- `test_linter_with_selective_rules` - Enable specific rules only
- `test_linter_with_disabled_rules` - Disable specific rules
- `test_diagnostic_location_accuracy` - Verify line/column accuracy
- `test_all_valid_fixtures_pass` - Batch test all valid fixtures
- `test_all_invalid_fixtures_fail` - Batch test all invalid fixtures

## Adding New Tests

### 1. Add New Fixture

Create a new YAML file in the appropriate subdirectory:

```yaml
# Test: new_rule_name
# Expected violations: N

# Your test YAML content here
key: value  # violation: reason (if applicable)
```

### 2. Add Test Function

Add a test function in `fixture_tests.rs`:

```rust
#[test]
fn test_new_rule() {
    let yaml = include_str!("fixtures/invalid/new_rule.yaml");
    let linter = Linter::new(LinterConfig::default());
    let diagnostics = linter.lint(yaml).unwrap();

    let rule_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule_id == RuleId::NewRule)
        .collect();

    assert_eq!(
        rule_errors.len(),
        N,
        "Expected N violations, found {}: {:?}",
        rule_errors.len(),
        rule_errors
    );
}
```

### 3. Update Documentation

Update `fixtures/README.md` with the new fixture information.

## Test Coverage

Generate test coverage reports:

```bash
# HTML report
cargo llvm-cov --html -p fast-yaml-linter

# Terminal output
cargo llvm-cov -p fast-yaml-linter

# With nextest (faster)
cargo llvm-cov nextest --html -p fast-yaml-linter
```

Coverage targets for fast-yaml-linter:
- Critical code: 80%+
- Rule implementations: 90%+
- Overall: 70%+

## Writing Quality Tests

### Test Naming Convention

Follow the pattern: `test_{fixture_category}_{fixture_name}`

Examples:
- `test_valid_simple`
- `test_invalid_duplicate_keys`
- `test_edge_case_unicode`

### Assertion Best Practices

```rust
// Good: Specific error message
assert_eq!(
    errors.len(),
    3,
    "Expected 3 duplicate key violations, found {}: {:?}",
    errors.len(),
    errors
);

// Good: Verify error content
assert!(
    error.message.contains("duplicate"),
    "Error message should mention 'duplicate'"
);

// Good: Check all violations found
for error in &errors {
    assert!(error.location.line > 0, "Valid line number required");
}
```

### Test Independence

Each test should be independent:

```rust
// Good: Each test creates its own linter
#[test]
fn test_rule_a() {
    let linter = Linter::new(LinterConfig::default());
    // test implementation
}

#[test]
fn test_rule_b() {
    let linter = Linter::new(LinterConfig::default());
    // test implementation
}

// Bad: Shared state between tests
static LINTER: Lazy<Linter> = Lazy::new(|| Linter::new(LinterConfig::default()));
```

### Fixture Quality

Good fixtures should:
- Include clear comments explaining what is being tested
- Specify expected violation counts
- Mark specific violations with inline comments
- Use realistic YAML content
- Test one rule or related set of rules

Example:

```yaml
# Test: duplicate_keys rule
# Expected violations: 2

user:
  name: John
  email: john@example.com
  name: Jane  # violation: duplicate 'name' key

settings:
  theme: dark
  theme: light  # violation: duplicate 'theme' key
```

## Continuous Integration

Tests run automatically in CI on:
- Every push to main
- Every pull request
- Daily scheduled runs

CI enforces:
- All tests pass (100%)
- No new warnings
- Coverage targets met
- Documentation builds successfully

## Debugging Failed Tests

### View Test Output

```bash
# Show all output
cargo nextest run -p fast-yaml-linter --nocapture

# Show only failing tests
cargo nextest run -p fast-yaml-linter --no-capture --failure-output immediate
```

### Inspect Diagnostics

Add debug output in tests:

```rust
#[test]
fn test_debug_diagnostics() {
    let diagnostics = linter.lint(yaml).unwrap();

    eprintln!("Found {} diagnostics:", diagnostics.len());
    for diag in &diagnostics {
        eprintln!(
            "  - Line {}, Col {}: {} ({})",
            diag.location.line,
            diag.location.column,
            diag.message,
            diag.rule_id
        );
    }

    // assertions...
}
```

### Check Fixture Content

```bash
# View fixture with line numbers
cat -n tests/fixtures/invalid/duplicate_keys.yaml

# Check for trailing whitespace
cat -A tests/fixtures/invalid/trailing_whitespace.yaml
```

## Performance Testing

While unit tests focus on correctness, benchmark performance separately:

```bash
# Run benchmarks
cargo bench -p fast-yaml-linter

# Compare against baseline
cargo bench -p fast-yaml-linter --save-baseline main
```

See `benches/` directory for benchmark implementations.
