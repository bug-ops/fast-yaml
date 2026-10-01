//! Scan-ahead limit scaled to the worker count, with a serialized retry at the full limit.
//!
//! Every worker may buffer up to [`MaxScanAhead`] characters of look-ahead (about 190 bytes
//! per character in the worst case), so a batch of adversarial files multiplies that peak by
//! the worker count. [`ScanAheadLane`] divides the default among the workers, and a file that
//! only fails because of the smaller limit is re-run at the full limit one at a time, so the
//! outcome of every file equals what a single-file run gives on any machine.

use std::sync::{Mutex, PoisonError};

use fast_yaml_core::limits::MaxScanAhead;

use crate::trace::debug_event;
use crate::workers::WorkerCount;

/// How a batch bounds the scanner look-ahead of its workers.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::limits::MaxScanAhead;
/// use fast_yaml_parallel::{Config, ScanAheadPolicy};
///
/// assert_eq!(
///     Config::new().scan_ahead_policy(),
///     ScanAheadPolicy::Fixed(MaxScanAhead::DEFAULT)
/// );
/// let scaled = Config::new().with_scan_ahead_policy(ScanAheadPolicy::Scaled);
/// assert_eq!(scaled.scan_ahead_policy(), ScanAheadPolicy::Scaled);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanAheadPolicy {
    /// Every worker parses at this limit, which a caller chose explicitly.
    Fixed(MaxScanAhead),
    /// Workers parse at the default limit divided by their number (never below 1 MiB);
    /// a file the smaller limit rejects is retried at the default limit, one file at a
    /// time. Use it only when the limit was not chosen explicitly.
    ///
    /// The memory bound depends on the worker count: with at most four workers the first
    /// attempts together stay within the default limit, above that the 1 MiB floor makes
    /// the aggregate `workers` x 1 MiB (128 MiB of look-ahead for 128 workers), plus one
    /// full-limit retry at a time. The lane is per batch run, so two concurrent runs can each
    /// hold a retry.
    Scaled,
}

/// The limits one batch run parses under, and the lane that serializes full-limit retries.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::limits::MaxScanAhead;
/// use fast_yaml_parallel::{ScanAheadLane, ScanAheadPolicy, WorkerCount};
///
/// let lane = ScanAheadLane::for_policy(ScanAheadPolicy::Scaled, WorkerCount::new(16)?);
/// assert_eq!(lane.first_limit().get(), 1 << 20);
///
/// let first_try = |limit: MaxScanAhead| {
///     if limit == MaxScanAhead::DEFAULT { Ok("parsed") } else { Err("too long") }
/// };
/// assert_eq!(lane.run(first_try, |err| *err == "too long"), Ok("parsed"));
/// # Ok::<(), fast_yaml_core::limits::LimitRangeError>(())
/// ```
#[derive(Debug)]
pub struct ScanAheadLane {
    first: MaxScanAhead,
    full: MaxScanAhead,
    retries: Mutex<()>,
}

/// Smallest limit a scaled lane starts with: 1 MiB of characters.
const MIN_SCALED: usize = 1 << 20;

impl From<Option<MaxScanAhead>> for ScanAheadPolicy {
    /// An explicit limit is final; without one the default is scaled.
    fn from(explicit: Option<MaxScanAhead>) -> Self {
        explicit.map_or(Self::Scaled, Self::Fixed)
    }
}

impl ScanAheadPolicy {
    /// The limit a file is finally parsed under: the fixed limit, or the default.
    #[must_use]
    pub const fn full_limit(self) -> MaxScanAhead {
        match self {
            Self::Fixed(limit) => limit,
            Self::Scaled => MaxScanAhead::DEFAULT,
        }
    }
}

impl ScanAheadLane {
    /// A lane where every attempt uses `full` and nothing is retried.
    const fn fixed(full: MaxScanAhead) -> Self {
        Self {
            first: full,
            full,
            retries: Mutex::new(()),
        }
    }

    /// A lane starting at `full / workers` (at least 1 MiB, at most `full`).
    fn scaled(full: MaxScanAhead, workers: WorkerCount) -> Self {
        let share = (full.get() / workers.get()).max(MIN_SCALED);
        let first = MaxScanAhead::new(share.min(full.get())).unwrap_or(full);
        Self {
            first,
            full,
            retries: Mutex::new(()),
        }
    }

