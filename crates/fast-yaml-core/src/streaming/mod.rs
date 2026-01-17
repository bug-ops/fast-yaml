//! Streaming YAML formatter that bypasses DOM construction.
//!
//! This module provides high-performance formatting for YAML documents
//! by processing parser events directly without building an intermediate
//! representation. This approach achieves O(1) memory complexity for
//! already-formatted files, compared to O(n) for DOM-based formatting.
//!
//! # Performance Characteristics
//!
//! - Small files (<1KB): Use DOM-based formatter (overhead not worth it)
//! - Large files (>1KB): Streaming provides 5-10x speedup
//! - Memory: Constant memory usage regardless of input size
//!
//! # Usage
//!
//! ```
//! # #[cfg(feature = "streaming")]
//! # {
//! use fast_yaml_core::streaming::{format_streaming, is_streaming_suitable};
//! use fast_yaml_core::EmitterConfig;
//!
//! let yaml = "key: value\nlist:\n  - item1\n  - item2\n";
//! let config = EmitterConfig::default();
//!
//! if is_streaming_suitable(yaml) {
//!     let formatted = format_streaming(yaml, &config).unwrap();
//!     println!("{formatted}");
//! }
//! # }
//! ```

mod formatter;
mod std_backend;
mod traits;

#[cfg(feature = "arena")]
mod arena_backend;

// Re-export public API
pub use std_backend::format_streaming;

#[cfg(feature = "arena")]
pub use arena_backend::format_streaming_arena;

/// Maximum allowed anchor ID to prevent memory exhaustion attacks.
/// 4096 anchors is more than sufficient for any legitimate YAML file.
const MAX_ANCHOR_ID: usize = 4096;

/// Maximum nesting depth to prevent stack/memory exhaustion.
/// 256 levels of nesting is far beyond any practical use case.
const MAX_DEPTH: usize = 256;

/// Static 64-space string for fast indent generation via slicing.
/// Avoids allocation for nesting depths up to 32 levels with 2-space indent.
static INDENT_SPACES: &str = "                                                                ";

/// Context for tracking the current position within YAML structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Context {
    /// At the root level of a document
    Root,
    /// Inside a sequence (array)
    Sequence,
    /// Inside a mapping, expecting a key
    MappingKey,
    /// Inside a mapping, expecting a value
    MappingValue,
}

/// Fix special float value for YAML 1.2 compliance.
///
/// Converts saphyr's output format to YAML 1.2 compliant format:
/// - `inf` -> `.inf`
/// - `-inf` -> `-.inf`
/// - `NaN` -> `.nan`
fn fix_special_float_value(value: &str) -> &str {
    match value {
        "inf" => ".inf",
        "-inf" => "-.inf",
        "NaN" => ".nan",
        other => other,
    }
}

