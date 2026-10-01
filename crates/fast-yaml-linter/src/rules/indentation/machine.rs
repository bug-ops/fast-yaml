//! The indentation check of yamllint, run over the tokens of [`super::super::token_stream::scanner`].

use std::collections::VecDeque;

use fast_yaml_core::ScalarStyle;

use crate::config::IndentSequences;
use crate::rules::token_stream::tokens::{Kind, Token};

/// A finding of the machine, with a 1-based line and column and a byte offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Problem {
    pub line: usize,
    pub column: usize,
    pub offset: usize,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParentKind {
    Root,
    BlockMap,
    FlowMap,
    BlockSeq,
    FlowSeq,
    BlockEntry,
    Key,
    Val,
}

#[derive(Debug, Clone, Copy)]
struct Parent {
    kind: ParentKind,
    indent: isize,
    line_indent: Option<usize>,
    explicit_key: bool,
    implicit_block_seq: bool,
}

impl Parent {
    const fn new(kind: ParentKind, indent: isize) -> Self {
        Self {
            kind,
            indent,
            line_indent: None,
            explicit_key: false,
            implicit_block_seq: false,
        }
    }
}

/// The token stream is not shaped the way the check expects.
struct Unexpected;

fn signed(n: usize) -> isize {
    isize::try_from(n).unwrap_or(isize::MAX)
}

fn is_kind(token: Option<&Token>, kinds: &[Kind]) -> bool {
    token.is_some_and(|t| kinds.contains(&t.kind))
}

/// Walks the tokens once, keeping yamllint's stack of enclosing structures.
pub(super) struct Machine<'a> {
    source: &'a str,
    check_multi_line_strings: bool,
    spaces: Option<isize>,
    sequences: IndentSequences,
    stack: Vec<Parent>,
    cur_line: usize,
    cur_line_indent: usize,
    prev: Option<Token>,
    window: VecDeque<Token>,
    problems: Vec<Problem>,
}

impl<'a> Machine<'a> {
    pub(super) fn new(
        source: &'a str,
        spaces: Option<usize>,
        sequences: IndentSequences,
        check_multi_line_strings: bool,
    ) -> Self {
        Self {
            source,
            check_multi_line_strings,
            spaces: spaces.map(signed),
            sequences,
            stack: vec![Parent::new(ParentKind::Root, 0)],
            cur_line: 0,
            cur_line_indent: 0,
            prev: None,
            window: VecDeque::with_capacity(3),
            problems: Vec::new(),
        }
    }

    pub(super) fn push(&mut self, token: Token) {
        self.window.push_back(token);
        if self.window.len() == 3 {
            self.step();
        }
    }

    pub(super) fn finish(mut self) -> Vec<Problem> {
        while !self.window.is_empty() {
            self.step();
        }
        self.problems
    }

    fn step(&mut self) {
        let Some(token) = self.window.pop_front() else {
            return;
        };
        let (next, nextnext) = (self.window.front().copied(), self.window.get(1).copied());
        let prev = self.prev;
        if self
            .check(&token, prev.as_ref(), next.as_ref(), nextnext.as_ref())
            .is_err()
        {
            self.problems.push(Problem {
                line: token.start.line + 1,
                column: token.start.column + 1,
                offset: token.start.pointer,
                message: "cannot infer indentation: unexpected token".to_owned(),
            });
        }
        self.prev = Some(token);
    }

    fn top(&self) -> Result<Parent, Unexpected> {
        self.stack.last().copied().ok_or(Unexpected)
    }

    fn below_top(&self) -> Result<Parent, Unexpected> {
        self.stack
            .len()
            .checked_sub(2)
            .and_then(|at| self.stack.get(at))
            .copied()
            .ok_or(Unexpected)
    }

    fn detect_indent(&mut self, base: isize, next: &Token) -> isize {
        let spaces = *self
            .spaces
            .get_or_insert_with(|| signed(next.start.column) - base);
        base + spaces
    }

    fn check(
        &mut self,
        token: &Token,
        prev: Option<&Token>,
        next: Option<&Token>,
        nextnext: Option<&Token>,
    ) -> Result<(), Unexpected> {
        let first_in_line = token.is_visible() && token.start.line + 1 > self.cur_line;

        if first_in_line {
            let found = token.start.column;
            let top = self.top()?;
            let mut expected = top.indent;
            if matches!(token.kind, Kind::FlowMappingEnd | Kind::FlowSequenceEnd) {
                expected = top.line_indent.map_or(expected, signed);
            } else if top.kind == ParentKind::Key && top.explicit_key && token.kind != Kind::Value {
                expected = self.detect_indent(expected, token);
            }
            if signed(found) != expected {
                let message = if expected < 0 {
                    format!("wrong indentation: expected at least {}", found + 1)
                } else {
                    format!("wrong indentation: expected {expected} but found {found}")
                };
                self.problems.push(Problem {
                    line: token.start.line + 1,
                    column: found + 1,
                    offset: token.start.pointer,
                    message,
                });
            }
        }

        if matches!(token.kind, Kind::Scalar { .. }) && self.check_multi_line_strings {
            self.check_scalar(token)?;
        }

        if token.is_visible() {
            self.cur_line = token.end_line;
            if first_in_line {
                self.cur_line_indent = token.start.column;
            }
        }

        self.update(token, prev, next, nextnext)?;
        self.unwind(token, next)
    }

