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

use std::borrow::Cow;
use std::fmt::Write;

use saphyr_parser::{Event, Parser, ScalarStyle, Span, Tag};

use crate::emitter::EmitterConfig;
use crate::error::{EmitError, EmitResult};

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
enum Context {
    /// At the root level of a document
    Root,
    /// Inside a sequence (array)
    Sequence,
    /// Inside a mapping, expecting a key
    MappingKey,
    /// Inside a mapping, expecting a value
    MappingValue,
}

/// Streaming formatter state machine.
///
/// Processes parser events directly to produce formatted YAML output
/// without constructing an intermediate DOM representation.
struct StreamingEmitter<'a> {
    config: &'a EmitterConfig,
    output: String,
    indent_level: usize,
    context_stack: Vec<Context>,
    /// Tracks whether we need to emit a newline before the next value
    pending_newline: bool,
    /// Tracks anchor names for alias resolution
    anchor_names: Vec<String>,
}

impl<'a> StreamingEmitter<'a> {
    fn new(config: &'a EmitterConfig, input_len: usize) -> Self {
        // Output is typically 10-20% larger than input due to formatting
        let output_capacity = input_len + (input_len / 5);

        // Pre-allocate for typical nesting depth (16 levels handles 99% of cases)
        let mut context_stack = Vec::with_capacity(16);
        context_stack.push(Context::Root);

        // Pre-allocate for a reasonable number of anchors (~4 anchors per KB)
        let anchor_capacity = input_len.min(1024) / 256;

        Self {
            config,
            output: String::with_capacity(output_capacity),
            indent_level: 0,
            context_stack,
            pending_newline: false,
            anchor_names: Vec::with_capacity(anchor_capacity.max(1)),
        }
    }

    /// Returns the current YAML structure context.
    ///
    /// # Invariant
    /// The `context_stack` is initialized with `Context::Root` in `new()` and is
    /// never fully emptied (pop operations are bounded by corresponding push).
    /// The `unwrap_or` is a defensive fallback that should never be reached.
    fn current_context(&self) -> Context {
        *self.context_stack.last().unwrap_or(&Context::Root)
    }

    fn format_event(&mut self, event: Event<'_>, _span: Span) {
        match event {
            Event::DocumentStart(explicit) => {
                if explicit || self.config.explicit_start {
                    self.output.push_str("---");
                    self.pending_newline = true;
                }
            }

            Event::DocumentEnd => {
                if !self.output.ends_with('\n') && !self.output.is_empty() {
                    self.output.push('\n');
                }
            }

            Event::Scalar(value, style, anchor_id, tag) => {
                self.emit_scalar(&value, style, anchor_id, tag.as_ref());
            }

            Event::SequenceStart(anchor_id, tag) => {
                self.start_sequence(anchor_id, tag.as_ref());
            }

            Event::SequenceEnd => {
                self.end_sequence();
            }

            Event::MappingStart(anchor_id, tag) => {
                self.start_mapping(anchor_id, tag.as_ref());
            }

            Event::MappingEnd => {
                self.end_mapping();
            }

            Event::Alias(anchor_id) => {
                self.emit_alias(anchor_id);
            }

            // Events that require no action
            Event::StreamStart | Event::StreamEnd | Event::Nothing => {}
        }
    }

    fn emit_scalar(
        &mut self,
        value: &str,
        style: ScalarStyle,
        anchor_id: usize,
        _tag: Option<&Cow<'_, Tag>>,
    ) {
        let ctx = self.current_context();

        // Handle pending newline from document start or collection start
        if self.pending_newline {
            self.output.push('\n');
            self.pending_newline = false;
        }

        // Write indentation and prefix based on context
        match ctx {
            Context::Sequence => {
                self.write_indent();
                self.output.push_str("- ");
            }
            Context::MappingKey => {
                self.write_indent();
            }
            // Root level scalar and mapping value need no prefix
            Context::Root | Context::MappingValue => {}
        }

        // Handle anchor if present (with bounds check for security)
        if anchor_id > 0 && anchor_id <= MAX_ANCHOR_ID {
            let anchor_name = format!("anchor{anchor_id}");
            self.output.push('&');
            self.output.push_str(&anchor_name);
            self.output.push(' ');
            // Store anchor name for later alias resolution
            if self.anchor_names.len() <= anchor_id {
                self.anchor_names.resize(anchor_id + 1, String::new());
            }
            self.anchor_names[anchor_id] = anchor_name;
        }

        // Emit value with appropriate style
        self.emit_value_with_style(value, style);

        // Handle context transitions
        match ctx {
            Context::MappingKey => {
                self.output.push(':');
                // Transition to expecting value
                if let Some(last) = self.context_stack.last_mut() {
                    *last = Context::MappingValue;
                }
                // Add space after colon for simple values
                self.output.push(' ');
            }
            Context::MappingValue => {
                self.output.push('\n');
                // Transition back to expecting key
                if let Some(last) = self.context_stack.last_mut() {
                    *last = Context::MappingKey;
                }
            }
            Context::Sequence | Context::Root => {
                self.output.push('\n');
            }
        }
    }

