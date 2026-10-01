//! Detection of mapping keys that YAML keeps distinct but a Python dict would merge.
//!
//! YAML 1.2.2 treats `1`, `true` and `1.0` as three different keys, while `1 == True == 1.0`
//! and they hash alike, so a `dict` or `set` would silently keep only one of them.
//! [`NumericKeys`] records the numeric keys of one mapping or set by kind, using Python
//! equality, and reports the first key that would collapse into a key of another kind.

// The module is private, but explicit crate visibility documents that nothing here is public API.
#![allow(clippy::redundant_pub_crate)]

use fast_yaml_core::{KeyError, KeyKind};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyFloat, PyInt, PySet};

/// Python type of a key that compares equal to keys of the other numeric types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NumericKind {
    Bool,
    Int,
    Float,
}

impl NumericKind {
    /// The kind of `key` when it is exactly a `bool`, `int` or `float`; subclasses are ordinary keys.
    fn of(key: &Bound<'_, PyAny>) -> Option<Self> {
        if key.is_exact_instance_of::<PyBool>() {
            Some(Self::Bool)
        } else if key.is_exact_instance_of::<PyInt>() {
            Some(Self::Int)
        } else if key.is_exact_instance_of::<PyFloat>() {
            Some(Self::Float)
        } else {
            None
        }
    }

    const fn others(self) -> [Self; 2] {
        match self {
            Self::Bool => [Self::Int, Self::Float],
            Self::Int => [Self::Bool, Self::Float],
            Self::Float => [Self::Bool, Self::Int],
        }
    }
}

impl From<NumericKind> for KeyKind {
    fn from(kind: NumericKind) -> Self {
        match kind {
            NumericKind::Bool => Self::Bool,
            NumericKind::Int => Self::Int,
            NumericKind::Float => Self::Float,
        }
    }
}

/// The key as YAML spells it for booleans (`true`); numbers keep their Python spelling.
fn yaml_text(kind: NumericKind, key: &Bound<'_, PyAny>) -> PyResult<Box<str>> {
    Ok(match kind {
        NumericKind::Bool => key.is_truthy()?.to_string(),
        NumericKind::Int => key.str()?.to_string(),
        NumericKind::Float => key.repr()?.to_string(),
    }
    .into())
}

/// Numeric keys seen so far in one mapping or set, grouped by kind.
pub(crate) struct NumericKeys<'py> {
    py: Python<'py>,
    bools: Option<Bound<'py, PySet>>,
    ints: Option<Bound<'py, PySet>>,
    floats: Option<Bound<'py, PySet>>,
}

impl<'py> NumericKeys<'py> {
    /// An empty table for one mapping or set.
    pub(crate) const fn new(py: Python<'py>) -> Self {
        Self {
            py,
            bools: None,
            ints: None,
            floats: None,
        }
    }

    const fn slot(&mut self, kind: NumericKind) -> &mut Option<Bound<'py, PySet>> {
        match kind {
            NumericKind::Bool => &mut self.bools,
            NumericKind::Int => &mut self.ints,
            NumericKind::Float => &mut self.floats,
        }
    }

    /// Records `key`; returns the collision when it equals an earlier key of another kind.
    ///
    /// Keys that are not exactly `bool`, `int` or `float` are not tracked. A repeated key of the
    /// same kind is fine: it is the ordinary duplicate-key case.
    pub(crate) fn record(&mut self, key: &Bound<'py, PyAny>) -> PyResult<Option<KeyError>> {
        let Some(kind) = NumericKind::of(key) else {
            return Ok(None);
        };
        for other in kind.others() {
            if let Some(seen) = self.slot(other)
                && seen.contains(key)?
            {
                return Ok(Some(KeyError::PythonCollision {
                    incoming: kind.into(),
                    kept: other.into(),
                    key: yaml_text(kind, key)?,
                    merged: false,
                }));
            }
        }
        let py = self.py;
        match self.slot(kind) {
            Some(seen) => seen.add(key)?,
            slot @ None => *slot = Some(PySet::new(py, [key])?),
        }
        Ok(None)
    }
}

/// Builds a Python `set` from `members`, or reports the index and collision of the first member
/// that equals an earlier member of another numeric kind.
pub(crate) fn build_set<'py>(
    py: Python<'py>,
    members: &[Bound<'py, PyAny>],
) -> PyResult<Result<Bound<'py, PySet>, (usize, KeyError)>> {
    let mut numeric = NumericKeys::new(py);
    for (index, member) in members.iter().enumerate() {
        if let Some(error) = numeric.record(member)? {
            return Ok(Err((index, error)));
        }
    }
    Ok(Ok(PySet::new(py, members)?))
}