    /// A lane for `policy` and the number of `workers`.
    #[must_use]
    pub fn for_policy(policy: ScanAheadPolicy, workers: WorkerCount) -> Self {
        match policy {
            ScanAheadPolicy::Fixed(limit) => Self::fixed(limit),
            ScanAheadPolicy::Scaled => Self::scaled(MaxScanAhead::DEFAULT, workers),
        }
    }

    /// The limit the first attempt on every file runs under.
    #[must_use]
    pub const fn first_limit(&self) -> MaxScanAhead {
        self.first
    }

    /// Runs `attempt` under the first limit and, when it fails with an error for which
    /// `exceeded` holds while the first limit is below the full one, once more under the full
    /// limit while holding the lane, so at most one full-limit parse runs at a time.
    ///
    /// # Errors
    ///
    /// Returns the error of the last attempt.
    pub fn run<T, E>(
        &self,
        attempt: impl Fn(MaxScanAhead) -> Result<T, E>,
        exceeded: impl Fn(&E) -> bool,
    ) -> Result<T, E> {
        match attempt(self.first) {
            Err(error) if self.first.get() < self.full.get() && exceeded(&error) => {
                let _lane = self.retries.lock().unwrap_or_else(PoisonError::into_inner);
                debug_event!(
                    "scan-ahead limit {} exceeded, retrying under {}",
                    self.first.get(),
                    self.full.get()
                );
                attempt(self.full)
            }
            result => result,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    fn workers(n: usize) -> WorkerCount {
        WorkerCount::new(n).unwrap()
    }

    #[test]
    fn test_scaled_first_limit_divides_the_default_down_to_one_mebichar() {
        let first = |n| ScanAheadLane::scaled(MaxScanAhead::DEFAULT, workers(n)).first_limit();
        assert_eq!(first(1), MaxScanAhead::DEFAULT);
        assert_eq!(first(2).get(), 2 << 20);
        assert_eq!(first(4).get(), 1 << 20);
        assert_eq!(first(128).get(), 1 << 20);
    }

    #[test]
    fn test_scaled_never_exceeds_a_small_full_limit() {
        let full = MaxScanAhead::new(1024).unwrap();
        assert_eq!(ScanAheadLane::scaled(full, workers(8)).first_limit(), full);
    }

    #[test]
    fn test_fixed_lane_never_retries() {
        let lane = ScanAheadLane::fixed(MaxScanAhead::DEFAULT);
        let calls = AtomicUsize::new(0);
        let result: Result<(), &str> = lane.run(
            |_| {
                calls.fetch_add(1, Ordering::Relaxed);
                Err("too long")
            },
            |_| true,
        );
        assert_eq!(result, Err("too long"));
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_scaled_lane_retries_only_the_exceeded_error_once() {
        let lane = ScanAheadLane::scaled(MaxScanAhead::DEFAULT, workers(8));
        let limits = Mutex::new(Vec::new());
        let result: Result<(), &str> = lane.run(
            |limit| {
                limits.lock().unwrap().push(limit);
                Err("too long")
            },
            |_| true,
        );
        assert_eq!(result, Err("too long"));
        assert_eq!(
            *limits.lock().unwrap(),
            [lane.first_limit(), MaxScanAhead::DEFAULT]
        );

        let calls = AtomicUsize::new(0);
        let other: Result<(), &str> = lane.run(
            |_| {
                calls.fetch_add(1, Ordering::Relaxed);
                Err("syntax")
            },
            |error| *error == "too long",
        );
        assert_eq!(other, Err("syntax"));
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_at_most_one_retry_holds_the_lane_at_a_time() {
        let lane = ScanAheadLane::scaled(MaxScanAhead::DEFAULT, workers(8));
        let (active, peak) = (AtomicUsize::new(0), AtomicUsize::new(0));
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    let result: Result<(), &str> = lane.run(
                        |limit| {
                            if limit == lane.first_limit() {
                                return Err("too long");
                            }
                            let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                            peak.fetch_max(now, Ordering::SeqCst);
                            std::thread::sleep(Duration::from_millis(10));
                            active.fetch_sub(1, Ordering::SeqCst);
                            Ok(())
                        },
                        |_| true,
                    );
                    assert!(result.is_ok());
                });
            }
        });
        assert_eq!(peak.load(Ordering::SeqCst), 1);
    }
}
