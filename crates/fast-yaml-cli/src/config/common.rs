//! Common configuration aggregating multiple config types.

#![allow(clippy::missing_const_for_fn)]

use super::{FormatterConfig, OutputConfig};
use crate::cli::Cli;

/// Common configuration aggregating output and formatter settings.
///
/// Use this when a command needs multiple configuration aspects.
/// This eliminates the need to pass many individual parameters.
#[derive(Debug, Clone, Default)]
pub struct CommonConfig {
    /// Output configuration (verbosity, colors, timing)
    pub output: OutputConfig,
    /// Formatter configuration (indent, width)
    pub formatter: FormatterConfig,
}

impl CommonConfig {
    /// Creates a new common configuration with default values.
    #[cfg(test)]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates configuration from CLI arguments.
    ///
    /// Extracts common configuration from the global CLI flags.
    #[must_use]
    pub fn from_cli(cli: &Cli) -> Self {
        Self {
            output: OutputConfig::from_cli(cli.verbosity, cli.no_color),
            formatter: FormatterConfig::default(),
        }
    }

    /// Sets the output configuration.
    #[cfg(test)]
    #[must_use]
    pub fn with_output(mut self, output: OutputConfig) -> Self {
        self.output = output;
        self
    }

    /// Sets the formatter configuration.
    #[must_use]
    pub fn with_formatter(mut self, formatter: FormatterConfig) -> Self {
        self.formatter = formatter;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Verbosity;
    use fast_yaml_core::Indent;

    #[test]
    fn test_default_config() {
        let config = CommonConfig::default();
        assert!(!config.output.is_quiet());
        assert!(!config.output.is_verbose());
        assert_eq!(config.formatter.indent(), Indent::DEFAULT);
    }

    #[test]
    fn test_new() {
        let config = CommonConfig::new();
        assert!(!config.output.is_quiet());
        assert_eq!(config.formatter.indent(), Indent::DEFAULT);
    }

    #[test]
    fn test_with_output() {
        let output = OutputConfig::new().with_verbosity(Verbosity::Quiet);
        let config = CommonConfig::new().with_output(output);
        assert!(config.output.is_quiet());
    }

    #[test]
    fn test_with_formatter() {
        let formatter = FormatterConfig::new().with_indent(Indent::new(4).unwrap());
        let config = CommonConfig::new().with_formatter(formatter);
        assert_eq!(config.formatter.indent().get(), 4);
    }

    #[test]
    fn test_builder_chaining() {
        let output = OutputConfig::new().with_verbosity(Verbosity::Verbose);
        let formatter = FormatterConfig::new().with_indent(Indent::new(4).unwrap());

        let config = CommonConfig::new()
            .with_output(output)
            .with_formatter(formatter);

        assert!(config.output.is_verbose());
        assert_eq!(config.formatter.indent().get(), 4);
    }
}