    fn update(
        &mut self,
        token: &Token,
        prev: Option<&Token>,
        next: Option<&Token>,
        nextnext: Option<&Token>,
    ) -> Result<(), Unexpected> {
        match token.kind {
            Kind::BlockMappingStart => {
                let next = next.ok_or(Unexpected)?;
                if !matches!(next.kind, Kind::Key { .. }) || next.start.line != token.start.line {
                    return Err(Unexpected);
                }
                let indent = signed(token.start.column);
                self.stack.push(Parent::new(ParentKind::BlockMap, indent));
            }
            Kind::FlowMappingStart | Kind::FlowSequenceStart => {
                let next = next.ok_or(Unexpected)?;
                let indent = if next.start.line == token.start.line {
                    signed(next.start.column)
                } else {
                    self.detect_indent(signed(self.cur_line_indent), next)
                };
                let kind = if token.kind == Kind::FlowMappingStart {
                    ParentKind::FlowMap
                } else {
                    ParentKind::FlowSeq
                };
                let mut parent = Parent::new(kind, indent);
                parent.line_indent = Some(self.cur_line_indent);
                self.stack.push(parent);
            }
            Kind::BlockSequenceStart => {
                let next = next.ok_or(Unexpected)?;
                if next.kind != Kind::BlockEntry || next.start.line != token.start.line {
                    return Err(Unexpected);
                }
                let indent = signed(token.start.column);
                self.stack.push(Parent::new(ParentKind::BlockSeq, indent));
            }
            Kind::BlockEntry if !is_kind(next, &[Kind::BlockEntry, Kind::BlockEnd]) => {
                let next = next.ok_or(Unexpected)?;
                if self.top()?.kind != ParentKind::BlockSeq {
                    let mut parent = Parent::new(ParentKind::BlockSeq, signed(token.start.column));
                    parent.implicit_block_seq = true;
                    self.stack.push(parent);
                }
                let same_line = next.start.line == token.end.line;
                let same_column = next.start.column == token.start.column;
                let indent = if same_line || same_column {
                    signed(next.start.column)
                } else {
                    self.detect_indent(signed(token.start.column), next)
                };
                self.stack.push(Parent::new(ParentKind::BlockEntry, indent));
            }
            Kind::Key { explicit } => {
                let mut parent = Parent::new(ParentKind::Key, self.top()?.indent);
                parent.explicit_key = explicit;
                self.stack.push(parent);
            }
            Kind::Value => self.update_value(prev, next, nextnext)?,
            _ => {}
        }
        Ok(())
    }

    fn update_value(
        &mut self,
        prev: Option<&Token>,
        next: Option<&Token>,
        nextnext: Option<&Token>,
    ) -> Result<(), Unexpected> {
        let top = self.top()?;
        if top.kind != ParentKind::Key {
            return Err(Unexpected);
        }
        let mut next = next.ok_or(Unexpected)?;
        if matches!(next.kind, Kind::Anchor | Kind::Tag) {
            let prev = prev.ok_or(Unexpected)?;
            let nextnext = nextnext.ok_or(Unexpected)?;
            if next.start.line == prev.start.line && next.start.line < nextnext.start.line {
                next = nextnext;
            }
        }
        if matches!(
            next.kind,
            Kind::BlockEnd | Kind::FlowMappingEnd | Kind::FlowSequenceEnd | Kind::Key { .. }
        ) {
            return Ok(());
        }
        let prev = prev.ok_or(Unexpected)?;
        let next_column = signed(next.start.column);
        let indent = if top.explicit_key {
            self.detect_indent(top.indent, next)
        } else if next.start.line == prev.start.line {
            next_column
        } else if matches!(next.kind, Kind::BlockSequenceStart | Kind::BlockEntry) {
            match self.sequences {
                IndentSequences::NotIndented => top.indent,
                IndentSequences::Indented => {
                    if self.spaces.is_none() && next_column == top.indent {
                        -1
                    } else {
                        self.detect_indent(top.indent, next)
                    }
                }
                IndentSequences::Whatever | IndentSequences::Consistent => {
                    let consistent = self.sequences == IndentSequences::Consistent;
                    if next_column == top.indent {
                        if consistent {
                            self.sequences = IndentSequences::NotIndented;
                        }
                        top.indent
                    } else {
                        if consistent {
                            self.sequences = IndentSequences::Indented;
                        }
                        self.detect_indent(top.indent, next)
                    }
                }
            }
        } else {
            self.detect_indent(top.indent, next)
        };
        self.stack.push(Parent::new(ParentKind::Val, indent));
        Ok(())
    }

