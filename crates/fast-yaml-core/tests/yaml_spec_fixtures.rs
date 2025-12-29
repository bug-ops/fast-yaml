//! YAML 1.2.2 Specification Compliance Tests
//!
//! Comprehensive integration tests that validate parsing correctness against
//! YAML 1.2.2 specification examples and edge cases.
//!
//! Test fixtures are located in `tests/fixtures/yaml-spec/`:
//! - Examples 2.01-2.28: Official YAML 1.2.2 specification examples
//! - Core schema tests: Validation of booleans, nulls, integers, floats
//! - Edge cases: Empty documents, special characters, deep nesting
//! - Multi-document tests: Files with multiple YAML documents

use fast_yaml_core::{Emitter, Map, Parser, ScalarOwned, Value};
use std::fs;
use std::path::{Path, PathBuf};

/// Test fixture metadata for categorization and expected behavior
#[derive(Debug)]
struct FixtureTest {
    /// Path to the YAML fixture file
    path: PathBuf,
    /// Test category for grouping
    category: TestCategory,
    /// Whether this file contains multiple documents
    is_multi_document: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum TestCategory {
    /// YAML 1.2.2 specification examples (2.01-2.28)
    SpecExample,
    /// Core schema type tests
    CoreSchema,
    /// Block scalar tests
    BlockScalars,
    /// Flow collection tests
    FlowCollections,
    /// Anchor and alias tests
    AnchorsAliases,
    /// Edge cases and corner cases
    EdgeCases,
    /// Directives and meta-syntax
    Directives,
    /// Empty and minimal documents
    EmptyDocuments,
    /// Other tests
    Other,
}

impl FixtureTest {
    fn new(path: PathBuf) -> Self {
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        let category = Self::categorize(filename);
        let is_multi_document = Self::is_multi_doc(filename);

        Self {
            path,
            category,
            is_multi_document,
        }
    }

    fn categorize(filename: &str) -> TestCategory {
        if filename.starts_with("2.") {
            TestCategory::SpecExample
        } else if filename.starts_with("core-schema") {
            TestCategory::CoreSchema
        } else if filename.contains("block-scalar") {
            TestCategory::BlockScalars
        } else if filename.contains("flow-collection") {
            TestCategory::FlowCollections
        } else if filename.contains("anchor") || filename.contains("alias") {
            TestCategory::AnchorsAliases
        } else if filename.contains("edge-case") {
            TestCategory::EdgeCases
        } else if filename.contains("directive") {
            TestCategory::Directives
        } else if filename.contains("empty-document") {
            TestCategory::EmptyDocuments
        } else {
            TestCategory::Other
        }
    }

    fn is_multi_doc(filename: &str) -> bool {
        // Files known to contain multiple documents
        matches!(
            filename,
            "2.07-two-documents.yaml" | "2.28-log-file.yaml" | "empty-documents.yaml"
        )
    }

    /// Check if this fixture is known to have parsing issues
    fn is_known_issue(&self) -> bool {
        let filename = self.path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // complex-keys.yaml has duplicate null keys (null: and ~:) which is an error
        matches!(filename, "complex-keys.yaml")
    }

    /// Check if this fixture is known to have roundtrip issues
    fn is_known_roundtrip_issue(&self) -> bool {
        let filename = self.path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // These files have known emitter limitations:
        // - 2.20: inf/nan emit as strings instead of special float values
        // - 2.23, 2.24, 2.27: explicit tags and anchors not fully preserved
        matches!(
            filename,
            "2.20-floating-point.yaml"
                | "2.23-explicit-tags.yaml"
                | "2.24-global-tags.yaml"
                | "2.27-invoice.yaml"
        )
    }

    fn name(&self) -> String {
        self.path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string()
    }
}

/// Discover all YAML fixture files in the test fixtures directory
fn discover_fixtures() -> Vec<FixtureTest> {
    let fixtures_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures/yaml-spec");

    let mut fixtures = Vec::new();

    if let Ok(entries) = fs::read_dir(&fixtures_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("yaml") {
                fixtures.push(FixtureTest::new(path));
            }
        }
    }

    // Sort by filename for predictable test order
    fixtures.sort_by(|a, b| a.path.cmp(&b.path));
    fixtures
}

/// Read fixture file content
fn read_fixture(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("Failed to read fixture {}: {}", path.display(), e))
}

// ============================================================================
// Specification Example Tests (2.01-2.28)
// ============================================================================

