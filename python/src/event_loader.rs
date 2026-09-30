//! Event-based YAML-to-Python loader.
//!
//! Uses `saphyr_parser` events directly instead of `saphyr`'s `YamlLoader`,
//! which silently drops core-schema collection tags (`!!set`, `!!omap`, …).
//! This loader preserves the `!!set` tag and converts the mapping to a Python `set`.

use std::collections::HashMap;

use fast_yaml_core::merge::{MergeError, MergeSource, MergeTarget, NodeRole, merge_into};
use fast_yaml_core::scalar::core_tag_suffix;
use fast_yaml_core::{
    LimitGuard, MergeKeyValidator, NormalizedInput, ParseError, ParseLimits, SourcePosition,
    SyntaxError,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyFloat, PyList, PySet};
use saphyr_parser::{Event, Parser, StrInput};

use crate::conversion::COMPLEX_KEY_MESSAGE;
use crate::numeric_keys::{KeyClash, NumericKeys, build_set};
use crate::repr_to_python;

/// Parse all YAML documents from `input` into Python objects.
///
/// Injects one implicit null document when the stream is non-empty but contains
/// no explicit documents (comment-only, whitespace-only, bare `---`/`...`),
/// matching YAML 1.2 §9.2 and `PyYAML` parity.
///
/// # Errors
///
/// Returns `PyValueError` on invalid YAML syntax, when `limits` are exceeded, or when a
/// mapping or set holds keys that YAML keeps distinct but Python equality would merge
/// (`1`, `true` and `1.0`).
pub fn load_all(py: Python<'_>, input: &str, limits: ParseLimits) -> PyResult<Vec<Py<PyAny>>> {
    let normalized = NormalizedInput::new(input).map_err(|e| limit_err(&e))?;
    let mut loader = EventLoader {
        parser: Parser::new_from_str(normalized.as_str()),
        anchors: HashMap::new(),
        merge_keys: MergeKeyValidator::default(),
        guard: LimitGuard::new(limits).sharing_anchors(),
        nan: None,
        last: START,
    };
    let docs = loader.load_stream(py)?;
    // Replicate fast-yaml-core: inject implicit null for non-empty, zero-doc streams
    if docs.is_empty() && !input.is_empty() {
        Ok(vec![py.None()])
    } else {
        Ok(docs)
    }
}

const START: SourcePosition = SourcePosition { line: 1, column: 1 };

/// The position of a key that `merge_into` reported on; it only reports keys it was given.
fn known(at: Option<SourcePosition>) -> SourcePosition {
    at.unwrap_or_else(|| unreachable!("a merge failure names a key that has a position"))
}

struct EventLoader<'input> {
    parser: Parser<'input, StrInput<'input>>,
    /// Anchor id → Python object, used to resolve YAML aliases.
    anchors: HashMap<usize, Py<PyAny>>,
    /// Rejects invalid `<<` values in document order and classifies merge keys.
    merge_keys: MergeKeyValidator,
    /// Enforces depth and alias limits before any recursion or aliasing happens.
    guard: LimitGuard,
    /// The one NaN object of this load: `nan != nan`, so dict lookups only collapse NaN keys by identity.
    nan: Option<Py<PyAny>>,
    /// SourcePosition of the latest event; stands in for the end of the stream, which has no span.
    last: SourcePosition,
}

impl<'input> EventLoader<'input> {
    /// Advance the parser and return the next meaningful event with its source position and role.
    fn next(&mut self) -> PyResult<(Event<'input>, SourcePosition, Option<NodeRole>)> {
        loop {
            match self.parser.next_event() {
                Some(Ok((Event::Nothing, _))) => {}
                Some(Ok((ev, span))) => {
                    self.guard.observe(&ev, span).map_err(|e| limit_err(&e))?;
                    let role = self
                        .merge_keys
                        .observe(&ev, span)
                        .map_err(|e| limit_err(&e))?;
                    self.last = span.into();
                    return Ok((ev, self.last, role));
                }
                Some(Err(e)) => {
                    return Err(limit_err(&ParseError::scanner(&e, self.guard.document())));
                }
                None => return Ok((Event::StreamEnd, self.last, None)),
            }
        }
    }

