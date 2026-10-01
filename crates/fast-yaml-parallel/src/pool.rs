//! Process-wide Rayon pool shared by every parallel entry point.
//!
//! Building a pool spawns OS threads, so a pool per call would dominate small batches. One
//! pool is cached and replaced only when a different worker count is requested.

#![allow(clippy::redundant_pub_crate)]

use std::sync::{Arc, Mutex, PoisonError};

use rayon::{ThreadPool, ThreadPoolBuildError, ThreadPoolBuilder};

use crate::config::Config;
use crate::error::{Error, Result};
use crate::trace::debug_event;
use crate::workers::WorkerCount;

static POOL: Mutex<Option<(WorkerCount, Arc<ThreadPool>)>> = Mutex::new(None);

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
/// use fast_yaml_parallel::{WorkerCount, shared_pool};
///
/// let pool = shared_pool(WorkerCount::new(2)?)?;
/// assert_eq!(pool.install(rayon::current_num_threads), 2);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn shared_pool(workers: WorkerCount) -> Result<Arc<ThreadPool>> {
    build(workers).map_err(Error::ThreadPool)
}

/// Failure to spawn the pool's threads; shared so one failure can be reported for many files.
pub(crate) type BuildError = Arc<ThreadPoolBuildError>;

fn build(workers: WorkerCount) -> std::result::Result<Arc<ThreadPool>, BuildError> {
    build_in(&POOL, workers)
}

fn build_in(
    slot: &Mutex<Option<(WorkerCount, Arc<ThreadPool>)>>,
    workers: WorkerCount,
) -> std::result::Result<Arc<ThreadPool>, BuildError> {
    let mut slot = slot.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((count, pool)) = slot.as_ref()
        && *count == workers
    {
        debug_event!("reusing the shared pool of {workers} threads");
        return Ok(Arc::clone(pool));
    }
    debug_event!("building a shared pool of {workers} threads");
    let pool = ThreadPoolBuilder::new()
        .num_threads(workers.get())
        .build()
        .map_err(Arc::new)?;
    let pool = Arc::new(pool);
    *slot = Some((workers, Arc::clone(&pool)));
    drop(slot);
    Ok(pool)
}

/// The pool `config` asks for, or `None` when the current Rayon pool already fits.
///
/// `None` also covers sequential settings, which callers handle without a pool.
pub(crate) fn for_config(
    config: &Config,
) -> std::result::Result<Option<Arc<ThreadPool>>, BuildError> {
    config.workers().dedicated_pool().map(build).transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workers::Workers;

    fn count(n: usize) -> WorkerCount {
        WorkerCount::new(n).unwrap()
    }

    fn fresh() -> Mutex<Option<(WorkerCount, Arc<ThreadPool>)>> {
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
            for_config(&Config::new().with_workers(Workers::Sequential))
                .unwrap()
                .is_none()
        );
    }

    fn pool_of(threads: usize) -> ThreadPool {
        ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap()
    }

    #[test]
    fn test_auto_inside_an_oversized_pool_builds_one_capped_pool() {
        pool_of(130).install(|| {
            let pool = for_config(&Config::new()).unwrap().unwrap();
            assert_eq!(pool.install(rayon::current_num_threads), 128);
        });
    }

    #[test]
    fn test_auto_builds_no_pool_up_to_the_cap() {
        pool_of(128).install(|| assert!(for_config(&Config::new()).unwrap().is_none()));
    }
}
