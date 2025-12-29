//! Shared conversion utilities for `PyO3` bindings.
//!
//! Provides conversion functions between Rust YAML types and Python objects.
//! Used by both the main module (lib.rs) and parallel processing (parallel.rs).

use fast_yaml_core::{ScalarOwned, Value};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

/// Convert `fast_yaml_core::Value` (`saphyr::YamlOwned`) to Python object.
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
pub fn value_to_python(py: Python<'_>, value: &Value) -> PyResult<Py<PyAny>> {
    match value {
        // Alias maps to None (aliases are resolved by saphyr)
        Value::Alias(_) => Ok(py.None()),

        Value::Value(scalar) => match scalar {
            ScalarOwned::Null => Ok(py.None()),

            ScalarOwned::Boolean(b) => {
                let py_bool = b.into_pyobject(py)?;
                Ok(py_bool.as_any().clone().unbind())
            }

            ScalarOwned::Integer(i) => {
                let py_int = i.into_pyobject(py)?;
                Ok(py_int.as_any().clone().unbind())
            }

            ScalarOwned::FloatingPoint(f) => {
                let py_float = f.into_pyobject(py)?;
                Ok(py_float.as_any().clone().unbind())
            }

            ScalarOwned::String(s) => {
                let py_str = s.into_pyobject(py)?;
                Ok(py_str.as_any().clone().unbind())
            }
        },

        Value::Sequence(arr) => {
            let list = PyList::empty(py);
            for item in arr {
                list.append(value_to_python(py, item)?)?;
            }
            Ok(list.into_any().unbind())
        }

        Value::Mapping(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map {
                let py_key = value_to_python(py, k)?;
                let py_value = value_to_python(py, v)?;
                dict.set_item(py_key, py_value)?;
            }
            Ok(dict.into_any().unbind())
        }

        Value::BadValue => Err(PyValueError::new_err("Invalid YAML value encountered")),

        // Tagged values - extract the inner value
        Value::Tagged(_, inner) => value_to_python(py, inner),

        // Representation values - the first element is the raw string representation
        Value::Representation(repr, _, _) => {
            let py_str = repr.into_pyobject(py)?;
            Ok(py_str.as_any().clone().unbind())
        }
    }
}

// Note: Tests for value_to_python are in Python test suite (tests/test_basic.py)
// Rust unit tests for PyO3 code require a full Python interpreter linkage,
// which is handled by maturin during the extension module build.
