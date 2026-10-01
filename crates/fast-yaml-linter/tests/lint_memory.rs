//! Peak heap of linting against loading (#579).
//!
//! Lint runs one loader pass; the node index and flow ranges are filled from its events, so with
//! every rule enabled its peak stays close to the peak of loading alone. A diagnostic costs a
//! few hundred bytes, so an input that makes a diagnostic per scalar is bounded by a larger
//! factor.

use std::sync::{Mutex, MutexGuard, PoisonError};

use fast_yaml_core::{ParseLimits, Parser};
use fast_yaml_linter::config::Preset;
use fast_yaml_linter::{LintConfig, Linter};
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

/// Asserts that linting `input` peaks at no more than `numerator / denominator` times loading it.
fn assert_lint_within(input: &str, config: &LintConfig, (numerator, denominator): (usize, usize)) {
    let load = peak_of(|| Parser::parse_all_with_limits(input, &ParseLimits::default()).unwrap());
    let lint = peak_of(|| {
        Linter::with_all_rules_and_config(config.clone())
            .lint(input)
            .unwrap()
    });
    assert!(
        lint * denominator <= load * numerator,
        "lint peak {lint} exceeds {numerator}/{denominator} x load peak {load}"
    );
}

#[test]
fn lint_of_a_large_root_flow_stays_close_to_loading_it() {
    let _serial = exclusive();
    let input = format!("[{}1]\n", "1, ".repeat(350 * 1024));
    assert_lint_within(&input, &LintConfig::default(), (5, 4));
}

#[test]
fn relaxed_preset_does_not_reparse_the_root_flow_line() {
    let _serial = exclusive();
    let input = format!("[{}1]\n", "1, ".repeat(350 * 1024));
    let config = LintConfig {
        rules: Preset::Relaxed.rules(),
        ..LintConfig::default()
    };
    assert_lint_within(&input, &config, (5, 4));
}

#[test]
fn a_diagnostic_per_scalar_costs_a_bounded_amount() {
    let _serial = exclusive();
    // Every comma lacks a space after it, so the commas rule reports 500 000 diagnostics
    let input = format!("[{}1]\n", "1,".repeat(500 * 1024));
    assert_lint_within(&input, &LintConfig::default(), (2, 1));
}