    fn unwind(&mut self, token: &Token, next: Option<&Token>) -> Result<(), Unexpected> {
        let mut consumed = false;
        loop {
            let top = self.top()?;
            let pop_one = match top.kind {
                ParentKind::FlowSeq if token.kind == Kind::FlowSequenceEnd && !consumed => {
                    consumed = true;
                    true
                }
                ParentKind::FlowMap if token.kind == Kind::FlowMappingEnd && !consumed => {
                    consumed = true;
                    true
                }
                ParentKind::BlockMap | ParentKind::BlockSeq
                    if token.kind == Kind::BlockEnd && !top.implicit_block_seq && !consumed =>
                {
                    consumed = true;
                    true
                }
                ParentKind::BlockEntry
                    if token.kind != Kind::BlockEntry
                        && self.below_top()?.implicit_block_seq
                        && !matches!(token.kind, Kind::Anchor | Kind::Tag)
                        && !is_kind(next, &[Kind::BlockEntry]) =>
                {
                    self.stack.pop();
                    true
                }
                ParentKind::BlockEntry if is_kind(next, &[Kind::BlockEntry, Kind::BlockEnd]) => {
                    true
                }
                ParentKind::Val
                    if token.kind != Kind::Value
                        && !matches!(token.kind, Kind::Anchor | Kind::Tag) =>
                {
                    if self.below_top()?.kind != ParentKind::Key {
                        return Err(Unexpected);
                    }
                    self.stack.pop();
                    true
                }
                ParentKind::Key
                    if matches!(
                        next.map(|t| t.kind),
                        Some(
                            Kind::BlockEnd
                                | Kind::FlowMappingEnd
                                | Kind::FlowSequenceEnd
                                | Kind::Key { .. }
                        )
                    ) =>
                {
                    true
                }
                _ => return Ok(()),
            };
            if pop_one {
                self.stack.pop();
            }
        }
    }

    fn check_scalar(&mut self, token: &Token) -> Result<(), Unexpected> {
        let Kind::Scalar { style, .. } = token.kind else {
            return Ok(());
        };
        if token.start.line == token.end.line {
            return Ok(());
        }
        let bytes = self.source.as_bytes();
        let limit = self
            .source
            .floor_char_boundary(token.end.pointer.saturating_sub(1));
        let mut expected: Option<isize> = None;
        let mut line_no = token.start.line + 1;
        let mut line_start = token.start.pointer;
        loop {
            let newline = self
                .source
                .get(line_start..limit)
                .and_then(|s| s.find('\n'));
            let Some(newline) = newline else {
                return Ok(());
            };
            line_start += newline + 1;
            line_no += 1;
            let indent = bytes
                .get(line_start..)
                .unwrap_or_default()
                .iter()
                .take_while(|b| **b == b' ')
                .count();
            if matches!(bytes.get(line_start + indent), Some(b'\n') | None) {
                continue;
            }
            let wanted = if let Some(wanted) = expected {
                wanted
            } else {
                let wanted = self.expected_scalar_indent(token, style, signed(indent))?;
                expected = Some(wanted);
                wanted
            };
            if signed(indent) != wanted {
                self.problems.push(Problem {
                    line: line_no,
                    column: indent + 1,
                    offset: line_start + indent,
                    message: format!("wrong indentation: expected {wanted} but found {indent}"),
                });
            }
        }
    }

    fn expected_scalar_indent(
        &mut self,
        token: &Token,
        style: ScalarStyle,
        found: isize,
    ) -> Result<isize, Unexpected> {
        let column = signed(token.start.column);
        let mut detect = |base: isize| {
            let spaces = *self.spaces.get_or_insert(found - base);
            base + spaces
        };
        Ok(match style {
            ScalarStyle::Plain => column,
            ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted => column + 1,
            ScalarStyle::Literal | ScalarStyle::Folded => {
                let top = self.stack.last().copied().ok_or(Unexpected)?;
                let below = self
                    .stack
                    .len()
                    .checked_sub(2)
                    .and_then(|at| self.stack.get(at))
                    .copied();
                match top.kind {
                    ParentKind::BlockEntry => detect(column),
                    ParentKind::Key if top.explicit_key => detect(column),
                    ParentKind::Key => return Err(Unexpected),
                    ParentKind::Val => {
                        let below = below.ok_or(Unexpected)?;
                        if token.start.line + 1 > self.cur_line {
                            detect(top.indent)
                        } else if below.explicit_key {
                            detect(column)
                        } else {
                            detect(below.indent)
                        }
                    }
                    _ => detect(top.indent),
                }
            }
        })
    }
}
