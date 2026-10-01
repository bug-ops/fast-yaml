//! Flow-style writer fed by the same events as the block formatter.

use saphyr_parser::{Event, ScalarStyle, Tag};

use super::walk::EventSink;
use crate::emitter::EmitterConfig;
use crate::error::EmitResult;
use crate::streaming::write_double_quoted;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Sequence,
    Mapping,
    Set,
}

struct Frame {
    kind: Kind,
    entries: usize,
    after_key: bool,
}

/// Writes `[a, b]`, `{k: v}` and `!!set {a, b}` without recursion.
///
/// The output has no trailing newline until [`finish`](Self::finish).
pub struct FlowWriter {
    out: String,
    frames: Vec<Frame>,
    explicit_start: bool,
}

impl FlowWriter {
    /// Creates a writer that honors `config.explicit_start`.
    pub const fn new(config: &EmitterConfig) -> Self {
        Self {
            out: String::new(),
            frames: Vec::new(),
            explicit_start: config.explicit_start,
        }
    }

    /// Returns the text, ending with a line break.
    pub fn finish(mut self) -> String {
        if !self.out.is_empty() && !self.out.ends_with('\n') {
            self.out.push('\n');
        }
        self.out
    }

    /// Writes the separator before the next node of the innermost collection.
    fn begin_node(&mut self) {
        let Some(frame) = self.frames.last() else {
            return;
        };
        if frame.after_key {
            self.out.push_str(": ");
        } else if frame.entries > 0 {
            self.out.push_str(", ");
        }
    }

    /// Records that a node of the innermost collection is complete.
    fn end_node(&mut self) {
        let Some(frame) = self.frames.last_mut() else {
            return;
        };
        match frame.kind {
            Kind::Mapping if !frame.after_key => frame.after_key = true,
            Kind::Mapping => {
                frame.after_key = false;
                frame.entries += 1;
            }
            Kind::Sequence | Kind::Set => frame.entries += 1,
        }
    }

    fn open(&mut self, kind: Kind, text: &str) {
        self.begin_node();
        self.out.push_str(text);
        self.frames.push(Frame {
            kind,
            entries: 0,
            after_key: false,
        });
    }

    fn close(&mut self, text: char) {
        self.frames.pop();
        self.out.push(text);
        self.end_node();
    }
}

fn is_set(tag: Option<&Tag>) -> bool {
    tag.is_some_and(|tag| tag.suffix == "set")
}

impl EventSink for FlowWriter {
    fn event(&mut self, event: Event<'_>) -> EmitResult<()> {
        match event {
            Event::DocumentStart(_) if self.explicit_start => self.out.push_str("---\n"),
            Event::Scalar(text, style, _, _) => {
                self.begin_node();
                if style == ScalarStyle::DoubleQuoted {
                    write_double_quoted(&mut self.out, &text);
                } else {
                    self.out.push_str(&text);
                }
                self.end_node();
            }
            Event::SequenceStart(..) => self.open(Kind::Sequence, "["),
            Event::MappingStart(_, tag) if is_set(tag.as_deref()) => {
                self.open(Kind::Set, "!!set {");
            }
            Event::MappingStart(..) => self.open(Kind::Mapping, "{"),
            Event::SequenceEnd => self.close(']'),
            Event::MappingEnd => self.close('}'),
            _ => {}
        }
        Ok(())
    }
}
