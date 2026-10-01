//! Rebuilds the token stream of `PyYAML`'s scanner from the node index and the source text.
//!
//! The node index holds scalars and aliases; what lies between them is only whitespace,
//! comments, indicators and node properties, which [`lex_gap`] reads. [`Synth`] then applies the
//! scanner's indent and simple-key rules to place the implicit block and key tokens.

use std::collections::VecDeque;

use fast_yaml_core::ScalarStyle;

use super::tokens::{Cursor, Kind, Mark, Token, real_end_line};
use crate::nodes::{Node, NodeIndex};

/// Longest key, in chars, `PyYAML` accepts before the `:`.
const MAX_SIMPLE_KEY: usize = 1024;

/// A piece of the source, with byte offsets.
#[derive(Debug, Clone, Copy)]
enum Lexeme {
    DocumentStart(usize),
    DocumentEnd(usize),
    Directive(usize, usize),
    FlowStart {
        sequence: bool,
        at: usize,
    },
    FlowEnd {
        sequence: bool,
        at: usize,
    },
    FlowEntry(usize),
    BlockEntry(usize),
    Key(usize),
    Value(usize),
    Anchor(usize, usize),
    Tag(usize, usize),
    Alias(usize, usize),
    BlockHeader(usize),
    Scalar {
        style: ScalarStyle,
        start: usize,
        end: usize,
        empty: bool,
    },
}

const fn is_blank(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

/// Reads the indicators and properties of `source[from..to]`, which holds no scalar.
fn lex_gap(source: &str, from: usize, to: usize, emit: &mut impl FnMut(Lexeme)) {
    let bytes = source.as_bytes();
    let mut at = from;
    while let Some((&byte, tail)) = bytes.get(at..to).and_then(<[u8]>::split_first) {
        let at_line_start = at == 0 || matches!(bytes.get(at - 1), Some(b'\n' | b'\r'));
        let marker_ends = |len: usize| bytes.get(at + len).is_none_or(|b| is_blank(*b));
        let rest = bytes.get(at..to).unwrap_or_default();
        match byte {
            b'#' | b'%' if byte == b'#' || at_line_start => {
                let len = rest
                    .iter()
                    .position(|b| matches!(b, b'\n' | b'\r'))
                    .unwrap_or(rest.len());
                if byte == b'%' {
                    emit(Lexeme::Directive(at, at + len));
                }
                at += len;
            }
            b'-' if at_line_start && rest.starts_with(b"---") && marker_ends(3) => {
                emit(Lexeme::DocumentStart(at));
                at += 3;
            }
            b'.' if at_line_start && rest.starts_with(b"...") && marker_ends(3) => {
                emit(Lexeme::DocumentEnd(at));
                at += 3;
            }
            b'-' => {
                emit(Lexeme::BlockEntry(at));
                at += 1;
            }
            b'?' => {
                emit(Lexeme::Key(at));
                at += 1;
            }
            b':' => {
                emit(Lexeme::Value(at));
                at += 1;
            }
            b',' => {
                emit(Lexeme::FlowEntry(at));
                at += 1;
            }
            b'[' | b'{' => {
                emit(Lexeme::FlowStart {
                    sequence: byte == b'[',
                    at,
                });
                at += 1;
            }
            b']' | b'}' => {
                emit(Lexeme::FlowEnd {
                    sequence: byte == b']',
                    at,
                });
                at += 1;
            }
            b'&' | b'!' => {
                let verbatim = rest.starts_with(b"!<");
                let len = rest
                    .iter()
                    .position(|b| {
                        if verbatim {
                            *b == b'>'
                        } else {
                            is_blank(*b) || matches!(b, b',' | b'[' | b']' | b'{' | b'}')
                        }
                    })
                    .map_or(rest.len(), |end| end + usize::from(verbatim));
                if byte == b'&' {
                    emit(Lexeme::Anchor(at, at + len));
                } else {
                    emit(Lexeme::Tag(at, at + len));
                }
                at += len;
            }
            b'|' | b'>' => {
                emit(Lexeme::BlockHeader(at));
                at += 1 + tail
                    .iter()
                    .take_while(|b| matches!(b, b'+' | b'-' | b'0'..=b'9'))
                    .count();
            }
            _ => at += 1,
        }
    }
}

/// Feeds the lexemes of `source` to `synth` in source order.
fn lex(
    source: &str,
    nodes: &NodeIndex<'_>,
    complete: bool,
    synth: &mut Synth<'_>,
    out: &mut impl FnMut(Token),
) {
    let mut header = None;
    let mut feed = |lexeme: Lexeme, synth: &mut Synth<'_>| match lexeme {
        Lexeme::BlockHeader(at) => header = Some(at),
        Lexeme::Scalar {
            style: style @ (ScalarStyle::Literal | ScalarStyle::Folded),
            start,
            end,
            empty,
        } => synth.feed(
            Lexeme::Scalar {
                style,
                start: header.take().unwrap_or(start),
                end,
                empty,
            },
            out,
        ),
        other => synth.feed(other, out),
    };
    let mut done = 0;
    for node in nodes.nodes() {
        let (lexeme, start, end) = match node {
            Node::Scalar(scalar) => {
                let (start, end) = (scalar.range.start().get(), scalar.range.end().get());
                let empty = nodes.text(scalar).is_empty();
                if empty && scalar.style == ScalarStyle::Plain {
                    continue;
                }
                let style = scalar.style;
                (
                    Lexeme::Scalar {
                        style,
                        start,
                        end,
                        empty,
                    },
                    start,
                    end,
                )
            }
            Node::Alias { range, .. } => {
                let (start, end) = (range.start().get(), range.end().get());
                (Lexeme::Alias(start, end), start, end)
            }
            Node::Open { .. } | Node::Close { .. } => continue,
        };
        if start < done {
            continue;
        }
        lex_gap(source, done, start, &mut |l| feed(l, synth));
        feed(lexeme, synth);
        done = end;
    }
    if complete {
        lex_gap(source, done, source.len(), &mut |l| feed(l, synth));
    }
}

