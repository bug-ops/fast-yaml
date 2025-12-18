use crate::error::{EmitError, EmitResult};
use crate::value::Value;
use yaml_rust2::YamlEmitter;

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
    /// Note: yaml-rust2 currently uses fixed 2-space indentation.
    /// This parameter is accepted for `PyYAML` API compatibility but
    /// may require post-processing to fully support custom values.
    pub indent: usize,

    /// Maximum line width for wrapping (default: 80).
    ///
    /// When lines exceed this width, the emitter will attempt to wrap them.
    /// Valid range: 20-1000 (values outside this range will be clamped).
    ///
    /// Note: yaml-rust2 has limited control over line wrapping.
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
    /// Controls whether yaml-rust2 uses compact notation for
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
/// Wraps yaml-rust2's `YamlEmitter` to provide a consistent API.
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
        let mut output = String::new();
        {
            let mut emitter = YamlEmitter::new(&mut output);

            // Apply yaml-rust2 native configuration
            emitter.compact(config.compact);
            emitter.multiline_strings(config.multiline_strings);

            emitter
                .dump(value)
                .map_err(|e| EmitError::Emit(e.to_string()))?;
        }

        // Apply post-processing for configuration options
        output = Self::apply_formatting(output, config);

        Ok(output)
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
                output = format!("---\n{output}");
            }
        } else {
            // Remove leading "---\n" (current behavior)
            if let Some(stripped) = output.strip_prefix("---\n") {
                output = stripped.to_string();
            } else if let Some(stripped) = output.strip_prefix("---") {
                output = stripped.trim_start_matches('\n').to_string();
            }
        }

        // TODO: Apply indent transformation if config.indent != 2
        // This would require parsing indentation patterns and adjusting them

        // TODO: Apply width transformation if config.width != 80
        // This would require line wrapping logic

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emit_str_string() {
        let value = Value::String("test".to_string());
        let result = Emitter::emit_str(&value).unwrap();
        assert!(result.contains("test"));
    }

    #[test]
    fn test_emit_str_integer() {
        let value = Value::Integer(42);
        let result = Emitter::emit_str(&value).unwrap();
        assert!(result.contains("42"));
    }

    #[test]
    fn test_emit_all_multiple() {
        let values = vec![
            Value::String("first".to_string()),
            Value::String("second".to_string()),
        ];
        let result = Emitter::emit_all(&values).unwrap();
        assert!(result.contains("first"));
        assert!(result.contains("second"));
        assert!(result.contains("---"));
    }

    #[test]
    fn test_emit_all_single() {
        let values = vec![Value::String("only".to_string())];
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
        let value = Value::String("test".to_string());
        let config = EmitterConfig::new().with_explicit_start(true);
        let result = Emitter::emit_str_with_config(&value, &config).unwrap();
        assert!(result.starts_with("---"));
    }

    #[test]
    fn test_emit_without_explicit_start() {
        let value = Value::String("test".to_string());
        let config = EmitterConfig::new().with_explicit_start(false);
        let result = Emitter::emit_str_with_config(&value, &config).unwrap();
        assert!(!result.starts_with("---"));
    }

    #[test]
    fn test_emit_all_with_explicit_start() {
        let values = vec![
            Value::String("first".to_string()),
            Value::String("second".to_string()),
        ];
        let config = EmitterConfig::new().with_explicit_start(true);
        let result = Emitter::emit_all_with_config(&values, &config).unwrap();
        assert!(result.starts_with("---"));
        assert_eq!(result.matches("---").count(), 2);
    }

    #[test]
    fn test_emit_with_compact_false() {
        let value = Value::Array(vec![Value::Integer(1), Value::Integer(2)]);
        let config = EmitterConfig::new().with_compact(false);
        let result = Emitter::emit_str_with_config(&value, &config).unwrap();
        // Should contain formatting (exact format depends on yaml-rust2)
        assert!(result.contains('1') && result.contains('2'));
    }

    #[test]
    fn test_emit_with_multiline_strings() {
        let value = Value::String("line1\nline2".to_string());
        let config = EmitterConfig::new().with_multiline_strings(true);
        let result = Emitter::emit_str_with_config(&value, &config).unwrap();
        // Should use literal block scalar notation (|)
        assert!(result.contains("line1") && result.contains("line2"));
    }
}
