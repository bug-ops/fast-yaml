//! Output configuration for verbosity and color handling.

/// Configuration for output behavior.
///
/// Controls verbosity, coloring, and timing information across all commands.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default)]
pub struct OutputConfig {
    /// Suppress all non-error output
    quiet: bool,
    /// Show detailed progress and timing
    verbose: bool,
    /// Use ANSI color codes in output
    use_color: bool,
    /// Show timing information
    show_timing: bool,
}

impl OutputConfig {
    /// Creates a new output configuration with default values.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates configuration from CLI global arguments.
    ///
    /// Automatically detects color support based on terminal capabilities
    /// and environment variables.
    #[must_use]
    pub fn from_cli(quiet: bool, verbose: bool, no_color: bool) -> Self {
        Self {
            quiet,
            verbose,
            use_color: !no_color && Self::detect_color_support(),
            show_timing: verbose,
        }
    }

    /// Detects if terminal supports colors.
    ///
    /// Checks:
    /// 1. `NO_COLOR` environment variable (takes precedence)
    /// 2. Terminal capabilities (if `colors` feature enabled)
    fn detect_color_support() -> bool {
        if std::env::var("NO_COLOR").is_ok() {
            return false;
        }
        #[cfg(feature = "colors")]
        {
            use is_terminal::IsTerminal;
            std::io::stderr().is_terminal()
        }
        #[cfg(not(feature = "colors"))]
        false
    }

    /// Sets quiet mode.
    #[must_use]
    pub const fn with_quiet(mut self, quiet: bool) -> Self {
        self.quiet = quiet;
        self
    }

    /// Sets verbose mode.
    #[must_use]
    pub const fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    /// Sets color usage.
    #[must_use]
    pub const fn with_color(mut self, color: bool) -> Self {
        self.use_color = color;
        self
    }

    /// Sets timing display.
    #[must_use]
    pub const fn with_timing(mut self, timing: bool) -> Self {
        self.show_timing = timing;
        self
    }

    /// Returns whether quiet mode is enabled.
    #[must_use]
    pub const fn is_quiet(&self) -> bool {
        self.quiet
    }

    /// Returns whether verbose mode is enabled.
    #[must_use]
    pub const fn is_verbose(&self) -> bool {
        self.verbose
    }

    /// Returns whether color output is enabled.
    #[must_use]
    pub const fn use_color(&self) -> bool {
        self.use_color
    }

    /// Returns whether timing information should be shown.
    #[must_use]
    pub const fn show_timing(&self) -> bool {
        self.show_timing
    }
}

#[cfg(test)]
#[allow(unsafe_code)]
mod tests {
    use super::*;

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
        assert!(!config.is_quiet());
        assert!(!config.is_verbose());
        assert!(!config.use_color());
        assert!(!config.show_timing());
    }

    #[test]
    fn test_from_cli_quiet() {
        let config = OutputConfig::from_cli(true, false, false);
        assert!(config.is_quiet());
        assert!(!config.is_verbose());
    }

    #[test]
    fn test_from_cli_verbose() {
        let config = OutputConfig::from_cli(false, true, false);
        assert!(!config.is_quiet());
        assert!(config.is_verbose());
        assert!(config.show_timing());
    }

    #[test]
    fn test_from_cli_no_color() {
        let config = OutputConfig::from_cli(false, false, true);
        assert!(!config.use_color());
    }

    #[test]
    fn test_with_quiet() {
        let config = OutputConfig::new().with_quiet(true);
        assert!(config.is_quiet());
    }

    #[test]
    fn test_with_verbose() {
        let config = OutputConfig::new().with_verbose(true);
        assert!(config.is_verbose());
    }

    #[test]
    fn test_with_color() {
        let config = OutputConfig::new().with_color(true);
        assert!(config.use_color());
    }

    #[test]
    fn test_with_timing() {
        let config = OutputConfig::new().with_timing(true);
        assert!(config.show_timing());
    }

    #[test]
    fn test_builder_chaining() {
        let config = OutputConfig::new()
            .with_quiet(false)
            .with_verbose(true)
            .with_color(true)
            .with_timing(true);

        assert!(!config.is_quiet());
        assert!(config.is_verbose());
        assert!(config.use_color());
        assert!(config.show_timing());
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_detect_color_support_with_no_color_env() {
        unsafe {
            std::env::set_var("NO_COLOR", "1");
        }
        let supported = OutputConfig::detect_color_support();
        unsafe {
            std::env::remove_var("NO_COLOR");
        }
        assert!(!supported);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_from_cli_respects_no_color_env() {
        unsafe {
            std::env::set_var("NO_COLOR", "1");
        }
        let config = OutputConfig::from_cli(false, false, false);
        unsafe {
            std::env::remove_var("NO_COLOR");
        }
        assert!(!config.use_color());
    }
}
