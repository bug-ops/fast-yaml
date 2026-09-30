//! Shared conversion utilities for `PyO3` bindings.
//!
//! Provides conversion functions between Rust YAML types and Python objects.
//! Used by both the main module (lib.rs) and parallel processing (parallel.rs).

use fast_yaml_core::{ScalarOwned, Value, is_core_set_tag};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use crate::numeric_keys::{NumericKeys, build_set};

pub const COMPLEX_KEY_MESSAGE: &str =
    "YAML complex keys (sequences or mappings as keys) are not supported as Python dict keys";

/// Whether `value` is a sequence or mapping, looking through tags, so a set counts as a mapping.
fn is_collection(value: &Value) -> bool {
    match value {
        Value::Sequence(_) | Value::Mapping(_) => true,
        Value::Tagged(_, inner) => is_collection(inner),
        _ => false,
    }
}

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
            // Pre-convert all items to avoid list resize operations
            let items: Vec<Py<PyAny>> = arr
                .iter()
                .map(|item| value_to_python(py, item))
                .collect::<PyResult<Vec<_>>>()?;
            let list = PyList::new(py, &items)?;
            Ok(list.into_any().unbind())
        }

        Value::Mapping(map) => {
            // Pre-convert all key-value pairs to minimize dict resize operations
            let pairs: Vec<(Py<PyAny>, Py<PyAny>)> = map
                .iter()
                .map(|(k, v)| {
                    if is_collection(k) {
                        return Err(PyValueError::new_err(COMPLEX_KEY_MESSAGE));
                    }
                    let py_key = value_to_python(py, k)?;
                    let py_value = value_to_python(py, v)?;
                    Ok((py_key, py_value))
                })
                .collect::<PyResult<Vec<_>>>()?;

            let dict = PyDict::new(py);
            let mut numeric = NumericKeys::new(py);
            for (key, value) in pairs {
                if let Some(clash) = numeric.record(key.bind(py))? {
                    return Err(PyValueError::new_err(format!("YAML parse error: {clash}")));
                }
                dict.set_item(key, value)?;
            }
            Ok(dict.into_any().unbind())
        }

        Value::BadValue => Err(PyValueError::new_err("Invalid YAML value encountered")),

        Value::Tagged(tag, inner) => match &**inner {
            Value::Mapping(map) if is_core_set_tag(tag) => {
                let members = map
                    .keys()
                    .map(|key| {
                        if is_collection(key) {
                            return Err(PyValueError::new_err(COMPLEX_KEY_MESSAGE));
                        }
                        value_to_python(py, key).map(|key| key.into_bound(py))
                    })
                    .collect::<PyResult<Vec<_>>>()?;
                match build_set(py, &members)? {
                    Ok(set) => Ok(set.into_any().unbind()),
                    Err((_, clash)) => {
                        Err(PyValueError::new_err(format!("YAML parse error: {clash}")))
                    }
                }
            }
            _ => value_to_python(py, inner),
        },

        Value::Representation(repr, style, tag) => {
            crate::repr_to_python(py, repr, *style, tag.as_ref())
        }
    }
}

// Note: Tests for value_to_python are in Python test suite (tests/test_basic.py)
// Rust unit tests for PyO3 code require a full Python interpreter linkage,
// which is handled by maturin during the extension module build.
