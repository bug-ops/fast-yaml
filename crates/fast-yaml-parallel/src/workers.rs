//! Typed worker counts: how many threads a parallel run may use.
//!
//! [`WorkerCount`] holds the one bound every surface shares (1 to 128 threads) and [`Workers`]
//! adds the "choose for me" and "no parallelism" settings, so no caller passes a magic number.

use std::fmt;
use std::num::NonZeroUsize;

use fast_yaml_core::limits::LimitRangeError;

/// A thread count from 1 to [`WorkerCount::MAX`] inclusive.
///
/// The upper bound is a resource-exhaustion limit shared by the library, the CLI and both
/// language bindings.
///
/// # Examples
///
/// ```
/// use fast_yaml_parallel::WorkerCount;
///
/// assert_eq!(WorkerCount::new(8)?.get(), 8);
/// assert!(WorkerCount::new(0).is_err());
/// assert!(WorkerCount::new(129).is_err());
/// # Ok::<(), fast_yaml_core::limits::LimitRangeError>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WorkerCount(NonZeroUsize);

impl WorkerCount {
    /// One thread.
    pub const MIN: Self = Self(NonZeroUsize::MIN);

    /// The largest accepted thread count (128).
    pub const MAX: Self = Self(NonZeroUsize::MIN.saturating_add(127));

    /// Creates a worker count.
    ///
    /// # Errors
    ///
    /// Returns [`LimitRangeError`] when `value` is outside `1..=128`.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_parallel::WorkerCount;
    ///
    /// assert_eq!(WorkerCount::new(128), Ok(WorkerCount::MAX));
    /// assert_eq!(WorkerCount::new(129).unwrap_err().max, 128);
    /// ```
    pub const fn new(value: usize) -> Result<Self, LimitRangeError> {
        match NonZeroUsize::new(value) {
            Some(count) if count.get() <= Self::MAX.get() => Ok(Self(count)),
            _ => Err(LimitRangeError {
                value,
                min: Self::MIN.get(),
                max: Self::MAX.get(),
            }),
        }
    }

    /// Returns the count as a plain number.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0.get()
    }

    /// Returns the count as a [`NonZeroUsize`].
    #[must_use]
    pub const fn get_nonzero(self) -> NonZeroUsize {
        self.0
    }
}

impl TryFrom<usize> for WorkerCount {
    type Error = LimitRangeError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl fmt::Display for WorkerCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// How many threads a parallel run uses.
///
/// # Examples
///
/// ```
/// use fast_yaml_parallel::{Config, WorkerCount, Workers};
///
/// let config = Config::new().with_workers(Workers::Fixed(WorkerCount::new(4)?));
/// assert_eq!(config.workers(), Workers::Fixed(WorkerCount::new(4)?));
/// assert_eq!(Workers::try_from(0), Ok(Workers::Sequential));
/// # Ok::<(), fast_yaml_core::limits::LimitRangeError>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Workers {
    /// Use the global Rayon pool, capped at [`WorkerCount::MAX`] threads.
    #[default]
    Auto,
    /// Run on the calling thread without parallelism.
    Sequential,
    /// Run in a pool of exactly this many threads.
    Fixed(WorkerCount),
}

impl Workers {
    /// The surface convention shared by the bindings: `0` is [`Sequential`](Self::Sequential),
    /// `1..=128` is [`Fixed`](Self::Fixed).
    ///
    /// # Errors
    ///
    /// Returns [`LimitRangeError`] (`min` 0, `max` 128) when `value` is above 128.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_parallel::{WorkerCount, Workers};
    ///
    /// assert_eq!(Workers::from_count(0), Ok(Workers::Sequential));
    /// assert_eq!(Workers::from_count(4)?, Workers::Fixed(WorkerCount::new(4)?));
    /// assert_eq!(Workers::from_count(129).unwrap_err().min, 0);
    /// # Ok::<(), fast_yaml_core::limits::LimitRangeError>(())
    /// ```
    pub const fn from_count(value: usize) -> Result<Self, LimitRangeError> {
        if value == 0 {
            return Ok(Self::Sequential);
        }
        match WorkerCount::new(value) {
            Ok(count) => Ok(Self::Fixed(count)),
            Err(err) => Err(LimitRangeError { min: 0, ..err }),
        }
    }

