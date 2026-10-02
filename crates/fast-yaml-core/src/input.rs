//! Parser input that has been validated and stripped of prefix byte order marks.
//!
//! Every entry point that hands text to the YAML parser goes through [`NormalizedInput::new`],
//! which is the only way to obtain one. The parser, the loader and the streaming formatter accept
//! nothing else, so text can neither skip validation nor be normalized twice.

use crate::DocumentIndex;
use std::borrow::Cow;
use std::ops::Range;

use crate::error::{ParseError, ParseResult, SourcePosition, SyntaxError};
use crate::limits::{DocumentCursor, MaxScanAhead};
use crate::scalar::is_c_printable;
use crate::scan_guard::GuardedParser;

const BOM: char = '\u{FEFF}';

/// Byte length of U+FEFF in UTF-8.
const BOM_LEN: usize = BOM.len_utf8();

/// YAML text that contains only printable characters and no byte order mark in a document prefix.
///
/// Normalization does two things:
///
/// 1. Every character outside the YAML 1.2.2 `c-printable` set (for example NUL, DEL, C1
///    controls, U+FFFE) is rejected with a [`ParseError::Syntax`] at its position.
/// 2. Each U+FEFF that YAML 1.2.2 section 9.1.1 allows in a document prefix is removed: one at
///    the start of the stream, one at the start of a line after a `...` line (blank and comment
///    lines between are skipped), and one at the start of a line that, after blank and comment
///    lines, holds a `%` directive or a `---` marker. A U+FEFF anywhere else is content.
///
/// Line and column numbers, including that of a rejected character, count characters of the
/// normalized text, so a line that lost a BOM reads as if the BOM were absent, like the leading
/// BOM always has; [`original_offset`](Self::original_offset) maps byte offsets back.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::NormalizedInput;
///
/// let input = NormalizedInput::new("\u{FEFF}a: 1\n...\n\u{FEFF}---\nb: 2\n")?;
/// assert_eq!(input.as_str(), "a: 1\n...\n---\nb: 2\n");
/// assert_eq!(input.original_offset(input.as_str().len()), input.as_str().len() + 6);
/// assert!(NormalizedInput::new("a: \u{7F}").is_err());
/// # Ok::<(), fast_yaml_core::ParseError>(())
/// ```
#[derive(Debug, Clone)]
pub struct NormalizedInput<'a> {
    text: Cow<'a, str>,
    /// Offsets in `text` at which a BOM was removed, ascending.
    removed_at: Vec<usize>,
    original_len: usize,
}

impl<'a> NormalizedInput<'a> {
    /// Validates `input` and strips its prefix byte order marks.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::Syntax`] positioned at the first character outside the YAML
    /// printable set.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{NormalizedInput, ParseError};
    ///
    /// assert!(NormalizedInput::new("a: 1\n").is_ok());
    /// let Err(ParseError::Syntax(err)) = NormalizedInput::new("a: 1\nb: \u{7F}") else {
    ///     unreachable!()
    /// };
    /// assert_eq!((err.line(), err.column()), (2, 4));
    /// ```
    pub fn new(input: &'a str) -> ParseResult<Self> {
        let (text, removed_at) = strip_prefix_boms(input);
        if let Some((offset, c)) = first_non_printable(&text) {
            return Err(ParseError::Syntax(SyntaxError::invalid_character(
                c,
                position_at(&text, offset),
                document_at(text.get(..offset).unwrap_or_default()),
            )));
        }
        Ok(Self {
            text,
            removed_at,
            original_len: input.len(),
        })
    }

