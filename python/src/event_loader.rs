//! Event-based YAML-to-Python loader.
//!
//! Uses `saphyr_parser` events directly instead of `saphyr`'s `YamlLoader`,
//! which silently drops core-schema collection tags (`!!set`, `!!omap`, …).
//! This loader preserves the `!!set` tag and converts the mapping to a Python `set`.

use std::collections::HashMap;

use fast_yaml_core::{LimitGuard, ParseError, ParseLimits};
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
/// Returns `PyValueError` on invalid YAML syntax or when `limits` are exceeded.
pub fn load_all(py: Python<'_>, input: &str, limits: ParseLimits) -> PyResult<Vec<Py<PyAny>>> {
    let mut loader = EventLoader {
        parser: Parser::new_from_str(fast_yaml_core::strip_bom(input)),
        anchors: HashMap::new(),
        guard: LimitGuard::new(limits),
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
    /// Enforces depth and alias limits before any recursion or aliasing happens.
    guard: LimitGuard,
}

impl<'input> EventLoader<'input> {
    /// Advance the parser and return the next meaningful event.
    fn next(&mut self) -> PyResult<Event<'input>> {
        loop {
            match self.parser.next_event() {
                Some(Ok((Event::Nothing, _))) => {}
                Some(Ok((ev, span))) => {
                    self.guard.observe(&ev, span).map_err(|e| limit_err(&e))?;
                    return Ok(ev);
                }
                Some(Err(ref e)) => return Err(scan_err(e)),
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
                Event::DocumentStart(_) => docs.push(self.load_document(py)?),
                _ => {} // ignore stray events, including DocumentEnd
            }
        }
        Ok(docs)
    }

    /// Build one document's root value from events, without recursion.
    ///
    /// Open containers live on a heap stack, so nesting depth (already bounded by the
    /// limit guard) never consumes host thread stack.
    fn load_document(&mut self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let mut stack: Vec<OpenNode> = Vec::new();
        loop {
            let finished = match self.next()? {
                Event::Scalar(s, style, anchor_id, tag) => {
                    let value = repr_to_python(py, &s, style, tag.as_deref())?;
                    self.store_anchor(anchor_id, &value, py);
                    value
                }
                Event::Alias(id) => self
                    .anchors
                    .get(&id)
                    .map_or_else(|| py.None(), |v| v.clone_ref(py)),
                Event::MappingStart(anchor_id, tag) => {
                    let is_set = tag
                        .as_ref()
                        .is_some_and(|t| t.is_yaml_core_schema() && t.suffix == "set");
                    stack.push(if is_set {
                        OpenNode::set(anchor_id)
                    } else {
                        OpenNode::mapping(anchor_id)
                    });
                    continue;
                }
                Event::SequenceStart(anchor_id, _tag) => {
                    stack.push(OpenNode::sequence(anchor_id));
                    continue;
                }
                Event::MappingEnd | Event::SequenceEnd => {
                    let Some(node) = stack.pop() else {
                        return Ok(py.None());
                    };
                    let anchor_id = node.anchor_id;
                    let value = node.finish(py)?;
                    self.store_anchor(anchor_id, &value, py);
                    value
                }
                Event::DocumentEnd | Event::StreamEnd => return Ok(py.None()),
                // Unexpected inside a value context; treat as null
                Event::DocumentStart(_) | Event::StreamStart | Event::Nothing => py.None(),
            };
            match stack.last_mut() {
                Some(parent) => parent.accept(py, finished)?,
                None => return Ok(finished),
            }
        }
    }

    fn store_anchor(&mut self, anchor_id: usize, value: &Py<PyAny>, py: Python<'_>) {
        if anchor_id > 0 {
            self.anchors.insert(anchor_id, value.clone_ref(py));
        }
    }
}

