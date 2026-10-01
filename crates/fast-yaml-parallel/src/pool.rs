//! Process-wide Rayon pool shared by every parallel entry point.
//!
//! Building a pool spawns OS threads, so a pool per call would dominate small batches. One
//! pool is cached and replaced only when a different worker count is requested.

#![allow(clippy::redundant_pub_crate)]

use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, PoisonError};

use rayon::{ThreadPool, ThreadPoolBuildError, ThreadPoolBuilder};

use crate::config::Config;
use crate::error::{Error, Result};

static POOL: Mutex<Option<(NonZeroUsize, Arc<ThreadPool>)>> = Mutex::new(None);

/// Returns the process-wide pool with exactly `workers` threads, building it on first use.
///
/// One pool is kept per process. Asking for a different worker count replaces it: its
/// threads exit once the last [`Arc`] handed out for it is dropped, so a caller that
/// alternates between two counts rebuilds the pool each time. The cached pool keeps its
/// `workers` threads resident until it is replaced or the process exits.
///
/// The pool is a `rayon::ThreadPool`, so callers need the same `rayon` major version as this
/// crate; a rayon upgrade is a breaking change of this function.
///
/// # Errors
///
/// Returns [`Error::ThreadPool`] when the operating system refuses to spawn the threads.
///
/// # Examples
///
/// ```
/// use std::num::NonZeroUsize;
/// use fast_yaml_parallel::shared_pool;
///
/// let workers = NonZeroUsize::new(2).unwrap();
/// let pool = shared_pool(workers)?;
/// assert_eq!(pool.install(rayon::current_num_threads), 2);
/// # Ok::<(), fast_yaml_parallel::Error>(())
/// ```
pub fn shared_pool(workers: NonZeroUsize) -> Result<Arc<ThreadPool>> {
    build(workers).map_err(Error::ThreadPool)
}

/// Failure to spawn the pool's threads; shared so one failure can be reported for many files.
pub(crate) type BuildError = Arc<ThreadPoolBuildError>;

fn build(workers: NonZeroUsize) -> std::result::Result<Arc<ThreadPool>, BuildError> {
    build_in(&POOL, workers)
}

fn build_in(
    slot: &Mutex<Option<(NonZeroUsize, Arc<ThreadPool>)>>,
    workers: NonZeroUsize,
) -> std::result::Result<Arc<ThreadPool>, BuildError> {
    let mut slot = slot.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((count, pool)) = slot.as_ref()
        && *count == workers
    {
        return Ok(Arc::clone(pool));
    }
    let pool = ThreadPoolBuilder::new()
        .num_threads(workers.get())
        .build()
        .map_err(Arc::new)?;
    let pool = Arc::new(pool);
    *slot = Some((workers, Arc::clone(&pool)));
    drop(slot);
    Ok(pool)
}

/// The pool `config` asks for, or `None` when the global Rayon pool already fits.
///
/// `None` also covers auto and sequential (`Some(0)`) settings, which callers handle without
/// a pool.
pub(crate) fn for_config(
    config: &Config,
) -> std::result::Result<Option<Arc<ThreadPool>>, BuildError> {
    match config.pool_workers() {
        Some(workers) if workers.get() != rayon::current_num_threads() => build(workers).map(Some),
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(n: usize) -> NonZeroUsize {
        NonZeroUsize::new(n).unwrap()
    }

    fn fresh() -> Mutex<Option<(NonZeroUsize, Arc<ThreadPool>)>> {
        Mutex::new(None)
    }

    #[test]
    fn test_pool_is_reused_for_the_same_worker_count() {
        let slot = fresh();
        let first = build_in(&slot, count(3)).unwrap();
        let second = build_in(&slot, count(3)).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.install(rayon::current_num_threads), 3);
    }

    #[test]
    fn test_pool_is_replaced_when_the_worker_count_changes() {
        let slot = fresh();
        let three = build_in(&slot, count(3)).unwrap();
        let five = build_in(&slot, count(5)).unwrap();
        assert!(!Arc::ptr_eq(&three, &five));
        assert_eq!(five.install(rayon::current_num_threads), 5);
        assert_eq!(three.install(rayon::current_num_threads), 3);
    }

    #[test]
    fn test_for_config_needs_no_pool_for_auto_and_sequential() {
        assert!(for_config(&Config::new()).unwrap().is_none());
        assert!(
            for_config(&Config::new().with_workers(Some(0)))
                .unwrap()
                .is_none()
        );
    }
}
