//! `PyYAML`-compatible tokens and the position cursor that stamps them.

use fast_yaml_core::ScalarStyle;

/// A position in the source; `line` and `column` count from 0, `column` in chars.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mark {
    pub index: usize,
    pub pointer: usize,
    pub line: usize,
    pub column: usize,
}

/// What a token is, with the details the indentation check reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    StreamStart,
    StreamEnd,
    DocumentStart,
    DocumentEnd,
    Directive,
    BlockSequenceStart,
    BlockMappingStart,
    BlockEnd,
    FlowSequenceStart,
    FlowSequenceEnd,
    FlowMappingStart,
    FlowMappingEnd,
    BlockEntry,
    FlowEntry,
    Key { explicit: bool },
    Value,
    Anchor,
    Tag,
    Alias,
    Scalar { style: ScalarStyle, empty: bool },
}

/// A token with the marks of its first and last character.
#[derive(Debug, Clone, Copy)]
pub struct Token {
    pub kind: Kind,
    pub start: Mark,
    pub end: Mark,
    /// 1-based line the token really ends on; scalars do not count trailing blank lines.
    pub end_line: usize,
}

impl Token {
    pub const fn new(kind: Kind, start: Mark, end: Mark) -> Self {
        Self {
            kind,
            start,
            end,
            end_line: end.line + 1,
        }
    }

    pub const fn is_visible(&self) -> bool {
        !matches!(
            self.kind,
            Kind::StreamStart | Kind::StreamEnd | Kind::BlockEnd | Kind::Scalar { empty: true, .. }
        )
    }
}

/// Walks the source forward and tells the mark of any later byte offset.
pub struct Cursor<'a> {
    source: &'a str,
    mark: Mark,
}

const fn is_break(c: char) -> bool {
    matches!(c, '\n' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

impl<'a> Cursor<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            mark: Mark::default(),
        }
    }

    /// The mark at byte offset `pointer`, which must not lie before an earlier one.
    pub fn mark_at(&mut self, pointer: usize) -> Mark {
        let Some(span) = self.source.get(self.mark.pointer..pointer) else {
            return self.mark;
        };
        let mut chars = span.chars().peekable();
        while let Some(c) = chars.next() {
            self.mark.index += 1;
            if is_break(c) || (c == '\r' && chars.peek() != Some(&'\n')) {
                self.mark.line += 1;
                self.mark.column = 0;
            } else {
                self.mark.column += 1;
            }
        }
        self.mark.pointer = pointer;
        self.mark
    }
}

/// The line a scalar really ends on: the line of its last non-blank char.
pub fn real_end_line(source: &str, start: Mark, end: Mark) -> usize {
    let bytes = source.as_bytes();
    let mut line = end.line + 1;
    let mut pos = end.pointer;
    while pos > 0 && pos >= start.pointer {
        pos -= 1;
        match bytes.get(pos) {
            Some(b'\n') => line = line.saturating_sub(1),
            Some(b' ' | b'\t' | b'\r' | 0x0b | 0x0c) => {}
            _ => break,
        }
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_counts_chars_and_breaks() {
        let source = "ключ: 1\r\nb: 2\rc\u{2028}d";
        let mut cursor = Cursor::new(source);
        let at = |needle: &str| source.find(needle).unwrap();
        let one = cursor.mark_at(at("1"));
        assert_eq!((one.line, one.column, one.index), (0, 6, 6));
        let b = cursor.mark_at(at("b"));
        assert_eq!((b.line, b.column), (1, 0));
        let c = cursor.mark_at(at("c"));
        assert_eq!((c.line, c.column), (2, 0));
        let d = cursor.mark_at(at("d"));
        assert_eq!((d.line, d.column), (3, 0));
    }

    #[test]
    fn real_end_line_skips_trailing_blank_lines() {
        let source = "a: |\n  x\n\n\nb: 1\n";
        let mut cursor = Cursor::new(source);
        let start = cursor.mark_at(7);
        let end = cursor.mark_at(source.find('b').unwrap());
        assert_eq!(end.line, 4);
        assert_eq!(real_end_line(source, start, end), 2);
    }
}