/// What an open container collects as its children arrive.
enum Children {
    Sequence(Vec<Py<PyAny>>),
    /// Merge (`<<`) values and explicit pairs, plus a key awaiting its value.
    Mapping {
        merges: Vec<Py<PyAny>>,
        explicit: Vec<(Py<PyAny>, Py<PyAny>)>,
        key: Option<Py<PyAny>>,
    },
    /// `!!set`: keys only, plus a key awaiting its (ignored) null value.
    Set {
        keys: Vec<Py<PyAny>>,
        key: Option<Py<PyAny>>,
    },
}

/// A container whose closing event has not been seen yet.
struct OpenNode {
    anchor_id: usize,
    children: Children,
}

impl OpenNode {
    const fn sequence(anchor_id: usize) -> Self {
        Self {
            anchor_id,
            children: Children::Sequence(Vec::new()),
        }
    }

    const fn mapping(anchor_id: usize) -> Self {
        Self {
            anchor_id,
            children: Children::Mapping {
                merges: Vec::new(),
                explicit: Vec::new(),
                key: None,
            },
        }
    }

    const fn set(anchor_id: usize) -> Self {
        Self {
            anchor_id,
            children: Children::Set {
                keys: Vec::new(),
                key: None,
            },
        }
    }

    /// Take the next completed child node.
    fn accept(&mut self, py: Python<'_>, value: Py<PyAny>) -> PyResult<()> {
        match &mut self.children {
            Children::Sequence(items) => items.push(value),
            Children::Mapping {
                merges,
                explicit,
                key,
            } => match key.take() {
                None => {
                    // Reject unhashable complex keys with the same error as the original pipeline
                    let bound = value.bind(py);
                    if bound.cast::<PyList>().is_ok() || bound.cast::<PyDict>().is_ok() {
                        return Err(PyValueError::new_err(
                            "YAML complex keys (sequences or mappings as keys) are not supported as Python dict keys",
                        ));
                    }
                    *key = Some(value);
                }
                Some(k) => {
                    let is_merge = k
                        .bind(py)
                        .cast::<PyString>()
                        .is_ok_and(|s| s.to_str().is_ok_and(|s| s == "<<"));
                    if is_merge {
                        merges.push(value);
                    } else {
                        explicit.push((k, value));
                    }
                }
            },
            Children::Set { keys, key } => match key.take() {
                None => *key = Some(value),
                Some(k) => keys.push(k),
            },
        }
        Ok(())
    }

    /// Build the Python object once the container's end event arrives.
    fn finish(self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match self.children {
            Children::Sequence(items) => Ok(PyList::new(py, &items)?.into_any().unbind()),
            Children::Set { keys, .. } => Ok(PySet::new(py, &keys)?.into_any().unbind()),
            Children::Mapping {
                merges, explicit, ..
            } => build_mapping(py, &merges, explicit),
        }
    }
}

/// Build a `PyDict` from explicit pairs and YAML 1.1 merge keys (`<<`).
///
/// Merged keys are applied first (lower priority); explicit keys always win.
fn build_mapping(
    py: Python<'_>,
    merges: &[Py<PyAny>],
    explicit: Vec<(Py<PyAny>, Py<PyAny>)>,
) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    for merge_val in merges {
        let bound = merge_val.bind(py);
        if let Ok(merge_dict) = bound.cast::<PyDict>() {
            merge_missing(&dict, merge_dict)?;
        } else if let Ok(seq) = bound.cast::<PyList>() {
            for item in seq.iter() {
                if let Ok(merge_dict) = item.cast::<PyDict>() {
                    merge_missing(&dict, merge_dict)?;
                }
            }
        }
    }
    for (key, value) in explicit {
        dict.set_item(key, value)?;
    }
    Ok(dict.into_any().unbind())
}

fn merge_missing(dict: &Bound<'_, PyDict>, source: &Bound<'_, PyDict>) -> PyResult<()> {
    for (mk, mv) in source.iter() {
        if !dict.contains(mk.clone())? {
            dict.set_item(mk, mv)?;
        }
    }
    Ok(())
}

fn scan_err(e: &ScanError) -> PyErr {
    PyValueError::new_err(format!("YAML parse error: {e}"))
}

fn limit_err(e: &ParseError) -> PyErr {
    PyValueError::new_err(format!("YAML parse error: {e}"))
}
