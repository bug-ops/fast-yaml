//! Peak heap of linting against loading (#579).
//!
//! Lint runs one loader pass; the node index and flow ranges are filled from its events, so with
//! every rule enabled its peak stays close to the peak of loading alone.

use std::sync::{Mutex, MutexGuard, PoisonError};

use fast_yaml_core::{ParseLimits, Parser};
use fast_yaml_linter::Linter;
use peak_alloc::PeakAlloc;

#[global_allocator]
static PEAK: PeakAlloc = PeakAlloc;

static SERIAL: Mutex<()> = Mutex::new(());

/// The allocator counts the whole process, so tests that measure must not overlap.
fn exclusive() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Peak heap growth of `run`.
fn peak_of<T>(run: impl FnOnce() -> T) -> usize {
    PEAK.reset_peak_usage();
    let before = PEAK.current_usage();
    drop(run());
    PEAK.peak_usage().saturating_sub(before)
}

#[test]
fn lint_of_a_large_root_flow_stays_close_to_loading_it() {
    let _serial = exclusive();
    let input = format!("[{}1]\n", "1, ".repeat(350 * 1024));
    let linter = Linter::with_all_rules();

    let load = peak_of(|| Parser::parse_all_with_limits(&input, &ParseLimits::default()).unwrap());
    let lint = peak_of(|| linter.lint(&input).unwrap());

    assert!(
        lint * 4 <= load * 5,
        "lint peak {lint} exceeds 1.25 x load peak {load}"
    );
}
