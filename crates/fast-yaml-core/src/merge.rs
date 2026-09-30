//! YAML 1.1 merge key (`<<`) resolution shared by every binding.
//!
//! [`merge_into`] is the single implementation of the merge algorithm; the core
//! parser and the Python event loader each supply a [`MergeTarget`] over their own
//! mapping representation, so key order and precedence cannot drift between surfaces.
//!
//! Semantics, in terms of the insertion order of the resulting mapping:
//!
//! - merged keys come first, regardless of where `<<` sits in the source mapping;
//! - an explicit key always wins and replaces the merged value in the merged key's position;
//! - for `<<: [*a, *b]` the earlier item wins and keys appear in forward order (a, then b);
//! - the merge is shallow;
//! - only the plain, untagged scalar `<<` is a merge key; quoted or tagged forms are ordinary keys;
//! - a merge value must be a mapping or a sequence of mappings, anything else is a [`MergeError`].

use std::num::NonZeroUsize;

use saphyr_parser::Tag;
use thiserror::Error;

use crate::value::{Map, Value};

/// Handle of the stand-in tag that keeps `!!set` visible after loading.
///
/// It contains NUL, which input validation rejects, so no document can spell it.
const SET_MARKER_HANDLE: &str = "tag:fast-yaml.internal:\0";

/// Handle of the per-key tag that keeps repeated plain `<<` keys distinct after loading.
const MERGE_KEY_MARKER_HANDLE: &str = "tag:fast-yaml.internal:\0merge";

/// One-based ordinal of a tagged `<<` key, in order of appearance in the stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MergeKeyId(NonZeroUsize);

impl MergeKeyId {
    /// The id of the key that follows `recorded` earlier keys.
    pub(crate) const fn after(recorded: usize) -> Self {
        Self(NonZeroUsize::MIN.saturating_add(recorded))
    }

    /// Zero-based position of this key in a table of all keys.
    pub(crate) const fn index(self) -> usize {
        self.0.get() - 1
    }
}

pub(crate) fn merge_key_tag(id: MergeKeyId) -> Tag {
    Tag {
        handle: MERGE_KEY_MARKER_HANDLE.into(),
        suffix: id.0.to_string(),
    }
}

pub(crate) fn merge_key_id(tag: &Tag) -> Option<MergeKeyId> {
    if !is_merge_key_marker(tag) {
        return None;
    }
    tag.suffix.parse().ok().map(MergeKeyId)
}

pub(crate) fn is_merge_key_marker(tag: &Tag) -> bool {
    tag.handle == MERGE_KEY_MARKER_HANDLE
}

pub(crate) fn set_marker_tag() -> Tag {
    Tag {
        handle: SET_MARKER_HANDLE.into(),
        suffix: "set".into(),
    }
}

pub(crate) fn is_set_marker(tag: &Tag) -> bool {
    tag.handle == SET_MARKER_HANDLE && tag.suffix == "set"
}

/// Why a `<<` value cannot be merged.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::{MergeError, ParseError, Parser};
///
/// let err = Parser::parse_str("m:\n  <<: 1\n").unwrap_err();
/// assert!(matches!(err, ParseError::Merge { error: MergeError::NotMapping, .. }));
/// ```
#[derive(Error, Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MergeError {
    /// The value is not a mapping, or a sequence item is not a mapping.
    #[error("merge key `<<` requires a mapping or a sequence of mappings")]
    NotMapping,
    /// The value is a `!!set`, which is not a mapping for merging purposes.
    #[error("merge key `<<` cannot merge a `!!set`")]
    SetSource,
}

/// Representation-independent view of a `<<` value.
#[derive(Debug)]
pub enum MergeSource<E, S> {
    /// A mapping whose entries are absorbed.
    Mapping(E),
    /// A sequence whose mapping items are absorbed in order.
    Sequence(S),
    /// A `!!set`; rejected with [`MergeError::SetSource`].
    Set,
    /// Anything else; rejected with [`MergeError::NotMapping`].
    Other,
}

/// Mapping sink that [`merge_into`] fills.
///
/// Implementors own the notion of key equality and of what a node is; the
/// algorithm only requires that [`set`](Self::set) keeps an existing key's position and
/// that [`set_if_absent`](Self::set_if_absent) never overwrites.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::{Map, MergeTarget, ScalarOwned, Value};
///
/// let text = |s: &str| Value::Value(ScalarOwned::String(s.into()));
/// let mut map = Map::new();
/// map.set(text("a"), text("1")).unwrap();
/// map.set(text("b"), text("2")).unwrap();
/// map.set_if_absent(text("a"), text("ignored")).unwrap();
/// map.set(text("a"), text("3")).unwrap();
///
/// let entries: Vec<_> = map.into_iter().collect();
/// assert_eq!(entries, [(text("a"), text("3")), (text("b"), text("2"))]);
/// ```
pub trait MergeTarget {
    /// Key and value type of the target and of its merge sources.
    type Node;
    /// Failure of a target operation, including a rejected merge value.
    type Error;
    /// Entries of a mapping merge source.
    type Entries: IntoIterator<Item = (Self::Node, Self::Node)>;
    /// Items of a sequence merge source.
    type Items: IntoIterator<Item = Self::Node>;