    /// The number of threads this setting actually runs: the capped size of the current Rayon
    /// pool for [`Auto`](Self::Auto), one for [`Sequential`](Self::Sequential).
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_parallel::{WorkerCount, Workers};
    ///
    /// assert_eq!(Workers::Sequential.threads(), WorkerCount::MIN);
    /// assert!(Workers::Auto.threads() <= WorkerCount::MAX);
    /// ```
    #[must_use]
    pub fn threads(self) -> WorkerCount {
        match self {
            Self::Auto => Self::capped_pool_threads(),
            Self::Sequential => WorkerCount::MIN,
            Self::Fixed(count) => count,
        }
    }

    /// The size of the dedicated pool this setting needs, or `None` when the current Rayon
    /// pool already fits (or no pool is needed).
    pub(crate) fn dedicated_pool(self) -> Option<WorkerCount> {
        let current = rayon::current_num_threads();
        match self {
            Self::Sequential => None,
            Self::Fixed(count) => (count.get() != current).then_some(count),
            Self::Auto => (current > WorkerCount::MAX.get()).then_some(WorkerCount::MAX),
        }
    }

    fn capped_pool_threads() -> WorkerCount {
        WorkerCount::new(rayon::current_num_threads().max(1)).unwrap_or(WorkerCount::MAX)
    }
}

impl TryFrom<usize> for Workers {
    type Error = LimitRangeError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        Self::from_count(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_count_bounds() {
        assert_eq!(WorkerCount::new(1), Ok(WorkerCount::MIN));
        assert_eq!(WorkerCount::new(128), Ok(WorkerCount::MAX));
        let err = WorkerCount::new(0).unwrap_err();
        assert_eq!((err.value, err.min, err.max), (0, 1, 128));
        assert!(WorkerCount::new(129).is_err());
        assert_eq!(WorkerCount::MAX.to_string(), "128");
    }

    #[test]
    fn workers_from_count_follows_the_binding_convention() {
        assert_eq!(Workers::try_from(0), Ok(Workers::Sequential));
        assert_eq!(
            Workers::try_from(4),
            Ok(Workers::Fixed(WorkerCount::new(4).unwrap()))
        );
        let err = Workers::try_from(129).unwrap_err();
        assert_eq!((err.value, err.min, err.max), (129, 0, 128));
        assert_eq!(err.to_string(), "must be between 0 and 128, got 129");
    }

    #[test]
    fn threads_report_what_runs() {
        let four = WorkerCount::new(4).unwrap();
        assert_eq!(Workers::Fixed(four).threads(), four);
        assert_eq!(Workers::Sequential.threads(), WorkerCount::MIN);
        assert!(Workers::Auto.threads() >= WorkerCount::MIN);
    }

    #[test]
    fn dedicated_pool_only_when_the_current_pool_does_not_fit() {
        let current = rayon::current_num_threads();
        assert_eq!(Workers::Sequential.dedicated_pool(), None);
        assert_eq!(Workers::Auto.dedicated_pool(), None);
        let same = WorkerCount::new(current.clamp(1, 128)).unwrap();
        if current <= 128 {
            assert_eq!(Workers::Fixed(same).dedicated_pool(), None);
        }
        let other = WorkerCount::new(if current == 3 { 5 } else { 3 }).unwrap();
        assert_eq!(Workers::Fixed(other).dedicated_pool(), Some(other));
    }

    #[test]
    fn auto_is_capped_inside_an_oversized_pool() {
        let big = rayon::ThreadPoolBuilder::new()
            .num_threads(130)
            .build()
            .unwrap();
        big.install(|| {
            assert_eq!(rayon::current_num_threads(), 130);
            assert_eq!(Workers::Auto.threads(), WorkerCount::MAX);
            assert_eq!(Workers::Auto.dedicated_pool(), Some(WorkerCount::MAX));
        });
    }

    #[test]
    fn auto_uses_the_current_pool_up_to_the_cap() {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(128)
            .build()
            .unwrap();
        pool.install(|| {
            assert_eq!(Workers::Auto.threads(), WorkerCount::MAX);
            assert_eq!(Workers::Auto.dedicated_pool(), None);
        });
    }
}
