//! Validation of user-supplied parse limits shared by every binding entry point.

use std::fmt::Display;

use fast_yaml_core::limits::{AliasBytes, Bounded, Bounds, Depth, Documents, ScanAhead};
use fast_yaml_core::{Indent, ParseLimits};
use fast_yaml_parallel::{WorkerCount, Workers};
use pyo3::exceptions::{PyOverflowError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyBool;

/// Builds the `ValueError` shared by all limit options.
pub fn range_error(option: &str, min: usize, max: usize, got: impl Display) -> PyErr {
    PyValueError::new_err(format!(
        "{option} must be between {min} and {max}, got {got}"
    ))
}

/// Largest value of an option the Node binding stores as `u32`, shared so both reject alike.
pub const U32_MAX: usize = u32::MAX as usize;

/// Converts the optional worker count of a Python option: `None` is auto, `0` sequential,
/// `1..=128` a fixed pool; values outside `0..=128` raise `ValueError`.
pub fn workers(option: &str, value: Option<&Bound<'_, PyAny>>) -> PyResult<Workers> {
    let Some(arg) = value else {
        return Ok(Workers::Auto);
    };
    let n = extract_in_range(option, 0, WorkerCount::MAX.get(), arg)?;
    Workers::from_count(n).map_err(|e| range_error(option, e.min, e.max, e.value))
}

/// The Python-visible value of `workers`: `None` for auto, `0` for sequential.
pub const fn workers_value(workers: Workers) -> Option<usize> {
    match workers {
        Workers::Auto => None,
        Workers::Sequential => Some(0),
        Workers::Fixed(count) => Some(count.get()),
    }
}

/// Extracts `arg` as an integer in `min..=max`: values outside it, negative or oversized ones
/// included, raise `ValueError` with the shared range message; `bool` and non-integers raise `TypeError`.
pub fn extract_in_range(
    option: &str,
    min: usize,
    max: usize,
    arg: &Bound<'_, PyAny>,
) -> PyResult<usize> {
    if arg.is_instance_of::<PyBool>() {
        return Err(PyTypeError::new_err(format!(
            "{option} must be an int, not bool"
        )));
    }
    match arg.extract::<i128>() {
        Ok(value) => usize::try_from(value)
            .ok()
            .filter(|n| (min..=max).contains(n))
            .ok_or_else(|| range_error(option, min, max, value)),
        Err(e) if e.is_instance_of::<PyOverflowError>(arg.py()) => {
            Err(range_error(option, min, max, arg.str()?))
        }
        Err(e) => Err(e),
    }
}

/// Like [`extract_in_range`], returning `default` when the argument is unset.
pub fn in_range_or(
    option: &str,
    min: usize,
    max: usize,
    default: usize,
    arg: Option<&Bound<'_, PyAny>>,
) -> PyResult<usize> {
    arg.map_or(Ok(default), |arg| extract_in_range(option, min, max, arg))
}

/// Validates a limit option of kind `K`, returning its default when unset.
pub fn bounded<K: Bounds>(option: &str, arg: Option<&Bound<'_, PyAny>>) -> PyResult<Bounded<K>> {
    let Some(arg) = arg else {
        return Ok(Bounded::DEFAULT);
    };
    let raw = extract_in_range(option, 1, K::MAX, arg)?;
    Bounded::new(raw).map_err(|e| range_error(option, e.min, e.max, e.value))
}

/// Validates an `indent` value in `1..=9`, defaulting to [`Indent::DEFAULT`] when unset.
pub fn indent(arg: Option<&Bound<'_, PyAny>>) -> PyResult<Indent> {
    let raw = in_range_or(
        "indent",
        Indent::MIN.get(),
        Indent::MAX.get(),
        Indent::DEFAULT.get(),
        arg,
    )?;
    Indent::new(raw).map_err(|e| range_error("indent", e.min, e.max, e.value))
}

/// Builds [`ParseLimits`] from the optional Python keyword arguments.
pub fn parse_limits(
    max_depth_arg: Option<&Bound<'_, PyAny>>,
    max_alias_bytes_arg: Option<&Bound<'_, PyAny>>,
    max_scan_ahead_arg: Option<&Bound<'_, PyAny>>,
    max_documents_arg: Option<&Bound<'_, PyAny>>,
) -> PyResult<ParseLimits> {
    Ok(ParseLimits {
        max_depth: bounded::<Depth>("max_depth", max_depth_arg)?,
        max_alias_bytes: bounded::<AliasBytes>("max_alias_bytes", max_alias_bytes_arg)?,
        max_scan_ahead: bounded::<ScanAhead>("max_scan_ahead", max_scan_ahead_arg)?,
        max_documents: bounded::<Documents>("max_documents", max_documents_arg)?,
        ..ParseLimits::default()
    })
}
