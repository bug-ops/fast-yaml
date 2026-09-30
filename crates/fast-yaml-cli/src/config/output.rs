//! Output configuration for verbosity and color handling.

/// How much non-error output a command prints.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Verbosity {
    /// Suppress all non-error output
    Quiet,
    /// Default output
    #[default]
    Normal,
    /// Show detailed progress and timing
    Verbose,
}

/// Configuration for output behavior.
///
/// Controls verbosity and coloring across all commands.
#[derive(Debug, Clone, Default)]
pub struct OutputConfig {
    verbosity: Verbosity,
    /// Use ANSI color codes in output
    use_color: bool,
}

impl OutputConfig {
    /// Creates a new output configuration with default values.
    #[cfg(test)]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates configuration from CLI global arguments.
    ///
    /// Automatically detects color support based on terminal capabilities
    /// and environment variables.
    #[must_use]
    pub fn from_cli(verbosity: Verbosity, no_color: bool) -> Self {
        Self::from_cli_with_env(verbosity, no_color, |name| std::env::var(name).ok())
    }

    fn from_cli_with_env(
        verbosity: Verbosity,
        no_color: bool,
        env: impl Fn(&str) -> Option<String>,
    ) -> Self {
        Self {
            verbosity,
            use_color: !no_color && Self::detect_color_support(env),
        }
    }

    /// Detects if terminal supports colors.
    ///
    /// Checks:
    /// 1. `NO_COLOR` environment variable (takes precedence)
    /// 2. Terminal capabilities (if `colors` feature enabled)
    fn detect_color_support(env: impl Fn(&str) -> Option<String>) -> bool {
        if env("NO_COLOR").is_some() {
            return false;
        }
        #[cfg(feature = "colors")]
        {
            use std::io::IsTerminal;
            std::io::stderr().is_terminal()
        }
        #[cfg(not(feature = "colors"))]
        false
    }

    /// Sets the verbosity.
    #[cfg(test)]
    #[must_use]
    pub const fn with_verbosity(mut self, verbosity: Verbosity) -> Self {
        self.verbosity = verbosity;
        self
    }

    /// Sets color usage.
    #[cfg(test)]
    #[must_use]
    pub const fn with_color(mut self, color: bool) -> Self {
        self.use_color = color;
        self
    }

    /// Returns whether quiet mode is enabled.
    #[must_use]
    pub const fn is_quiet(&self) -> bool {
        matches!(self.verbosity, Verbosity::Quiet)
    }

    /// Returns whether verbose mode is enabled.
    #[cfg(any(test, feature = "linter"))]
    #[must_use]
    pub const fn is_verbose(&self) -> bool {
        matches!(self.verbosity, Verbosity::Verbose)
    }

    /// Returns whether color output is enabled.
    #[must_use]
    pub const fn use_color(&self) -> bool {
        self.use_color
    }

    /// Returns whether timing information should be shown.
    #[must_use]
    pub const fn show_timing(&self) -> bool {
        matches!(self.verbosity, Verbosity::Verbose)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_env(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn test_default_config() {
        let config = OutputConfig::default();
        assert!(!config.is_quiet());
        assert!(!config.is_verbose());
        assert!(!config.use_color());
        assert!(!config.show_timing());
    }

    #[test]
    fn test_new() {
        let config = OutputConfig::new();
        assert_eq!(config.verbosity, Verbosity::Normal);
        assert!(!config.use_color());
    }

    #[test]
    fn test_from_cli_quiet() {
        let config = OutputConfig::from_cli_with_env(Verbosity::Quiet, false, no_env);
        assert!(config.is_quiet());
        assert!(!config.is_verbose());
        assert!(!config.show_timing());
    }

    #[test]
    fn test_from_cli_verbose() {
        let config = OutputConfig::from_cli_with_env(Verbosity::Verbose, false, no_env);
        assert!(!config.is_quiet());
        assert!(config.is_verbose());
        assert!(config.show_timing());
    }

    #[test]
    fn test_from_cli_no_color() {
        let config = OutputConfig::from_cli_with_env(Verbosity::Normal, true, no_env);
        assert!(!config.use_color());
    }

    #[test]
    fn test_with_verbosity() {
        assert!(
            OutputConfig::new()
                .with_verbosity(Verbosity::Quiet)
                .is_quiet()
        );
        assert!(
            OutputConfig::new()
                .with_verbosity(Verbosity::Verbose)
                .is_verbose()
        );
    }

    #[test]
    fn test_with_color() {
        let config = OutputConfig::new().with_color(true);
        assert!(config.use_color());
    }

    #[test]
    fn test_detect_color_support_with_no_color_env() {
        let env = |name: &str| (name == "NO_COLOR").then(|| "1".to_string());
        assert!(!OutputConfig::detect_color_support(env));
    }

    #[test]
    fn test_from_cli_respects_no_color_env() {
        let env = |name: &str| (name == "NO_COLOR").then(|| "1".to_string());
        let config = OutputConfig::from_cli_with_env(Verbosity::Normal, false, env);
        assert!(!config.use_color());
    }
}
