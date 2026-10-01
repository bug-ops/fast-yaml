//! Configuration for parallel processing behavior.

use std::num::NonZeroUsize;

use fast_yaml_core::KeyDomain;
use fast_yaml_core::limits::{MaxInputBytes, MaxScanAhead, ParseLimits};

use crate::scan_ahead::ScanAheadPolicy;

/// Maximum number of threads allowed (security limit).
const MAX_THREADS: usize = 128;

/// [`MAX_THREADS`] as a non-zero count.
const MAX_POOL_THREADS: NonZeroUsize = NonZeroUsize::MIN.saturating_add(MAX_THREADS - 1);

/// `n`, or one when `n` is zero.
const fn at_least_one(n: usize) -> NonZeroUsize {
    NonZeroUsize::MIN.saturating_add(n.saturating_sub(1))
}

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
/// - Maximum documents per input: 100 000 (`ParseLimits::max_documents`, via [`with_parse_limits`](Config::with_parse_limits))
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

    /// Maximum input size (`DoS` protection, default: 100MB)
    pub(crate) max_input_bytes: MaxInputBytes,

    /// Sequential threshold: use sequential for small inputs (default: 4KB)
    pub(crate) sequential_threshold: usize,

    /// Parser resource limits applied to every parse
    pub(crate) parse_limits: ParseLimits,

    /// How batch file operations bound the scanner look-ahead of their workers
    pub(crate) scan_ahead: ScanAheadPolicy,

    /// Which keys count as the same key when parsing
    pub(crate) key_domain: KeyDomain,
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

    /// Sets how [`FileProcessor`](crate::FileProcessor) formatting bounds the scanner
    /// look-ahead of its workers. Default: [`ScanAheadPolicy::Fixed`] at the default limit, or
    /// the limit of [`with_parse_limits`](Config::with_parse_limits). The scan-ahead limit of
    /// the `EmitterConfig` passed to the format methods is not consulted, so this `Config` is
    /// the one place that sets it. Calling [`with_parse_limits`](Config::with_parse_limits)
    /// after this resets the policy to `Fixed`
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_parallel::{Config, ScanAheadPolicy};
    ///
    /// let config = Config::new().with_scan_ahead_policy(ScanAheadPolicy::Scaled);
    /// ```
    #[must_use]
    pub const fn with_scan_ahead_policy(mut self, policy: ScanAheadPolicy) -> Self {
        self.scan_ahead = policy;
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
    /// the streaming formatter has its own fixed depth limit of 256 (see #427). The exception
    /// is `max_scan_ahead`: this call also sets the scan-ahead policy to
    /// [`ScanAheadPolicy::Fixed`] at that limit, which the format paths honor. Call
    /// [`with_scan_ahead_policy`](Config::with_scan_ahead_policy) afterwards to scale instead.
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
        self.scan_ahead = ScanAheadPolicy::Fixed(limits.max_scan_ahead);
        self
    }

    /// Sets which keys [`parse_parallel`](crate::parse_parallel) treats as the same key.
    ///
    /// Choose the domain of the host the documents are converted to, so keys that YAML keeps
    /// distinct but the host would merge (`1` and `true` in a Python dict) fail with a positioned
    /// error instead of silently losing an entry. Default: [`KeyDomain::Yaml`]. File-level
    /// parsing ignores it, because it only validates files.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::KeyDomain;
    /// use fast_yaml_parallel::{Config, parse_parallel_with_config};
    ///
    /// let config = Config::new().with_key_domain(KeyDomain::Python);
    /// assert!(parse_parallel_with_config("1: a\ntrue: b\n", &config).is_err());
    /// assert!(parse_parallel_with_config("1: a\ntrue: b\n", &Config::new()).is_ok());
    /// ```
    #[must_use]
    pub const fn with_key_domain(mut self, keys: KeyDomain) -> Self {
        self.key_domain = keys;
        self
    }

    /// Returns the key domain.
    #[must_use]
    pub const fn key_domain(&self) -> KeyDomain {
        self.key_domain
    }

    /// Returns the parser resource limits.
    #[must_use]
    pub const fn parse_limits(&self) -> ParseLimits {
        self.parse_limits
    }

    /// Returns how batch file operations bound the scanner look-ahead.
    #[must_use]
    pub const fn scan_ahead_policy(&self) -> ScanAheadPolicy {
        self.scan_ahead
    }

    /// Returns worker count setting.
    #[must_use]
    pub const fn workers(&self) -> Option<usize> {
        self.workers
    }

    /// Returns maximum input size.
    #[must_use]
    pub const fn max_input_bytes(&self) -> MaxInputBytes {
        self.max_input_bytes
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
            workers: None, // Auto-detect CPU count
            max_input_bytes: MaxInputBytes::DEFAULT,
            sequential_threshold: 4096, // 4KB
            parse_limits: ParseLimits::default(),
            scan_ahead: ScanAheadPolicy::Fixed(MaxScanAhead::DEFAULT),
            key_domain: KeyDomain::Yaml,
        }
    }
}

