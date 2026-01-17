//! Parallel processing configuration.

/// Configuration for parallel processing.
///
/// Controls worker threads and memory-mapped I/O thresholds for batch operations.
#[derive(Debug, Clone)]
pub struct ParallelConfig {
    /// Number of worker threads (0 = auto-detect)
    workers: usize,
    /// File size threshold for memory-mapped reading (bytes)
    mmap_threshold: usize,
}

impl ParallelConfig {
    /// Default memory-mapped I/O threshold (512 KB).
    pub const DEFAULT_MMAP_THRESHOLD: usize = 512 * 1024;

    /// Creates a new parallel configuration with default values.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the number of worker threads.
    ///
    /// If set to 0, the number of workers will be automatically detected
    /// based on available CPU cores.
    #[must_use]
    pub const fn with_workers(mut self, workers: usize) -> Self {
        self.workers = workers;
        self
    }

    /// Sets the memory-mapped I/O threshold in bytes.
    ///
    /// Files larger than this threshold will use memory-mapped I/O
    /// for better performance.
    #[must_use]
    pub const fn with_mmap_threshold(mut self, threshold: usize) -> Self {
        self.mmap_threshold = threshold;
        self
    }

    /// Returns the effective number of workers.
    ///
    /// If workers is 0, auto-detects based on available CPU cores.
    #[must_use]
    pub fn effective_workers(&self) -> usize {
        if self.workers == 0 {
            num_cpus::get()
        } else {
            self.workers
        }
    }

    /// Returns the configured number of workers.
    #[must_use]
    pub const fn workers(&self) -> usize {
        self.workers
    }

    /// Returns the memory-mapped I/O threshold.
    #[must_use]
    pub const fn mmap_threshold(&self) -> usize {
        self.mmap_threshold
    }
}

impl Default for ParallelConfig {
    fn default() -> Self {
        Self {
            workers: 0,
            mmap_threshold: Self::DEFAULT_MMAP_THRESHOLD,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ParallelConfig::default();
        assert_eq!(config.workers(), 0);
        assert_eq!(config.mmap_threshold(), ParallelConfig::DEFAULT_MMAP_THRESHOLD);
    }

    #[test]
    fn test_new() {
        let config = ParallelConfig::new();
        assert_eq!(config.workers(), 0);
        assert_eq!(config.mmap_threshold(), 512 * 1024);
    }

    #[test]
    fn test_with_workers() {
        let config = ParallelConfig::new().with_workers(4);
        assert_eq!(config.workers(), 4);
    }

    #[test]
    fn test_with_mmap_threshold() {
        let config = ParallelConfig::new().with_mmap_threshold(1024 * 1024);
        assert_eq!(config.mmap_threshold(), 1024 * 1024);
    }

    #[test]
    fn test_builder_chaining() {
        let config = ParallelConfig::new()
            .with_workers(8)
            .with_mmap_threshold(2 * 1024 * 1024);

        assert_eq!(config.workers(), 8);
        assert_eq!(config.mmap_threshold(), 2 * 1024 * 1024);
    }

    #[test]
    fn test_effective_workers_auto_detect() {
        let config = ParallelConfig::new().with_workers(0);
        let effective = config.effective_workers();
        assert!(effective > 0);
        assert_eq!(effective, num_cpus::get());
    }

    #[test]
    fn test_effective_workers_explicit() {
        let config = ParallelConfig::new().with_workers(4);
        assert_eq!(config.effective_workers(), 4);
    }

    #[test]
    fn test_effective_workers_override() {
        let config = ParallelConfig::new().with_workers(16);
        assert_eq!(config.effective_workers(), 16);
    }
}