    fn load_stream(&mut self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        // Consume StreamStart
        self.next()?;

        let mut docs = Vec::new();
        loop {
            match self.next()?.0 {
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
            let (event, at, role) = self.next()?;
            let merge_key = role == Some(NodeRole::MergeKey);
            let finished = match event {
                Event::Scalar(s, style, anchor_id, tag) => {
                    let value = repr_to_python(py, &s, style, tag.as_deref())?;
                    let value = self.share_nan(py, value);
                    self.store_anchor(anchor_id, &value, py);
                    value
                }
                Event::Alias(id) => {
                    let Some(value) = self.anchors.get(&id) else {
                        return Err(limit_err(&ParseError::Syntax(
                            SyntaxError::recursive_alias(at, self.guard.document()),
                        )));
                    };
                    value.clone_ref(py)
                }
                Event::MappingStart(anchor_id, tag) => {
                    let is_set = tag.as_deref().and_then(core_tag_suffix) == Some("set");
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
                Some(parent) => parent.accept(py, finished, merge_key, at)?,
                None => return Ok(finished),
            }
        }
    }

    fn store_anchor(&mut self, anchor_id: usize, value: &Py<PyAny>, py: Python<'_>) {
        if anchor_id > 0 {
            self.anchors.insert(anchor_id, value.clone_ref(py));
        }
    }

    /// Replaces every NaN with the load's single NaN object so NaN keys collapse like in core.
    fn share_nan(&mut self, py: Python<'_>, value: Py<PyAny>) -> Py<PyAny> {
        let is_nan = value
            .bind(py)
            .cast_exact::<PyFloat>()
            .is_ok_and(|float| float.value().is_nan());
        if is_nan {
            self.nan.get_or_insert(value).clone_ref(py)
        } else {
            value
        }
    }
}

/// A `<<` value and the position of its key.
struct MergeValue {
    value: Py<PyAny>,
    at: SourcePosition,
}

/// An explicit mapping pair and the position of its key.
struct Pair {
    key: Py<PyAny>,
    value: Py<PyAny>,
    at: SourcePosition,
}

/// What an open container collects as its children arrive.
enum Children {
    Sequence(Vec<Py<PyAny>>),
    /// The last merge (`<<`) value and explicit pairs, plus a key awaiting its value.
    Mapping {
        merge: Option<MergeValue>,
        explicit: Vec<Pair>,
        key: Option<PendingKey>,
    },
    /// `!!set`: keys only, plus a key awaiting its (ignored) null value.
    Set {
        keys: Vec<(Py<PyAny>, SourcePosition)>,
        key: Option<(Py<PyAny>, SourcePosition)>,
    },
}

/// A mapping key whose value has not arrived yet.
enum PendingKey {
    /// The plain, untagged scalar `<<`; quoted or tagged forms are ordinary keys.
    Merge(SourcePosition),
    Key(Py<PyAny>, SourcePosition),
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
                merge: None,
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

    /// Take the next completed child node; `merge_key` marks a plain untagged `<<` scalar.
    fn accept(
        &mut self,
        py: Python<'_>,
        value: Py<PyAny>,
        merge_key: bool,
        at: SourcePosition,
    ) -> PyResult<()> {
        match &mut self.children {
            Children::Sequence(items) => items.push(value),
            Children::Mapping {
                merge,
                explicit,
                key,
            } => match key.take() {
                None if merge_key => *key = Some(PendingKey::Merge(at)),
                None => {
                    reject_complex_key(value.bind(py))?;
                    *key = Some(PendingKey::Key(value, at));
                }
                Some(PendingKey::Merge(key_at)) => {
                    // A repeated `<<` keeps only the last value; the validator checked every one
                    *merge = Some(MergeValue { value, at: key_at });
                }
                Some(PendingKey::Key(k, key_at)) => explicit.push(Pair {
                    key: k,
                    value,
                    at: key_at,
                }),
            },
            Children::Set { keys, key } => match key.take() {
                None => {
                    reject_complex_key(value.bind(py))?;
                    *key = Some((value, at));
                }
                Some(k) => keys.push(k),
            },
        }
        Ok(())
    }

    /// Build the Python object once the container's end event arrives.
    fn finish(self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match self.children {
            Children::Sequence(items) => Ok(PyList::new(py, &items)?.into_any().unbind()),
            Children::Set { keys, .. } => build_py_set(py, &keys),
            Children::Mapping {
                merge, explicit, ..
            } => build_mapping(py, merge, &explicit),
        }
    }
}

/// Why a mapping could not be built.
enum PyMergeFailure {
    /// A `<<` value that is neither a mapping nor a sequence of mappings.
    Rejected(MergeError),
    /// Keys that YAML keeps distinct but a Python dict would merge.
    Clash(KeyClash, KeyOrigin),
    Py(PyErr),
}

impl From<MergeError> for PyMergeFailure {
    fn from(error: MergeError) -> Self {
        Self::Rejected(error)
    }
}

impl From<PyErr> for PyMergeFailure {
    fn from(err: PyErr) -> Self {
        Self::Py(err)
    }
}

/// Where the incoming key of a clash came from.
#[derive(Clone, Copy)]
enum KeyOrigin {
    /// Absorbed from a `<<` source.
    Merged,
    /// The explicit pair with this index.
    Explicit(usize),
}

/// `PyDict` sink for [`merge_into`].
///
/// Relies on the [`MergeTarget::set`] contract (one call per explicit pair, in order) to map a
/// failure back to the pair's source position.
struct PyMergeTarget<'py> {
    dict: Bound<'py, PyDict>,
    numeric: NumericKeys<'py>,
    /// Explicit pairs applied so far; `set` is only called for those.
    explicit_seen: usize,
}

impl<'py> PyMergeTarget<'py> {
    fn new(py: Python<'py>) -> Self {
        Self {
            dict: PyDict::new(py),
            numeric: NumericKeys::new(py),
            explicit_seen: 0,
        }
    }

