//! Conversion of parser line/column positions to source offsets.
//!
//! The parser's own marker index counts characters and drifts after non-ASCII directive names,
//! while line and column stay exact, so offsets are always derived from those two.

use saphyr_parser::Marker;

use crate::position::SourcePosition;

/// Char offsets at which each line starts, splitting like saphyr (`\n`, `\r\n`, lone `\r`).
pub fn line_starts(chars: &[char]) -> Vec<usize> {
    let mut starts = vec![0];
    for (i, &c) in chars.iter().enumerate() {
        let ends_line = c == '\n' || (c == '\r' && chars.get(i + 1) != Some(&'\n'));
        if ends_line {
            starts.push(i + 1);
        }
    }
    starts
}

/// Converts a line and column to a char offset, or `usize::MAX` past the last line.
pub fn char_offset(line_starts: &[usize], position: SourcePosition) -> usize {
    line_starts
        .get(position.line().index())
        .map_or(usize::MAX, |start| start + position.column().index())
}

type Chars<'t> = std::iter::Peekable<std::str::Chars<'t>>;

/// Line, column and char index while walking text the way the parser counts them.
struct Walk {
    index: usize,
    line: usize,
    col: usize,
}

impl Walk {
    fn take(&mut self, chars: &mut Chars<'_>) {
        let Some(c) = chars.next() else { return };
        self.index += 1;
        if c == '\n' || (c == '\r' && chars.peek() != Some(&'\n')) {
            self.line += 1;
            self.col = 0;
        } else {
            self.col += 1;
        }
    }

    fn take_while(&mut self, chars: &mut Chars<'_>, keep: impl Fn(char) -> bool) {
        while chars.peek().is_some_and(|&c| keep(c)) {
            self.take(chars);
        }
    }
}

/// Moves `start`, where `text` begins, past the leading `&anchor` and `!tag` tokens, comments and
/// blanks of a node, to the first character of its content.
///
/// saphyr 0.3 reports a node's span from its first property; positions in this crate refer to the
/// content, as they did before.
#[allow(
    clippy::disallowed_methods,
    reason = "advances the parser's own char-based marker"
)]
pub fn skip_properties(text: &str, start: Marker) -> Marker {
    let mut walk = Walk {
        index: start.index(),
        line: start.line(),
        col: start.col(),
    };
    let mut chars = text.chars().peekable();
    let token = |c: char| !matches!(c, ' ' | '\t' | '\r' | '\n' | ',' | '[' | ']' | '{' | '}');
    while let Some(&c) = chars.peek() {
        match c {
            ' ' | '\t' | '\r' | '\n' => walk.take(&mut chars),
            '#' => walk.take_while(&mut chars, |c| !matches!(c, '\n' | '\r')),
            '&' => walk.take_while(&mut chars, token),
            '!' => {
                walk.take(&mut chars);
                if chars.peek() == Some(&'<') {
                    walk.take_while(&mut chars, |c| c != '>');
                    walk.take(&mut chars);
                } else {
                    walk.take_while(&mut chars, token);
                }
            }
            _ => break,
        }
    }
    Marker::new(walk.index, walk.line, walk.col)
}

/// Forward-only converter from line/column to byte offsets in a source text.
///
/// Successive positions of a stream cost one pass over the source; a position before the
/// previous one restarts from the beginning, so any order is correct.
#[derive(Debug)]
pub struct ByteCursor<'a> {
    source: &'a str,
    line: usize,
    line_start: usize,
    column: usize,
    byte: usize,
}

impl<'a> ByteCursor<'a> {
    pub const fn new(source: &'a str) -> Self {
        Self {
            source,
            line: 0,
            line_start: 0,
            column: 0,
            byte: 0,
        }
    }

