//! Configuration for parallel processing behavior.

/// Maximum number of threads allowed (security limit).
const MAX_THREADS: usize = 128;

/// Maximum input size in bytes (100MB default).
const DEFAULT_MAX_INPUT_SIZE: usize = 100 * 1024 * 1024;

/// Maximum number of documents allowed (denial-of-service protection).
const DEFAULT_MAX_DOCUMENTS: usize = 100_000;

/// Configuration for parallel processing behavior.
///
/// Controls thread pool size, chunking thresholds, resource limits,
/// and performance tuning parameters.
///
/// # Security Limits
///
/// To prevent denial-of-service attacks and resource exhaustion, the following limits are enforced:
/// - Maximum threads: 128
/// - Maximum input size: 100MB (configurable)
/// - Maximum document count: 100,000 (configurable)
///
/// # Examples
///
/// ```
/// use fast_yaml_parallel::ParallelConfig;
///
/// let config = ParallelConfig::new()
///     .with_thread_count(Some(8))
///     .with_min_chunk_size(2048);
/// ```
#[derive(Debug, Clone)]
pub struct ParallelConfig {
    /// Thread pool size (None = CPU count, Some(0) = sequential).
    pub(crate) thread_count: Option<usize>,

    /// Minimum bytes per chunk (prevents over-chunking small files).
    pub(crate) min_chunk_size: usize,

    /// Maximum bytes per chunk (prevents memory spikes).
    pub(crate) max_chunk_size: usize,

    /// Maximum total input size in bytes (denial-of-service protection).
    pub(crate) max_input_size: usize,

    /// Maximum number of documents allowed (denial-of-service protection).
    pub(crate) max_documents: usize,
}

impl ParallelConfig {
    /// Creates default configuration (auto thread count).
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_parallel::ParallelConfig;
    ///
    /// let config = ParallelConfig::new();
    /// ```
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets thread pool size.
    ///
    /// - `None`: Use all available CPU cores (default, capped at 128)
    /// - `Some(0)`: Sequential processing (no parallelism)
    /// - `Some(n)`: Use exactly `n` threads (max 128)
    ///
    /// # Security
    ///
    /// Thread count is capped at 128 to prevent resource exhaustion.
    /// Values exceeding this limit will be clamped at runtime.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_parallel::ParallelConfig;
    ///
    /// let config = ParallelConfig::new().with_thread_count(Some(4));
    /// ```
    #[must_use]
    pub const fn with_thread_count(mut self, count: Option<usize>) -> Self {
        self.thread_count = count;
        self
    }

    /// Sets maximum total input size in bytes.
    ///
    /// Input exceeding this size will be rejected with `ConfigError`.
    /// Default: 100MB
    ///
    /// # Security
    ///
    /// This limit prevents denial-of-service attacks via extremely large inputs.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_parallel::ParallelConfig;
    ///
    /// // Allow up to 200MB
    /// let config = ParallelConfig::new()
    ///     .with_max_input_size(200 * 1024 * 1024);
    /// ```
    #[must_use]
    pub const fn with_max_input_size(mut self, size: usize) -> Self {
        self.max_input_size = size;
        self
    }

    /// Sets maximum number of documents allowed.
    ///
    /// Input with more documents than this will be rejected with `ConfigError`.
    /// Default: 100,000
    ///
    /// # Security
    ///
    /// This limit prevents denial-of-service attacks via excessive document counts.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_parallel::ParallelConfig;
    ///
    /// // Allow up to 1 million documents
    /// let config = ParallelConfig::new()
    ///     .with_max_documents(1_000_000);
    /// ```
    #[must_use]
    pub const fn with_max_documents(mut self, count: usize) -> Self {
        self.max_documents = count;
        self
    }