    /// Parses the whole text once to check that no construct makes the scanner read more than
    /// `max` characters past the last node it reported; see [`MaxScanAhead`].
    ///
    /// Callers that hand the text to a parser configured with the same limit get this check
    /// from the parser itself; this is for code that scans the text another way.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::LimitExceeded`] with [`LimitKind::ScanAhead`](crate::LimitKind::ScanAhead)
    /// when the limit is exceeded. Syntax errors are not reported: they are the caller's own
    /// parse to find.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{MaxScanAhead, NormalizedInput};
    ///
    /// let input = NormalizedInput::new("[1, 2, 3, 4, 5, 6, 7, 8]")?;
    /// assert!(input.check_scan_ahead(MaxScanAhead::DEFAULT).is_ok());
    /// assert!(input.check_scan_ahead(MaxScanAhead::new(4)?).is_err());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn check_scan_ahead(&self, max: MaxScanAhead) -> ParseResult<()> {
        for event in self.scanner(max) {
            match event {
                Err(error @ ParseError::LimitExceeded { .. }) => return Err(error),
                Err(_) => return Ok(()),
                Ok(_) => {}
            }
        }
        Ok(())
    }

    /// The parser every crate-internal consumer of this text drives.
    pub(crate) fn scanner(&self, max: MaxScanAhead) -> GuardedParser<'_> {
        GuardedParser::new(&self.text, max)
    }

    /// Returns the normalized text.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::NormalizedInput;
    ///
    /// let input = NormalizedInput::new("\u{FEFF}a: 1\n")?;
    /// assert_eq!(input.as_str(), "a: 1\n");
    /// # Ok::<(), fast_yaml_core::ParseError>(())
    /// ```
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Returns the byte length of the text this input was made from.
    ///
    /// Zero only for an empty original, even when normalization left nothing (a lone BOM).
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::NormalizedInput;
    ///
    /// let input = NormalizedInput::new("\u{FEFF}")?;
    /// assert_eq!((input.as_str().len(), input.original_len()), (0, 3));
    /// # Ok::<(), fast_yaml_core::ParseError>(())
    /// ```
    #[must_use]
    pub const fn original_len(&self) -> usize {
        self.original_len
    }

    /// Maps a byte offset in the normalized text to the same position in the original text,
    /// counting every removed BOM at or before it.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::NormalizedInput;
    ///
    /// let input = NormalizedInput::new("a\n...\n\u{FEFF}b: 1\n")?;
    /// assert_eq!(input.original_offset(0), 0);
    /// let b = input.as_str().find('b').unwrap();
    /// assert_eq!(input.original_offset(b), b + 3);
    /// # Ok::<(), fast_yaml_core::ParseError>(())
    /// ```
    #[must_use]
    pub fn original_offset(&self, offset: usize) -> usize {
        offset
            + BOM_LEN
                * self
                    .removed_at
                    .partition_point(|&removed| removed <= offset)
    }

    /// Returns the part of this input in `range`, without validating or stripping again.
    ///
    /// Returns `None` unless `range` lies within the text on character boundaries and starts at
    /// the beginning of a line, which keeps error columns of the part valid. The part does not
    /// track removed BOMs.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::NormalizedInput;
    ///
    /// let input = NormalizedInput::new("a: 1\n---\nb: 2\n")?;
    /// let second = input.slice(5..input.as_str().len()).unwrap();
    /// assert_eq!(second.as_str(), "---\nb: 2\n");
    /// assert!(input.slice(1..4).is_none(), "a part must start at a line start");
    /// # Ok::<(), fast_yaml_core::ParseError>(())
    /// ```
    #[must_use]
    pub fn slice(&self, range: Range<usize>) -> Option<NormalizedInput<'_>> {
        let text = self.text.get(range.clone())?;
        let at_line_start = range.start == 0
            || self
                .text
                .as_bytes()
                .get(range.start - 1)
                .is_some_and(|b| matches!(b, b'\n' | b'\r'));
        at_line_start.then(|| NormalizedInput {
            original_len: text.len(),
            text: Cow::Borrowed(text),
            removed_at: Vec::new(),
        })
    }
}

const SCAN_CHUNK: usize = 16;