    fn check(&mut self, key: &Bound<'py, PyAny>, origin: KeyOrigin) -> Result<(), PyMergeFailure> {
        self.numeric
            .record(key)?
            .map_or(Ok(()), |clash| Err(PyMergeFailure::Clash(clash, origin)))
    }
}

impl<'py> MergeTarget for PyMergeTarget<'py> {
    type Node = Bound<'py, PyAny>;
    type Error = PyMergeFailure;
    type Entries = Vec<(Bound<'py, PyAny>, Bound<'py, PyAny>)>;
    type Items = Vec<Bound<'py, PyAny>>;

    fn classify(
        &self,
        node: Bound<'py, PyAny>,
    ) -> Result<MergeSource<Self::Entries, Self::Items>, PyMergeFailure> {
        if let Ok(dict) = node.cast::<PyDict>() {
            return Ok(MergeSource::Mapping(dict.iter().collect()));
        }
        if let Ok(list) = node.cast::<PyList>() {
            return Ok(MergeSource::Sequence(list.iter().collect()));
        }
        if node.cast::<PySet>().is_ok() {
            return Ok(MergeSource::Set);
        }
        Ok(MergeSource::Other)
    }

    fn set_if_absent(
        &mut self,
        key: Bound<'py, PyAny>,
        value: Bound<'py, PyAny>,
    ) -> Result<(), PyMergeFailure> {
        self.check(&key, KeyOrigin::Merged)?;
        if self.dict.contains(&key)? {
            return Ok(());
        }
        Ok(self.dict.set_item(key, value)?)
    }

    fn set(
        &mut self,
        key: Bound<'py, PyAny>,
        value: Bound<'py, PyAny>,
    ) -> Result<(), PyMergeFailure> {
        let index = self.explicit_seen;
        self.explicit_seen += 1;
        self.check(&key, KeyOrigin::Explicit(index))?;
        Ok(self.dict.set_item(key, value)?)
    }
}

/// Build a `PyDict` from explicit pairs and the YAML 1.1 merge key (`<<`).
///
/// Key order and precedence follow [`fast_yaml_core::merge`].
fn build_mapping(
    py: Python<'_>,
    merge: Option<MergeValue>,
    explicit: &[Pair],
) -> PyResult<Py<PyAny>> {
    let merge_at = merge.as_ref().map(|m| m.at);
    let mut target = PyMergeTarget::new(py);
    merge_into(
        &mut target,
        merge.map(|m| m.value.into_bound(py)),
        explicit
            .iter()
            .map(|p| (p.key.bind(py).clone(), p.value.bind(py).clone())),
    )
    .map_err(|failure| match failure {
        PyMergeFailure::Rejected(error) => {
            let SourcePosition { line, column } = known(merge_at);
            PyValueError::new_err(format!(
                "YAML parse error: {error} at line {line}, column {column}"
            ))
        }
        PyMergeFailure::Clash(clash, origin) => {
            let at = match origin {
                KeyOrigin::Merged => merge_at,
                KeyOrigin::Explicit(index) => explicit.get(index).map(|p| p.at),
            };
            clash_err(&clash, known(at))
        }
        PyMergeFailure::Py(err) => err,
    })?;
    Ok(target.dict.into_any().unbind())
}

/// Rejects unhashable complex keys (and set members) with the same error as the original pipeline.
fn reject_complex_key(key: &Bound<'_, PyAny>) -> PyResult<()> {
    if key.cast::<PyList>().is_ok() || key.cast::<PyDict>().is_ok() || key.cast::<PySet>().is_ok() {
        return Err(PyValueError::new_err(COMPLEX_KEY_MESSAGE));
    }
    Ok(())
}

/// Build a `PySet` from `!!set` keys.
fn build_py_set(py: Python<'_>, keys: &[(Py<PyAny>, SourcePosition)]) -> PyResult<Py<PyAny>> {
    let members: Vec<_> = keys.iter().map(|(key, _)| key.bind(py).clone()).collect();
    match build_set(py, &members)? {
        Ok(set) => Ok(set.into_any().unbind()),
        Err((index, clash)) => Err(clash_err(&clash, keys[index].1)),
    }
}

fn clash_err(clash: &KeyClash, SourcePosition { line, column }: SourcePosition) -> PyErr {
    PyValueError::new_err(format!(
        "YAML parse error: {clash} at line {line}, column {column}"
    ))
}

fn limit_err(e: &ParseError) -> PyErr {
    PyValueError::new_err(format!("YAML parse error: {e}"))
}
