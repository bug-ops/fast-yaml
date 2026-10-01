//! YAML 1.1 merge key (`<<`) resolution shared by every binding.
//!
//! [`merge_into`] is the single implementation of the merge algorithm; the core
//! loader and the Python event loader each supply a [`MergeTarget`] over their own
//! mapping representation, so key order and precedence cannot drift between surfaces.
//!
//! Semantics, in terms of the insertion order of the resulting mapping:
//!
//! - merged keys come first, regardless of where `<<` sits in the source mapping;
//! - an explicit key always wins and replaces the merged value in the merged key's position;
//! - for `<<: [*a, *b]` the earlier item wins and keys appear in forward order (a, then b);
//! - the merge is shallow;
//! - only the plain, untagged scalar `<<` and any scalar tagged `!!merge` are merge keys; quoted
//!   or otherwise tagged forms are ordinary keys, and so is every key of a `!!set`;
//! - a merge value must be a mapping or a sequence of mappings, anything else is a [`MergeError`].

use std::collections::HashSet;

use saphyr_parser::{Event, ScalarStyle, Tag};
use thiserror::Error;

use crate::scalar::core_tag_suffix_raw;
use crate::value::{Mapping, Value};

/// Whether `tag` is the core-schema `!!set` tag, in any spelling.
pub(crate) fn is_core_set_tag(tag: &Tag) -> bool {
    core_tag_suffix_raw(tag) == Some("set")
}

/// Whether `event` is a scalar that makes the key it stands for a merge key: the plain,
/// untagged `<<`, or any scalar tagged `!!merge`.
fn is_merge_key_scalar(event: &Event<'_>) -> bool {
    match event {
        Event::Scalar(_, _, _, Some(tag)) => core_tag_suffix_raw(tag) == Some("merge"),
        Event::Scalar(text, ScalarStyle::Plain, _, None) => text == "<<",
        _ => false,
    }
}

/// Structural role of a node within its parent, as reported by
/// the merge key validator behind [`EventStream`](crate::events::EventStream).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NodeRole {
    /// The root of a document.
    Root,
    /// An item of a sequence.
    Item,
    /// A mapping key that is not a merge key, including every key of a `!!set`.
    Key,
    /// A `<<` or `!!merge` key, or an alias in key position to an anchored one.
    MergeKey,
    /// A mapping value.
    Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameKind {
    Mapping,
    Set,
    Sequence,
}

/// An open container and how many child nodes it has received.
#[derive(Debug)]
struct Frame {
    kind: FrameKind,
    children: usize,
}

/// Classifies parser events by their role in the tree, spotting `<<` merge keys.
///
/// The single definition of which keys are merge keys, shared by the core loader and the
/// streaming formatter through the merge key validator, which `EventStream` applies as well.
#[derive(Debug, Default)]
pub(crate) struct MergeKeyTracker {
    frames: Vec<Frame>,
    merge_anchors: HashSet<usize>,
}

impl MergeKeyTracker {
    /// Feeds the next event; returns the role of the node it starts, `None` for other events.
    pub(crate) fn observe(&mut self, event: &Event<'_>) -> Option<NodeRole> {
        let role = match event {
            Event::Scalar(..)
            | Event::Alias(_)
            | Event::MappingStart(..)
            | Event::SequenceStart(..) => Some(self.next_role(event)),
            _ => None,
        };
        match event {
            Event::Scalar(_, _, anchor @ 1.., _) if is_merge_key_scalar(event) => {
                self.merge_anchors.insert(*anchor);
            }
            Event::MappingStart(_, tag) => {
                let kind = if tag.as_deref().is_some_and(is_core_set_tag) {
                    FrameKind::Set
                } else {
                    FrameKind::Mapping
                };
                self.frames.push(Frame { kind, children: 0 });
            }
            Event::SequenceStart(..) => self.frames.push(Frame {
                kind: FrameKind::Sequence,
                children: 0,
            }),
            Event::MappingEnd | Event::SequenceEnd => {
                self.frames.pop();
            }
            _ => {}
        }
        role
    }

