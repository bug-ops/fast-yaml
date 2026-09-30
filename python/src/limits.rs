//! Validation of user-supplied parse limits shared by every binding entry point.

use std::fmt::Display;

use fast_yaml_core::{MaxAliasBytes, MaxDepth, MaxInputBytes, ParseLimits};
use pyo3::exceptions::{PyOverflowError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyBool;

/// Builds the `ValueError` shared by all limit options.
pub fn range_error(option: &str, max: usize, got: impl Display) -> PyErr {
    PyValueError::new_err(format!("{option} must be between 1 and {max}, got {got}"))
}

/// Extracts `arg` as `usize`: negative and oversized integers raise `ValueError`, `bool` and non-integers `TypeError`.
fn extract_usize(option: &str, max: usize, arg: &Bound<'_, PyAny>) -> PyResult<usize> {
    if arg.is_instance_of::<PyBool>() {
        return Err(PyTypeError::new_err(format!(
            "{option} must be an int, not bool"
        )));
    }
    match arg.extract::<i128>() {
        Ok(value) => usize::try_from(value).map_err(|_| range_error(option, max, value)),
        Err(e) if e.is_instance_of::<PyOverflowError>(arg.py()) => {
            Err(range_error(option, max, arg.str()?))
        }
        Err(e) => Err(e),
    }
}

/// Validates a `max_depth` value, returning [`MaxDepth::DEFAULT`] when unset.
pub fn max_depth(arg: Option<&Bound<'_, PyAny>>) -> PyResult<MaxDepth> {
    let Some(arg) = arg else {
        return Ok(MaxDepth::DEFAULT);
    };
    let raw = extract_usize("max_depth", MaxDepth::MAX.get(), arg)?;
    MaxDepth::new(raw).map_err(|e| range_error("max_depth", e.max, e.value))
}

/// Validates a `max_alias_bytes` value, returning [`MaxAliasBytes::DEFAULT`] when unset.
pub fn max_alias_bytes(arg: Option<&Bound<'_, PyAny>>) -> PyResult<MaxAliasBytes> {
    let Some(arg) = arg else {
        return Ok(MaxAliasBytes::DEFAULT);
    };
    let raw = extract_usize("max_alias_bytes", MaxAliasBytes::MAX.get(), arg)?;
    MaxAliasBytes::new(raw).map_err(|e| range_error("max_alias_bytes", e.max, e.value))
}

/// Validates a `max_input_bytes` value, returning [`MaxInputBytes::DEFAULT`] when unset.
pub fn max_input_bytes(arg: Option<&Bound<'_, PyAny>>) -> PyResult<MaxInputBytes> {
    let Some(arg) = arg else {
        return Ok(MaxInputBytes::DEFAULT);
    };
    let raw = extract_usize("max_input_bytes", MaxInputBytes::MAX.get(), arg)?;
    MaxInputBytes::new(raw).map_err(|e| range_error("max_input_bytes", e.max, e.value))
}

/// Builds [`ParseLimits`] from the optional Python keyword arguments.
pub fn parse_limits(
    max_depth_arg: Option<&Bound<'_, PyAny>>,
    max_alias_bytes_arg: Option<&Bound<'_, PyAny>>,
) -> PyResult<ParseLimits> {
    Ok(ParseLimits {
        max_depth: max_depth(max_depth_arg)?,
        max_alias_bytes: max_alias_bytes(max_alias_bytes_arg)?,
        ..ParseLimits::default()
    })
}
