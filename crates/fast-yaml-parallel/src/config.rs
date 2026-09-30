//! Configuration for parallel processing behavior.

use fast_yaml_core::limits::{MaxDocuments, MaxInputBytes, ParseLimits};

/// Maximum number of threads allowed (security limit).
const MAX_THREADS: usize = 128;

/// Configuration for parallel processing behavior.
///
/// Simplified configuration with essential fields for both document-level
/// and file-level parallelism.
///
/// # Security Limits
///
/// To prevent denial-of-service attacks and resource exhaustion:
/// - Maximum threads: 128
/// - Maximum input size: 100MB (configurable via [`with_max_input_bytes`](Config::with_max_input_bytes))
/// - Maximum documents per input: 100 000 (configurable via [`with_max_documents`](Config::with_max_documents))
///
/// # Examples
///
/// ```
/// use fast_yaml_parallel::Config;
///
/// let config = Config::new()
///     .with_workers(Some(8))
///     .with_sequential_threshold(2048);
/// ```
#[derive(Debug, Clone)]
pub struct Config {
    /// Worker count: None = auto (CPU count), Some(0) = sequential, Some(n) = n threads
    pub(crate) workers: Option<usize>,

    /// Mmap threshold for large file reading (default: 512KB)
    pub(crate) mmap_threshold: usize,

    /// Maximum input size (`DoS` protection, default: 100MB)
    pub(crate) max_input_bytes: MaxInputBytes,

    /// Maximum documents in one input (`DoS` protection, default: 100 000)
    pub(crate) max_documents: MaxDocuments,

    /// Sequential threshold: use sequential for small inputs (default: 4KB)
    pub(crate) sequential_threshold: usize,

    /// Parser resource limits applied to every parse
    pub(crate) parse_limits: ParseLimits,
}

impl Config {
    /// Creates default configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_parallel::Config;
    ///
    /// let config = Config::new();
    /// ```
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets worker count.
    ///
    /// - `None`: Auto-detect CPU count (default, capped at 128)
    /// - `Some(0)`: Sequential processing (no parallelism)
    /// - `Some(n)`: Use exactly `n` threads (capped at 128)
    ///
    /// # Security
    ///
    /// Thread count is capped at 128 to prevent resource exhaustion.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_parallel::Config;
    ///
    /// let config = Config::new().with_workers(Some(4));
    /// ```
    #[must_use]
    pub const fn with_workers(mut self, workers: Option<usize>) -> Self {
        self.workers = workers;
        self
    }

    /// Sets memory-map threshold for file reading.
    ///
    /// Files larger than this threshold will use memory-mapped I/O.
    /// Default: 512KB
    ///
    /// # Tuning Guidance
    ///
    /// The optimal threshold depends on your workload:
    ///
    /// - **Lower (256KB-512KB)**: Better for many medium files (100KB-1MB)
    ///   - Pros: Less virtual memory pressure, faster for small-to-medium files
    ///   - Cons: More heap allocations for files just above threshold
    ///
    /// - **Higher (1MB-2MB)**: Better for fewer large files (>2MB)
    ///   - Pros: Fewer mmaps, better for very large files
    ///   - Cons: More heap usage for medium files
    ///
    /// Consider your typical file size distribution and available memory.
    /// Profile with real data before changing the default.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_parallel::Config;
    ///
    /// let config = Config::new()
    ///     .with_mmap_threshold(1024 * 1024); // 1MB
    /// ```
    #[must_use]
    pub const fn with_mmap_threshold(mut self, threshold: usize) -> Self {
        self.mmap_threshold = threshold;
        self
    }

    /// Sets maximum input size in bytes.
    ///
    /// Input exceeding this size will be rejected; files are checked before being read.
    /// Default: 100MB
    ///
    /// # Security
    ///
    /// This limit prevents denial-of-service attacks via extremely large inputs.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::limits::MaxInputBytes;
    /// use fast_yaml_parallel::Config;
    ///
    /// let config = Config::new()
    ///     .with_max_input_bytes(MaxInputBytes::new(200 * 1024 * 1024).unwrap()); // 200MB
    /// ```
    #[must_use]
    pub const fn with_max_input_bytes(mut self, max: MaxInputBytes) -> Self {
        self.max_input_bytes = max;
        self
    }

    /// Sets the maximum number of documents accepted from one input.
    ///
    /// [`parse_parallel`](crate::parse_parallel) rejects input with more documents before parsing
    /// when the chunk count already exceeds the limit, and again after parsing.
    /// Default: 100 000
    ///
    /// # Security
    ///
    /// This limit bounds the memory spent on input made of a huge number of tiny documents.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::limits::MaxDocuments;
    /// use fast_yaml_parallel::{Config, Error, parse_parallel_with_config};
    ///
    /// let config = Config::new().with_max_documents(MaxDocuments::new(2).unwrap());
    /// assert!(parse_parallel_with_config("a: 1\n---\nb: 2\n", &config).is_ok());
    /// assert!(matches!(
    ///     parse_parallel_with_config("---\na\n---\nb\n---\nc\n", &config),
    ///     Err(Error::TooManyDocuments { .. })
    /// ));
    /// ```
    #[must_use]
    pub const fn with_max_documents(mut self, max: MaxDocuments) -> Self {
        self.max_documents = max;
        self
    }

    /// Sets sequential processing threshold.
    ///
    /// Inputs smaller than this threshold will use sequential processing
    /// to avoid parallelism overhead. Default: 4KB
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_parallel::Config;
    ///
    /// let config = Config::new()
    ///     .with_sequential_threshold(2048);
    /// ```
    #[must_use]
    pub const fn with_sequential_threshold(mut self, threshold: usize) -> Self {
        self.sequential_threshold = threshold;
        self
    }

