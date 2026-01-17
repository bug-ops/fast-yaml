//! Formatter configuration for YAML formatting.

use fast_yaml_core::EmitterConfig;

#[cfg(feature = "linter")]
use fast_yaml_linter::LintConfig;

/// Configuration for YAML formatting.
///
/// Controls indentation and line width for formatting operations.
#[derive(Debug, Clone)]
pub struct FormatterConfig {
    /// Indentation width (2-8 spaces)
    indent: u8,
    /// Maximum line width
    width: usize,
}

impl FormatterConfig {
    /// Minimum allowed indentation.
    pub const MIN_INDENT: u8 = 2;

    /// Maximum allowed indentation.
    pub const MAX_INDENT: u8 = 8;

    /// Default maximum line width.
    pub const DEFAULT_WIDTH: usize = 80;

    /// Creates a new formatter configuration with default values.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets indentation width, clamping to valid range.
    #[must_use]
    pub fn with_indent(mut self, indent: u8) -> Self {
        self.indent = indent.clamp(Self::MIN_INDENT, Self::MAX_INDENT);
        self
    }

    /// Sets maximum line width.
    #[must_use]
    pub const fn with_width(mut self, width: usize) -> Self {
        self.width = width;
        self
    }

    /// Returns the indentation width.
    #[must_use]
    pub const fn indent(&self) -> u8 {
        self.indent
    }

    /// Returns the maximum line width.
    #[must_use]
    pub const fn width(&self) -> usize {
        self.width
    }

    /// Converts to `EmitterConfig` for fast-yaml-core.
    #[must_use]
    pub fn to_emitter_config(&self) -> EmitterConfig {
        EmitterConfig::new()
            .with_indent(self.indent as usize)
            .with_width(self.width)
    }

    /// Converts to `LintConfig` for fast-yaml-linter.
    #[cfg(feature = "linter")]
    #[must_use]
    pub fn to_lint_config(&self, max_line_length: usize) -> LintConfig {
        LintConfig::new()
            .with_indent_size(self.indent as usize)
            .with_max_line_length(Some(max_line_length))
    }
}

impl Default for FormatterConfig {
    fn default() -> Self {
        Self {
            indent: 2,
            width: Self::DEFAULT_WIDTH,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = FormatterConfig::default();
        assert_eq!(config.indent(), 2);
        assert_eq!(config.width(), FormatterConfig::DEFAULT_WIDTH);
    }

    #[test]
    fn test_new() {
        let config = FormatterConfig::new();
        assert_eq!(config.indent(), 2);
        assert_eq!(config.width(), 80);
    }

    #[test]
    fn test_with_indent() {
        let config = FormatterConfig::new().with_indent(4);
        assert_eq!(config.indent(), 4);
    }

    #[test]
    fn test_with_width() {
        let config = FormatterConfig::new().with_width(120);
        assert_eq!(config.width(), 120);
    }

    #[test]
    fn test_indent_clamping_min() {
        let config = FormatterConfig::new().with_indent(1);
        assert_eq!(config.indent(), FormatterConfig::MIN_INDENT);
    }

    #[test]
    fn test_indent_clamping_max() {
        let config = FormatterConfig::new().with_indent(10);
        assert_eq!(config.indent(), FormatterConfig::MAX_INDENT);
    }

    #[test]
    fn test_builder_chaining() {
        let config = FormatterConfig::new()
            .with_indent(4)
            .with_width(100);

        assert_eq!(config.indent(), 4);
        assert_eq!(config.width(), 100);
    }

    #[test]
    fn test_to_emitter_config() {
        let config = FormatterConfig::new()
            .with_indent(4)
            .with_width(120);

        let _emitter_config = config.to_emitter_config();
    }

    #[cfg(feature = "linter")]
    #[test]
    fn test_to_lint_config() {
        let config = FormatterConfig::new()
            .with_indent(4);

        let _lint_config = config.to_lint_config(100);
    }
}