/// A key that may turn out to be followed by `:`.
#[derive(Debug, Clone, Copy)]
struct SimpleKey {
    token_number: usize,
    required: bool,
    mark: Mark,
}

/// The state of `PyYAML`'s scanner that decides where implicit tokens go.
struct Synth<'a> {
    source: &'a str,
    cursor: Cursor<'a>,
    flow_level: usize,
    indent: isize,
    indents: Vec<isize>,
    allow_simple_key: bool,
    simple_keys: Vec<Option<SimpleKey>>,
    queue: VecDeque<Token>,
    taken: usize,
    last_line: usize,
    failed: bool,
}

fn signed(n: usize) -> isize {
    isize::try_from(n).unwrap_or(isize::MAX)
}

impl<'a> Synth<'a> {
    fn new(source: &'a str) -> Self {
        let mut cursor = Cursor::new(source);
        let origin = cursor.mark_at(0);
        Self {
            source,
            cursor,
            flow_level: 0,
            indent: -1,
            indents: Vec::new(),
            allow_simple_key: true,
            simple_keys: vec![None],
            queue: VecDeque::from([Token::new(Kind::StreamStart, origin, origin)]),
            taken: 0,
            last_line: 0,
            failed: false,
        }
    }

    fn fail(&mut self) {
        self.failed = true;
        self.queue.clear();
    }

    fn stale(&mut self, line: usize, index: usize) {
        let mut required_lost = false;
        for slot in &mut self.simple_keys {
            let Some(key) = *slot else {
                continue;
            };
            if key.mark.line != line || index.saturating_sub(key.mark.index) > MAX_SIMPLE_KEY {
                *slot = None;
                required_lost |= key.required;
            }
        }
        if required_lost {
            self.fail();
        }
    }

    fn unwind(&mut self, column: isize, mark: Mark) {
        if self.flow_level > 0 {
            return;
        }
        while self.indent > column {
            self.indent = self.indents.pop().unwrap_or(-1);
            self.queue.push_back(Token::new(Kind::BlockEnd, mark, mark));
        }
    }

    fn add_indent(&mut self, column: usize) -> bool {
        let column = signed(column);
        if self.indent < column {
            self.indents.push(self.indent);
            self.indent = column;
            true
        } else {
            false
        }
    }

