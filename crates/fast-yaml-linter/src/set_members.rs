//! Finds `!!set` members that carry a value while the source loads.

use std::collections::HashMap;

use fast_yaml_core::events::{AnchorId, Event, EventItem};
use fast_yaml_core::merge::is_set_tag;
use fast_yaml_core::{ResolvedScalar, resolve_scalar};

use crate::source::offset::ByteRange;

/// A `!!set` member whose value is not null.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetMember {
    /// The member key, `None` for a collection used as a key.
    pub key: Option<String>,
    /// The key as written.
    pub range: ByteRange,
}

/// Whether a value node is null.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Nullness {
    Null,
    NonNull,
}

/// A member key waiting for its value.
struct Key {
    range: ByteRange,
    text: Option<String>,
}

/// Position within an open `!!set`.
#[derive(Default)]
enum SetState {
    #[default]
    AwaitingKey,
    AwaitingValue(Key),
}

/// One open collection.
enum Frame {
    Set(SetState),
    Other,
}

/// Whether `source` can hold a `!!set`: every spelling of the tag (`!!set`, `!e!set`,
/// `!<tag:yaml.org,2002:set>`) starts with `!` and ends in the `set` suffix.
pub fn may_contain_set(source: &str) -> bool {
    source.contains('!') && source.contains("set")
}

/// Folds parser events into the list of `!!set` members that carry a value.
#[derive(Default)]
pub struct SetMembers {
    frames: Vec<Frame>,
    anchors: HashMap<AnchorId, Nullness>,
    members: Vec<SetMember>,
}

impl SetMembers {
    /// Folds one event; `range` is its byte range in the source.
    pub(crate) fn observe(&mut self, item: &EventItem<'_>, range: ByteRange) {
        match &item.event {
            Event::DocumentStart { .. } => {
                self.frames.clear();
                self.anchors.clear();
            }
            Event::Scalar {
                value,
                style,
                anchor,
                tag,
            } => {
                let null = resolve_scalar(value, *style, tag.as_ref()) == ResolvedScalar::Null;
                let nullness = if null {
                    Nullness::Null
                } else {
                    Nullness::NonNull
                };
                if let Some(id) = anchor {
                    self.anchors.insert(*id, nullness);
                }
                self.node(range, Some(value), nullness);
            }
            Event::Alias(id) => {
                let nullness = self.anchors.get(id).copied().unwrap_or(Nullness::Null);
                self.node(range, None, nullness);
            }
            Event::MappingStart { anchor, tag } => {
                let frame = if tag.as_ref().is_some_and(is_set_tag) {
                    Frame::Set(SetState::default())
                } else {
                    Frame::Other
                };
                self.collection(range, *anchor, frame);
            }
            Event::SequenceStart { anchor, .. } => self.collection(range, *anchor, Frame::Other),
            Event::MappingEnd | Event::SequenceEnd => {
                self.frames.pop();
            }
            Event::StreamStart | Event::StreamEnd | Event::DocumentEnd => {}
        }
    }

    pub(crate) fn into_members(self) -> Vec<SetMember> {
        self.members
    }

    /// Places a node in the innermost `!!set`, reporting its member when it is a non-null value.
    fn node(&mut self, range: ByteRange, text: Option<&str>, nullness: Nullness) {
        let Some(Frame::Set(state)) = self.frames.last_mut() else {
            return;
        };
        match std::mem::take(state) {
            SetState::AwaitingKey => {
                *state = SetState::AwaitingValue(Key {
                    range,
                    text: text.map(str::to_owned),
                });
            }
            SetState::AwaitingValue(Key { range, text }) => {
                if nullness == Nullness::NonNull {
                    self.members.push(SetMember { key: text, range });
                }
            }
        }
    }

    fn collection(&mut self, range: ByteRange, anchor: Option<AnchorId>, frame: Frame) {
        self.node(range, None, Nullness::NonNull);
        if let Some(id) = anchor {
            self.anchors.insert(id, Nullness::NonNull);
        }
        self.frames.push(frame);
    }
}