    /// Classifies a merge value or a sequence item.
    ///
    /// # Errors
    ///
    /// Returns the target's error when the node cannot be inspected.
    fn classify(
        &self,
        node: Self::Node,
    ) -> Result<MergeSource<Self::Entries, Self::Items>, Self::Error>;

    /// Wraps a rejected merge value in the target's error type.
    fn reject(error: MergeError) -> Self::Error;

    /// Stores `value` under `key` only when `key` is absent; an existing entry is left untouched.
    ///
    /// # Errors
    ///
    /// Returns the target's error when the lookup or store fails.
    fn set_if_absent(&mut self, key: Self::Node, value: Self::Node) -> Result<(), Self::Error>;

    /// Stores `value` under `key`; an existing key keeps its position and gets the new value.
    ///
    /// [`merge_into`] calls this exactly once per explicit pair, in iteration order, and never for
    /// merged entries; implementors may count calls to tell which pair failed.
    ///
    /// # Errors
    ///
    /// Returns the target's error when the store fails.
    fn set(&mut self, key: Self::Node, value: Self::Node) -> Result<(), Self::Error>;
}

impl MergeTarget for Map {
    type Node = Value;
    type Error = MergeError;
    type Entries = Self;
    type Items = Vec<Value>;

    fn classify(&self, node: Value) -> Result<MergeSource<Self, Vec<Value>>, MergeError> {
        Ok(match node {
            Value::Mapping(map) => MergeSource::Mapping(map),
            Value::Sequence(items) => MergeSource::Sequence(items),
            Value::Tagged(tag, _) if is_set_marker(&tag) => MergeSource::Set,
            _ => MergeSource::Other,
        })
    }

    fn reject(error: MergeError) -> MergeError {
        error
    }

    fn set_if_absent(&mut self, key: Value, value: Value) -> Result<(), MergeError> {
        // `entry().or_insert()` moves an occupied key to the back
        if !self.contains_key(&key) {
            self.insert(key, value);
        }
        Ok(())
    }

    fn set(&mut self, key: Value, value: Value) -> Result<(), MergeError> {
        // `insert` would move an existing key to the back
        self.replace(key, value);
        Ok(())
    }
}

fn absorb<T: MergeTarget>(
    target: &mut T,
    entries: impl IntoIterator<Item = (T::Node, T::Node)>,
) -> Result<(), T::Error> {
    for (key, value) in entries {
        target.set_if_absent(key, value)?;
    }
    Ok(())
}

/// Fills `target` from a `<<` value followed by the explicit pairs of the mapping.
///
/// `merge` is the single `<<` value (a repeated `<<` keeps only the last, like any
/// duplicate key). Merged entries are added first; a key already present is skipped,
/// so the earlier sequence item wins. Explicit pairs are then applied and always win.
///
/// # Errors
///
/// Propagates the first error returned by the target, and rejects a `merge` that is not a
/// mapping or a sequence of mappings.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::{Map, ScalarOwned, Value, merge::merge_into};
///
/// let text = |s: &str| Value::Value(ScalarOwned::String(s.into()));
/// let base: Map = [(text("x"), text("1")), (text("y"), text("2"))].into_iter().collect();
///
/// let mut merged = Map::new();
/// merge_into(&mut merged, Some(Value::Mapping(base)), [(text("x"), text("0"))]).unwrap();
///
/// let entries: Vec<_> = merged.into_iter().collect();
/// assert_eq!(entries, [(text("x"), text("0")), (text("y"), text("2"))]);
/// ```
pub fn merge_into<T: MergeTarget>(
    target: &mut T,
    merge: Option<T::Node>,
    explicit: impl IntoIterator<Item = (T::Node, T::Node)>,
) -> Result<(), T::Error> {
    if let Some(node) = merge {
        match target.classify(node)? {
            MergeSource::Mapping(entries) => absorb(target, entries)?,
            MergeSource::Sequence(items) => {
                for item in items {
                    match target.classify(item)? {
                        MergeSource::Mapping(entries) => absorb(target, entries)?,
                        MergeSource::Set => return Err(T::reject(MergeError::SetSource)),
                        MergeSource::Sequence(_) | MergeSource::Other => {
                            return Err(T::reject(MergeError::NotMapping));
                        }
                    }
                }
            }
            MergeSource::Set => return Err(T::reject(MergeError::SetSource)),
            MergeSource::Other => return Err(T::reject(MergeError::NotMapping)),
        }
    }
    for (key, value) in explicit {
        target.set(key, value)?;
    }
    Ok(())
}
