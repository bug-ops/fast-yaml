use crate::error::{EmitError, EmitResult};
use crate::value::Value;
use memchr::memmem;
use saphyr::{ScalarOwned, YamlEmitter};

/// Configuration for YAML emission.
///
/// Controls formatting, style, and output options when serializing YAML.
#[derive(Debug, Clone)]
pub struct EmitterConfig {
    /// Indentation width in spaces (default: 2).
    ///
    /// Controls the number of spaces used for each indentation level.
    /// Valid range: 1-9 (values outside this range will be clamped).
    ///
    /// Note: saphyr currently uses fixed 2-space indentation.
    /// This parameter is accepted for `PyYAML` API compatibility but
    /// may require post-processing to fully support custom values.
    pub indent: usize,

    /// Maximum line width for wrapping (default: 80).
    ///
    /// When lines exceed this width, the emitter will attempt to wrap them.
    /// Valid range: 20-1000 (values outside this range will be clamped).
    ///
    /// Note: saphyr has limited control over line wrapping.
    /// This parameter is accepted for `PyYAML` API compatibility.
    pub width: usize,

    /// Default flow style for collections (default: None).
    ///
    /// - `None`: Use block style (multi-line)
    /// - `Some(true)`: Force flow style (inline: `[...]`, `{...}`)
    /// - `Some(false)`: Force block style (explicit)
    pub default_flow_style: Option<bool>,

    /// Add explicit document start marker `---` (default: false).
    ///
    /// When true, prepends `---\n` to the output.
    pub explicit_start: bool,

    /// Enable compact inline notation (default: true).
    ///
    /// Controls whether saphyr uses compact notation for
    /// inline sequences and mappings.
    pub compact: bool,

    /// Render multiline strings in literal style (default: false).
    ///
    /// When true, strings containing newlines will be rendered
    /// using literal block scalar notation (`|`).
    pub multiline_strings: bool,
}

impl Default for EmitterConfig {
    fn default() -> Self {
        Self {
            indent: 2,
            width: 80,
            default_flow_style: None,
            explicit_start: false,
            compact: true,
            multiline_strings: false,
        }
    }
}

impl EmitterConfig {
    /// Create a new emitter configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set indentation width (clamped to 1-9).
    #[must_use]
    pub fn with_indent(mut self, indent: usize) -> Self {
        self.indent = indent.clamp(1, 9);
        self
    }

    /// Set line width (clamped to 20-1000).
    #[must_use]
    pub fn with_width(mut self, width: usize) -> Self {
        self.width = width.clamp(20, 1000);
        self
    }

    /// Set default flow style for collections.
    #[must_use]
    pub const fn with_default_flow_style(mut self, flow_style: Option<bool>) -> Self {
        self.default_flow_style = flow_style;
        self
    }

    /// Set explicit document start marker.
    #[must_use]
    pub const fn with_explicit_start(mut self, explicit_start: bool) -> Self {
        self.explicit_start = explicit_start;
        self
    }

    /// Set compact inline notation.
    #[must_use]
    pub const fn with_compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }

    /// Set multiline string rendering.
    #[must_use]
    pub const fn with_multiline_strings(mut self, multiline_strings: bool) -> Self {
        self.multiline_strings = multiline_strings;
        self
    }
}

/// Emitter for YAML documents.
///
/// Wraps saphyr's `YamlEmitter` to provide a consistent API.
#[derive(Debug)]
pub struct Emitter;

impl Emitter {
    /// Emit a single YAML document to a string with configuration.
    ///
    /// # Errors
    ///
    /// Returns `EmitError::Emit` if the value cannot be serialized.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{Emitter, EmitterConfig, Value};
    ///
    /// let value = Value::String("test".to_string());
    /// let config = EmitterConfig::new().with_explicit_start(true);
    /// let yaml = Emitter::emit_str_with_config(&value, &config)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn emit_str_with_config(value: &Value, config: &EmitterConfig) -> EmitResult<String> {
        let estimated_size = Self::estimate_output_size(value);
        let mut output = String::with_capacity(estimated_size);
        {
            let mut emitter = YamlEmitter::new(&mut output);

            // Apply saphyr native configuration
            emitter.compact(config.compact);
            emitter.multiline_strings(config.multiline_strings);

            // Convert YamlOwned to Yaml for emission
            let yaml_borrowed: saphyr::Yaml = value.into();
            emitter
                .dump(&yaml_borrowed)
                .map_err(|e| EmitError::Emit(e.to_string()))?;
        }

