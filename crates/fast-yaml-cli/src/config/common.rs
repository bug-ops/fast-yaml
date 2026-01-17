//! Common configuration aggregating multiple config types.

#![allow(clippy::missing_const_for_fn)]

use super::{FormatterConfig, IoConfig, OutputConfig, ParallelConfig};
use crate::cli::Cli;

/// Common configuration aggregating output, formatter, I/O, and parallel settings.
///
/// Use this when a command needs multiple configuration aspects.
/// This eliminates the need to pass many individual parameters.
#[derive(Debug, Clone, Default)]
pub struct CommonConfig {
    /// Output configuration (verbosity, colors, timing)
    pub output: OutputConfig,
    /// Formatter configuration (indent, width)
    pub formatter: FormatterConfig,
    /// I/O configuration (in-place, output path)
    pub io: IoConfig,
    /// Parallel processing configuration (workers, mmap threshold)
    pub parallel: ParallelConfig,
}

impl CommonConfig {
    /// Creates a new common configuration with default values.
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
            output: OutputConfig::from_cli(cli.quiet, cli.verbose, cli.no_color),
            formatter: FormatterConfig::default(),
            io: IoConfig::new()
                .with_in_place(cli.in_place)
                .with_output_path(cli.output.clone()),
            parallel: ParallelConfig::default(),
        }
    }

    /// Sets the output configuration.
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

    /// Sets the I/O configuration.
    #[must_use]
    pub fn with_io(mut self, io: IoConfig) -> Self {
        self.io = io;
        self
    }

    /// Sets the parallel processing configuration.
    #[must_use]
    pub fn with_parallel(mut self, parallel: ParallelConfig) -> Self {
        self.parallel = parallel;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_default_config() {
        let config = CommonConfig::default();
        assert!(!config.output.is_quiet());
        assert!(!config.output.is_verbose());
        assert_eq!(config.formatter.indent(), 2);
        assert!(!config.io.is_in_place());
    }

    #[test]
    fn test_new() {
        let config = CommonConfig::new();
        assert!(!config.output.is_quiet());
        assert!(!config.output.is_verbose());
        assert_eq!(config.formatter.indent(), 2);
        assert!(!config.io.is_in_place());
    }

    #[test]
    fn test_with_output() {
        let output = OutputConfig::new().with_quiet(true);
        let config = CommonConfig::new().with_output(output);
        assert!(config.output.is_quiet());
    }

    #[test]
    fn test_with_formatter() {
        let formatter = FormatterConfig::new().with_indent(4);
        let config = CommonConfig::new().with_formatter(formatter);
        assert_eq!(config.formatter.indent(), 4);
    }

    #[test]
    fn test_with_io() {
        let io = IoConfig::new().with_in_place(true);
        let config = CommonConfig::new().with_io(io);
        assert!(config.io.is_in_place());
    }

    #[test]
    fn test_builder_chaining() {
        let output = OutputConfig::new().with_verbose(true);
        let formatter = FormatterConfig::new().with_indent(4);
        let io = IoConfig::new().with_output_path(Some(PathBuf::from("out.yaml")));

        let config = CommonConfig::new()
            .with_output(output)
            .with_formatter(formatter)
            .with_io(io);

        assert!(config.output.is_verbose());
        assert_eq!(config.formatter.indent(), 4);
        assert_eq!(
            config.io.output_path(),
            Some(PathBuf::from("out.yaml").as_path())
        );
    }
}