    fn next_role(&mut self, event: &Event<'_>) -> NodeRole {
        let Some(frame) = self.frames.last_mut() else {
            return NodeRole::Root;
        };
        frame.children += 1;
        let is_key = frame.children % 2 == 1;
        match (frame.kind, is_key) {
            (FrameKind::Sequence, _) => NodeRole::Item,
            (FrameKind::Mapping | FrameKind::Set, false) => NodeRole::Value,
            (FrameKind::Set, true) => NodeRole::Key,
            (FrameKind::Mapping, true) => {
                let merge_key = match event {
                    Event::Alias(id) => self.merge_anchors.contains(id),
                    other => is_merge_key_scalar(other),
                };
                if merge_key {
                    NodeRole::MergeKey
                } else {
                    NodeRole::Key
                }
            }
        }
    }
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
/// use fast_yaml_core::{Mapping, MergeTarget, Value};
///
/// let text = |s: &str| Value::String(s.into());
/// let mut map = Mapping::new();
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
    /// Failure of a target operation; a rejected merge value converts into it.
    type Error: From<MergeError>;
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
    /// [`merge_into`] calls this exactly once per explicit pair, in iteration order, and never for
    /// merged entries; implementors may count calls to tell which pair failed.
    ///
    /// # Errors
    ///
    /// Returns the target's error when the store fails.
    fn set(&mut self, key: Self::Node, value: Self::Node) -> Result<(), Self::Error>;
}

impl MergeTarget for Mapping {
    type Node = Value;
    type Error = MergeError;
    type Entries = Self;
    type Items = Vec<Value>;

    fn classify(&self, node: Value) -> Result<MergeSource<Self, Vec<Value>>, MergeError> {
        Ok(match node {
            Value::Mapping(map) => MergeSource::Mapping(map),
            Value::Sequence(items) => MergeSource::Sequence(items),
            Value::Set(_) => MergeSource::Set,
            _ => MergeSource::Other,
        })
    }

    fn set_if_absent(&mut self, key: Value, value: Value) -> Result<(), MergeError> {
        if !self.contains_key(&key) {
            self.insert(key, value);
        }
        Ok(())
    }

    fn set(&mut self, key: Value, value: Value) -> Result<(), MergeError> {
        self.insert(key, value);
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
/// use fast_yaml_core::{Mapping, Value, merge::merge_into};
///
/// let text = |s: &str| Value::String(s.into());
/// let base: Mapping = [(text("x"), text("1")), (text("y"), text("2"))].into_iter().collect();
///
/// let mut merged = Mapping::new();
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
                        MergeSource::Set => return Err(MergeError::SetSource.into()),
                        MergeSource::Sequence(_) | MergeSource::Other => {
                            return Err(MergeError::NotMapping.into());
                        }
                    }
                }
            }
            MergeSource::Set => return Err(MergeError::SetSource.into()),
            MergeSource::Other => return Err(MergeError::NotMapping.into()),
        }
    }
    for (key, value) in explicit {
        target.set(key, value)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use saphyr_parser::Parser;

    fn roles(yaml: &str) -> Vec<NodeRole> {
        let mut tracker = MergeKeyTracker::default();
        Parser::new_from_str(yaml)
            .filter_map(|event| tracker.observe(&event.unwrap().0))
            .collect()
    }

    #[test]
    fn a_set_source_is_rejected_when_merging_into_a_mapping() {
        let set = Value::Set(std::iter::once(crate::Value::Int(1)).collect());
        let mut target = Mapping::new();
        assert_eq!(
            merge_into(&mut target, Some(set), std::iter::empty()),
            Err(MergeError::SetSource)
        );
    }

    #[test]
    fn set_keys_are_never_merge_keys() {
        use NodeRole::{Key, MergeKey, Root, Value};
        assert_eq!(roles("{<<: 1}"), [Root, MergeKey, Value]);
        assert_eq!(roles("!!set {<<: 1}"), [Root, Key, Value]);
        assert_eq!(
            roles("!<tag:yaml.org,2002:set> {<<, !!merge x}"),
            [Root, Key, Value, Key, Value]
        );
    }

    #[test]
    fn merge_tag_and_its_anchor_mark_merge_keys() {
        use NodeRole::{MergeKey, Root, Value};
        assert_eq!(
            roles("{!!merge x: 1, !!merge 'y': 2}"),
            [Root, MergeKey, Value, MergeKey, Value]
        );
        assert_eq!(
            roles("[&k !!merge <<, {*k : 1}]")[..3],
            [Root, NodeRole::Item, NodeRole::Item]
        );
        assert_eq!(roles("[&k !!merge <<, {*k : 1}]")[3], MergeKey);
    }

    #[test]
    fn core_tag_suffix_reads_each_spelling() {
        let tag = |h: &str, s: &str| Tag {
            handle: h.into(),
            suffix: s.into(),
        };
        assert!(is_core_set_tag(&tag("tag:yaml.org,2002:", "set")));
        assert!(is_core_set_tag(&tag("", "tag:yaml.org,2002:set")));
        assert!(!is_core_set_tag(&tag("!", "set")));
    }
}