const fn is_plain_ascii(b: u8) -> bool {
    matches!(b, b'\t' | b'\n' | b'\r' | 0x20..=0x7E)
}

/// Offset of the first byte that is not printable ASCII, scanning in chunks the optimizer vectorizes.
fn first_non_plain_ascii(bytes: &[u8]) -> Option<usize> {
    let (chunks, tail) = bytes.as_chunks::<SCAN_CHUNK>();
    for (n, chunk) in chunks.iter().enumerate() {
        if !chunk.iter().all(|&b| is_plain_ascii(b)) {
            return chunk
                .iter()
                .position(|&b| !is_plain_ascii(b))
                .map(|i| n * SCAN_CHUNK + i);
        }
    }
    let start = bytes.len() - tail.len();
    tail.iter()
        .position(|&b| !is_plain_ascii(b))
        .map(|i| start + i)
}

/// Finds the first character outside `c-printable`, with its byte offset.
fn first_non_printable(text: &str) -> Option<(usize, char)> {
    let mut base = 0;
    while let Some(rest) = text.get(base..) {
        let at = first_non_plain_ascii(rest.as_bytes())?;
        let offset = base + at;
        let c = text.get(offset..)?.chars().next()?;
        if !is_c_printable(c) {
            return Some((offset, c));
        }
        base = offset + c.len_utf8();
    }
    None
}

/// Characters the document-index scan may read past a node: the scan runs on the error path of
/// input that the caller's limit never saw, so it gets a small budget of its own.
const DOCUMENT_SCAN_BUDGET: MaxScanAhead = match MaxScanAhead::new(64 * 1024) {
    Ok(budget) => budget,
    Err(_) => MaxScanAhead::DEFAULT,
};

/// Index of the document the text after `prefix` belongs to, as scanner errors count it.
///
/// When the budget trips, the events seen so far give the document the last node was in, and the
/// column-0 `---` lines after it give the documents that followed. That count is a heuristic: it
/// also counts such lines inside block scalars and quotes and ignores lone `\r` line breaks, so on
/// this error path the reported document can be off.
fn document_at(prefix: &str) -> DocumentIndex {
    let mut parser = GuardedParser::new(prefix, DOCUMENT_SCAN_BUDGET);
    let mut cursor = DocumentCursor::default();
    let end = position_at(prefix, prefix.len());
    while let Some(event) = parser.next_event() {
        match event {
            Ok((event, span)) => {
                // The parser closes the truncated prefix with a DocumentEnd at its end; the text after it is not past that document.
                let at = SourcePosition::from_span(span);
                if matches!(event, saphyr_parser::Event::DocumentEnd)
                    && (at.line, at.column) >= (end.line, end.column)
                {
                    break;
                }
                cursor.observe(&event);
            }
            Err(ParseError::LimitExceeded { .. }) => {
                let markers = prefix
                    .lines()
                    .skip(parser.last_position().line)
                    .filter(|line| is_document_start(line))
                    .count();
                return DocumentIndex::new(
                    cursor.index() + markers.saturating_sub(usize::from(!cursor.is_open())),
                );
            }
            Err(_) => break,
        }
    }
    DocumentIndex::new(cursor.index())
}

fn is_document_start(line: &str) -> bool {
    line.strip_prefix("---")
        .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t']))
}

/// Line and column (1-indexed, in characters) of a byte offset, counting `\n`, `\r\n` and lone
/// `\r` as one line break each.
fn position_at(text: &str, offset: usize) -> SourcePosition {
    let (mut line, mut column) = (1, 1);
    let mut prev = None;
    for c in text.get(..offset).unwrap_or_default().chars() {
        match c {
            '\n' => (line, column) = (line + usize::from(prev != Some('\r')), 1),
            '\r' => (line, column) = (line + 1, 1),
            _ => column += 1,
        }
        prev = Some(c);
    }
    SourcePosition { line, column }
}

