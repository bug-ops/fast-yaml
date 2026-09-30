//! Bounded conversion of Python objects into YAML values for rule configuration.
//!
//! Conversion is type-exact (no truthiness coercion) and capped in depth and node
//! count, so cyclic or shared-reference inputs fail with `ValueError` instead of
//! exhausting the stack or memory.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyFloat, PyInt, PyList, PyMapping, PyString, PyTuple};
use serde_norway::{Mapping, Value};

const MAX_DEPTH: usize = 16;
const MAX_NODES: usize = 100_000;

/// Converts Python objects into [`Value`] under a shared node budget.
#[derive(Default)]
pub struct ValueConverter {
    nodes: usize,
}

impl ValueConverter {
    /// Converts `obj`, where `depth` is the nesting level of `obj` (0 for a root).
    pub fn convert(&mut self, obj: &Bound<'_, PyAny>, depth: usize) -> PyResult<Value> {
        if depth > MAX_DEPTH {
            return Err(PyValueError::new_err(format!(
                "rule configuration is nested deeper than {MAX_DEPTH} levels (or contains a cycle)"
            )));
        }
        self.nodes += 1;
        if self.nodes > MAX_NODES {
            return Err(PyValueError::new_err(format!(
                "rule configuration has more than {MAX_NODES} values"
            )));
        }

        if obj.is_none() {
            Ok(Value::Null)
        } else if let Ok(flag) = obj.cast::<PyBool>() {
            Ok(Value::Bool(flag.is_true()))
        } else if obj.is_instance_of::<PyInt>() {
            convert_int(obj)
        } else if obj.is_instance_of::<PyFloat>() {
            Ok(Value::from(obj.extract::<f64>()?))
        } else if let Ok(text) = obj.cast::<PyString>() {
            Ok(Value::String(text.to_str()?.to_owned()))
        } else if let Ok(list) = obj.cast::<PyList>() {
            self.convert_items(list.iter(), depth)
        } else if let Ok(tuple) = obj.cast::<PyTuple>() {
            self.convert_items(tuple.iter(), depth)
        } else if let Ok(source) = obj.cast::<PyMapping>() {
            let mut mapping = Mapping::new();
            for entry in entries(source)? {
                let (key, value) = entry?;
                let key = string_key(&key)?;
                mapping.insert(Value::String(key), self.convert(&value, depth + 1)?);
            }
            Ok(Value::Mapping(mapping))
        } else {
            Err(PyValueError::new_err(format!(
                "unsupported value of type '{}' in rule configuration",
                obj.get_type().name()?
            )))
        }
    }

    fn convert_items<'py>(
        &mut self,
        items: impl Iterator<Item = Bound<'py, PyAny>>,
        depth: usize,
    ) -> PyResult<Value> {
        items
            .map(|item| self.convert(&item, depth + 1))
            .collect::<PyResult<Vec<_>>>()
            .map(Value::Sequence)
    }

    /// Converts a mapping of rule names to entries, checking each name with
    /// `check_name` before its entry is converted.
    pub fn convert_rules(
        &mut self,
        obj: &Bound<'_, PyAny>,
        check_name: impl Fn(&str) -> PyResult<()>,
    ) -> PyResult<Value> {
        let Ok(source) = obj.cast::<PyMapping>() else {
            return self.convert(obj, 0);
        };
        let mut mapping = Mapping::new();
        for entry in entries(source)? {
            let (key, value) = entry?;
            let name = string_key(&key)?;
            check_name(&name)?;
            mapping.insert(Value::String(name), self.convert(&value, 1)?);
        }
        Ok(Value::Mapping(mapping))
    }
}

type Entry<'py> = PyResult<(Bound<'py, PyAny>, Bound<'py, PyAny>)>;

/// Iterates `items()` lazily so a huge custom mapping is bounded by the node budget.
fn entries<'py>(source: &Bound<'py, PyMapping>) -> PyResult<impl Iterator<Item = Entry<'py>>> {
    Ok(source
        .call_method0("items")?
        .try_iter()?
        .map(|item| item?.extract()))
}

fn string_key(key: &Bound<'_, PyAny>) -> PyResult<String> {
    key.cast::<PyString>()
        .map_err(|_| {
            PyValueError::new_err(format!(
                "mapping keys must be strings, got '{}'",
                key.get_type()
                    .name()
                    .map_or_else(|_| "?".into(), |n| n.to_string())
            ))
        })?
        .to_str()
        .map(ToOwned::to_owned)
}

fn convert_int(obj: &Bound<'_, PyAny>) -> PyResult<Value> {
    match (obj.extract::<i64>(), obj.extract::<u64>()) {
        (Ok(value), _) => Ok(Value::from(value)),
        (_, Ok(value)) => Ok(Value::from(value)),
        _ => Err(PyValueError::new_err(
            "integer in rule configuration is out of range",
        )),
    }
}