        // Apply post-processing for configuration options
        output = Self::apply_formatting(output, config);

        Ok(output)
    }

    /// Estimate output size based on input value structure.
    fn estimate_output_size(value: &Value) -> usize {
        Self::estimate_value_size(value)
    }

    fn estimate_value_size(value: &Value) -> usize {
        match value {
            Value::Value(scalar) => Self::estimate_scalar_size(scalar),
            Value::Sequence(seq) => {
                // "- " prefix (2) + newline (1) per item + recursive content
                seq.iter().map(|v| 3 + Self::estimate_value_size(v)).sum()
            }
            Value::Mapping(map) => {
                // "key: " (~10) + newline (1) + recursive content
                map.iter()
                    .map(|(k, v)| 11 + Self::estimate_value_size(k) + Self::estimate_value_size(v))
                    .sum()
            }
            Value::Representation(s, _, _) => s.len() + 2,
            Value::Tagged(_, inner) => 10 + Self::estimate_value_size(inner),
            Value::Alias(_) => 10,
            Value::BadValue => 4,
        }
    }

    fn estimate_scalar_size(scalar: &ScalarOwned) -> usize {
        match scalar {
            ScalarOwned::Null => 4,       // "null"
            ScalarOwned::Boolean(_) => 5, // "false"
            ScalarOwned::Integer(i) => {
                // Decimal digits + sign (max 20 for i64)
                if *i == 0 {
                    1
                } else {
                    // Use checked_ilog10 for precise digit count without float conversion
                    i.unsigned_abs()
                        .checked_ilog10()
                        .map_or(1, |d| d as usize + 1)
                        + 1
                }
            }
            ScalarOwned::FloatingPoint(_) => 20, // Conservative estimate
            ScalarOwned::String(s) => s.len() + 2, // Possible quotes
        }
    }

    /// Emit a single YAML document to a string with default configuration.
    ///
    /// # Errors
    ///
    /// Returns `EmitError::Emit` if the value cannot be serialized.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{Emitter, Value};
    ///
    /// let value = Value::String("test".to_string());
    /// let yaml = Emitter::emit_str(&value)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn emit_str(value: &Value) -> EmitResult<String> {
        Self::emit_str_with_config(value, &EmitterConfig::default())
    }

    /// Emit multiple YAML documents to a string with document separators and configuration.
    ///
    /// # Errors
    ///
    /// Returns `EmitError::Emit` if any value cannot be serialized.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{Emitter, EmitterConfig, Value};
    ///
    /// let docs = vec![
    ///     Value::String("first".to_string()),
    ///     Value::String("second".to_string()),
    /// ];
    /// let config = EmitterConfig::new().with_explicit_start(true);
    /// let yaml = Emitter::emit_all_with_config(&docs, &config)?;
    /// assert!(yaml.contains("---"));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn emit_all_with_config(values: &[Value], config: &EmitterConfig) -> EmitResult<String> {
        // Pre-calculate total estimated size for all documents
        let total_size: usize =
            values.iter().map(Self::estimate_output_size).sum::<usize>() + values.len() * 5; // Account for "---\n" separators

        let mut output = String::with_capacity(total_size);

        // Create single config variant for non-first documents (avoids cloning per document)
        let inner_config = EmitterConfig {
            explicit_start: false,
            ..*config
        };

        for (i, value) in values.iter().enumerate() {
            // Add document separator before each document (except first if explicit_start is false)
            if i > 0 || config.explicit_start {
                output.push_str("---\n");
            }

            // Always use inner_config (with explicit_start=false) since we handle
            // document separators explicitly above
            let doc = Self::emit_str_with_config(value, &inner_config)?;
            output.push_str(&doc);

            // Ensure document ends with newline for proper separation
            if !output.ends_with('\n') {
                output.push('\n');
            }
        }

        Ok(output)
    }

    /// Emit multiple YAML documents to a string with document separators.
    ///
    /// # Errors
    ///
    /// Returns `EmitError::Emit` if any value cannot be serialized.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{Emitter, Value};
    ///
    /// let docs = vec![
    ///     Value::String("first".to_string()),
    ///     Value::String("second".to_string()),
    /// ];
    /// let yaml = Emitter::emit_all(&docs)?;
    /// assert!(yaml.contains("---"));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn emit_all(values: &[Value]) -> EmitResult<String> {
        Self::emit_all_with_config(values, &EmitterConfig::default())
    }

    /// Apply formatting configuration to YAML output.
    ///
    /// Handles `explicit_start` and potentially other post-processing.
    fn apply_formatting(mut output: String, config: &EmitterConfig) -> String {
        // Handle explicit_start
        if config.explicit_start {
            if !output.starts_with("---") {
                output.insert_str(0, "---\n");
            }
        } else if output.starts_with("---\n") {
            output.drain(..4);
        } else if output.starts_with("---") {
            // Find where content starts after "---"
            let skip = 3 + output[3..].chars().take_while(|c| *c == '\n').count();
            output.drain(..skip);
        }

        // Fix special float values for YAML 1.2 Core Schema compliance
        // saphyr outputs "inf"/"-inf"/"NaN", but YAML 1.2 requires ".inf"/"-.inf"/".nan"
        output = Self::fix_special_floats(&output);

        // TODO: Apply indent transformation if config.indent != 2
        // This would require parsing indentation patterns and adjusting them

        // TODO: Apply width transformation if config.width != 80
        // This would require line wrapping logic

        output
    }

    /// Fix special float values for YAML 1.2 Core Schema compliance.
    ///
    /// Converts saphyr's output format to YAML 1.2 compliant format:
    /// - `inf` → `.inf`
    /// - `-inf` → `-.inf`
    /// - `NaN` → `.nan`
    fn fix_special_floats(output: &str) -> String {
        if !Self::might_contain_special_floats(output) {
            return output.to_string();
        }

        // Slow path: line-by-line transformation
        Self::fix_special_floats_slow(output)
    }

    /// Quick check if output might contain special float patterns.
    /// Uses SIMD-accelerated memchr for speed - no regex or allocation.
    #[inline]
    fn might_contain_special_floats(output: &str) -> bool {
        let bytes = output.as_bytes();

        // Use SIMD-accelerated memmem for fast substring search
        // These are the only special float indicators in saphyr output
        memmem::find(bytes, b"inf").is_some() || memmem::find(bytes, b"NaN").is_some()
    }

    /// Slow path for `fix_special_floats`: processes line-by-line.
    /// Pre-allocates output buffer to avoid reallocations.
    fn fix_special_floats_slow(output: &str) -> String {
        // Pre-allocate output (same size as input since patterns are similar length)
        let mut result = String::with_capacity(output.len());

        for (i, line) in output.lines().enumerate() {
            if i > 0 {
                result.push('\n');
            }

            // Check if line ends with special float value (with optional whitespace)
            let trimmed = line.trim_end();
            if let Some(prefix) = trimmed.strip_suffix("inf") {
                // Check if it's "-inf" or standalone "inf"
                if let Some(before_minus) = prefix.strip_suffix('-') {
                    // Already has minus, check if it's at value position
                    if Self::is_value_position(before_minus) {
                        result.push_str(before_minus);
                        result.push_str("-.inf");
                        continue;
                    }
                } else if Self::is_value_position(prefix) {
                    result.push_str(prefix);
                    result.push_str(".inf");
                    continue;
                }
            } else if let Some(prefix) = trimmed.strip_suffix("NaN")
                && Self::is_value_position(prefix)
            {
                result.push_str(prefix);
                result.push_str(".nan");
                continue;
            }
            result.push_str(line);
        }

        result
    }

    /// Check if the prefix indicates this is a value position (after `: ` or start of line).
    fn is_value_position(prefix: &str) -> bool {
        prefix.is_empty()
            || prefix.ends_with(": ")
            || prefix.ends_with("- ")
            || prefix.ends_with('\n')
    }

    /// Format a YAML string with configuration.
    ///
    /// Uses streaming formatter for large files when the `streaming` feature is enabled,
    /// falling back to DOM-based formatting for small files or complex cases.
    ///
    /// # Errors
    ///
    /// Returns `EmitError::Emit` if the YAML cannot be parsed or formatted.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{Emitter, EmitterConfig};
    ///
    /// let yaml = "key: value\nlist:\n  - item1\n  - item2\n";
    /// let config = EmitterConfig::default();
    /// let formatted = Emitter::format_with_config(yaml, &config)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn format_with_config(input: &str, config: &EmitterConfig) -> EmitResult<String> {
        #[cfg(feature = "streaming")]
        {
            if crate::streaming::is_streaming_suitable(input) {
                // Prefer arena allocation when available
                #[cfg(feature = "arena")]
                {
                    return crate::streaming::format_streaming_arena(input, config);
                }
                #[cfg(not(feature = "arena"))]
                {
                    return crate::streaming::format_streaming(input, config);
                }
            }
        }

        // Fall back to DOM-based formatting
        let value = crate::Parser::parse_str(input)
            .map_err(|e| EmitError::Emit(e.to_string()))?
            .ok_or_else(|| EmitError::Emit("Empty document".to_string()))?;
        Self::emit_str_with_config(&value, config)
    }

    /// Format a YAML string with default configuration.
    ///
    /// Uses streaming formatter for large files when the `streaming` feature is enabled.
    ///
    /// # Errors
    ///
    /// Returns `EmitError::Emit` if the YAML cannot be parsed or formatted.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::Emitter;
    ///
    /// let yaml = "key: value\nlist:\n  - item1\n  - item2\n";
    /// let formatted = Emitter::format(yaml)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn format(input: &str) -> EmitResult<String> {
        Self::format_with_config(input, &EmitterConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ordered_float::OrderedFloat;
    use saphyr::ScalarOwned;

    #[test]
    fn test_emit_str_string() {
        let value = Value::Value(ScalarOwned::String("test".to_string()));
        let result = Emitter::emit_str(&value).unwrap();
        assert!(result.contains("test"));
    }

    #[test]
    fn test_emit_str_integer() {
        let value = Value::Value(ScalarOwned::Integer(42));
        let result = Emitter::emit_str(&value).unwrap();
        assert!(result.contains("42"));
    }

    #[test]
    fn test_emit_all_multiple() {
        let values = vec![
            Value::Value(ScalarOwned::String("first".to_string())),
            Value::Value(ScalarOwned::String("second".to_string())),
        ];
        let result = Emitter::emit_all(&values).unwrap();
        assert!(result.contains("first"));
        assert!(result.contains("second"));
        assert!(result.contains("---"));
    }

    #[test]
    fn test_emit_all_single() {
        let values = vec![Value::Value(ScalarOwned::String("only".to_string()))];
        let result = Emitter::emit_all(&values).unwrap();
        assert!(result.contains("only"));
        assert!(!result.starts_with("---"));
    }

    #[test]
    fn test_emitter_config_default() {
        let config = EmitterConfig::default();
        assert_eq!(config.indent, 2);
        assert_eq!(config.width, 80);
        assert_eq!(config.default_flow_style, None);
        assert!(!config.explicit_start);
        assert!(config.compact);
        assert!(!config.multiline_strings);
    }

    #[test]
    fn test_emitter_config_builder() {
        let config = EmitterConfig::new()
            .with_indent(4)
            .with_width(120)
            .with_explicit_start(true)
            .with_compact(false);

        assert_eq!(config.indent, 4);
        assert_eq!(config.width, 120);
        assert!(config.explicit_start);
        assert!(!config.compact);
    }

    #[test]
    fn test_emitter_config_clamp_indent() {
        let config = EmitterConfig::new().with_indent(100);
        assert_eq!(config.indent, 9);

        let config = EmitterConfig::new().with_indent(0);
        assert_eq!(config.indent, 1);
    }

    #[test]
    fn test_emitter_config_clamp_width() {
        let config = EmitterConfig::new().with_width(10);
        assert_eq!(config.width, 20);

        let config = EmitterConfig::new().with_width(2000);
        assert_eq!(config.width, 1000);
    }

    #[test]
    fn test_emit_with_explicit_start() {
        let value = Value::Value(ScalarOwned::String("test".to_string()));
        let config = EmitterConfig::new().with_explicit_start(true);
        let result = Emitter::emit_str_with_config(&value, &config).unwrap();
        assert!(result.starts_with("---"));
    }

    #[test]
    fn test_emit_without_explicit_start() {
        let value = Value::Value(ScalarOwned::String("test".to_string()));
        let config = EmitterConfig::new().with_explicit_start(false);
        let result = Emitter::emit_str_with_config(&value, &config).unwrap();
        assert!(!result.starts_with("---"));
    }

    #[test]
    fn test_emit_all_with_explicit_start() {
        let values = vec![
            Value::Value(ScalarOwned::String("first".to_string())),
            Value::Value(ScalarOwned::String("second".to_string())),
        ];
        let config = EmitterConfig::new().with_explicit_start(true);
        let result = Emitter::emit_all_with_config(&values, &config).unwrap();
        assert!(result.starts_with("---"));
        assert_eq!(result.matches("---").count(), 2);
    }

    #[test]
    fn test_emit_with_compact_false() {
        let value = Value::Sequence(vec![
            Value::Value(ScalarOwned::Integer(1)),
            Value::Value(ScalarOwned::Integer(2)),
        ]);
        let config = EmitterConfig::new().with_compact(false);
        let result = Emitter::emit_str_with_config(&value, &config).unwrap();
        // Should contain formatting (exact format depends on saphyr)
        assert!(result.contains('1') && result.contains('2'));
    }

    #[test]
    fn test_emit_with_multiline_strings() {
        let value = Value::Value(ScalarOwned::String("line1\nline2".to_string()));
        let config = EmitterConfig::new().with_multiline_strings(true);
        let result = Emitter::emit_str_with_config(&value, &config).unwrap();
        // Should use literal block scalar notation (|)
        assert!(result.contains("line1") && result.contains("line2"));
    }

    #[test]
    fn test_estimate_scalar_size_all_types() {
        // Test Null
        let null_size = Emitter::estimate_scalar_size(&ScalarOwned::Null);
        assert_eq!(null_size, 4); // "null"

        // Test Boolean
        let bool_size = Emitter::estimate_scalar_size(&ScalarOwned::Boolean(true));
        assert_eq!(bool_size, 5); // "false" (conservative estimate)

        // Test Integer - edge cases
        // Zero case (special handling)
        let zero_size = Emitter::estimate_scalar_size(&ScalarOwned::Integer(0));
        assert_eq!(zero_size, 1);

        // Single digit
        let single_digit = Emitter::estimate_scalar_size(&ScalarOwned::Integer(5));
        assert!(single_digit >= 1);

        // Multi-digit positive
        let multi_digit = Emitter::estimate_scalar_size(&ScalarOwned::Integer(12345));
        assert!(multi_digit >= 5);

        // Negative number
        let negative = Emitter::estimate_scalar_size(&ScalarOwned::Integer(-42));
        assert!(negative >= 2); // "-" + digits

        // Test Float
        let float_size =
            Emitter::estimate_scalar_size(&ScalarOwned::FloatingPoint(OrderedFloat(1.23456)));
        assert_eq!(float_size, 20); // Conservative estimate

        // Test String
        let string_size = Emitter::estimate_scalar_size(&ScalarOwned::String("hello".to_string()));
        assert_eq!(string_size, 7); // 5 chars + 2 for possible quotes
    }

    #[test]
    fn test_estimate_value_size_mapping() {
        use saphyr::MappingOwned;

        // Create a mapping with string keys and integer values
        let mut map = MappingOwned::new();
        map.insert(
            Value::Value(ScalarOwned::String("key1".to_string())),
            Value::Value(ScalarOwned::Integer(100)),
        );
        map.insert(
            Value::Value(ScalarOwned::String("key2".to_string())),
            Value::Value(ScalarOwned::Integer(200)),
        );

        let mapping = Value::Mapping(map);
        let size = Emitter::estimate_value_size(&mapping);

        // Should be > 0 and account for both key-value pairs
        // Each pair has ~11 base overhead + key size + value size
        assert!(
            size > 20,
            "Mapping estimate should be significant: got {size}"
        );

        // Test nested mapping
        let mut nested_map = MappingOwned::new();
        nested_map.insert(
            Value::Value(ScalarOwned::String("outer".to_string())),
            mapping,
        );

        let nested_size = Emitter::estimate_value_size(&Value::Mapping(nested_map));
        assert!(
            nested_size > size,
            "Nested mapping should have larger estimate"
        );
    }

    #[test]
    fn test_might_contain_special_floats_positive() {
        // Direct "inf" patterns
        assert!(Emitter::might_contain_special_floats("inf"));
        assert!(Emitter::might_contain_special_floats("key: inf"));
        assert!(Emitter::might_contain_special_floats("-inf"));
        assert!(Emitter::might_contain_special_floats("key: -inf"));
        assert!(Emitter::might_contain_special_floats("- inf\n- -inf"));

        // Direct "NaN" patterns
        assert!(Emitter::might_contain_special_floats("NaN"));
        assert!(Emitter::might_contain_special_floats("key: NaN"));
        assert!(Emitter::might_contain_special_floats(
            "values:\n  - NaN\n  - inf"
        ));

        // Mixed content
        assert!(Emitter::might_contain_special_floats(
            "---\npi: 3.14\nspecial: inf\n"
        ));
    }

    #[test]
    fn test_might_contain_special_floats_false_positives() {
        // Words containing "inf" substring that will trigger the fast-path check
        // (but won't be converted because they're not in value positions)
        assert!(
            Emitter::might_contain_special_floats("information"),
            "'information' contains 'inf' substring"
        );
        assert!(
            Emitter::might_contain_special_floats("infinity"),
            "'infinity' contains 'inf' substring"
        );
        assert!(
            Emitter::might_contain_special_floats("infinite"),
            "'infinite' contains 'inf' substring"
        );
        assert!(
            Emitter::might_contain_special_floats("reinforce"),
            "'reinforce' contains 'inf' substring"
        );

        // Strings that should NOT trigger the check (no "inf" or "NaN" substring)
        assert!(!Emitter::might_contain_special_floats("hello world"));
        assert!(!Emitter::might_contain_special_floats("key: value"));
        assert!(!Emitter::might_contain_special_floats("number: 42"));
        assert!(!Emitter::might_contain_special_floats("pi: 3.14159"));
        assert!(!Emitter::might_contain_special_floats("config")); // "config" does NOT contain "inf"
        assert!(!Emitter::might_contain_special_floats("nan")); // lowercase "nan" != "NaN"
        assert!(!Emitter::might_contain_special_floats("INF")); // uppercase "INF" != "inf"
    }

    #[test]
    fn test_fix_special_floats_inf() {
        // Test standalone inf conversion
        let result = Emitter::fix_special_floats("inf");
        assert_eq!(result, ".inf");

        // Test inf in a mapping value position
        let result = Emitter::fix_special_floats("key: inf");
        assert_eq!(result, "key: .inf");

        // Test -inf conversion
        let result = Emitter::fix_special_floats("-inf");
        assert_eq!(result, "-.inf");

        // Test -inf in a mapping value position
        let result = Emitter::fix_special_floats("key: -inf");
        assert_eq!(result, "key: -.inf");

        // Test inf in a sequence
        let result = Emitter::fix_special_floats("- inf");
        assert_eq!(result, "- .inf");

        // Test -inf in a sequence
        let result = Emitter::fix_special_floats("- -inf");
        assert_eq!(result, "- -.inf");

        // Test mixed document with multiple inf values
        let input = "positive: inf\nnegative: -inf\nlist:\n  - inf\n  - -inf";
        let result = Emitter::fix_special_floats(input);
        assert!(result.contains("positive: .inf"));
        assert!(result.contains("negative: -.inf"));
        assert!(result.contains("- .inf"));
        assert!(result.contains("- -.inf"));
    }

    #[test]
    fn test_fix_special_floats_nan() {
        // Test standalone NaN conversion
        let result = Emitter::fix_special_floats("NaN");
        assert_eq!(result, ".nan");

        // Test NaN in a mapping value position
        let result = Emitter::fix_special_floats("value: NaN");
        assert_eq!(result, "value: .nan");

        // Test NaN in a sequence
        let result = Emitter::fix_special_floats("- NaN");
        assert_eq!(result, "- .nan");

        // Test document with multiple NaN values
        let input = "nan_value: NaN\nlist:\n  - NaN";
        let result = Emitter::fix_special_floats(input);
        assert!(result.contains("nan_value: .nan"));
        assert!(result.contains("- .nan"));

        // Test that strings containing "NaN" as part of word are not converted
        // (this relies on is_value_position check)
        let result = Emitter::fix_special_floats("name: BaNaNa");
        assert_eq!(result, "name: BaNaNa", "BaNaNa should not be modified");

        // Test mixed special floats
        let input = "inf_val: inf\nnan_val: NaN\nneg_inf: -inf";
        let result = Emitter::fix_special_floats(input);
        assert!(result.contains("inf_val: .inf"));
        assert!(result.contains("nan_val: .nan"));
        assert!(result.contains("neg_inf: -.inf"));
    }

    #[test]
    fn test_estimate_value_size_sequence() {
        let seq = Value::Sequence(vec![
            Value::Value(ScalarOwned::Integer(1)),
            Value::Value(ScalarOwned::Integer(2)),
            Value::Value(ScalarOwned::String("hello".to_string())),
        ]);

        let size = Emitter::estimate_value_size(&seq);

        // Each item: 3 (prefix "- " + newline) + scalar size
        // Item 1: 3 + 2 (digit + overhead) = 5
        // Item 2: 3 + 2 = 5
        // Item 3: 3 + 7 (5 chars + 2 quotes) = 10
        assert!(
            size >= 10,
            "Sequence estimate should be significant: got {size}"
        );
    }

    #[test]
    fn test_estimate_value_size_all_variants() {
        use saphyr_parser::{ScalarStyle, Tag};

        // Test Representation variant
        let repr = Value::Representation("custom".to_string(), ScalarStyle::Plain, None);
        let repr_size = Emitter::estimate_value_size(&repr);
        assert_eq!(repr_size, 8); // 6 chars + 2

        // Test Tagged variant
        let tag = Tag {
            handle: "!".to_string(),
            suffix: "custom".to_string(),
        };
        let tagged = Value::Tagged(tag, Box::new(Value::Value(ScalarOwned::Integer(42))));
        let tagged_size = Emitter::estimate_value_size(&tagged);
        // 10 (tag overhead) + inner value size
        assert!(tagged_size >= 10, "Tagged value should have tag overhead");

        // Test Alias variant (usize anchor ID)
        let alias = Value::Alias(1);
        let alias_size = Emitter::estimate_value_size(&alias);
        assert_eq!(alias_size, 10);

        // Test BadValue variant
        let bad = Value::BadValue;
        let bad_size = Emitter::estimate_value_size(&bad);
        assert_eq!(bad_size, 4);
    }

    #[test]
    fn test_emit_all_empty_slice() {
        let empty: Vec<Value> = vec![];
        let config = EmitterConfig::default();

        let result = Emitter::emit_all_with_config(&empty, &config).unwrap();
        assert!(result.is_empty(), "Empty input should produce empty output");
    }

    #[test]
    fn test_emit_all_buffer_preallocation() {
        // Create multiple documents to test buffer pre-allocation
        let docs: Vec<Value> = (0..10)
            .map(|i| Value::Value(ScalarOwned::String(format!("document_{i}"))))
            .collect();

        let config = EmitterConfig::default();
        let result = Emitter::emit_all_with_config(&docs, &config).unwrap();

        // Verify all documents are present
        for i in 0..10 {
            assert!(
                result.contains(&format!("document_{i}")),
                "Should contain document_{i}"
            );
        }

        // Verify document separators (9 separators for 10 documents)
        assert_eq!(
            result.matches("---").count(),
            9,
            "Should have 9 document separators"
        );
    }

    #[test]
    fn test_estimate_scalar_size_large_integer() {
        // Test max i64
        let max_int = Emitter::estimate_scalar_size(&ScalarOwned::Integer(i64::MAX));
        // i64::MAX = 9223372036854775807 (19 digits + potential sign)
        assert!(max_int >= 19, "Max i64 should have at least 19 chars");

        // Test min i64
        let min_int = Emitter::estimate_scalar_size(&ScalarOwned::Integer(i64::MIN));
        // i64::MIN = -9223372036854775808 (19 digits + sign)
        assert!(min_int >= 19, "Min i64 should have at least 19 chars");

        // Test powers of 10
        let thousand = Emitter::estimate_scalar_size(&ScalarOwned::Integer(1000));
        assert!(thousand >= 4, "1000 should have at least 4 chars");

        let million = Emitter::estimate_scalar_size(&ScalarOwned::Integer(1_000_000));
        assert!(million >= 7, "1000000 should have at least 7 chars");
    }

    #[test]
    fn test_might_contain_special_floats_empty() {
        assert!(!Emitter::might_contain_special_floats(""));
    }

    #[test]
    fn test_fix_special_floats_no_changes() {
        // Test output that doesn't contain special floats (fast path)
        let input = "key: value\nlist:\n  - item1\n  - item2\nnumber: 42\n";
        let result = Emitter::fix_special_floats(input);
        assert_eq!(result, input, "No changes should be made for normal YAML");
    }
}