    /// Returns the byte offset of `position`, clamped to the end of the source.
    pub fn offset(&mut self, position: SourcePosition) -> usize {
        let (line, column) = (position.line().index(), position.column().index());
        if (line, column) < (self.line, self.column) {
            *self = Self::new(self.source);
        }
        while self.line < line {
            match self.next_line_start() {
                Some(start) => {
                    self.line += 1;
                    (self.line_start, self.column, self.byte) = (start, 0, start);
                }
                None => return self.source.len(),
            }
        }
        for c in self.source[self.byte..].chars() {
            if self.column >= column || matches!(c, '\n' | '\r') {
                break;
            }
            self.byte += c.len_utf8();
            self.column += 1;
        }
        self.byte
    }

    fn next_line_start(&self) -> Option<usize> {
        let bytes = self.source.as_bytes();
        let mut i = self.byte;
        while i < bytes.len() {
            match bytes[i] {
                b'\n' => return Some(i + 1),
                b'\r' if bytes.get(i + 1) != Some(&b'\n') => return Some(i + 1),
                _ => i += 1,
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::position::OneBased;

    const fn at(line: usize, column: usize) -> SourcePosition {
        SourcePosition::new(
            OneBased::from_index(line - 1),
            OneBased::from_index(column - 1),
        )
    }

    #[allow(
        clippy::disallowed_methods,
        reason = "reads the char-based marker back"
    )]
    fn skipped(text: &str) -> (usize, usize, usize) {
        let end = skip_properties(text, Marker::new(0, 1, 0));
        (end.index(), end.line(), end.col())
    }

    #[test]
    fn properties_comments_and_blanks_are_skipped_up_to_the_content() {
        assert_eq!(skipped("&a 1"), (3, 1, 3));
        assert_eq!(skipped("!!str &a x"), (9, 1, 9));
        assert_eq!(skipped("&a # c\n  !t\n  [1]"), (14, 3, 2));
        assert_eq!(skipped("!<tag:a,b> v"), (11, 1, 11));
        assert_eq!(skipped("&a\r\n  x"), (6, 2, 2));
        assert_eq!(skipped("&a\r  x"), (5, 2, 2));
        assert_eq!(skipped("&a[1]"), (2, 1, 2));
    }

    #[test]
    fn content_is_never_skipped() {
        assert_eq!(skipped("foo &x"), (0, 1, 0));
        assert_eq!(skipped("'&x'"), (0, 1, 0));
    }

    #[test]
    fn cursor_converts_non_ascii_columns_to_bytes() {
        let mut cursor = ByteCursor::new("é: &x 1\nü: &y 2\n");
        assert_eq!(cursor.offset(at(1, 5)), 5);
        assert_eq!(cursor.offset(at(2, 1)), 9);
        assert_eq!(cursor.offset(at(2, 5)), 14);
    }

    #[test]
    fn cursor_resets_when_moving_backwards() {
        let mut cursor = ByteCursor::new("a\nbb\nccc");
        assert_eq!(cursor.offset(at(3, 3)), 7);
        assert_eq!(cursor.offset(at(2, 2)), 3);
    }

    #[test]
    fn cursor_splits_lines_like_the_parser() {
        let mut cursor = ByteCursor::new("a\r\nb\rc\nd");
        assert_eq!(cursor.offset(at(2, 1)), 3);
        assert_eq!(cursor.offset(at(3, 1)), 5);
        assert_eq!(cursor.offset(at(4, 1)), 7);
    }

    #[test]
    fn cursor_clamps_past_the_end() {
        let mut cursor = ByteCursor::new("ab\n");
        assert_eq!(cursor.offset(at(9, 1)), 3);
        assert_eq!(cursor.offset(at(1, 99)), 2);
    }

    #[test]
    fn char_offset_matches_line_starts() {
        let chars: Vec<char> = "ab\ncd".chars().collect();
        let starts = line_starts(&chars);
        assert_eq!(char_offset(&starts, at(2, 2)), 4);
        assert_eq!(char_offset(&starts, at(3, 1)), usize::MAX);
    }
}
