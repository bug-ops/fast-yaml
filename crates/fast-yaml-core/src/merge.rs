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
//! - the merge is shallow and non-mapping merge values are ignored.

use std::convert::Infallible;

use crate::value::{Map, Value};

/// Representation-independent view of a `<<` value.
#[derive(Debug)]
pub enum MergeSource<E, S> {
    /// A mapping whose entries are absorbed.
    Mapping(E),
    /// A sequence whose mapping items are absorbed in order.
    Sequence(S),
    /// Anything else; contributes nothing.
    Ignored,
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
    /// Failure of a target operation; [`Infallible`] when none can fail.
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

    /// Stores `value` under `key` only when `key` is absent; an existing entry is left untouched.
    ///
    /// # Errors
    ///
    /// Returns the target's error when the lookup or store fails.
    fn set_if_absent(&mut self, key: Self::Node, value: Self::Node) -> Result<(), Self::Error>;

    /// Stores `value` under `key`; an existing key keeps its position and gets the new value.
    ///
    /// # Errors
    ///
    /// Returns the target's error when the store fails.
    fn set(&mut self, key: Self::Node, value: Self::Node) -> Result<(), Self::Error>;
}

impl MergeTarget for Map {
    type Node = Value;
    type Error = Infallible;
    type Entries = Self;
    type Items = Vec<Value>;

    fn classify(&self, node: Value) -> Result<MergeSource<Self, Vec<Value>>, Infallible> {
        Ok(match node {
            Value::Mapping(map) => MergeSource::Mapping(map),
            Value::Sequence(items) => MergeSource::Sequence(items),
            _ => MergeSource::Ignored,
        })
    }

    fn set_if_absent(&mut self, key: Value, value: Value) -> Result<(), Infallible> {
        // `entry().or_insert()` moves an occupied key to the back
        if !self.contains_key(&key) {
            self.insert(key, value);
        }
        Ok(())
    }

    fn set(&mut self, key: Value, value: Value) -> Result<(), Infallible> {
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
/// Propagates the first error returned by the target.
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
                    if let MergeSource::Mapping(entries) = target.classify(item)? {
                        absorb(target, entries)?;
                    }
                }
            }
            MergeSource::Ignored => {}
        }
    }
    for (key, value) in explicit {
        target.set(key, value)?;
    }
    Ok(())
}