    fn take_simple_key(&mut self) -> Option<SimpleKey> {
        self.simple_keys.last_mut().and_then(Option::take)
    }

    fn remove_simple_key(&mut self) {
        if self.take_simple_key().is_some_and(|key| key.required) {
            self.fail();
        }
    }

    fn save_simple_key(&mut self, mark: Mark) {
        let required = self.flow_level == 0 && self.indent == signed(mark.column);
        if self.allow_simple_key {
            self.remove_simple_key();
            let token_number = self.taken + self.queue.len();
            if let Some(slot) = self.simple_keys.last_mut() {
                *slot = Some(SimpleKey {
                    token_number,
                    required,
                    mark,
                });
            }
        }
    }

    fn push(&mut self, kind: Kind, start: Mark, end: Mark) {
        self.queue.push_back(Token::new(kind, start, end));
    }

    fn release(&mut self, out: &mut impl FnMut(Token)) {
        while !self.queue.is_empty() {
            let pending = self.simple_keys.iter().flatten().map(|k| k.token_number);
            if pending.min() == Some(self.taken) {
                break;
            }
            if let Some(token) = self.queue.pop_front() {
                self.taken += 1;
                out(token);
            }
        }
    }

    fn feed(&mut self, lexeme: Lexeme, out: &mut impl FnMut(Token)) {
        if self.failed {
            return;
        }
        let (from, to) = match lexeme {
            Lexeme::DocumentStart(at) | Lexeme::DocumentEnd(at) => (at, at + 3),
            Lexeme::FlowStart { at, .. }
            | Lexeme::FlowEnd { at, .. }
            | Lexeme::FlowEntry(at)
            | Lexeme::BlockEntry(at)
            | Lexeme::Key(at)
            | Lexeme::Value(at)
            | Lexeme::BlockHeader(at) => (at, at + 1),
            Lexeme::Directive(start, end)
            | Lexeme::Anchor(start, end)
            | Lexeme::Tag(start, end)
            | Lexeme::Alias(start, end)
            | Lexeme::Scalar { start, end, .. } => (start, end),
        };
        let start = self.cursor.mark_at(from);
        if self.flow_level == 0 && start.line > self.last_line {
            self.allow_simple_key = true;
        }
        self.stale(start.line, start.index);
        if self.failed {
            return;
        }
        self.unwind(signed(start.column), start);
        let end = self.cursor.mark_at(to);
        self.dispatch(lexeme, start, end);
        if self.failed {
            return;
        }
        self.last_line = end.line;
        self.stale(end.line, end.index);
        self.release(out);
    }

    fn dispatch(&mut self, lexeme: Lexeme, start: Mark, end: Mark) {
        match lexeme {
            Lexeme::DocumentStart(_) => self.marker(Kind::DocumentStart, start, end),
            Lexeme::DocumentEnd(_) => self.marker(Kind::DocumentEnd, start, end),
            Lexeme::Directive(..) => self.marker(Kind::Directive, start, end),
            Lexeme::FlowStart { sequence, .. } => {
                self.save_simple_key(start);
                self.flow_level += 1;
                self.simple_keys.push(None);
                self.allow_simple_key = true;
                let kind = if sequence {
                    Kind::FlowSequenceStart
                } else {
                    Kind::FlowMappingStart
                };
                self.push(kind, start, end);
            }
            Lexeme::FlowEnd { sequence, .. } => {
                self.remove_simple_key();
                if self.flow_level > 0 {
                    self.flow_level -= 1;
                    self.simple_keys.pop();
                }
                self.allow_simple_key = false;
                let kind = if sequence {
                    Kind::FlowSequenceEnd
                } else {
                    Kind::FlowMappingEnd
                };
                self.push(kind, start, end);
            }
            Lexeme::FlowEntry(_) => {
                self.remove_simple_key();
                self.allow_simple_key = true;
                self.push(Kind::FlowEntry, start, end);
            }
            Lexeme::BlockEntry(_) => {
                if self.open_block(Kind::BlockSequenceStart, start) {
                    self.allow_simple_key = true;
                    self.remove_simple_key();
                    self.push(Kind::BlockEntry, start, end);
                }
            }
            Lexeme::Key(_) => {
                if self.open_block(Kind::BlockMappingStart, start) {
                    self.allow_simple_key = self.flow_level == 0;
                    self.remove_simple_key();
                    self.push(Kind::Key { explicit: true }, start, end);
                }
            }
            Lexeme::Value(_) => self.value(start, end),
            Lexeme::Anchor(..) => self.property(Kind::Anchor, start, end),
            Lexeme::Tag(..) => self.property(Kind::Tag, start, end),
            Lexeme::Alias(..) => self.property(Kind::Alias, start, end),
            Lexeme::BlockHeader(_) => {}
            Lexeme::Scalar { style, empty, .. } => self.scalar(style, empty, start, end),
        }
    }