/// What a line holds, as far as BOM placement cares.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LineKind {
    Blank,
    Comment,
    /// A `%` directive or a `---` marker.
    Marker,
    /// A `...` marker.
    DocumentEnd,
    Other,
}

fn classify(body: &str) -> LineKind {
    let is_marker = |prefix: &str| {
        body.strip_prefix(prefix)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t']))
    };
    let trimmed = body.trim_start_matches([' ', '\t']);
    if trimmed.is_empty() {
        LineKind::Blank
    } else if trimmed.starts_with('#') {
        LineKind::Comment
    } else if body.starts_with('%') || is_marker("---") {
        LineKind::Marker
    } else if is_marker("...") {
        LineKind::DocumentEnd
    } else {
        LineKind::Other
    }
}

/// Removes the prefix BOMs; returns the text and the normalized offsets where BOMs were removed.
fn strip_prefix_boms(input: &str) -> (Cow<'_, str>, Vec<usize>) {
    if !input.contains(BOM) {
        return (Cow::Borrowed(input), Vec::new());
    }
    let strip = prefix_bom_offsets(input);
    if strip.is_empty() {
        return (Cow::Borrowed(input), Vec::new());
    }
    let mut text = String::with_capacity(input.len() - BOM_LEN * strip.len());
    let mut removed_at = Vec::with_capacity(strip.len());
    let mut from = 0;
    for &offset in &strip {
        text.push_str(input.get(from..offset).unwrap_or_default());
        removed_at.push(text.len());
        from = offset + BOM_LEN;
    }
    text.push_str(input.get(from..).unwrap_or_default());
    (Cow::Owned(text), removed_at)
}