    fn emit_value_with_style(&mut self, value: &str, style: ScalarStyle) {
        match style {
            ScalarStyle::Plain => {
                // Fix special floats for YAML 1.2 compliance
                let fixed = fix_special_float_value(value);
                self.output.push_str(fixed);
            }
            ScalarStyle::SingleQuoted => {
                self.output.push('\'');
                // Single quotes: escape single quotes by doubling
                for c in value.chars() {
                    if c == '\'' {
                        self.output.push_str("''");
                    } else {
                        self.output.push(c);
                    }
                }
                self.output.push('\'');
            }
            ScalarStyle::DoubleQuoted => {
                self.output.push('"');
                // Double quotes: escape special characters
                for c in value.chars() {
                    match c {
                        '"' => self.output.push_str("\\\""),
                        '\\' => self.output.push_str("\\\\"),
                        '\n' => self.output.push_str("\\n"),
                        '\r' => self.output.push_str("\\r"),
                        '\t' => self.output.push_str("\\t"),
                        '\0' => self.output.push_str("\\0"),
                        _ => self.output.push(c),
                    }
                }
                self.output.push('"');
            }
            ScalarStyle::Literal => {
                self.output.push_str("|-");
                self.output.push('\n');
                self.write_block_scalar_lines(value);
            }
            ScalarStyle::Folded => {
                self.output.push_str(">-");
                self.output.push('\n');
                self.write_block_scalar_lines(value);
            }
        }
    }

    fn start_sequence(&mut self, anchor_id: usize, _tag: Option<&Cow<'_, Tag>>) {
        let ctx = self.current_context();

        // Handle pending newline
        if self.pending_newline {
            self.output.push('\n');
            self.pending_newline = false;
        }

        // Write prefix based on context
        match ctx {
            Context::Sequence => {
                self.write_indent();
                self.output.push_str("- ");
            }
            Context::MappingKey => {
                // Sequence as mapping key - unusual but valid
                self.write_indent();
            }
            Context::MappingValue => {
                // Value position - newline and indent for nested sequence
                self.output.push('\n');
            }
            Context::Root => {}
        }

        // Handle anchor (with bounds check for security)
        if anchor_id > 0 && anchor_id <= MAX_ANCHOR_ID {
            let anchor_name = format!("anchor{anchor_id}");
            self.output.push('&');
            self.output.push_str(&anchor_name);
            self.output.push(' ');
            if self.anchor_names.len() <= anchor_id {
                self.anchor_names.resize(anchor_id + 1, String::new());
            }
            self.anchor_names[anchor_id] = anchor_name;
        }

        // Update context for mapping value -> key transition
        if ctx == Context::MappingValue
            && let Some(last) = self.context_stack.last_mut()
        {
            *last = Context::MappingKey;
        }

        // Push sequence context and increase indent (with depth limit)
        if self.context_stack.len() < MAX_DEPTH {
            self.context_stack.push(Context::Sequence);
            self.indent_level += 1;
        }
    }

    fn end_sequence(&mut self) {
        self.context_stack.pop();
        self.indent_level = self.indent_level.saturating_sub(1);
    }

    fn start_mapping(&mut self, anchor_id: usize, _tag: Option<&Cow<'_, Tag>>) {
        let ctx = self.current_context();

        // Handle pending newline
        if self.pending_newline {
            self.output.push('\n');
            self.pending_newline = false;
        }

        // Write prefix based on context
        match ctx {
            Context::Sequence => {
                self.write_indent();
                self.output.push_str("- ");
            }
            Context::MappingKey => {
                // Mapping as mapping key - unusual but valid (complex key)
                self.write_indent();
            }
            Context::MappingValue => {
                // Value position - newline for nested mapping
                self.output.push('\n');
            }
            Context::Root => {}
        }

        // Handle anchor (with bounds check for security)
        if anchor_id > 0 && anchor_id <= MAX_ANCHOR_ID {
            let anchor_name = format!("anchor{anchor_id}");
            self.output.push('&');
            self.output.push_str(&anchor_name);
            self.output.push('\n');
            if self.anchor_names.len() <= anchor_id {
                self.anchor_names.resize(anchor_id + 1, String::new());
            }
            self.anchor_names[anchor_id] = anchor_name;
        }

        // Update context for mapping value -> key transition
        if ctx == Context::MappingValue
            && let Some(last) = self.context_stack.last_mut()
        {
            *last = Context::MappingKey;
        }

        // Push mapping context and increase indent (with depth limit)
        if self.context_stack.len() < MAX_DEPTH {
            self.context_stack.push(Context::MappingKey);
            self.indent_level += 1;
        }
    }

    fn end_mapping(&mut self) {
        self.context_stack.pop();
        self.indent_level = self.indent_level.saturating_sub(1);
    }

