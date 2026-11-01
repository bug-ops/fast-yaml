//! Common configuration aggregating multiple config types.

#![allow(clippy::missing_const_for_fn)]

use super::OutputConfig;
use crate::cli::ResolvedCli;

/// Common configuration shared by every subcommand.
///
/// Use this when a command needs multiple configuration aspects.
/// This eliminates the need to pass many individual parameters.
#[derive(Debug, Clone, Default)]
pub struct CommonConfig {
    /// Output configuration (verbosity, colors, timing)
    pub output: OutputConfig,
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
    pub fn from_cli(cli: &ResolvedCli) -> Self {
        Self {
            output: OutputConfig::from_cli(cli.verbosity, cli.no_color),
        }
    }

    /// Sets the output configuration.
    #[cfg(test)]
    #[must_use]
    pub fn with_output(mut self, output: OutputConfig) -> Self {
        self.output = output;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Verbosity;

    #[test]
    fn test_default_config() {
        let config = CommonConfig::default();
        assert!(!config.output.is_quiet());
        assert!(!config.output.is_verbose());
    }

    #[test]
    fn test_new() {
        let config = CommonConfig::new();
        assert!(!config.output.is_quiet());
    }

    #[test]
    fn test_with_output() {
        let output = OutputConfig::new().with_verbosity(Verbosity::Quiet);
        let config = CommonConfig::new().with_output(output);
        assert!(config.output.is_quiet());
    }
}
