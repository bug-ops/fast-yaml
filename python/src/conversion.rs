//! Shared conversion utilities for `PyO3` bindings.
//!
//! Provides conversion functions between Rust YAML types and Python objects.
//! Used by both the main module (lib.rs) and parallel processing (parallel.rs).

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

/// Convert `fast_yaml_core::Value` (`yaml_rust2::Yaml`) to Python object.
///
/// Handles YAML 1.2.2 Core Schema types including special float values
/// (.inf, -.inf, .nan) as defined in the specification.
///
/// # Arguments
/// * `py` - Python interpreter reference
/// * `value` - YAML value to convert
///
/// # Returns
/// * `PyResult<Py<PyAny>>` - Python object or error
pub fn value_to_python(py: Python<'_>, value: &fast_yaml_core::Value) -> PyResult<Py<PyAny>> {
    match value {
        // Null and Alias both map to None (aliases are resolved by yaml-rust2)
        fast_yaml_core::Value::Null | fast_yaml_core::Value::Alias(_) => Ok(py.None()),

        fast_yaml_core::Value::Boolean(b) => {
            let py_bool = b.into_pyobject(py)?;
            Ok(py_bool.as_any().clone().unbind())
        }

        fast_yaml_core::Value::Integer(i) => {
            let py_int = i.into_pyobject(py)?;
            Ok(py_int.as_any().clone().unbind())
        }

        fast_yaml_core::Value::Real(s) => {
            // YAML 1.2.2 special float values (Section 10.2.1.4)
            // Avoid allocation by checking case-insensitively without to_lowercase()
            let f: f64 = if s.eq_ignore_ascii_case(".inf") || s.eq_ignore_ascii_case("+.inf") {
                f64::INFINITY
            } else if s.eq_ignore_ascii_case("-.inf") {
                f64::NEG_INFINITY
            } else if s.eq_ignore_ascii_case(".nan") {
                f64::NAN
            } else {
                s.parse()
                    .map_err(|e| PyValueError::new_err(format!("Invalid float value '{s}': {e}")))?
            };
            let py_float = f.into_pyobject(py)?;
            Ok(py_float.as_any().clone().unbind())
        }

        fast_yaml_core::Value::String(s) => {
            let py_str = s.into_pyobject(py)?;
            Ok(py_str.as_any().clone().unbind())
        }

        fast_yaml_core::Value::Array(arr) => {
            let list = PyList::empty(py);
            for item in arr {
                list.append(value_to_python(py, item)?)?;
            }
            Ok(list.into_any().unbind())
        }

        fast_yaml_core::Value::Hash(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map {
                let py_key = value_to_python(py, k)?;
                let py_value = value_to_python(py, v)?;
                dict.set_item(py_key, py_value)?;
            }
            Ok(dict.into_any().unbind())
        }

        fast_yaml_core::Value::BadValue => {
            Err(PyValueError::new_err("Invalid YAML value encountered"))
        }
    }
}

// Note: Tests for value_to_python are in Python test suite (tests/test_basic.py)
// Rust unit tests for PyO3 code require a full Python interpreter linkage,
// which is handled by maturin during the extension module build.