/// Byte offsets of the BOMs that sit in a document prefix, ascending.
fn prefix_bom_offsets(input: &str) -> Vec<usize> {
    let bytes = input.as_bytes();
    let mut strip = Vec::new();
    // BOM lines whose fate depends on the next line that is neither blank nor a comment.
    let mut pending = Vec::new();
    let mut after_document_end = false;
    let mut pos = 0;
    while pos < input.len() {
        let rest = bytes.get(pos..).unwrap_or_default();
        let len = memchr::memchr2(b'\n', b'\r', rest).unwrap_or(rest.len());
        let terminator = if rest.get(len..).is_some_and(|r| r.starts_with(b"\r\n")) {
            2
        } else {
            usize::from(len < rest.len())
        };
        let line = input.get(pos..pos + len).unwrap_or_default();
        let (has_bom, body) = line
            .strip_prefix(BOM)
            .map_or((false, line), |body| (true, body));
        match classify(body) {
            LineKind::Blank | LineKind::Comment => {
                if has_bom {
                    if pos == 0 || after_document_end {
                        strip.push(pos);
                    } else {
                        pending.push(pos);
                    }
                }
            }
            kind => {
                if kind == LineKind::Marker {
                    strip.append(&mut pending);
                }
                pending.clear();
                if has_bom && (pos == 0 || after_document_end || kind == LineKind::Marker) {
                    strip.push(pos);
                }
                after_document_end = kind == LineKind::DocumentEnd;
            }
        }
        pos += len + terminator;
    }
    strip
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normalized(input: &str) -> String {
        NormalizedInput::new(input).unwrap().as_str().to_owned()
    }

    const B: char = BOM;

    #[test]
    fn printable_input_is_borrowed_unchanged() {
        let input = NormalizedInput::new("a: 1\n\tb: \u{85}\u{A0}\u{10FFFF}\r\n").unwrap();
        assert!(matches!(input.text, Cow::Borrowed(_)));
        assert_eq!(input.original_offset(5), 5);
    }

    #[test]
    fn rejected_characters_report_the_document_they_are_in() {
        for (text, document) in [
            ("a: \0\n", 0),
            ("a: 1\n---\nb: \x7f\n", 1),
            ("a: 1\n---\nb: 2\n---\nc: \x01", 2),
            ("\u{FEFF}a: 1\n...\n\u{FEFF}b: \u{7F}", 1),
            ("a: 1\n...\n\x01", 1),
            ("---\n\x01", 0),
        ] {
            let err = NormalizedInput::new(text).unwrap_err();
            assert_eq!(err.document_index().get(), document, "{text:?}");
        }
    }

    #[test]
    fn non_printable_characters_are_rejected_with_exact_positions() {
        for (text, c, line, column) in [
            ("a: 1\0\n", '\0', 1, 5),
            ("a: 1\n\u{7F}", '\u{7F}', 2, 1),
            ("\u{FEFF}a: \u{86}", '\u{86}', 1, 4),
            ("a: 1\n...\n\u{FEFF}b: \u{7F}", '\u{7F}', 3, 4),
            ("a\r\nb: \u{FFFE}", '\u{FFFE}', 2, 4),
            ("# é\n\u{FFFF}", '\u{FFFF}', 2, 1),
            ("a: \u{1}", '\u{1}', 1, 4),
            ("a\rb\r c\0", '\0', 3, 3),
        ] {
            let ParseError::Syntax(err) = NormalizedInput::new(text).unwrap_err() else {
                panic!("syntax error expected for {text:?}");
            };
            assert_eq!((err.line(), err.column()), (line, column), "{text:?}");
            let shown = err.to_string();
            assert!(
                shown.contains(&format!("{:04X}", u32::from(c))) || c == '\0',
                "{shown}"
            );
        }
    }

    #[test]
    fn stream_leading_bom_is_stripped_once() {
        assert_eq!(normalized(&format!("{B}a: 1")), "a: 1");
        assert_eq!(normalized(&format!("{B}{B}a: 1")), format!("{B}a: 1"));
        assert_eq!(normalized(&format!("{B}")), "");
        assert_eq!(normalized(&format!("{B}\n{B}a")), format!("\n{B}a"));
    }

    #[test]
    fn bom_after_document_end_is_a_prefix_bom() {
        assert_eq!(normalized(&format!("a\n...\n{B}b: 1")), "a\n...\nb: 1");
        assert_eq!(
            normalized(&format!("a\n...\n\n# c\n{B}b: 1")),
            "a\n...\n\n# c\nb: 1"
        );
        assert_eq!(
            normalized(&format!("a\n...\n{B}# c\n{B}b")),
            "a\n...\n# c\nb"
        );
        assert_eq!(normalized(&format!("a\n{B}b: 1")), format!("a\n{B}b: 1"));
    }

    #[test]
    fn bom_before_directive_or_marker_is_a_prefix_bom() {
        assert_eq!(
            normalized(&format!("a\n{B}%YAML 1.2\n---\nb")),
            "a\n%YAML 1.2\n---\nb"
        );
        assert_eq!(normalized(&format!("a\n{B}---\nb")), "a\n---\nb");
        assert_eq!(normalized(&format!("a\n{B}--- y")), "a\n--- y");
        assert_eq!(
            normalized(&format!("a\n{B}# c\n\n{B}\n{B}---\nb")),
            "a\n# c\n\n\n---\nb"
        );
        assert_eq!(
            normalized(&format!("a\n{B}# c\nb")),
            format!("a\n{B}# c\nb")
        );
        assert_eq!(
            normalized(&format!("a\n{B}----\nb")),
            format!("a\n{B}----\nb")
        );
    }

    #[test]
    fn bom_in_the_middle_of_a_line_is_content() {
        let input = format!("a: {B}1\n...\nb: {B}2\n");
        assert_eq!(normalized(&input), input);
    }

    #[test]
    fn original_offsets_count_every_removed_bom() {
        let text = format!("{B}a\n...\n{B}---\nb");
        let input = NormalizedInput::new(&text).unwrap();
        assert_eq!(input.as_str(), "a\n...\n---\nb");
        assert_eq!(input.original_offset(0), 3);
        assert_eq!(input.original_offset(1), 4);
        assert_eq!(input.original_offset(6), 6 + 3 + 3);
        assert_eq!(
            input.original_offset(input.as_str().len()),
            input.as_str().len() + 6
        );
    }

    #[test]
    fn crlf_and_lone_cr_lines_are_handled() {
        assert_eq!(normalized(&format!("a\r\n...\r\n{B}b")), "a\r\n...\r\nb");
        assert_eq!(normalized(&format!("a\r...\r{B}b")), "a\r...\rb");
    }

    #[test]
    fn original_len_survives_a_bom_only_input() {
        let text = format!("{B}");
        let input = NormalizedInput::new(&text).unwrap();
        assert_eq!(input.as_str(), "");
        assert_eq!(input.original_len(), 3);
        assert_eq!(NormalizedInput::new("").unwrap().original_len(), 0);
    }

    #[test]
    fn slices_must_start_at_a_line_start_inside_the_text() {
        let input = NormalizedInput::new("a: 1\n---\nb: 2\n").unwrap();
        assert_eq!(input.slice(5..9).unwrap().as_str(), "---\n");
        assert_eq!(input.slice(0..4).unwrap().as_str(), "a: 1");
        assert!(input.slice(1..4).is_none());
        assert!(input.slice(5..99).is_none());
        let multibyte = NormalizedInput::new("é\nb").unwrap();
        assert!(multibyte.slice(1..3).is_none());
        assert_eq!(multibyte.slice(3..4).unwrap().original_len(), 1);
    }

    #[test]
    fn many_bom_comment_lines_are_linear() {
        let input = format!("a\n{}", format!("{B}# c\n").repeat(50_000));
        assert_eq!(NormalizedInput::new(&input).unwrap().as_str(), input);
    }

    fn control_at(len: usize, pos: usize, filler: &str) -> String {
        let mut text = filler.repeat(len);
        text.insert(pos, '\u{1}');
        text
    }

    #[test]
    fn control_character_is_found_at_chunk_boundaries() {
        for len in [15, 16, 17, 32, 33] {
            for pos in [0, 15, 16, 17] {
                if pos > len {
                    continue;
                }
                let text = control_at(len, pos, "a");
                assert_eq!(
                    first_non_printable(&text),
                    Some((pos, '\u{1}')),
                    "len {len} pos {pos}"
                );
            }
        }
    }

    #[test]
    fn printable_text_of_chunk_sized_lengths_is_accepted() {
        for len in [0, 15, 16, 17, 32, 33] {
            assert_eq!(first_non_printable(&"a".repeat(len)), None, "len {len}");
        }
    }

    #[test]
    fn non_ascii_char_at_chunk_seam_is_decoded_whole() {
        for pad in [14, 15, 16, 17] {
            let text = format!("{}\u{e9}\u{2028}x", "a".repeat(pad));
            assert_eq!(first_non_printable(&text), None, "pad {pad}");
            let text = format!("{}\u{e9}\u{FFFE}", "a".repeat(pad));
            assert_eq!(
                first_non_printable(&text),
                Some((pad + 2, '\u{FFFE}')),
                "pad {pad}"
            );
        }
    }

    #[test]
    fn multibyte_char_before_control_in_one_chunk() {
        let text = "\u{e9}\u{4e2d}\u{1F600}\u{7}tail";
        assert_eq!(first_non_printable(text), Some((9, '\u{7}')));
    }

    #[test]
    fn delete_and_c1_controls_are_rejected() {
        assert_eq!(first_non_printable("ab\u{7F}"), Some((2, '\u{7F}')));
        assert_eq!(first_non_printable("ab\u{80}"), Some((2, '\u{80}')));
        assert_eq!(first_non_printable("tab\there\r\n"), None);
    }
}