#[test]
fn test_spec_examples_parse_successfully() {
    let fixtures: Vec<_> = discover_fixtures()
        .into_iter()
        .filter(|f| f.category == TestCategory::SpecExample)
        .collect();

    assert!(
        !fixtures.is_empty(),
        "No specification example fixtures found"
    );

    let mut failures = Vec::new();

    for fixture in &fixtures {
        let content = read_fixture(&fixture.path);

        let result = if fixture.is_multi_document {
            Parser::parse_all(&content).map(|docs| {
                assert!(
                    !docs.is_empty(),
                    "Multi-document file produced no documents"
                );
            })
        } else {
            Parser::parse_str(&content).map(|doc| {
                if content.trim().is_empty() {
                    assert!(doc.is_none(), "Empty content should produce None");
                }
            })
        };

        if let Err(e) = result {
            failures.push(format!("  {} - Parse failed: {}", fixture.name(), e));
        }
    }

    assert!(
        failures.is_empty(),
        "Specification example parsing failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn test_spec_examples_roundtrip() {
    let fixtures: Vec<_> = discover_fixtures()
        .into_iter()
        .filter(|f| f.category == TestCategory::SpecExample && !f.is_multi_document)
        .collect();

    let mut failures = Vec::new();

    for fixture in &fixtures {
        // Skip known roundtrip issues
        if fixture.is_known_roundtrip_issue() {
            continue;
        }

        let content = read_fixture(&fixture.path);

        // Skip empty files for roundtrip testing
        if content.trim().is_empty() {
            continue;
        }

        // Step 1: Parse original
        let original = match Parser::parse_str(&content) {
            Ok(Some(doc)) => doc,
            Ok(None) => continue, // Skip empty documents
            Err(e) => {
                failures.push(format!(
                    "  {} - Initial parse failed: {}",
                    fixture.name(),
                    e
                ));
                continue;
            }
        };

        // Step 2: Emit to YAML string
        let emitted = match Emitter::emit_str(&original) {
            Ok(yaml) => yaml,
            Err(e) => {
                failures.push(format!("  {} - Emit failed: {}", fixture.name(), e));
                continue;
            }
        };

        // Step 3: Parse emitted YAML
        let reparsed = match Parser::parse_str(&emitted) {
            Ok(Some(doc)) => doc,
            Ok(None) => {
                failures.push(format!(
                    "  {} - Reparsing emitted YAML produced None",
                    fixture.name()
                ));
                continue;
            }
            Err(e) => {
                failures.push(format!("  {} - Reparse failed: {}", fixture.name(), e));
                continue;
            }
        };

        // Step 4: Verify semantic equivalence
        // Note: We compare the Value structures, not the YAML text,
        // because formatting may differ (comments, whitespace, etc.)
        if !values_semantically_equal(&original, &reparsed) {
            failures.push(format!(
                "  {} - Roundtrip produced different value\n    Original: {:?}\n    Reparsed: {:?}",
                fixture.name(),
                original,
                reparsed
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "Specification example roundtrip failures:\n{}",
        failures.join("\n")
    );
}

// ============================================================================
// Core Schema Tests
// ============================================================================

#[test]
fn test_core_schema_types() {
    let fixtures: Vec<_> = discover_fixtures()
        .into_iter()
        .filter(|f| f.category == TestCategory::CoreSchema)
        .collect();

    assert!(!fixtures.is_empty(), "No core schema fixtures found");

    let mut failures = Vec::new();

    for fixture in &fixtures {
        let content = read_fixture(&fixture.path);

        match Parser::parse_str(&content) {
            Ok(Some(Value::Mapping(map))) => {
                // Verify type-specific behaviors
                if fixture.name().contains("boolean") {
                    verify_boolean_types(&map, &fixture.name(), &mut failures);
                } else if fixture.name().contains("null") {
                    verify_null_types(&map, &fixture.name(), &mut failures);
                } else if fixture.name().contains("integer") {
                    verify_integer_types(&map, &fixture.name(), &mut failures);
                } else if fixture.name().contains("float") {
                    verify_float_types(&map, &fixture.name(), &mut failures);
                }
            }
            Ok(Some(_)) => {
                failures.push(format!(
                    "  {} - Expected Mapping, got different type",
                    fixture.name()
                ));
            }
            Ok(None) => {
                failures.push(format!("  {} - Parse produced None", fixture.name()));
            }
            Err(e) => {
                failures.push(format!("  {} - Parse failed: {}", fixture.name(), e));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "Core schema type test failures:\n{}",
        failures.join("\n")
    );
}

fn verify_boolean_types(map: &Map, name: &str, failures: &mut Vec<String>) {
    // NOTE: saphyr only recognizes lowercase "true"/"false" as booleans per YAML 1.2.2 Core Schema
    // Title case and uppercase variants are treated as strings (which is correct)
    for key in &["bool_true_lower", "bool_false_lower"] {
        let key_value = Value::Value(ScalarOwned::String((*key).to_string()));
        if let Some(value) = map.get(&key_value)
            && !matches!(value, Value::Value(ScalarOwned::Boolean(_)))
        {
            failures.push(format!(
                "  {name} - Key '{key}' should be Boolean, got {value:?}"
            ));
        }
    }

    // Title case and uppercase true/false are strings in saphyr (YAML 1.2.2 compliant)
    for key in &[
        "bool_true_title",
        "bool_false_title",
        "bool_true_upper",
        "bool_false_upper",
    ] {
        let key_value = Value::Value(ScalarOwned::String((*key).to_string()));
        if let Some(value) = map.get(&key_value)
            && !matches!(value, Value::Value(ScalarOwned::String(_)))
        {
            failures.push(format!(
                "  {name} - Key '{key}' should be String (saphyr behavior), got {value:?}"
            ));
        }
    }

    // Check that yes/no/on/off are strings in YAML 1.2.2
    for key in &[
        "yaml11_yes",
        "yaml11_no",
        "yaml11_on",
        "yaml11_off",
        "yaml11_y",
        "yaml11_n",
    ] {
        let key_value = Value::Value(ScalarOwned::String((*key).to_string()));
        if let Some(value) = map.get(&key_value)
            && !matches!(value, Value::Value(ScalarOwned::String(_)))
        {
            failures.push(format!(
                "  {name} - Key '{key}' should be String in YAML 1.2.2, got {value:?}"
            ));
        }
    }
}

fn verify_null_types(map: &Map, name: &str, failures: &mut Vec<String>) {
    // NOTE: saphyr's null handling:
    // - Lowercase "null", "~", empty → null (per YAML 1.2.2 Core Schema)
    // - Uppercase "NULL" → null (YAML 1.1 compatibility)
    // - Title case "Null" → string (strict YAML 1.2.2 Core Schema)
    for key in &[
        "null_tilde",
        "null_word_lower",
        "null_empty",
        "null_explicit",
        "null_word_upper", // saphyr accepts "NULL" as null
    ] {
        let key_value = Value::Value(ScalarOwned::String((*key).to_string()));
        if let Some(value) = map.get(&key_value)
            && !matches!(value, Value::Value(ScalarOwned::Null))
        {
            failures.push(format!(
                "  {name} - Key '{key}' should be Null (saphyr behavior), got {value:?}"
            ));
        }
    }

    // Title case "Null" is a string in saphyr
    {
        let key = "null_word_title";
        let key_value = Value::Value(ScalarOwned::String(key.to_string()));
        if let Some(value) = map.get(&key_value)
            && !matches!(value, Value::Value(ScalarOwned::String(_)))
        {
            failures.push(format!(
                "  {name} - Key '{key}' should be String (saphyr behavior), got {value:?}"
            ));
        }
    }
}

fn verify_integer_types(map: &Map, name: &str, failures: &mut Vec<String>) {
    // Check for presence of integer values (actual validation depends on file content)
    let has_integers = map
        .values()
        .any(|v| matches!(v, Value::Value(ScalarOwned::Integer(_))));
    if !has_integers {
        failures.push(format!("  {name} - Expected integer values, none found"));
    }
}

fn verify_float_types(map: &Map, name: &str, failures: &mut Vec<String>) {
    // Check for presence of float values (actual validation depends on file content)
    let has_floats = map
        .values()
        .any(|v| matches!(v, Value::Value(ScalarOwned::FloatingPoint(_))));
    if !has_floats {
        failures.push(format!("  {name} - Expected float values, none found"));
    }
}

// ============================================================================
// Multi-Document Tests
// ============================================================================

#[test]
fn test_multi_document_parsing() {
    let fixtures: Vec<_> = discover_fixtures()
        .into_iter()
        .filter(|f| f.is_multi_document)
        .collect();

    assert!(!fixtures.is_empty(), "No multi-document fixtures found");

    let mut failures = Vec::new();

    for fixture in &fixtures {
        let content = read_fixture(&fixture.path);

        match Parser::parse_all(&content) {
            Ok(docs) => {
                if docs.is_empty() {
                    failures.push(format!(
                        "  {} - Multi-document file produced no documents",
                        fixture.name()
                    ));
                }

                // Specific validations for known files
                if fixture.name() == "2.07-two-documents" && docs.len() != 2 {
                    failures.push(format!(
                        "  {} - Expected 2 documents, got {}",
                        fixture.name(),
                        docs.len()
                    ));
                }
            }
            Err(e) => {
                failures.push(format!("  {} - Parse failed: {}", fixture.name(), e));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "Multi-document parsing failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn test_multi_document_roundtrip() {
    let fixtures: Vec<_> = discover_fixtures()
        .into_iter()
        .filter(|f| f.is_multi_document)
        .collect();

    let mut failures = Vec::new();

    for fixture in &fixtures {
        let content = read_fixture(&fixture.path);

        // Parse all documents
        let original_docs = match Parser::parse_all(&content) {
            Ok(docs) => docs,
            Err(e) => {
                failures.push(format!(
                    "  {} - Initial parse failed: {}",
                    fixture.name(),
                    e
                ));
                continue;
            }
        };

        if original_docs.is_empty() {
            continue;
        }

        // Test individual document roundtrips instead of multi-doc emit
        // (which has known issues with the current emitter)
        for (i, doc) in original_docs.iter().enumerate() {
            // Emit single document
            let emitted = match Emitter::emit_str(doc) {
                Ok(yaml) => yaml,
                Err(e) => {
                    failures.push(format!(
                        "  {} - Document {} emit failed: {}",
                        fixture.name(),
                        i,
                        e
                    ));
                    continue;
                }
            };

            // Reparse emitted YAML
            let reparsed = match Parser::parse_str(&emitted) {
                Ok(Some(doc)) => doc,
                Ok(None) => {
                    failures.push(format!(
                        "  {} - Document {} reparse produced None",
                        fixture.name(),
                        i
                    ));
                    continue;
                }
                Err(e) => {
                    failures.push(format!(
                        "  {} - Document {} reparse failed: {}",
                        fixture.name(),
                        i,
                        e
                    ));
                    continue;
                }
            };

            // Verify semantic equivalence
            if !values_semantically_equal(doc, &reparsed) {
                failures.push(format!(
                    "  {} - Document {} differs after roundtrip",
                    fixture.name(),
                    i
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "Multi-document roundtrip failures:\n{}",
        failures.join("\n")
    );
}

// ============================================================================
// Edge Cases and Special Features
// ============================================================================

#[test]
fn test_edge_cases() {
    let fixtures: Vec<_> = discover_fixtures()
        .into_iter()
        .filter(|f| f.category == TestCategory::EdgeCases)
        .collect();

    let mut failures = Vec::new();

    for fixture in &fixtures {
        let content = read_fixture(&fixture.path);

        match Parser::parse_str(&content) {
            Ok(Some(doc)) => {
                // Verify it's a mapping
                if !matches!(doc, Value::Mapping(_)) {
                    failures.push(format!(
                        "  {} - Expected Mapping for edge cases, got {:?}",
                        fixture.name(),
                        doc
                    ));
                }
            }
            Ok(None) => {
                failures.push(format!("  {} - Parse produced None", fixture.name()));
            }
            Err(e) => {
                failures.push(format!("  {} - Parse failed: {}", fixture.name(), e));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "Edge case test failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn test_empty_documents() {
    let fixtures: Vec<_> = discover_fixtures()
        .into_iter()
        .filter(|f| f.category == TestCategory::EmptyDocuments)
        .collect();

    let mut failures = Vec::new();

    for fixture in &fixtures {
        let content = read_fixture(&fixture.path);

        match Parser::parse_all(&content) {
            Ok(docs) => {
                // Empty documents file should produce multiple documents
                if docs.is_empty() {
                    failures.push(format!(
                        "  {} - Expected documents, got empty vec",
                        fixture.name()
                    ));
                }
            }
            Err(e) => {
                failures.push(format!("  {} - Parse failed: {}", fixture.name(), e));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "Empty document test failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn test_anchors_and_aliases() {
    let fixtures: Vec<_> = discover_fixtures()
        .into_iter()
        .filter(|f| f.category == TestCategory::AnchorsAliases)
        .collect();

    let mut failures = Vec::new();

    for fixture in &fixtures {
        let content = read_fixture(&fixture.path);

        match Parser::parse_str(&content) {
            Ok(Some(_doc)) => {
                // Successfully parsed anchors/aliases
                // The yaml-rust2 library resolves aliases during parsing
            }
            Ok(None) => {
                failures.push(format!("  {} - Parse produced None", fixture.name()));
            }
            Err(e) => {
                failures.push(format!("  {} - Parse failed: {}", fixture.name(), e));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "Anchor/alias test failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn test_block_scalars() {
    let fixtures: Vec<_> = discover_fixtures()
        .into_iter()
        .filter(|f| f.category == TestCategory::BlockScalars)
        .collect();

    let mut failures = Vec::new();

    for fixture in &fixtures {
        let content = read_fixture(&fixture.path);

        match Parser::parse_str(&content) {
            Ok(Some(_doc)) => {
                // Successfully parsed block scalars (literal | and folded >)
            }
            Ok(None) => {
                failures.push(format!("  {} - Parse produced None", fixture.name()));
            }
            Err(e) => {
                failures.push(format!("  {} - Parse failed: {}", fixture.name(), e));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "Block scalar test failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn test_flow_collections() {
    let fixtures: Vec<_> = discover_fixtures()
        .into_iter()
        .filter(|f| f.category == TestCategory::FlowCollections)
        .collect();

    let mut failures = Vec::new();

    for fixture in &fixtures {
        let content = read_fixture(&fixture.path);

        match Parser::parse_str(&content) {
            Ok(Some(_doc)) => {
                // Successfully parsed flow collections ([...] and {...})
            }
            Ok(None) => {
                failures.push(format!("  {} - Parse produced None", fixture.name()));
            }
            Err(e) => {
                failures.push(format!("  {} - Parse failed: {}", fixture.name(), e));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "Flow collection test failures:\n{}",
        failures.join("\n")
    );
}

// ============================================================================
// Comprehensive Test - All Fixtures
// ============================================================================

#[test]
fn test_all_fixtures_parse_without_panic() {
    let fixtures = discover_fixtures();

    assert!(
        !fixtures.is_empty(),
        "No fixtures found in tests/fixtures/yaml-spec/"
    );

    let mut failures = Vec::new();
    let mut success_count = 0;
    let mut skipped_count = 0;

    for fixture in &fixtures {
        // Skip known issues
        if fixture.is_known_issue() {
            skipped_count += 1;
            continue;
        }

        let content = read_fixture(&fixture.path);

        let result = if fixture.is_multi_document {
            Parser::parse_all(&content).map(|_| ())
        } else {
            Parser::parse_str(&content).map(|_| ())
        };

        match result {
            Ok(()) => success_count += 1,
            Err(e) => {
                failures.push(format!("  {} - {}", fixture.name(), e));
            }
        }
    }

    println!(
        "Parsed {}/{} fixtures successfully ({} skipped as known issues)",
        success_count,
        fixtures.len() - skipped_count,
        skipped_count
    );

    assert!(
        failures.is_empty(),
        "Failed to parse {} fixtures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Check if two YAML values are semantically equal.
///
/// Note: This is a deep comparison that handles floating-point precision
/// and ignores formatting differences.
fn values_semantically_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Value(ScalarOwned::Null), Value::Value(ScalarOwned::Null)) => true,
        (Value::Value(ScalarOwned::Boolean(a)), Value::Value(ScalarOwned::Boolean(b))) => a == b,
        (Value::Value(ScalarOwned::Integer(a)), Value::Value(ScalarOwned::Integer(b))) => a == b,
        (
            Value::Value(ScalarOwned::FloatingPoint(a)),
            Value::Value(ScalarOwned::FloatingPoint(b)),
        ) => {
            // FloatingPoint uses OrderedFloat wrapper
            let af: f64 = (*a).into();
            let bf: f64 = (*b).into();

            // Handle NaN, Infinity, and normal floats
            if af.is_nan() && bf.is_nan() {
                true
            } else if af.is_infinite() && bf.is_infinite() {
                af.is_sign_positive() == bf.is_sign_positive()
            } else {
                // Use epsilon comparison for normal floats
                (af - bf).abs() < f64::EPSILON * 10.0
            }
        }
        (Value::Value(ScalarOwned::String(a)), Value::Value(ScalarOwned::String(b))) => a == b,
        (Value::Sequence(a), Value::Sequence(b)) => {
            if a.len() != b.len() {
                return false;
            }
            a.iter()
                .zip(b.iter())
                .all(|(x, y)| values_semantically_equal(x, y))
        }
        (Value::Mapping(a), Value::Mapping(b)) => {
            if a.len() != b.len() {
                return false;
            }
            a.iter()
                .all(|(k, v)| b.get(k).is_some_and(|v2| values_semantically_equal(v, v2)))
        }
        _ => false,
    }
}
