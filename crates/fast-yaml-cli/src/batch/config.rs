//! Configuration types for batch file processing.

/// Configuration for parallel file processing.
#[derive(Debug, Clone)]
pub struct ProcessingConfig {
    /// Indentation width (2-8 spaces)
    pub indent: u8,
    /// Maximum line width
    pub width: usize,
    /// Edit files in-place
    pub in_place: bool,
    /// Show changes without modifying files (dry-run mode)
    pub dry_run: bool,
    /// Number of parallel workers (0 = auto-detect)
    pub workers: usize,
    /// File size threshold for memory-mapped reading (bytes)
    pub mmap_threshold: usize,
    /// Enable verbose progress output
    pub verbose: bool,
}

impl ProcessingConfig {
    /// Default memory-map threshold: 512KB
    pub const DEFAULT_MMAP_THRESHOLD: usize = 512 * 1024;

    /// Minimum indentation width
    pub const MIN_INDENT: u8 = 2;

    /// Maximum indentation width
    pub const MAX_INDENT: u8 = 8;

    /// Creates a new `ProcessingConfig` with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the effective number of workers to use.
    /// If workers is 0, returns the number of logical CPU cores.
    pub fn effective_workers(&self) -> usize {
        if self.workers == 0 {
            num_cpus::get()
        } else {
            self.workers
        }
    }

    /// Sets the indentation width (clamped to `MIN_INDENT..=MAX_INDENT`)
    #[must_use]
    pub fn with_indent(mut self, indent: u8) -> Self {
        self.indent = indent.clamp(Self::MIN_INDENT, Self::MAX_INDENT);
        self
    }

    /// Sets the maximum line width
    #[must_use]
    pub const fn with_width(mut self, width: usize) -> Self {
        self.width = width;
        self
    }

    /// Enables in-place file editing
    #[must_use]
    pub const fn with_in_place(mut self, in_place: bool) -> Self {
        self.in_place = in_place;
        self
    }

    /// Enables dry-run mode (show changes without modifying files)
    #[must_use]
    pub const fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    /// Sets the number of parallel workers (0 = auto-detect)
    #[must_use]
    pub const fn with_workers(mut self, workers: usize) -> Self {
        self.workers = workers;
        self
    }

    /// Sets the memory-map threshold
    #[must_use]
    pub const fn with_mmap_threshold(mut self, threshold: usize) -> Self {
        self.mmap_threshold = threshold;
        self
    }

    /// Enables verbose progress output
    #[must_use]
    pub const fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }
}

impl Default for ProcessingConfig {
    fn default() -> Self {
        Self {
            indent: 2,
            width: 80,
            in_place: false,
            dry_run: false,
            workers: 0, // Auto-detect
            mmap_threshold: Self::DEFAULT_MMAP_THRESHOLD,
            verbose: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ProcessingConfig::default();
        assert_eq!(config.indent, 2);
        assert_eq!(config.width, 80);
        assert!(!config.in_place);
        assert!(!config.dry_run);
        assert_eq!(config.workers, 0);
        assert_eq!(
            config.mmap_threshold,
            ProcessingConfig::DEFAULT_MMAP_THRESHOLD
        );
        assert!(!config.verbose);
    }

    #[test]
    fn test_effective_workers_default() {
        let config = ProcessingConfig::default();
        let workers = config.effective_workers();
        assert!(workers > 0);
        assert_eq!(workers, num_cpus::get());
    }

    #[test]
    fn test_effective_workers_custom() {
        let config = ProcessingConfig::default().with_workers(4);
        assert_eq!(config.effective_workers(), 4);
    }

    #[test]
    fn test_builder_pattern() {
        let config = ProcessingConfig::new()
            .with_indent(4)
            .with_width(120)
            .with_in_place(true)
            .with_dry_run(false)
            .with_workers(8)
            .with_mmap_threshold(2 * 1024 * 1024)
            .with_verbose(true);

        assert_eq!(config.indent, 4);
        assert_eq!(config.width, 120);
        assert!(config.in_place);
        assert!(!config.dry_run);
        assert_eq!(config.workers, 8);
        assert_eq!(config.mmap_threshold, 2 * 1024 * 1024);
        assert!(config.verbose);
    }

    #[test]
    fn test_indent_clamping() {
        let config = ProcessingConfig::new().with_indent(1);
        assert_eq!(config.indent, ProcessingConfig::MIN_INDENT);

        let config = ProcessingConfig::new().with_indent(10);
        assert_eq!(config.indent, ProcessingConfig::MAX_INDENT);

        let config = ProcessingConfig::new().with_indent(4);
        assert_eq!(config.indent, 4);
    }

    #[test]
    fn test_new_equals_default() {
        let new_config = ProcessingConfig::new();
        let default_config = ProcessingConfig::default();

        assert_eq!(new_config.indent, default_config.indent);
        assert_eq!(new_config.width, default_config.width);
        assert_eq!(new_config.in_place, default_config.in_place);
        assert_eq!(new_config.dry_run, default_config.dry_run);
        assert_eq!(new_config.workers, default_config.workers);
        assert_eq!(new_config.mmap_threshold, default_config.mmap_threshold);
        assert_eq!(new_config.verbose, default_config.verbose);
    }

    #[test]
    fn test_conflicting_flags() {
        let config = ProcessingConfig::new()
            .with_in_place(true)
            .with_dry_run(true);

        assert!(config.in_place);
        assert!(config.dry_run);
    }
}