    fn marker(&mut self, kind: Kind, start: Mark, end: Mark) {
        self.unwind(-1, start);
        self.remove_simple_key();
        self.allow_simple_key = false;
        self.push(kind, start, end);
    }

    fn property(&mut self, kind: Kind, start: Mark, end: Mark) {
        self.save_simple_key(start);
        self.allow_simple_key = false;
        self.push(kind, start, end);
    }

    /// Opens the block collection an indicator at `start` belongs to; false if it cannot be here.
    fn open_block(&mut self, kind: Kind, start: Mark) -> bool {
        if self.flow_level == 0 {
            if !self.allow_simple_key {
                self.fail();
                return false;
            }
            if self.add_indent(start.column) {
                self.push(kind, start, start);
            }
        }
        true
    }

    fn scalar(&mut self, style: ScalarStyle, empty: bool, start: Mark, mut end: Mark) {
        if matches!(style, ScalarStyle::Literal | ScalarStyle::Folded) {
            self.remove_simple_key();
            self.allow_simple_key = true;
            let line_start = end.pointer.saturating_sub(end.column);
            let indent_only = self
                .source
                .get(line_start..end.pointer)
                .is_some_and(|text| text.bytes().all(|b| b == b' '));
            if end.pointer < self.source.len() && indent_only {
                end.pointer = line_start;
                end.index -= end.column;
                end.column = 0;
            }
        } else {
            self.save_simple_key(start);
            self.allow_simple_key = false;
        }
        let mut token = Token::new(Kind::Scalar { style, empty }, start, end);
        token.end_line = real_end_line(self.source, start, end);
        self.queue.push_back(token);
    }

    fn value(&mut self, start: Mark, end: Mark) {
        if let Some(key) = self.take_simple_key() {
            let at = key.token_number.saturating_sub(self.taken);
            self.queue.insert(
                at,
                Token::new(Kind::Key { explicit: false }, key.mark, key.mark),
            );
            if self.flow_level == 0 && self.add_indent(key.mark.column) {
                self.queue
                    .insert(at, Token::new(Kind::BlockMappingStart, key.mark, key.mark));
            }
            self.allow_simple_key = false;
        } else {
            if self.flow_level == 0 {
                if !self.allow_simple_key {
                    self.fail();
                    return;
                }
                if self.add_indent(start.column) {
                    self.push(Kind::BlockMappingStart, start, start);
                }
            }
            self.allow_simple_key = self.flow_level == 0;
        }
        self.push(Kind::Value, start, end);
    }

    fn finish(&mut self, out: &mut impl FnMut(Token)) {
        if self.failed {
            return;
        }
        let end = self.cursor.mark_at(self.source.len());
        self.unwind(-1, end);
        self.remove_simple_key();
        if self.failed {
            return;
        }
        self.allow_simple_key = false;
        self.simple_keys.clear();
        self.push(Kind::StreamEnd, end, end);
        self.release(out);
    }
}

/// Streams the tokens `PyYAML` would scan from `source`, whose scalars and aliases `nodes` lists.
///
/// Stops where `PyYAML`'s scanner would report an error. When the parser stopped early
/// (`complete` is false) the text after its last node is not read, since where it failed is
/// unknown.
pub(super) fn scan(
    source: &str,
    nodes: &NodeIndex<'_>,
    complete: bool,
    mut out: impl FnMut(Token),
) {
    let mut synth = Synth::new(source);
    synth.release(&mut out);
    lex(source, nodes, complete, &mut synth, &mut out);
    synth.finish(&mut out);
}
