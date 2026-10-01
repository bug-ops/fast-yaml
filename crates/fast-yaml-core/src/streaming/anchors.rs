//! Recovery of original anchor names from the source text between parser events.
//!
//! The parser reports anchors as numeric ids only. The `&name` of an anchored node lies in the
//! gap between the end of the previous event and the start of the node, among punctuation,
//! whitespace, comments, directives and other node properties, so it is found there instead of
//! by scanning the whole input (which cannot tell an anchor from `&x` inside a scalar).

// Parser markers count characters; the cursor below converts them to byte offsets.
#![allow(clippy::disallowed_methods)]

use saphyr_parser::{Event, Span};

/// Whether `c` ends an anchor name in saphyr's scanner (blank, flow indicator, NUL or BOM).
const fn is_anchor_terminator(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t' | '\r' | '\n' | ',' | '[' | ']' | '{' | '}' | '\0' | '\u{feff}'
    )
}

/// Whether `name` can be written back as `&name` and re-scanned as the same anchor.
fn is_valid_anchor_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains(|c: char| {
            is_anchor_terminator(c)
                || c.is_control()
                || c.is_whitespace()
                || matches!(c, '\u{fffe}' | '\u{ffff}')
        })
}

/// Returns the first anchor name in `gap`, skipping comments, directive lines and tags.
fn first_anchor(gap: &str) -> Option<&str> {
    let bytes = gap.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let token_start = i == 0
            || matches!(
                bytes[i - 1],
                b' ' | b'\t' | b'\r' | b'\n' | b'[' | b'{' | b','
            );
        let line_start = i == 0 || bytes[i - 1] == b'\n';
        let skip_to = |pred: fn(u8) -> bool| {
            bytes[i..]
                .iter()
                .position(|&b| pred(b))
                .map_or(bytes.len(), |n| i + n)
        };
        match bytes[i] {
            b'#' if i == 0 || bytes[i - 1].is_ascii_whitespace() => {
                i = skip_to(|b| b == b'\n');
            }
            b'%' if line_start => i = skip_to(|b| b == b'\n'),
            b'!' if token_start => i = skip_to(|b| b.is_ascii_whitespace()),
            b'&' if token_start => {
                let rest = &gap[i + 1..];
                let end = rest.find(is_anchor_terminator).unwrap_or(rest.len());
                return Some(&rest[..end]);
            }
            _ => i += 1,
        }
    }
    None
}

/// Tracks the end of the previous event and converts its character index to a byte offset.
///
/// Parser markers count characters; the cursor only moves forward, so the conversions over a
/// whole stream cost one pass over the source.
pub(super) struct AnchorNames<'a> {
    source: &'a str,
    byte: usize,
    chars: usize,
    prev_end: usize,
}

impl<'a> AnchorNames<'a> {
    pub(super) const fn new(source: &'a str) -> Self {
        Self {
            source,
            byte: 0,
            chars: 0,
            prev_end: 0,
        }
    }

    fn seek(&mut self, char_index: usize) -> usize {
        if char_index < self.chars {
            (self.byte, self.chars) = (0, 0);
        }
        for c in self.source[self.byte..].chars() {
            if self.chars == char_index {
                break;
            }
            self.byte += c.len_utf8();
            self.chars += 1;
        }
        self.byte
    }

    /// Returns the anchor name written between the previous event and the node whose event has
    /// `span`, or `None` when it cannot be recovered or re-emitted.
    pub(super) fn name_before(&mut self, span: Span) -> Option<&'a str> {
        self.name_before_index(span.start.index())
    }

    fn name_before_index(&mut self, node_start: usize) -> Option<&'a str> {
        let from = self.seek(self.prev_end);
        let to = self.seek(node_start.max(self.prev_end));
        first_anchor(&self.source[from..to]).filter(|name| is_valid_anchor_name(name))
    }

    /// Records the span of the event just handled as the start of the next gap.
    ///
    /// A document start reports the span of its own `---` or, for an implicit document, of the
    /// root node's leading properties, so the gap after it begins where that span begins.
    pub(super) fn advance(&mut self, event: &Event<'_>, span: Span) {
        self.prev_end = match event {
            Event::DocumentStart(_) => span.start.index(),
            _ => span.end.index(),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_anchor_skips_comments_tags_and_directives() {
        assert_eq!(first_anchor(": &x "), Some("x"));
        assert_eq!(first_anchor("- &a\n  "), Some("a"));
        assert_eq!(first_anchor("# &no\n  &yes "), Some("yes"));
        assert_eq!(first_anchor("%TAG !e! tag:&x,2000:\n--- &d "), Some("d"));
        assert_eq!(first_anchor("!t&no &yes "), Some("yes"));
        assert_eq!(first_anchor("!<tag:a&b> "), None);
        assert_eq!(first_anchor("[&a "), Some("a"));
        assert_eq!(first_anchor("a&b "), None);
        assert_eq!(first_anchor(": "), None);
    }

    #[test]
    fn invalid_names_are_not_recovered() {
        let mut names = AnchorNames::new("a: &\u{1}x v");
        assert_eq!(names.name_before_index(6), None);
    }

    #[test]
    fn cursor_moves_forward_and_resets_backwards() {
        let mut names = AnchorNames::new("é: &x 1\nü: &y 2\n");
        assert_eq!(names.name_before_index(5), Some("x"));
        names.prev_end = 8;
        assert_eq!(names.name_before_index(13), Some("y"));
        names.prev_end = 0;
        assert_eq!(names.name_before_index(5), Some("x"));
    }
}
