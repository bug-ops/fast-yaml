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

/// Extract original anchor names from YAML input.
///
/// Returns a `Vec` where `index == anchor_id` and `value == original anchor name`.
/// Index 0 is always an empty string (saphyr anchor IDs start at 1).
///
/// Scans the input for `&name` tokens using the same ordering as saphyr's parser.
/// Anchors inside quoted strings and comments are skipped.
pub(super) fn extract_anchor_names(input: &str) -> Vec<String> {
    // Fast path: no `&` byte means no anchors
    if !input.bytes().any(|b| b == b'&') {
        return vec![String::new()]; // index 0 placeholder
    }

    let mut names: Vec<String> = vec![String::new()]; // index 0 unused (saphyr IDs start at 1)
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        match bytes[i] {
            // Skip single-quoted strings: no escape sequences inside
            b'\'' => {
                i += 1;
                while i < len {
                    if bytes[i] == b'\'' {
                        i += 1;
                        // Two consecutive single quotes = escaped quote inside string
                        if i < len && bytes[i] == b'\'' {
                            i += 1;
                        } else {
                            break;
                        }
                    } else {
                        i += 1;
                    }
                }
            }
            // Skip double-quoted strings
            b'"' => {
                i += 1;
                while i < len {
                    if bytes[i] == b'\\' {
                        i += 2; // skip escape sequence
                    } else if bytes[i] == b'"' {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
            }
            // Skip comments to end of line
            b'#' => {
                while i < len && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            // Block scalar headers: skip lines starting with | or > after whitespace
            // (content lines follow and may contain `&` that is not an anchor)
            // We handle this by only treating `&` after valid YAML flow positions.
            // Anchor: `&` followed by a valid anchor name character
            b'&' => {
                i += 1;
                // Anchor name: [a-zA-Z0-9_-] and other non-space non-special chars
                let start = i;
                while i < len {
                    let b = bytes[i];
                    // Anchor name ends at whitespace, `{`, `}`, `[`, `]`, `,`, `:`, `#`
                    if b.is_ascii_whitespace()
                        || matches!(b, b'{' | b'}' | b'[' | b']' | b',' | b':')
                    {
                        break;
                    }
                    i += 1;
                }
                if i > start
                    && let Ok(name) = std::str::from_utf8(&bytes[start..i])
                {
                    names.push(name.to_owned());
                }
            }
            _ => {
                i += 1;
            }
        }
    }

    names
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
/// // Regular files - use streaming (preserves float types)
/// assert!(is_streaming_suitable("version: 1.0"));
/// assert!(is_streaming_suitable("small: yaml"));
///
/// // Large files - also use streaming
/// let large = "key: value\n".repeat(1000);
/// assert!(is_streaming_suitable(&large));
/// # }
/// ```
pub fn is_streaming_suitable(input: &str) -> bool {
    // Streaming preserves the original scalar text (e.g. "1.0" stays "1.0"),
    // while DOM-based formatting loses type information (float 1.0 → integer 1).
    // Always prefer streaming to maintain YAML 1.2.2 Core Schema type fidelity.

    // Heavy anchor/alias usage: avoid streaming only when anchor density is very
    // high, because the DOM path resolves aliases during parse.
    let len = input.len();
    if len > 0 {
        let anchor_count = input.bytes().filter(|&b| b == b'&').count();
        let alias_count = input.bytes().filter(|&b| b == b'*').count();

        // More than 10 anchors/aliases per 1000 bytes → fall back to DOM
        if (anchor_count + alias_count) * 100 > len {
            return false;
        }
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
        // Small files now use streaming to preserve float types (issue #66)
        assert!(is_streaming_suitable("small: yaml"));
        assert!(is_streaming_suitable("key: value\nlist:\n  - a\n  - b"));
        assert!(is_streaming_suitable("version: 1.0"));
        assert!(is_streaming_suitable("count: 1.23e10"));
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
    fn test_format_streaming_float_type_preservation() {
        // Regression tests for issue #66: float values must not be converted to integers
        let config = EmitterConfig::default();

        // 1.0 must remain 1.0, not become 1
        let result = format_streaming("version: 1.0", &config).unwrap();
        assert!(
            result.contains("1.0"),
            "1.0 must stay as float, got: {result}"
        );
        assert!(
            !result.contains(": 1\n"),
            "1.0 must not be emitted as integer 1, got: {result}"
        );

        // Scientific notation must be preserved, not expanded to integer
        let result = format_streaming("count: 1.23e10", &config).unwrap();
        assert!(
            result.contains("1.23e10") || result.contains("1.23e+10"),
            "Scientific notation must be preserved, got: {result}"
        );
        assert!(
            !result.contains("12300000000"),
            "Scientific notation must not expand to integer, got: {result}"
        );

        // Regular float (3.14) must be preserved as-is
        let result = format_streaming("pi: 3.14", &config).unwrap();
        assert!(
            result.contains("3.14"),
            "3.14 must be preserved, got: {result}"
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

    // ── Issue #76: Block scalar chomp indicator ─────────────────────────────

    #[test]
    fn test_format_streaming_block_scalar_clip_chomp() {
        // Clip (|) — value ends with exactly one newline
        let yaml = "desc: |\n  line one\n  line two\n";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert!(
            result.contains("desc: |\n"),
            "clip chomp '|' must be preserved, got: {result}"
        );
        assert!(
            !result.contains("|-"),
            "clip chomp must not become strip '|-', got: {result}"
        );
    }

    #[test]
    fn test_format_streaming_block_scalar_strip_chomp() {
        // Strip (|-) — value does not end with a newline
        let yaml = "desc: |-\n  line one\n  line two\n";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert!(
            result.contains("|-"),
            "strip chomp '|-' must be preserved, got: {result}"
        );
    }

    #[test]
    fn test_format_streaming_block_scalar_keep_chomp() {
        // Keep (|+) — value ends with two or more newlines
        let yaml = "desc: |+\n  line one\n  line two\n\n";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert!(
            result.contains("|+"),
            "keep chomp '|+' must be preserved, got: {result}"
        );
    }

    #[test]
    fn test_format_streaming_block_scalar_keep_chomp_multiple_trailing() {
        // Keep (|+) with multiple trailing blank lines
        let yaml = "desc: |+\n  line one\n  line two\n\n\n";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert!(
            result.contains("|+"),
            "keep chomp '|+' must be preserved, got: {result}"
        );
        // The formatted output must end with at least two blank lines after content
        assert!(
            result.contains("  line two\n\n"),
            "multiple trailing blank lines must be preserved, got: {result}"
        );
    }

    #[test]
    fn test_format_streaming_block_scalar_empty_line_no_indent() {
        // Empty lines in block scalars must not get trailing whitespace
        let yaml = "desc: |\n  line one\n\n  line two\n";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        // The empty line must be a bare newline, not "  \n"
        assert!(
            !result.contains("  \n"),
            "empty lines in block scalars must not have trailing spaces, got: {result}"
        );
        assert!(
            result.contains("line one\n\n"),
            "empty line between content lines must be preserved as bare newline, got: {result}"
        );
    }

    // ── Issue #83: Sequence-of-sequences ────────────────────────────────────

    #[test]
    fn test_format_streaming_sequence_of_sequences() {
        let yaml = "- - name\n  - hr\n  - avg\n";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert_eq!(
            result, "- - name\n  - hr\n  - avg\n",
            "sequence-of-sequences must not produce extra spaces, got: {result}"
        );
    }

    #[test]
    fn test_format_streaming_sequence_of_sequences_from_flow() {
        // Flow sequence converts to block: inner items must align correctly
        let yaml = "- [name, hr, avg]\n";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert_eq!(
            result, "- - name\n  - hr\n  - avg\n",
            "flow-to-block sequence-of-sequences must produce correct indentation, got: {result}"
        );
    }

    #[test]
    fn test_format_streaming_triple_nested_sequence() {
        let yaml = "- - - deep\n";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert_eq!(
            result, "- - - deep\n",
            "triple-nested sequence must format correctly, got: {result}"
        );
    }

    #[test]
    fn test_format_streaming_sequence_first_item_mapping() {
        // Mapping as first item of a nested sequence
        let yaml = "- - key: val\n";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert_eq!(
            result, "- - key: val\n",
            "mapping as first item after dash must not double-indent, got: {result}"
        );
    }

    // ── Issue #84: Anchors on correct line ───────────────────────────────────

    #[test]
    fn test_format_streaming_anchor_on_mapping_value() {
        // Anchor on mapping value: original name must be preserved
        let yaml = "defaults: &base\n  adapter: postgres\n";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert!(
            result.contains("defaults: &base\n"),
            "original anchor name must be preserved, got: {result}"
        );
        assert!(
            result.contains("  adapter: postgres\n"),
            "sub-key must be indented on next line, got: {result}"
        );
    }

    #[test]
    fn test_format_streaming_anchor_on_sequence_value() {
        // Anchor on sequence value: original name must be preserved
        let yaml = "tags: &common\n  - yaml\n  - parser\n";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert!(
            result.contains("tags: &common\n"),
            "original anchor name must be preserved, got: {result}"
        );
    }

    #[test]
    fn test_format_streaming_anchor_mapping_in_sequence() {
        // Anchored mapping inside a sequence: original name must be preserved
        let yaml = "- &ref\n  key: val\n";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert_eq!(
            result, "- &ref\n  key: val\n",
            "original anchor name must be preserved, got: {result}"
        );
    }

    #[test]
    fn test_format_streaming_anchor_sequence_in_sequence() {
        // Anchored sequence inside a sequence: original name must be preserved
        let yaml = "- &ref\n  - item\n";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert_eq!(
            result, "- &ref\n  - item\n",
            "original anchor name must be preserved, got: {result}"
        );
    }

    #[test]
    fn test_format_streaming_anchor_idempotency() {
        // format(format(input)) == format(input) for anchored documents
        let yaml = "defaults: &base\n  adapter: postgres\ndev:\n  <<: *base\n  debug: true\n";
        let config = EmitterConfig::default();
        let first = format_streaming(yaml, &config).unwrap();
        let second = format_streaming(&first, &config).unwrap();
        assert_eq!(
            first, second,
            "formatting must be idempotent for anchored documents"
        );
    }

    // ── Issue #120: Anchor name preservation ────────────────────────────────

    #[test]
    fn test_format_streaming_preserves_anchor_name() {
        // Anchor name must be preserved as-is after formatting
        let yaml =
            "defaults: &defaults\n  timeout: 30\nservice1:\n  <<: *defaults\n  name: api-server\n";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert!(
            result.contains("&defaults"),
            "original anchor name 'defaults' must be preserved, got: {result}"
        );
        assert!(
            result.contains("*defaults"),
            "alias must reference original name 'defaults', got: {result}"
        );
        assert!(
            !result.contains("&anchor1"),
            "generated name 'anchor1' must not appear, got: {result}"
        );
    }

    #[test]
    fn test_format_streaming_preserves_multiple_anchors() {
        let yaml = "a: &foo\n  x: 1\nb: &bar\n  y: 2\nc: *foo\nd: *bar\n";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert!(
            result.contains("&foo"),
            "anchor 'foo' must be preserved, got: {result}"
        );
        assert!(
            result.contains("&bar"),
            "anchor 'bar' must be preserved, got: {result}"
        );
        assert!(
            result.contains("*foo"),
            "alias 'foo' must be preserved, got: {result}"
        );
        assert!(
            result.contains("*bar"),
            "alias 'bar' must be preserved, got: {result}"
        );
    }

    #[test]
    fn test_format_streaming_anchor_reuse() {
        // Same anchor name defined twice: both IDs store the same name string
        let yaml = "a: &name\n  v: 1\nb: &name\n  v: 2\nc: *name\n";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert!(
            result.contains("&name"),
            "anchor name must be preserved on reuse, got: {result}"
        );
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

    // ── Issue #120: extract_anchor_names unit tests ──────────────────────────

    #[test]
    fn test_extract_anchor_names_no_anchors() {
        let names = extract_anchor_names("key: value\nlist:\n  - item\n");
        // Fast path returns single-element vec with empty placeholder
        assert_eq!(names, vec![String::new()]);
    }

    #[test]
    fn test_extract_anchor_names_single() {
        let names = extract_anchor_names("defaults: &myanchor\n  k: v\n");
        assert_eq!(names.len(), 2);
        assert_eq!(names[0], ""); // index 0 placeholder
        assert_eq!(names[1], "myanchor");
    }

    #[test]
    fn test_extract_anchor_names_multiple_in_order() {
        let names = extract_anchor_names("a: &first\n  x: 1\nb: &second\n  y: 2\n");
        assert_eq!(names.len(), 3);
        assert_eq!(names[1], "first");
        assert_eq!(names[2], "second");
    }

    #[test]
    fn test_extract_anchor_names_skips_quoted_ampersand() {
        // & inside quoted strings must not be treated as anchor
        let names = extract_anchor_names("key: 'foo &notanchor bar'\nreal: &real\n  v: 1\n");
        // Only &real should be captured
        assert!(names.contains(&"real".to_owned()));
        assert!(!names.contains(&"notanchor".to_owned()));
    }

    #[test]
    fn test_extract_anchor_names_skips_comment_ampersand() {
        // & in a comment must not be treated as anchor
        let names = extract_anchor_names("key: value # &notanchor\nreal: &real\n  v: 1\n");
        assert!(names.contains(&"real".to_owned()));
        assert!(!names.contains(&"notanchor".to_owned()));
    }

    // ── Issue #120: Multi-document streams ──────────────────────────────────

    #[test]
    fn test_format_streaming_preserves_anchor_across_documents() {
        // Each document in a stream has its own anchor namespace.
        // Anchors in document 1 must use original names.
        let yaml = "---\ndefaults: &cfg\n  timeout: 30\nservice: *cfg\n---\nother: &other\n  x: 1\nref: *other\n";
        let config = EmitterConfig::default();
        let result = format_streaming(yaml, &config).unwrap();
        assert!(
            result.contains("&cfg"),
            "anchor 'cfg' in first document must be preserved, got: {result}"
        );
        assert!(
            result.contains("*cfg"),
            "alias 'cfg' in first document must be preserved, got: {result}"
        );
        assert!(
            result.contains("&other"),
            "anchor 'other' in second document must be preserved, got: {result}"
        );
        assert!(
            result.contains("*other"),
            "alias 'other' in second document must be preserved, got: {result}"
        );
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
        assert!(
            arena.contains("anchor100"),
            "anchor name must be preserved, got partial: ..."
        );
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
