//! Detection of mapping keys that YAML keeps distinct but a Python dict would merge.
//!
//! YAML 1.2.2 treats `1`, `true` and `1.0` as three different keys, while `1 == True == 1.0`
//! and they hash alike, so a `dict` or `set` would silently keep only one of them.
//! [`NumericKeys`] records the numeric keys of one mapping or set by kind, using Python
//! equality, and reports the first key that would collapse into a key of another kind.

// The module is private, but explicit crate visibility documents that nothing here is public API.
#![allow(clippy::redundant_pub_crate)]

use std::fmt;

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

impl fmt::Display for NumericKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Bool => "bool",
            Self::Int => "int",
            Self::Float => "float",
        })
    }
}

/// A key spelled as YAML spells it (`true`), for error messages.
#[derive(Debug)]
pub(crate) struct KeyText(String);

impl fmt::Display for KeyText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A key that equals an earlier key of another kind under Python equality.
#[derive(Debug)]
pub(crate) struct KeyClash {
    kept: NumericKind,
    incoming: NumericKind,
    key: KeyText,
}

impl fmt::Display for KeyClash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} key {} is distinct in YAML but equal as a Python dict key to a key of type {}",
            self.incoming, self.key, self.kept
        )
    }
}

/// The key as YAML spells it for booleans (`true`); numbers keep their Python spelling.
fn yaml_text(kind: NumericKind, key: &Bound<'_, PyAny>) -> PyResult<KeyText> {
    Ok(KeyText(match kind {
        NumericKind::Bool => key.is_truthy()?.to_string(),
        NumericKind::Int => key.str()?.to_string(),
        NumericKind::Float => key.repr()?.to_string(),
    }))
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

    /// Records `key`; returns the clash when it equals an earlier key of another kind.
    ///
    /// Keys that are not exactly `bool`, `int` or `float` are not tracked. A repeated key of the
    /// same kind is fine: it is the ordinary duplicate-key case.
    pub(crate) fn record(&mut self, key: &Bound<'py, PyAny>) -> PyResult<Option<KeyClash>> {
        let Some(kind) = NumericKind::of(key) else {
            return Ok(None);
        };
        for other in kind.others() {
            if let Some(seen) = self.slot(other)
                && seen.contains(key)?
            {
                return Ok(Some(KeyClash {
                    kept: other,
                    incoming: kind,
                    key: yaml_text(kind, key)?,
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
