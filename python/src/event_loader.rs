//! Event-based YAML-to-Python loader.
//!
//! Uses `saphyr_parser` events directly instead of `saphyr`'s `YamlLoader`,
//! which silently drops core-schema collection tags (`!!set`, `!!omap`, …).
//! This loader preserves the `!!set` tag and converts the mapping to a Python `set`.

use std::collections::HashMap;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PySet, PyString};
use saphyr_parser::{Event, Parser, ScanError, StrInput};

use crate::repr_to_python;

/// Parse all YAML documents from `input` into Python objects.
///
/// Injects one implicit null document when the stream is non-empty but contains
/// no explicit documents (comment-only, whitespace-only, bare `---`/`...`),
/// matching YAML 1.2 §9.2 and `PyYAML` parity.
///
/// # Errors
///
/// Returns `PyValueError` on invalid YAML syntax.
pub fn load_all(py: Python<'_>, input: &str) -> PyResult<Vec<Py<PyAny>>> {
    let mut loader = EventLoader {
        parser: Parser::new_from_str(input),
        anchors: HashMap::new(),
    };
    let docs = loader.load_stream(py)?;
    // Replicate fast-yaml-core: inject implicit null for non-empty, zero-doc streams
    if docs.is_empty() && !input.is_empty() {
        Ok(vec![py.None()])
    } else {
        Ok(docs)
    }
}

struct EventLoader<'input> {
    parser: Parser<'input, StrInput<'input>>,
    /// Anchor id → Python object, used to resolve YAML aliases.
    anchors: HashMap<usize, Py<PyAny>>,
}

impl<'input> EventLoader<'input> {
    /// Advance the parser and return the next meaningful event.
    fn next(&mut self) -> PyResult<Event<'input>> {
        loop {
            match self.parser.next_event() {
                Some(Ok((Event::Nothing, _))) => {}
                Some(Ok((ev, _))) => return Ok(ev),
                Some(Err(ref e)) => {
                    return Err(scan_err(e));
                }
                None => return Ok(Event::StreamEnd),
            }
        }
    }

    fn load_stream(&mut self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        // Consume StreamStart
        self.next()?;

        let mut docs = Vec::new();
        loop {
            match self.next()? {
                Event::StreamEnd => break,
                Event::DocumentStart(_) => {
                    let value = self.parse_node(py)?.unwrap_or_else(|| py.None());
                    // Consume DocumentEnd (best-effort; parse_node may have consumed it
                    // already as a None return).
                    docs.push(value);
                }
                _ => {} // ignore stray events
            }
        }
        Ok(docs)
    }

    /// Consume the next event and build a Python value.
    ///
    /// Returns `None` when a container-end or document-end event is consumed
    /// (signals the caller to stop iterating).
    fn parse_node(&mut self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        match self.next()? {
            Event::Scalar(s, style, anchor_id, tag) => {
                let value = repr_to_python(py, &s, style, tag.as_deref())?;
                self.store_anchor(anchor_id, &value, py);
                Ok(Some(value))
            }

            Event::MappingStart(anchor_id, tag) => {
                let is_set = tag
                    .as_ref()
                    .is_some_and(|t| t.is_yaml_core_schema() && t.suffix == "set");
                let value = if is_set {
                    self.parse_set(py)?
                } else {
                    self.parse_mapping(py)?
                };
                self.store_anchor(anchor_id, &value, py);
                Ok(Some(value))
            }

            Event::SequenceStart(anchor_id, _tag) => {
                let value = self.parse_sequence(py)?;
                self.store_anchor(anchor_id, &value, py);
                Ok(Some(value))
            }

            Event::Alias(id) => {
                let resolved = self
                    .anchors
                    .get(&id)
                    .map_or_else(|| py.None(), |v| v.clone_ref(py));
                Ok(Some(resolved))
            }

            // Container / document terminators: signal caller to stop
            Event::MappingEnd | Event::SequenceEnd | Event::DocumentEnd | Event::StreamEnd => {
                Ok(None)
            }

            Event::DocumentStart(_) | Event::StreamStart | Event::Nothing => {
                // Unexpected inside a value context; treat as null
                Ok(Some(py.None()))
            }
        }
    }

    /// Consume key-value pairs up to `MappingEnd`, returning a `PyDict`.
    ///
    /// Handles YAML 1.1 merge keys (`<<`).
    fn parse_mapping(&mut self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let dict = PyDict::new(py);
        let mut merges: Vec<Py<PyAny>> = Vec::new();
        let mut explicit: Vec<(Py<PyAny>, Py<PyAny>)> = Vec::new();

        loop {
            let Some(key) = self.parse_node(py)? else {
                break; // MappingEnd consumed
            };

            // Reject unhashable complex keys with the same error as the original pipeline
            {
                let bound = key.bind(py);
                if bound.cast::<PyList>().is_ok() || bound.cast::<PyDict>().is_ok() {
                    return Err(PyValueError::new_err(
                        "YAML complex keys (sequences or mappings as keys) are not supported as Python dict keys",
                    ));
                }
            }

            let value = self.parse_node(py)?.unwrap_or_else(|| py.None());

            let is_merge = key
                .bind(py)
                .cast::<PyString>()
                .is_ok_and(|s| s.to_str().is_ok_and(|s| s == "<<"));

            if is_merge {
                merges.push(value);
            } else {
                explicit.push((key, value));
            }
        }

        // Apply merged keys first (lower priority)
        for merge_val in merges {
            let bound = merge_val.bind(py);
            if let Ok(merge_dict) = bound.cast::<PyDict>() {
                for (mk, mv) in merge_dict.iter() {
                    if !dict.contains(mk.clone())? {
                        dict.set_item(mk, mv)?;
                    }
                }
            } else if let Ok(seq) = bound.cast::<PyList>() {
                for item in seq.iter() {
                    if let Ok(merge_dict) = item.cast::<PyDict>() {
                        for (mk, mv) in merge_dict.iter() {
                            if !dict.contains(mk.clone())? {
                                dict.set_item(mk, mv)?;
                            }
                        }
                    }
                }
            }
        }
        // Explicit keys always win
        for (key, value) in explicit {
            dict.set_item(key, value)?;
        }

        Ok(dict.into_any().unbind())
    }

    /// Consume key-null pairs up to `MappingEnd`, returning a `PySet` of keys.
    ///
    /// Per YAML spec §10.3.3, a `!!set` is a mapping where every value is null.
    fn parse_set(&mut self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let mut keys: Vec<Py<PyAny>> = Vec::new();
        loop {
            let Some(key) = self.parse_node(py)? else {
                break; // MappingEnd consumed
            };
            // Consume the associated null value; ignore it
            let _null = self.parse_node(py)?;
            keys.push(key);
        }
        let set = PySet::new(py, &keys)?;
        Ok(set.into_any().unbind())
    }

    /// Consume items up to `SequenceEnd`, returning a `PyList`.
    fn parse_sequence(&mut self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let mut items: Vec<Py<PyAny>> = Vec::new();
        loop {
            match self.parse_node(py)? {
                None => break, // SequenceEnd consumed
                Some(v) => items.push(v),
            }
        }
        let list = PyList::new(py, &items)?;
        Ok(list.into_any().unbind())
    }

    fn store_anchor(&mut self, anchor_id: usize, value: &Py<PyAny>, py: Python<'_>) {
        if anchor_id > 0 {
            self.anchors.insert(anchor_id, value.clone_ref(py));
        }
    }
}

fn scan_err(e: &ScanError) -> PyErr {
    PyValueError::new_err(format!("YAML parse error: {e}"))
}
