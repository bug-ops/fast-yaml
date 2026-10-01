//! Iterative walk of a [`Value`] into parser events.
//!
//! The walker keeps its own stack of open collections, so nesting depth costs heap, never call
//! stack. Two sinks consume the events: the streaming formatter for block style and
//! [`FlowWriter`](super::flow::FlowWriter) for flow style.

use std::borrow::Cow;

use saphyr_parser::{Event, Tag};

use super::scalar::{Position, present};
use crate::error::{EmitError, EmitResult};
use crate::limits::MaxDepth;
use crate::value::{Iter, SetIter, Value};

/// Consumer of the events a [`Value`] is walked into.
pub trait EventSink {
    /// Handles one event.
    ///
    /// # Errors
    ///
    /// Returns the sink's emit error, for example when a formatter limit is exceeded.
    fn event(&mut self, event: Event<'_>) -> EmitResult<()>;
}

/// The style collections are written in, which decides how sets and complex keys are walked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Block collections, with set members carrying null values.
    Block,
    /// Flow collections, with set members alone.
    Flow,
}

/// What the walk needs from the configuration.
#[derive(Debug, Clone, Copy)]
pub struct WalkOptions {
    /// Collection style.
    pub layout: Layout,
    /// Whether multiline strings become literal blocks.
    pub multiline: bool,
    /// Deepest nesting of collections.
    pub max_depth: MaxDepth,
}

enum Frame<'v> {
    Sequence(std::slice::Iter<'v, Value>),
    Mapping {
        entries: Iter<'v>,
        value: Option<&'v Value>,
    },
    Set {
        members: SetIter<'v>,
        /// A block-layout member was written and its null value is still owed.
        owes_value: bool,
    },
}

/// The next node to write and whether it stands in key position.
#[derive(Clone, Copy)]
struct Node<'v> {
    value: &'v Value,
    is_key: bool,
}

/// Walks `value` as one document into `sink`.
///
/// # Errors
///
/// Returns [`EmitError::DepthLimitExceeded`] beyond `options.max_depth`,
/// [`EmitError::SetAsKey`] for a set in key position in block layout, and
/// [`EmitError::ComplexFlowKey`] for a collection in key position in flow layout.
pub fn walk(value: &Value, options: WalkOptions, sink: &mut dyn EventSink) -> EmitResult<()> {
    sink.event(Event::StreamStart)?;
    sink.event(Event::DocumentStart(false))?;
    let mut stack: Vec<Frame<'_>> = Vec::new();
    let mut next = Some(Node {
        value,
        is_key: false,
    });
    loop {
        if let Some(node) = next.take() {
            open(node, options, &mut stack, sink)?;
        }
        let Some(top) = stack.last_mut() else {
            break;
        };
        next = match top {
            Frame::Sequence(items) => items.next().map(|value| Node {
                value,
                is_key: false,
            }),
            Frame::Set {
                members,
                owes_value,
            } => {
                if std::mem::take(owes_value) {
                    scalar(&Value::Null, Position::Block, options, sink)?;
                }
                let member = members.next();
                *owes_value = member.is_some() && options.layout == Layout::Block;
                member.map(|value| Node {
                    value,
                    is_key: true,
                })
            }
            Frame::Mapping { entries, value } => value.take().map_or_else(
                || {
                    entries.next().map(|(key, entry)| {
                        *value = Some(entry);
                        Node {
                            value: key,
                            is_key: true,
                        }
                    })
                },
                |pending| {
                    Some(Node {
                        value: pending,
                        is_key: false,
                    })
                },
            ),
        };
        if next.is_none() {
            let closed = stack.pop();
            sink.event(match closed {
                Some(Frame::Sequence(_)) => Event::SequenceEnd,
                Some(Frame::Mapping { .. } | Frame::Set { .. }) | None => Event::MappingEnd,
            })?;
        }
    }
    sink.event(Event::DocumentEnd)?;
    sink.event(Event::StreamEnd)
}

/// Writes a scalar, or the start of a collection that is pushed on the stack.
fn open<'v>(
    node: Node<'v>,
    options: WalkOptions,
    stack: &mut Vec<Frame<'v>>,
    sink: &mut dyn EventSink,
) -> EmitResult<()> {
    let Node { value, is_key } = node;
    if is_key
        && !matches!(
            value,
            Value::Sequence(_) | Value::Mapping(_) | Value::Set(_)
        )
    {
        let position = match options.layout {
            Layout::Flow => Position::FlowKey,
            Layout::Block => Position::Key,
        };
        return scalar(value, position, options, sink);
    }
    let position = match options.layout {
        Layout::Flow => Position::Flow,
        Layout::Block => Position::Block,
    };
    match value {
        Value::Sequence(items) => {
            reject_key(is_key, value, options.layout)?;
            descend(options.max_depth, stack.len())?;
            sink.event(Event::SequenceStart(0, None))?;
            stack.push(Frame::Sequence(items.iter()));
        }
        Value::Mapping(map) => {
            reject_key(is_key, value, options.layout)?;
            descend(options.max_depth, stack.len())?;
            sink.event(Event::MappingStart(0, None))?;
            stack.push(Frame::Mapping {
                entries: map.iter(),
                value: None,
            });
        }
        Value::Set(set) => {
            reject_key(is_key, value, options.layout)?;
            descend(options.max_depth, stack.len())?;
            sink.event(Event::MappingStart(0, Some(Cow::Owned(set_tag()))))?;
            stack.push(Frame::Set {
                members: set.iter(),
                owes_value: false,
            });
        }
        scalar_value => scalar(scalar_value, position, options, sink)?,
    }
    Ok(())
}

/// Writes one scalar; a set member in block layout is followed by its implied null value.
fn scalar(
    value: &Value,
    position: Position,
    options: WalkOptions,
    sink: &mut dyn EventSink,
) -> EmitResult<()> {
    if let Some((text, style)) = present(value, position, options.multiline) {
        sink.event(Event::Scalar(text, style, 0, None))?;
    }
    Ok(())
}

const fn reject_key(is_key: bool, value: &Value, layout: Layout) -> EmitResult<()> {
    match (is_key, layout, value) {
        (true, Layout::Flow, _) => Err(EmitError::ComplexFlowKey),
        (true, Layout::Block, Value::Set(_)) => Err(EmitError::SetAsKey),
        _ => Ok(()),
    }
}

fn descend(max: MaxDepth, depth: usize) -> EmitResult<()> {
    max.descend(depth)
        .map(drop)
        .map_err(|_| EmitError::DepthLimitExceeded { limit: max.get() })
}

/// The tag a set is written with: `!!set`.
pub fn set_tag() -> Tag {
    Tag {
        handle: "tag:yaml.org,2002:".into(),
        suffix: "set".into(),
    }
}
