//! Conversion of parser line/column positions to source offsets.
//!
//! The parser's own marker index counts characters and drifts after non-ASCII directive names,
//! while line and column stay exact, so offsets are always derived from those two.

use crate::error::SourcePosition;

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
        .get(position.line.wrapping_sub(1))
        .map_or(usize::MAX, |start| {
            start + position.column.saturating_sub(1)
        })
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
            line: 1,
            line_start: 0,
            column: 1,
            byte: 0,
        }
    }

    /// Returns the byte offset of `position`, clamped to the end of the source.
    pub fn offset(&mut self, position: SourcePosition) -> usize {
        if (position.line, position.column) < (self.line, self.column) {
            *self = Self::new(self.source);
        }
        while self.line < position.line {
            match self.next_line_start() {
                Some(start) => {
                    self.line += 1;
                    (self.line_start, self.column, self.byte) = (start, 1, start);
                }
                None => return self.source.len(),
            }
        }
        for c in self.source[self.byte..].chars() {
            if self.column >= position.column || matches!(c, '\n' | '\r') {
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

    const fn at(line: usize, column: usize) -> SourcePosition {
        SourcePosition { line, column }
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
