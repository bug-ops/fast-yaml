//! Validation of user-supplied parse limits shared by every binding entry point.

use std::fmt::Display;

use fast_yaml_core::limits::{AliasBytes, Bounded, Bounds, Depth, Documents, ScanAhead};
use fast_yaml_core::{Indent, ParseLimits, Width};
use pyo3::exceptions::{PyOverflowError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyBool;

/// Builds the `ValueError` shared by all limit options.
pub fn range_error(option: &str, min: usize, max: usize, got: impl Display) -> PyErr {
    PyValueError::new_err(format!(
        "{option} must be between {min} and {max}, got {got}"
    ))
}

/// Extracts `arg` as `usize`: negative and oversized integers raise `ValueError`, `bool` and non-integers `TypeError`.
fn extract_usize(option: &str, max: usize, arg: &Bound<'_, PyAny>) -> PyResult<usize> {
    if arg.is_instance_of::<PyBool>() {
        return Err(PyTypeError::new_err(format!(
            "{option} must be an int, not bool"
        )));
    }
    match arg.extract::<i128>() {
        Ok(value) => usize::try_from(value).map_err(|_| range_error(option, 1, max, value)),
        Err(e) if e.is_instance_of::<PyOverflowError>(arg.py()) => {
            Err(range_error(option, 1, max, arg.str()?))
        }
        Err(e) => Err(e),
    }
}

/// Validates a limit option of kind `K`, returning its default when unset.
pub fn bounded<K: Bounds>(option: &str, arg: Option<&Bound<'_, PyAny>>) -> PyResult<Bounded<K>> {
    let Some(arg) = arg else {
        return Ok(Bounded::DEFAULT);
    };
    let raw = extract_usize(option, K::MAX, arg)?;
    Bounded::new(raw).map_err(|e| range_error(option, e.min, e.max, e.value))
}

/// Validates an `indent` value in `1..=9`.
pub fn indent(raw: usize) -> PyResult<Indent> {
    Indent::new(raw).map_err(|e| range_error("indent", e.min, e.max, e.value))
}

/// Validates a `width` value in `20..=1000`.
pub fn width(raw: usize) -> PyResult<Width> {
    Width::new(raw).map_err(|e| range_error("width", e.min, e.max, e.value))
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
