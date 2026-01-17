//! I/O configuration for file operations.

use std::path::{Path, PathBuf};

/// Configuration for file I/O operations.
///
/// Controls how files are read and written during command execution.
#[derive(Debug, Clone, Default)]
pub struct IoConfig {
    /// Edit files in-place
    in_place: bool,
    /// Show changes without modifying (dry-run)
    dry_run: bool,
    /// Output file path (None = stdout)
    output_path: Option<PathBuf>,
}

impl IoConfig {
    /// Creates a new I/O configuration with default values.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets in-place editing mode.
    #[must_use]
    pub const fn with_in_place(mut self, in_place: bool) -> Self {
        self.in_place = in_place;
        self
    }

    /// Sets dry-run mode.
    #[must_use]
    pub const fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    /// Sets output file path.
    #[must_use]
    pub fn with_output_path(mut self, path: Option<PathBuf>) -> Self {
        self.output_path = path;
        self
    }

    /// Returns whether in-place editing is enabled.
    #[must_use]
    pub const fn is_in_place(&self) -> bool {
        self.in_place
    }

    /// Returns whether dry-run mode is enabled.
    #[must_use]
    pub const fn is_dry_run(&self) -> bool {
        self.dry_run
    }

    /// Returns the output file path, if any.
    #[must_use]
    pub fn output_path(&self) -> Option<&Path> {
        self.output_path.as_deref()
    }

    /// Validates configuration constraints.
    ///
    /// # Errors
    ///
    /// Returns an error if both `in_place` and `output_path` are set,
    /// as these options are mutually exclusive.
    #[allow(clippy::missing_const_for_fn)]
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.in_place && self.output_path.is_some() {
            return Err("--in-place and --output are mutually exclusive");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = IoConfig::default();
        assert!(!config.is_in_place());
        assert!(!config.is_dry_run());
        assert!(config.output_path().is_none());
    }

    #[test]
    fn test_new() {
        let config = IoConfig::new();
        assert!(!config.is_in_place());
        assert!(!config.is_dry_run());
        assert!(config.output_path().is_none());
    }

    #[test]
    fn test_with_in_place() {
        let config = IoConfig::new().with_in_place(true);
        assert!(config.is_in_place());
    }

    #[test]
    fn test_with_dry_run() {
        let config = IoConfig::new().with_dry_run(true);
        assert!(config.is_dry_run());
    }

    #[test]
    fn test_with_output_path() {
        let path = PathBuf::from("output.yaml");
        let config = IoConfig::new().with_output_path(Some(path.clone()));
        assert_eq!(config.output_path(), Some(path.as_path()));
    }

    #[test]
    fn test_builder_chaining() {
        let path = PathBuf::from("out.yaml");
        let config = IoConfig::new()
            .with_in_place(false)
            .with_dry_run(true)
            .with_output_path(Some(path.clone()));

        assert!(!config.is_in_place());
        assert!(config.is_dry_run());
        assert_eq!(config.output_path(), Some(path.as_path()));
    }

    #[test]
    fn test_validate_valid_config() {
        let config = IoConfig::new()
            .with_in_place(false)
            .with_output_path(Some(PathBuf::from("out.yaml")));

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_in_place_only() {
        let config = IoConfig::new().with_in_place(true);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_output_path_only() {
        let config = IoConfig::new()
            .with_output_path(Some(PathBuf::from("out.yaml")));
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_mutually_exclusive() {
        let config = IoConfig::new()
            .with_in_place(true)
            .with_output_path(Some(PathBuf::from("out.yaml")));

        let result = config.validate();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "--in-place and --output are mutually exclusive"
        );
    }

    #[test]
    fn test_validate_dry_run_compatible() {
        let config = IoConfig::new()
            .with_in_place(true)
            .with_dry_run(true);

        assert!(config.validate().is_ok());
    }
}
