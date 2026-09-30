//! Event-driven tree builder behind [`Parser`](crate::Parser).
//!
//! Replaces saphyr's `YamlLoader` because that loader inserts mapping entries with
//! `LinkedHashMap::insert`, which moves a repeated key to the back. Here a repeated key keeps
//! its first position and takes the last value, whatever its anchor, tag or shape.

#![allow(clippy::redundant_pub_crate)]

use std::collections::HashMap;

use saphyr_parser::{Event, Tag};

use crate::value::{Map, Value};

struct Frame {
    node: Value,
    anchor: usize,
    tag: Option<Tag>,
    pending_key: Option<Value>,
}

/// Builds [`Value`] documents from parser events, keeping scalars as unresolved representations.
#[derive(Default)]
pub(crate) struct ValueLoader {
    docs: Vec<Value>,
    stack: Vec<Frame>,
    root: Option<Value>,
    anchors: HashMap<usize, Value>,
}

impl ValueLoader {
    pub(crate) fn on_event(&mut self, event: Event<'_>) {
        match event {
            Event::DocumentStart(_) => self.anchors.clear(),
            Event::DocumentEnd => {
                let doc = self.root.take().unwrap_or(Value::BadValue);
                self.docs.push(doc);
            }
            Event::SequenceStart(anchor, tag) => {
                self.open(
                    Value::Sequence(Vec::new()),
                    anchor,
                    tag.map(std::borrow::Cow::into_owned),
                );
            }
            Event::MappingStart(anchor, tag) => {
                self.open(
                    Value::Mapping(Map::new()),
                    anchor,
                    tag.map(std::borrow::Cow::into_owned),
                );
            }
            Event::SequenceEnd | Event::MappingEnd => {
                if let Some(Frame {
                    mut node,
                    anchor,
                    tag,
                    ..
                }) = self.stack.pop()
                {
                    if let Some(tag) = tag.filter(|t| !t.is_yaml_core_schema()) {
                        node = Value::Tagged(tag, Box::new(node));
                    }
                    self.insert(node, anchor);
                }
            }
            Event::Scalar(text, style, anchor, tag) => {
                let node = Value::Representation(
                    text.into_owned(),
                    style,
                    tag.map(std::borrow::Cow::into_owned),
                );
                self.insert(node, anchor);
            }
            Event::Alias(id) => {
                let node = self.anchors.get(&id).cloned().unwrap_or(Value::BadValue);
                self.insert(node, 0);
            }
            Event::Nothing | Event::StreamStart | Event::StreamEnd => {}
        }
    }

    pub(crate) fn into_documents(self) -> Vec<Value> {
        self.docs
    }

    fn open(&mut self, node: Value, anchor: usize, tag: Option<Tag>) {
        self.stack.push(Frame {
            node,
            anchor,
            tag,
            pending_key: None,
        });
    }

    fn insert(&mut self, node: Value, anchor: usize) {
        if anchor > 0 {
            self.anchors.insert(anchor, node.clone());
        }
        let Some(frame) = self.stack.last_mut() else {
            self.root = Some(node);
            return;
        };
        match &mut frame.node {
            Value::Sequence(items) => items.push(node),
            Value::Mapping(map) => match frame.pending_key.take() {
                None => frame.pending_key = Some(node),
                Some(key) => {
                    map.replace(key, node);
                }
            },
            _ => unreachable!("only collections are opened"),
        }
    }
}