impl Config {
    /// The worker count a dedicated pool needs: `Some(n)` with `n > 0`, capped; `None` for
    /// auto and sequential settings, which run without one.
    pub(crate) fn pool_workers(&self) -> Option<NonZeroUsize> {
        self.workers
            .and_then(NonZeroUsize::new)
            .map(|workers| workers.min(MAX_POOL_THREADS))
    }

    /// How many threads run a batch: the dedicated pool, the global pool, or one when
    /// sequential.
    pub(crate) fn worker_count(&self) -> NonZeroUsize {
        self.pool_workers().unwrap_or_else(|| {
            at_least_one(match self.workers {
                Some(_) => 0,
                None => rayon::current_num_threads(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.workers, None);
        assert_eq!(config.max_input_bytes, MaxInputBytes::DEFAULT);
        assert_eq!(config.sequential_threshold, 4096);
    }

    #[test]
    fn test_config_builder() {
        let config = Config::new()
            .with_workers(Some(4))
            .with_max_input_bytes(MaxInputBytes::new(50 * 1024 * 1024).unwrap())
            .with_sequential_threshold(2048);

        assert_eq!(config.workers, Some(4));
        assert_eq!(config.max_input_bytes.get(), 50 * 1024 * 1024);
        assert_eq!(config.sequential_threshold, 2048);
    }

    #[test]
    fn test_sequential_mode() {
        let config = Config::new().with_workers(Some(0));
        assert_eq!(config.workers, Some(0));
    }

    #[test]
    fn test_pool_workers_capping() {
        let pool = |workers| Config::new().with_workers(workers).pool_workers();
        assert_eq!(pool(Some(4)).map(NonZeroUsize::get), Some(4));
        assert_eq!(pool(Some(10_000)).map(NonZeroUsize::get), Some(MAX_THREADS));
        assert_eq!(pool(Some(0)), None);
        assert_eq!(pool(None), None);
    }

    #[test]
    fn test_worker_count_is_never_zero() {
        let count = |workers| Config::new().with_workers(workers).worker_count().get();
        assert_eq!(count(Some(4)), 4);
        assert_eq!(count(Some(0)), 1);
        assert!(count(None) >= 1);
        assert_eq!(at_least_one(0).get(), 1);
        assert_eq!(at_least_one(7).get(), 7);
    }

    #[test]
    fn test_getters() {
        let config = Config::new()
            .with_workers(Some(8))
            .with_max_input_bytes(MaxInputBytes::new(50_000_000).unwrap())
            .with_sequential_threshold(8192);

        assert_eq!(config.workers(), Some(8));
        assert_eq!(config.max_input_bytes().get(), 50_000_000);
        assert_eq!(config.sequential_threshold(), 8192);
    }

    #[test]
    fn test_new_equals_default() {
        let config1 = Config::new();
        let config2 = Config::default();

        assert_eq!(config1.workers, config2.workers);
        assert_eq!(config1.max_input_bytes, config2.max_input_bytes);
        assert_eq!(config1.sequential_threshold, config2.sequential_threshold);
    }
}