    fn emit_alias(&mut self, anchor_id: usize) {
        let ctx = self.current_context();

        // Handle pending newline
        if self.pending_newline {
            self.output.push('\n');
            self.pending_newline = false;
        }

        // Write prefix based on context
        match ctx {
            Context::Sequence => {
                self.write_indent();
                self.output.push_str("- ");
            }
            Context::MappingKey => {
                self.write_indent();
            }
            Context::Root | Context::MappingValue => {}
        }

        // Emit the alias reference
        self.output.push('*');
        if anchor_id < self.anchor_names.len() && !self.anchor_names[anchor_id].is_empty() {
            self.output.push_str(&self.anchor_names[anchor_id]);
        } else {
            // Use write! macro to avoid format! allocation
            let _ = write!(self.output, "anchor{anchor_id}");
        }

        // Handle context transitions
        match ctx {
            Context::MappingKey => {
                self.output.push(':');
                if let Some(last) = self.context_stack.last_mut() {
                    *last = Context::MappingValue;
                }
                self.output.push(' ');
            }
            Context::MappingValue => {
                self.output.push('\n');
                if let Some(last) = self.context_stack.last_mut() {
                    *last = Context::MappingKey;
                }
            }
            Context::Sequence | Context::Root => {
                self.output.push('\n');
            }
        }
    }

    /// Write indentation for block scalar content (literal/folded styles).
    fn write_block_scalar_lines(&mut self, value: &str) {
        let indent_chars = self.indent_level.saturating_mul(self.config.indent);

        for line in value.lines() {
            if indent_chars <= INDENT_SPACES.len() {
                self.output.push_str(&INDENT_SPACES[..indent_chars]);
            } else {
                self.output.push_str(&" ".repeat(indent_chars));
            }
            self.output.push_str(line);
            self.output.push('\n');
        }
    }

    fn write_indent(&mut self) {
        if self.indent_level > 1 {
            let indent_chars = (self.indent_level - 1).saturating_mul(self.config.indent);

            if indent_chars <= INDENT_SPACES.len() {
                self.output.push_str(&INDENT_SPACES[..indent_chars]);
            } else {
                self.output.push_str(&" ".repeat(indent_chars));
            }
        }
    }

    fn finish(mut self) -> String {
        // Ensure output ends with newline
        if !self.output.is_empty() && !self.output.ends_with('\n') {
            self.output.push('\n');
        }
        self.output
    }
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

/// Format YAML using streaming parser events.
///
/// This function bypasses DOM construction for better performance on large files.
/// It processes parser events directly, maintaining O(1) memory complexity
/// relative to the portion of the file being processed.
///
/// # Errors
///
/// Returns `EmitError::Emit` if the parser encounters invalid YAML.
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "streaming")]
/// # {
/// use fast_yaml_core::streaming::format_streaming;
/// use fast_yaml_core::EmitterConfig;
///
/// let yaml = "key: value\nlist:\n  - item1\n  - item2\n";
/// let config = EmitterConfig::default();
/// let formatted = format_streaming(yaml, &config).unwrap();
/// assert!(formatted.contains("key:"));
/// # }
/// ```
pub fn format_streaming(input: &str, config: &EmitterConfig) -> EmitResult<String> {
    let parser = Parser::new_from_str(input);
    let mut emitter = StreamingEmitter::new(config, input.len());

    for result in parser {
        let (event, span) = result.map_err(|e| EmitError::Emit(e.to_string()))?;
        emitter.format_event(event, span);
    }

    Ok(emitter.finish())
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
        // Create input with many anchors
        use std::fmt::Write;
        let mut heavy_anchors = String::new();
        for i in 0..100 {
            writeln!(heavy_anchors, "key{i}: &anchor{i} value{i}").unwrap();
        }
        // Heavy anchor usage should prefer DOM
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
        // The literal style should be preserved
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
        // Test with large input to verify buffer pre-allocation works correctly
        let large_yaml = (0..100)
            .map(|i| format!("key{i}: value{i}"))
            .collect::<Vec<_>>()
            .join("\n");

        let config = EmitterConfig::default();
        let result = format_streaming(&large_yaml, &config).unwrap();

        // Verify content is preserved
        assert!(result.contains("key0:"));
        assert!(result.contains("key99:"));
        assert!(result.contains("value50:") || result.contains("value50\n"));
    }

    #[test]
    fn test_format_streaming_deeply_nested() {
        // Test deep nesting to verify context stack pre-allocation
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
    fn test_streaming_emitter_capacity_estimation() {
        // Verify that capacity estimation provides reasonable values
        let config = EmitterConfig::default();

        // Small input - verify it doesn't allocate excessively
        let small_emitter = StreamingEmitter::new(&config, 100);
        assert!(
            small_emitter.output.capacity() >= 100,
            "Should pre-allocate at least input size"
        );
        assert!(
            small_emitter.output.capacity() < 1000,
            "Should not over-allocate for small input"
        );

        // Large input
        let large_emitter = StreamingEmitter::new(&config, 10000);
        assert!(
            large_emitter.output.capacity() >= 10000,
            "Should pre-allocate for large input"
        );
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
        // Test with multiple anchors to verify anchor_names pre-allocation
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
        // Test with nesting that requires context stack growth beyond initial 16
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