    /// Sets minimum total size in bytes for parallel processing.
    ///
    /// If total input size is below this threshold AND fewer than 4 documents,
    /// sequential processing will be used to avoid parallelism overhead.
    /// Default: 4KB
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_parallel::ParallelConfig;
    ///
    /// let config = ParallelConfig::new().with_min_chunk_size(2048);
    /// ```
    #[must_use]
    pub const fn with_min_chunk_size(mut self, size: usize) -> Self {
        self.min_chunk_size = size;
        self
    }

    /// Sets maximum chunk size in bytes.
    ///
    /// Large documents exceeding this will be processed sequentially.
    /// Default: 10MB (prevents memory spikes)
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_parallel::ParallelConfig;
    ///
    /// let config = ParallelConfig::new().with_max_chunk_size(5 * 1024 * 1024);
    /// ```
    #[must_use]
    pub const fn with_max_chunk_size(mut self, size: usize) -> Self {
        self.max_chunk_size = size;
        self
    }
}

impl Default for ParallelConfig {
    fn default() -> Self {
        Self {
            thread_count: None,                     // Auto-detect CPU count
            min_chunk_size: 4096,                   // 4KB minimum total size
            max_chunk_size: 10 * 1024 * 1024,       // 10MB maximum
            max_input_size: DEFAULT_MAX_INPUT_SIZE, // 100MB maximum input
            max_documents: DEFAULT_MAX_DOCUMENTS,   // 100k maximum documents
        }
    }
}

impl ParallelConfig {
    /// Returns the effective thread count, capped at security limit.
    ///
    /// # Security
    ///
    /// Thread count is capped at 128 to prevent resource exhaustion,
    /// even if user requests more or CPU count exceeds this.
    pub(crate) fn effective_thread_count(&self) -> usize {
        let count = self.thread_count.unwrap_or_else(num_cpus::get);
        count.min(MAX_THREADS)
    }

    /// Returns maximum input size limit.
    pub(crate) const fn max_input_size(&self) -> usize {
        self.max_input_size
    }

    /// Returns maximum document count limit.
    pub(crate) const fn max_documents(&self) -> usize {
        self.max_documents
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ParallelConfig::default();
        assert_eq!(config.thread_count, None);
        assert_eq!(config.min_chunk_size, 4096);
        assert_eq!(config.max_chunk_size, 10 * 1024 * 1024);
        assert_eq!(config.max_input_size, 100 * 1024 * 1024);
        assert_eq!(config.max_documents, 100_000);
    }

    #[test]
    fn test_config_builder() {
        let config = ParallelConfig::new()
            .with_thread_count(Some(4))
            .with_min_chunk_size(2048)
            .with_max_chunk_size(5 * 1024 * 1024)
            .with_max_input_size(50 * 1024 * 1024)
            .with_max_documents(50_000);

        assert_eq!(config.thread_count, Some(4));
        assert_eq!(config.min_chunk_size, 2048);
        assert_eq!(config.max_chunk_size, 5 * 1024 * 1024);
        assert_eq!(config.max_input_size, 50 * 1024 * 1024);
        assert_eq!(config.max_documents, 50_000);
    }

    #[test]
    fn test_sequential_mode() {
        let config = ParallelConfig::new().with_thread_count(Some(0));
        assert_eq!(config.thread_count, Some(0));
    }

    #[test]
    fn test_effective_thread_count_capping() {
        // Normal case
        let config = ParallelConfig::new().with_thread_count(Some(4));
        assert_eq!(config.effective_thread_count(), 4);

        // Excessive thread count (should be capped)
        let config = ParallelConfig::new().with_thread_count(Some(10_000));
        assert_eq!(config.effective_thread_count(), MAX_THREADS);

        // Auto-detect (should be capped if CPU count > MAX_THREADS)
        let config = ParallelConfig::new();
        assert!(config.effective_thread_count() <= MAX_THREADS);

        // Sequential mode
        let config = ParallelConfig::new().with_thread_count(Some(0));
        assert_eq!(config.effective_thread_count(), 0);
    }
}