    /// Sets the parser resource limits.
    ///
    /// Document-level parsing ([`parse_parallel`](crate::parse_parallel)) shares one alias
    /// budget across all chunks of a call. File-level parsing
    /// ([`FileProcessor::parse_files`](crate::FileProcessor::parse_files)) gives each file its own
    /// budget, so peak memory can reach `workers x max_alias_bytes`.
    ///
    /// Only parsing honors these limits: the format paths
    /// ([`FileProcessor::format_files`](crate::FileProcessor::format_files)) ignore them, because
    /// the streaming formatter has its own fixed depth limit of 256 (see #427).
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::limits::{MaxDepth, ParseLimits};
    /// use fast_yaml_parallel::Config;
    ///
    /// let limits = ParseLimits { max_depth: MaxDepth::new(8).unwrap(), ..ParseLimits::default() };
    /// let config = Config::new().with_parse_limits(limits);
    /// assert_eq!(config.parse_limits().max_depth.get(), 8);
    /// ```
    #[must_use]
    pub const fn with_parse_limits(mut self, limits: ParseLimits) -> Self {
        self.parse_limits = limits;
        self
    }

    /// Returns the parser resource limits.
    #[must_use]
    pub const fn parse_limits(&self) -> ParseLimits {
        self.parse_limits
    }

    /// Returns worker count setting.
    #[must_use]
    pub const fn workers(&self) -> Option<usize> {
        self.workers
    }

    /// Returns mmap threshold.
    #[must_use]
    pub const fn mmap_threshold(&self) -> usize {
        self.mmap_threshold
    }

    /// Returns maximum input size.
    #[must_use]
    pub const fn max_input_bytes(&self) -> MaxInputBytes {
        self.max_input_bytes
    }

    /// Returns maximum document count.
    #[must_use]
    pub const fn max_documents(&self) -> MaxDocuments {
        self.max_documents
    }

    /// Returns sequential threshold.
    #[must_use]
    pub const fn sequential_threshold(&self) -> usize {
        self.sequential_threshold
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            workers: None,              // Auto-detect CPU count
            mmap_threshold: 512 * 1024, // 512KB
            max_input_bytes: MaxInputBytes::DEFAULT,
            max_documents: MaxDocuments::DEFAULT,
            sequential_threshold: 4096, // 4KB
            parse_limits: ParseLimits::default(),
        }
    }
}

impl Config {
    /// Returns the effective worker count, capped at security limit.
    ///
    /// # Security
    ///
    /// Worker count is capped at 128 to prevent resource exhaustion.
    pub(crate) fn effective_workers(&self) -> usize {
        let count = self.workers.unwrap_or_else(num_cpus::get);
        count.min(MAX_THREADS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.workers, None);
        assert_eq!(config.mmap_threshold, 512 * 1024);
        assert_eq!(config.max_input_bytes, MaxInputBytes::DEFAULT);
        assert_eq!(config.max_documents, MaxDocuments::DEFAULT);
        assert_eq!(config.sequential_threshold, 4096);
    }

    #[test]
    fn test_config_builder() {
        let config = Config::new()
            .with_workers(Some(4))
            .with_mmap_threshold(1024 * 1024)
            .with_max_input_bytes(MaxInputBytes::new(50 * 1024 * 1024).unwrap())
            .with_max_documents(MaxDocuments::new(7).unwrap())
            .with_sequential_threshold(2048);

        assert_eq!(config.workers, Some(4));
        assert_eq!(config.mmap_threshold, 1024 * 1024);
        assert_eq!(config.max_input_bytes.get(), 50 * 1024 * 1024);
        assert_eq!(config.max_documents.get(), 7);
        assert_eq!(config.sequential_threshold, 2048);
    }

    #[test]
    fn test_sequential_mode() {
        let config = Config::new().with_workers(Some(0));
        assert_eq!(config.workers, Some(0));
    }

    #[test]
    fn test_effective_workers_capping() {
        // Normal case
        let config = Config::new().with_workers(Some(4));
        assert_eq!(config.effective_workers(), 4);

        // Excessive worker count (should be capped)
        let config = Config::new().with_workers(Some(10_000));
        assert_eq!(config.effective_workers(), MAX_THREADS);

        // Auto-detect (should be capped if CPU count > MAX_THREADS)
        let config = Config::new();
        assert!(config.effective_workers() <= MAX_THREADS);

        // Sequential mode
        let config = Config::new().with_workers(Some(0));
        assert_eq!(config.effective_workers(), 0);
    }

    #[test]
    fn test_getters() {
        let config = Config::new()
            .with_workers(Some(8))
            .with_mmap_threshold(2048)
            .with_max_input_bytes(MaxInputBytes::new(50_000_000).unwrap())
            .with_max_documents(MaxDocuments::new(9).unwrap())
            .with_sequential_threshold(8192);

        assert_eq!(config.workers(), Some(8));
        assert_eq!(config.mmap_threshold(), 2048);
        assert_eq!(config.max_input_bytes().get(), 50_000_000);
        assert_eq!(config.max_documents().get(), 9);
        assert_eq!(config.sequential_threshold(), 8192);
    }

    #[test]
    fn test_new_equals_default() {
        let config1 = Config::new();
        let config2 = Config::default();

        assert_eq!(config1.workers, config2.workers);
        assert_eq!(config1.mmap_threshold, config2.mmap_threshold);
        assert_eq!(config1.max_input_bytes, config2.max_input_bytes);
        assert_eq!(config1.max_documents, config2.max_documents);
        assert_eq!(config1.sequential_threshold, config2.sequential_threshold);
    }
}
