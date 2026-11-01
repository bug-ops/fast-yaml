//! Formatter configuration for YAML formatting.

use fast_yaml_core::{EmitterConfig, Indent, MaxDepth, MaxDocuments, MaxScanAhead, ParseLimits};

/// Configuration for YAML formatting.
///
/// Controls indentation and the parser limits for formatting operations.
#[derive(Debug, Clone)]
pub struct FormatterConfig {
    indent: Indent,
    parse_limits: ParseLimits,
}

impl FormatterConfig {
    /// Creates a new formatter configuration with default values.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the indentation width.
    #[must_use]
    pub const fn with_indent(mut self, indent: Indent) -> Self {
        self.indent = indent;
        self
    }

    /// Sets the maximum nesting depth.
    #[must_use]
    pub const fn with_max_depth(mut self, max_depth: MaxDepth) -> Self {
        self.parse_limits.max_depth = max_depth;
        self
    }

    /// Sets the maximum document count per stream.
    #[must_use]
    pub const fn with_max_documents(mut self, max_documents: MaxDocuments) -> Self {
        self.parse_limits.max_documents = max_documents;
        self
    }

    /// Sets the scan-ahead limit.
    #[must_use]
    pub const fn with_max_scan_ahead(mut self, max_scan_ahead: MaxScanAhead) -> Self {
        self.parse_limits.max_scan_ahead = max_scan_ahead;
        self
    }

    /// Converts to `EmitterConfig` for fast-yaml-core.
    #[must_use]
    pub fn to_emitter_config(&self) -> EmitterConfig {
        EmitterConfig::new()
            .with_indent(self.indent)
            .with_parse_limits(self.parse_limits)
    }
}

impl Default for FormatterConfig {
    fn default() -> Self {
        Self {
            indent: Indent::DEFAULT,
            parse_limits: ParseLimits::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn indent(n: usize) -> Indent {
        Indent::new(n).unwrap()
    }

    #[test]
    fn test_to_emitter_config_carries_every_setting() {
        let depth = MaxDepth::new(7).unwrap();
        let scan = MaxScanAhead::new(99).unwrap();
        let emitter = FormatterConfig::new()
            .with_indent(indent(4))
            .with_max_depth(depth)
            .with_max_scan_ahead(scan)
            .to_emitter_config();
        assert_eq!(emitter.indent.get(), 4);
        assert_eq!(emitter.parse_limits.max_depth, depth);
        assert_eq!(emitter.parse_limits.max_scan_ahead, scan);
    }
}