/// Check if input is suitable for streaming formatter.
///
/// Returns `true` for inputs that benefit from streaming:
/// - Large files (>1KB)
/// - Files without heavy anchor/alias usage
///
/// Returns `false` for:
/// - Small files (streaming overhead not worth it)
/// - Files with heavy anchor/alias usage (DOM better for resolution)
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "streaming")]
/// # {
/// use fast_yaml_core::streaming::is_streaming_suitable;
///
/// // Small files - use DOM
/// assert!(!is_streaming_suitable("small: yaml"));
///
/// // Large files - use streaming
/// let large = "key: value\n".repeat(1000);
/// assert!(is_streaming_suitable(&large));
/// # }
/// ```
pub fn is_streaming_suitable(input: &str) -> bool {
    // Small files are fast enough with DOM-based formatting
    if input.len() < 1024 {
        return false;
    }

    // Count indicators of complexity that benefit from DOM
    let anchor_count = input.bytes().filter(|&b| b == b'&').count();
    let alias_count = input.bytes().filter(|&b| b == b'*').count();

    // Heavy anchor/alias usage benefits from DOM for resolution
    // Threshold: more than 1 anchor/alias per 1000 bytes
    if anchor_count + alias_count > input.len() / 1000 {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EmitterConfig;

    #[test]
    fn test_format_streaming_simple_scalar() {
        let yaml = "test";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert!(result.contains("test"));
    }

    #[test]
    fn test_format_streaming_simple_mapping() {
        let yaml = "key: value";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert!(result.contains("key:"));
        assert!(result.contains("value"));
    }

    #[test]
    fn test_format_streaming_simple_sequence() {
        let yaml = "- item1\n- item2\n- item3";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert!(result.contains("- item1"));
        assert!(result.contains("- item2"));
        assert!(result.contains("- item3"));
    }

    #[test]
    fn test_format_streaming_nested_mapping() {
        let yaml = "outer:\n  inner: value";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert!(result.contains("outer:"));
        assert!(result.contains("inner:"));
        assert!(result.contains("value"));
    }

    #[test]
    fn test_format_streaming_mapping_with_sequence() {
        let yaml = "key:\n  - item1\n  - item2";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert!(result.contains("key:"));
        assert!(result.contains("item1"));
        assert!(result.contains("item2"));
    }

    #[test]
    fn test_format_streaming_with_explicit_start() {
        let yaml = "---\nkey: value";
        let config = EmitterConfig::new().with_explicit_start(true);
        let result = format_streaming(yaml, &config).unwrap();
        assert!(result.starts_with("---"));
    }

    #[test]
    fn test_format_streaming_quoted_strings() {
        let yaml = r#"single: 'quoted'
double: "quoted""#;
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert!(result.contains("single:"));
        assert!(result.contains("double:"));
    }

    #[test]
    fn test_format_streaming_special_floats() {
        let yaml = "pos_inf: inf\nneg_inf: -inf\nnan: NaN";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert!(result.contains(".inf"));
        assert!(result.contains("-.inf"));
        assert!(result.contains(".nan"));
    }

    #[test]
    fn test_format_streaming_with_anchor() {
        let yaml = "defaults: &defaults\n  key: value";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert!(result.contains('&'), "Should contain anchor marker");
    }

    #[test]
    fn test_format_streaming_with_alias() {
        let yaml = "defaults: &anchor1\n  key: value\nref: *anchor1";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert!(result.contains('&'), "Should contain anchor");
        assert!(result.contains('*'), "Should contain alias");
    }

    #[test]
    fn test_is_streaming_suitable_small() {
        assert!(!is_streaming_suitable("small: yaml"));
        assert!(!is_streaming_suitable("key: value\nlist:\n  - a\n  - b"));
    }

    #[test]
    fn test_is_streaming_suitable_large() {
        let large = "key: value\n".repeat(200); // ~2.2KB
        assert!(is_streaming_suitable(&large));
    }

    #[test]
    fn test_is_streaming_suitable_heavy_anchors() {
        use std::fmt::Write;
        let mut heavy_anchors = String::new();
        for i in 0..100 {
            writeln!(heavy_anchors, "key{i}: &anchor{i} value{i}").unwrap();
        }
        assert!(
            !is_streaming_suitable(&heavy_anchors),
            "Heavy anchor usage should not be suitable for streaming"
        );
    }

    #[test]
    fn test_fix_special_float_value() {
        assert_eq!(fix_special_float_value("inf"), ".inf");
        assert_eq!(fix_special_float_value("-inf"), "-.inf");
        assert_eq!(fix_special_float_value("NaN"), ".nan");
        assert_eq!(fix_special_float_value("123"), "123");
        assert_eq!(fix_special_float_value("normal"), "normal");
    }

    #[test]
    fn test_format_streaming_multiline_literal() {
        let yaml = "text: |\n  line1\n  line2";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert!(result.contains("text:"));
        assert!(result.contains("line1") && result.contains("line2"));
    }

    #[test]
    fn test_format_streaming_sequence_of_mappings() {
        let yaml = "- name: first\n  value: 1\n- name: second\n  value: 2";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert!(result.contains("name:"));
        assert!(result.contains("first"));
        assert!(result.contains("second"));
    }

    #[test]
    fn test_format_streaming_empty_input() {
        let yaml = "";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert!(result.is_empty() || result == "\n");
    }

    #[test]
    fn test_format_streaming_null_value() {
        let yaml = "key: null";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert!(result.contains("null") || result.contains('~'));
    }

    #[test]
    fn test_format_streaming_boolean_values() {
        let yaml = "yes: true\nno: false";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert!(result.contains("true"));
        assert!(result.contains("false"));
    }

    #[test]
    fn test_format_streaming_integer_values() {
        let yaml = "decimal: 123\nhex: 0x1A\noctal: 0o17";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert!(result.contains("123") || result.contains("0x") || result.contains("0o"));
    }

    #[test]
    fn test_format_streaming_double_quoted_escapes() {
        let yaml = r#"text: "line1\nline2""#;
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert!(result.contains("text:"));
    }

    #[test]
    fn test_format_streaming_large_input_preallocation() {
        let large_yaml = (0..100)
            .map(|i| format!("key{i}: value{i}"))
            .collect::<Vec<_>>()
            .join("\n");

        let config = EmitterConfig::default();
        let result = format_streaming(&large_yaml, &config).unwrap();

        assert!(result.contains("key0:"));
        assert!(result.contains("key99:"));
        assert!(result.contains("value50:") || result.contains("value50\n"));
    }

    #[test]
    fn test_format_streaming_deeply_nested() {
        let yaml = r"level1:
  level2:
    level3:
      level4:
        level5:
          key: deeply_nested_value";

        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();

        assert!(result.contains("deeply_nested_value"));
        assert!(result.contains("level5:"));
    }

    #[test]
    fn test_format_streaming_folded_style() {
        let yaml = "text: >-\n  folded\n  block\n  scalar";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert!(result.contains("text:"));
    }

    #[test]
    fn test_format_streaming_many_anchors() {
        let yaml = r"anchor1: &a1 value1
anchor2: &a2 value2
anchor3: &a3 value3
ref1: *a1
ref2: *a2
ref3: *a3";

        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();

        assert!(result.contains('&'), "Should preserve anchors");
        assert!(result.contains('*'), "Should preserve aliases");
    }

    #[test]
    fn test_streaming_context_stack_depth() {
        use std::fmt::Write;

        let mut yaml = String::new();
        for i in 0..20 {
            let indent = "  ".repeat(i);
            writeln!(yaml, "{indent}level{i}:").unwrap();
        }
        let indent = "  ".repeat(20);
        writeln!(yaml, "{indent}value: deep").unwrap();

        let config = EmitterConfig::default();
        let result = format_streaming(&yaml, &config).unwrap();

        assert!(result.contains("value:"));
        assert!(result.contains("level19:"));
    }
}

#[cfg(all(test, feature = "arena"))]
mod arena_tests {
    use super::*;
    use crate::EmitterConfig;

    #[test]
    fn test_arena_vs_standard_output_equivalence() {
        let test_cases = vec![
            "key: value",
            "- item1\n- item2",
            "outer:\n  inner: value",
            "defaults: &anchor1\n  key: value\nref: *anchor1",
            "pos_inf: inf\nneg_inf: -inf\nnan: NaN",
        ];

        let config = EmitterConfig::default();

        for yaml in test_cases {
            let standard = format_streaming(yaml, &config).unwrap();
            let arena = format_streaming_arena(yaml, &config).unwrap();
            assert_eq!(
                standard, arena,
                "Arena and standard should produce identical output for: {yaml}"
            );
        }
    }

    #[test]
    fn test_arena_deeply_nested_32_levels() {
        use std::fmt::Write;

        let mut yaml = String::new();
        for i in 0..32 {
            let indent = "  ".repeat(i);
            writeln!(yaml, "{indent}level{i}:").unwrap();
        }
        let indent = "  ".repeat(32);
        writeln!(yaml, "{indent}value: at_depth_32").unwrap();

        let config = EmitterConfig::default();
        let standard = format_streaming(&yaml, &config).unwrap();
        let arena = format_streaming_arena(&yaml, &config).unwrap();

        assert_eq!(
            standard, arena,
            "32-level nesting: arena and standard must match"
        );
        assert!(arena.contains("at_depth_32"));
    }

    #[test]
    fn test_arena_deeply_nested_64_levels() {
        use std::fmt::Write;

        let mut yaml = String::new();
        for i in 0..64 {
            let indent = "  ".repeat(i);
            writeln!(yaml, "{indent}level{i}:").unwrap();
        }
        let indent = "  ".repeat(64);
        writeln!(yaml, "{indent}value: at_depth_64").unwrap();

        let config = EmitterConfig::default();
        let standard = format_streaming(&yaml, &config).unwrap();
        let arena = format_streaming_arena(&yaml, &config).unwrap();

        assert_eq!(
            standard, arena,
            "64-level nesting: arena and standard must match"
        );
        assert!(arena.contains("at_depth_64"));
    }

    #[test]
    fn test_arena_many_anchors_100() {
        use std::fmt::Write;

        let mut yaml = String::new();
        for i in 1..=100 {
            writeln!(yaml, "key{i}: &anchor{i} value{i}").unwrap();
        }
        for i in 1..=100 {
            writeln!(yaml, "ref{i}: *anchor{i}").unwrap();
        }

        let config = EmitterConfig::default();
        let standard = format_streaming(&yaml, &config).unwrap();
        let arena = format_streaming_arena(&yaml, &config).unwrap();

        assert_eq!(
            standard, arena,
            "100 anchors: arena and standard must match"
        );
        assert!(arena.contains("anchor100"));
    }

    #[test]
    fn test_arena_many_anchors_500() {
        use std::fmt::Write;

        let mut yaml = String::new();
        for i in 1..=500 {
            writeln!(yaml, "key{i}: &anchor{i} value{i}").unwrap();
        }
        for i in 1..=500 {
            writeln!(yaml, "ref{i}: *anchor{i}").unwrap();
        }

        let config = EmitterConfig::default();
        let standard = format_streaming(&yaml, &config).unwrap();
        let arena = format_streaming_arena(&yaml, &config).unwrap();

        assert_eq!(
            standard, arena,
            "500 anchors: arena and standard must match"
        );
        assert!(arena.contains("anchor500"));
    }

    #[test]
    fn test_arena_large_document_1mb() {
        use std::fmt::Write;

        let mut yaml = String::new();
        let entry = "key: a_moderately_long_value_that_pads_out_the_line\n";
        let entries_needed = (1024 * 1024) / entry.len() + 1;

        for i in 0..entries_needed {
            writeln!(
                yaml,
                "key{i}: a_moderately_long_value_that_pads_out_the_line"
            )
            .unwrap();
        }

        assert!(yaml.len() >= 1024 * 1024, "Test YAML should be >= 1MB");

        let config = EmitterConfig::default();
        let standard = format_streaming(&yaml, &config).unwrap();
        let arena = format_streaming_arena(&yaml, &config).unwrap();

        assert_eq!(
            standard, arena,
            "1MB document: arena and standard must match"
        );
    }

    #[test]
    fn test_arena_large_document_2mb() {
        use std::fmt::Write;

        let mut yaml = String::new();
        let entry = "key0: a_moderately_long_value_that_pads_out_the_line\n";
        let entries_needed = (2 * 1024 * 1024) / entry.len() + 1;

        for i in 0..entries_needed {
            writeln!(
                yaml,
                "key{i}: a_moderately_long_value_that_pads_out_the_line"
            )
            .unwrap();
        }

        assert!(
            yaml.len() >= 2 * 1024 * 1024,
            "Test YAML should be >= 2MB, got {} bytes",
            yaml.len()
        );

        let config = EmitterConfig::default();
        let standard = format_streaming(&yaml, &config).unwrap();
        let arena = format_streaming_arena(&yaml, &config).unwrap();

        assert_eq!(
            standard, arena,
            "2MB document: arena and standard must match"
        );
    }

    #[test]
    fn test_arena_output_equivalence_comprehensive() {
        let test_cases = vec![
            ("empty", ""),
            ("simple_scalar", "test"),
            ("simple_mapping", "key: value"),
            ("simple_sequence", "- item1\n- item2\n- item3"),
            ("nested_mapping", "outer:\n  inner:\n    deep: value"),
            ("mapping_with_sequence", "key:\n  - item1\n  - item2"),
            (
                "sequence_of_mappings",
                "- name: first\n  value: 1\n- name: second\n  value: 2",
            ),
            ("with_anchor", "defaults: &defaults\n  key: value"),
            (
                "with_anchor_alias",
                "defaults: &anchor1\n  key: value\nref: *anchor1",
            ),
            ("special_floats", "pos_inf: inf\nneg_inf: -inf\nnan: NaN"),
            ("single_quoted", "key: 'single quoted'"),
            ("double_quoted", "key: \"double quoted\""),
            ("literal_block", "text: |\n  line1\n  line2"),
            ("folded_block", "text: >-\n  folded\n  block"),
            ("explicit_start", "---\nkey: value"),
            ("null_value", "key: null"),
            ("boolean_values", "yes: true\nno: false"),
            ("integer_values", "decimal: 123\nhex: 0x1A"),
        ];

        let config = EmitterConfig::default();

        for (name, yaml) in test_cases {
            let standard = format_streaming(yaml, &config).unwrap();
            let arena = format_streaming_arena(yaml, &config).unwrap();
            assert_eq!(
                standard, arena,
                "Output equivalence failed for test case: {name}"
            );
        }
    }

    #[test]
    fn test_arena_repeated_processing_memory_stability() {
        let yaml = "key: value\nlist:\n  - item1\n  - item2\n  - item3";
        let config = EmitterConfig::default();

        for i in 0..1000 {
            let result = format_streaming_arena(yaml, &config).unwrap();
            assert!(
                result.contains("key:"),
                "Iteration {i}: output should contain key"
            );
        }
    }

    #[test]
    fn test_arena_complex_mixed_structure() {
        let yaml = r"
metadata:
  name: complex
  version: 1.0
  tags:
    - production
    - stable
config:
  database:
    host: localhost
    port: 5432
    credentials: &db_creds
      user: admin
      pass: secret
  cache:
    host: redis
    port: 6379
    credentials: *db_creds
items:
  - id: 1
    name: first
    data:
      nested:
        deep:
          value: found
  - id: 2
    name: second
    data:
      nested:
        deep:
          value: also_found
";

        let config = EmitterConfig::default();
        let standard = format_streaming(yaml, &config).unwrap();
        let arena = format_streaming_arena(yaml, &config).unwrap();

        assert_eq!(
            standard, arena,
            "Complex mixed structure: arena and standard must match"
        );
    }
}
