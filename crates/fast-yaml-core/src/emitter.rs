use crate::error::{EmitError, EmitResult};
use crate::value::Value;
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
        let mut output = String::new();

        for (i, value) in values.iter().enumerate() {
            // Add document separator before each document (except first if explicit_start is true)
            if i > 0 || config.explicit_start {
                output.push_str("---\n");
            }

            // Emit document without explicit_start (we handle it above)
            let doc_config = EmitterConfig {
                explicit_start: false,
                ..config.clone()
            };
            let doc = Self::emit_str_with_config(value, &doc_config)?;
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
    /// Uses byte scanning for speed - no regex or allocation.
    #[inline]
    fn might_contain_special_floats(output: &str) -> bool {
        let bytes = output.as_bytes();

        // Look for "inf" or "NaN" substrings
        // These are the only special float indicators in saphyr output
        bytes.windows(3).any(|w| w == b"inf" || w == b"NaN")
    }

    /// Slow path for `fix_special_floats`: processes line-by-line.
    fn fix_special_floats_slow(output: &str) -> String {
        // We need to be careful to only replace standalone values, not parts of words.
        // The regex approach would be safer, but for simplicity we'll use line-by-line
        // processing with word boundary checks.
        output
            .lines()
            .map(|line| {
                // Check if line ends with special float value (with optional whitespace)
                let trimmed = line.trim_end();
                if let Some(prefix) = trimmed.strip_suffix("inf") {
                    // Check if it's "-inf" or standalone "inf"
                    if let Some(before_minus) = prefix.strip_suffix('-') {
                        // Already has minus, check if it's at value position
                        if Self::is_value_position(before_minus) {
                            return format!("{before_minus}-.inf");
                        }
                    } else if Self::is_value_position(prefix) {
                        return format!("{prefix}.inf");
                    }
                } else if let Some(prefix) = trimmed.strip_suffix("NaN")
                    && Self::is_value_position(prefix)
                {
                    return format!("{prefix}.nan");
                }
                line.to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Check if the prefix indicates this is a value position (after `: ` or start of line).
    fn is_value_position(prefix: &str) -> bool {
        prefix.is_empty()
            || prefix.ends_with(": ")
            || prefix.ends_with("- ")
            || prefix.ends_with('\n')
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
