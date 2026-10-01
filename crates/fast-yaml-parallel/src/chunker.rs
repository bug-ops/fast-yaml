//! Document boundary detection and chunking.

#![allow(clippy::redundant_pub_crate)]

use fast_yaml_core::NormalizedInput;
use fast_yaml_core::limits::MaxDocuments;

use crate::error::{Error, Result};

/// Position of a chunk in the whole input, used to relocate parse error marks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SourceOrigin {
    /// Line breaks before the chunk.
    pub line: usize,
}

/// Represents a document chunk with metadata.
#[derive(Debug, Clone)]
pub(crate) struct Chunk<'a> {
    /// Zero-based index of this chunk in the stream.
    pub index: usize,

    /// Normalized text of this chunk (includes `---` prefix if present).
    pub input: NormalizedInput<'a>,

    /// Where the chunk starts in the original input.
    pub origin: SourceOrigin,
}

/// A byte position together with its line coordinate.
#[derive(Debug, Clone, Copy)]
struct Cursor {
    byte: usize,
    origin: SourceOrigin,
}

/// What a line means for document boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineKind {
    DocStart,
    DocEnd,
    Directive,
    Blank,
    Content,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Inside a document; only `---` and `...` are significant.
    Document,
    /// Before the first document or after `...`; directives and a new document may follow.
    AfterEnd,
}

/// Splits normalized YAML input into chunks that each parse independently of the others.
///
/// The chunk list mirrors the document stream of `Parser::parse_all`:
/// - Lines end with `\n`, `\r\n` or a lone `\r`
/// - A column-0 `---` or `...` followed by a space, tab or line end is a marker
/// - `---` starts a document, even when its body is empty
/// - After `...` a `%` directive block belongs to the next document, and any other content
///   starts one implicitly; trailing comments stay with the previous chunk
/// - Input without boundaries is a single chunk unless it is completely empty
/// - A column-0 `---` inside a top-level block scalar still splits, unlike `saphyr` (#407)
///
/// # Performance
///
/// O(n) in input length with zero-copy slicing.
///
/// # Errors
///
/// Returns [`Error::TooManyDocuments`] as soon as the chunk count would pass `max`, so an
/// input of millions of empty documents is never fully materialized.
pub(crate) fn chunk_documents<'a>(
    whole: &'a NormalizedInput<'_>,
    max: Option<MaxDocuments>,
) -> Result<Vec<Chunk<'a>>> {
    let input = whole.as_str();
    if input.is_empty() {
        return Ok(Vec::new());
    }
    let admit = |count: usize| match max {
        Some(limit) if count >= limit.get() => Err(Error::TooManyDocuments {
            count: count + 1,
            limit,
        }),
        _ => Ok(()),
    };
    let part = |range: std::ops::Range<usize>| {
        whole
            .slice(range)
            .unwrap_or_else(|| unreachable!("chunks start at line starts"))
    };

    let mut chunks = Vec::new();
    let mut state = State::AfterEnd;
    let mut start = Cursor {
        byte: 0,
        origin: SourceOrigin::default(),
    };
    let mut has_doc = false;
    let mut pending_directives: Option<Cursor> = None;
    let mut here = start;

    for line in Lines::new(input) {
        let boundary = match (state, classify(line.text)) {
            (State::Document, LineKind::DocStart) => Some(here),
            (State::Document, LineKind::DocEnd) => {
                state = State::AfterEnd;
                None
            }
            (State::Document, _) | (State::AfterEnd, LineKind::Blank) => None,
            (State::AfterEnd, LineKind::DocEnd) => {
                pending_directives = None;
                None
            }
            (State::AfterEnd, LineKind::Directive) => {
                pending_directives.get_or_insert(here);
                None
            }
            (State::AfterEnd, LineKind::DocStart | LineKind::Content) => {
                Some(pending_directives.unwrap_or(here))
            }
        };

        if let Some(boundary) = boundary {
            if has_doc {
                admit(chunks.len())?;
                chunks.push(Chunk {
                    index: chunks.len(),
                    input: part(start.byte..boundary.byte),
                    origin: start.origin,
                });
                start = boundary;
            }
            has_doc = true;
            state = State::Document;
            pending_directives = None;
        }

        here.byte += line.len;
        here.origin.line += 1;
    }

    admit(chunks.len())?;
    chunks.push(Chunk {
        index: chunks.len(),
        input: part(start.byte..input.len()),
        origin: start.origin,
    });

    Ok(chunks)
}

fn classify(text: &str) -> LineKind {
    let is_marker = |marker: &str| {
        text.strip_prefix(marker)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t']))
    };
    if is_marker("---") {
        LineKind::DocStart
    } else if is_marker("...") {
        LineKind::DocEnd
    } else if text.starts_with('%') {
        LineKind::Directive
    } else {
        let trimmed = text.trim_start_matches([' ', '\t']);
        if trimmed.is_empty() || trimmed.starts_with('#') {
            LineKind::Blank
        } else {
            LineKind::Content
        }
    }
}

/// One input line: its text without terminator plus its byte length including it.
struct Line<'a> {
    text: &'a str,
    len: usize,
}

/// Iterator over lines terminated by `\n`, `\r\n` or a lone `\r`.
struct Lines<'a> {
    rest: &'a str,
}

impl<'a> Lines<'a> {
    const fn new(input: &'a str) -> Self {
        Self { rest: input }
    }
}

impl<'a> Iterator for Lines<'a> {
    type Item = Line<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.rest.is_empty() {
            return None;
        }
        let (text, terminator_len) = match self.rest.find(['\n', '\r']) {
            Some(pos) => {
                let crlf = self.rest[pos..].starts_with("\r\n");
                (&self.rest[..pos], if crlf { 2 } else { 1 })
            }
            None => (self.rest, 0),
        };
        let len = text.len() + terminator_len;
        self.rest = &self.rest[len..];
        Some(Line { text, len })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normalized(input: &str) -> NormalizedInput<'_> {
        NormalizedInput::new(input).unwrap()
    }

    fn contents(input: &str) -> Vec<String> {
        let whole = normalized(input);
        chunk_documents(&whole, None)
            .unwrap()
            .iter()
            .map(|c| c.input.as_str().to_owned())
            .collect()
    }

    #[test]
    fn test_chunk_single_document() {
        let yaml = "foo: 1\nbar: 2";
        let whole = normalized(yaml);
        let chunks = chunk_documents(&whole, None).unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].index, 0);
        assert_eq!(chunks[0].input.as_str(), yaml);
    }

    #[test]
    fn test_chunk_explicit_multi_document() {
        let whole = normalized("---\nfoo: 1\n---\nbar: 2");
        let chunks = chunk_documents(&whole, None).unwrap();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].index, 0);
        assert_eq!(chunks[1].index, 1);
    }

    #[test]
    fn test_chunk_implicit_first_document() {
        assert_eq!(
            contents("implicit: true\n---\nexplicit: true"),
            ["implicit: true\n", "---\nexplicit: true"]
        );
    }

    #[test]
    fn test_chunk_empty_documents() {
        assert_eq!(contents("---\n\n---\nvalid: true").len(), 2);
    }

    #[test]
    fn test_chunk_origin() {
        let whole = normalized("first\n---\nsécond\n---\nthird");
        let chunks = chunk_documents(&whole, None).unwrap();
        assert_eq!(chunks[1].origin, SourceOrigin { line: 1 });
        assert_eq!(chunks[2].origin, SourceOrigin { line: 3 });
    }

    #[test]
    fn test_chunk_empty_input() {
        assert!(chunk_documents(&normalized(""), None).unwrap().is_empty());
    }

    #[test]
    fn test_chunk_only_separator() {
        assert_eq!(contents("---"), ["---"]);
    }

    #[test]
    fn test_chunk_separator_with_spaces_and_comment() {
        assert_eq!(contents("---   \nfoo: 1").len(), 1);
        assert_eq!(contents("---  # comment\nfoo: 1").len(), 1);
    }

    #[test]
    fn test_chunk_marker_lookalikes_are_content() {
        assert_eq!(contents("key: ---value\n---\nfoo: 1").len(), 2);
        assert_eq!(contents("key: ---\nfoo: 1").len(), 1);
        assert_eq!(contents("  ---\nfoo: 1").len(), 1);
        assert_eq!(contents("\t---\nfoo: 1").len(), 1);
        assert_eq!(contents("a\n---x\nb").len(), 1);
    }

    #[test]
    fn test_chunk_marker_followed_by_nbsp_or_nel_is_not_a_marker() {
        assert_eq!(contents("a: 1\n---\u{A0}b\n").len(), 1);
        assert_eq!(contents("a: 1\n---\u{85}b\n").len(), 1);
    }

    #[test]
    fn test_chunk_multiple_separators_no_content() {
        assert_eq!(contents("---\n---\n---\n").len(), 3);
    }

    #[test]
    fn test_chunk_separator_at_end() {
        assert_eq!(contents("foo: 1\n---").len(), 2);
    }

    #[test]
    fn test_chunk_unicode_separator() {
        assert_eq!(
            contents("---\nключ: значение\n---\n日本語: テスト").len(),
            2
        );
    }

    #[test]
    fn test_chunk_first_chunk_keeps_preamble() {
        assert_eq!(contents("\n\n---\nfoo: 1"), ["\n\n---\nfoo: 1"]);
    }

    #[test]
    fn test_chunk_comment_only_input_is_single_chunk() {
        assert_eq!(contents("# c\n"), ["# c\n"]);
    }

    #[test]
    fn test_chunk_line_endings() {
        assert_eq!(contents("---\r\nfoo: 1\r\n---\r\nbar: 2").len(), 2);
        assert_eq!(
            contents("---\nfoo: 1\r\n---\r\nbar: 2\n---\nbaz: 3").len(),
            3
        );
        assert_eq!(
            contents("---\ra: 1\r---\rb: 2\r"),
            ["---\ra: 1\r", "---\rb: 2\r"]
        );
        assert_eq!(contents("a\r...\rb"), ["a\r...\r", "b"]);
    }

    #[test]
    fn test_chunk_document_end_marker() {
        assert_eq!(contents("a\n...\nb"), ["a\n...\n", "b"]);
        assert_eq!(contents("a\n... # c\nb"), ["a\n... # c\n", "b"]);
        assert_eq!(contents("a\n...\n...\nb"), ["a\n...\n...\n", "b"]);
        assert_eq!(
            contents("--- |\nfoo\n...\nbar"),
            ["--- |\nfoo\n...\n", "bar"]
        );
        assert_eq!(contents("a\n...x\n"), ["a\n...x\n"]);
    }

    #[test]
    fn test_chunk_trailing_comment_after_end_stays_attached() {
        assert_eq!(contents("a: 1\n...\n# c\n"), ["a: 1\n...\n# c\n"]);
    }

    #[test]
    fn test_chunk_directives_after_end_attach_to_next_chunk() {
        assert_eq!(
            contents("a\n...\n# c\n%YAML 1.2\n%TAG !e! tag:x,2000:\n---\nb"),
            ["a\n...\n# c\n", "%YAML 1.2\n%TAG !e! tag:x,2000:\n---\nb"]
        );
        assert_eq!(
            contents("%YAML 1.2\n---\na\n...\n%YAML 1.2\n---\nb\n"),
            ["%YAML 1.2\n---\na\n...\n", "%YAML 1.2\n---\nb\n"]
        );
    }

    #[test]
    fn test_chunk_percent_line_inside_document_is_content() {
        assert_eq!(contents("a\n%YAML 1.2\n---\nb").len(), 2);
        assert_eq!(contents("a\n%YAML 1.2\n---\nb")[0], "a\n%YAML 1.2\n");
    }

    #[test]
    fn test_chunk_documents_stops_at_the_limit() {
        let text = "---\n".repeat(10_000);
        let input = normalized(&text);
        let limit = |n| Some(MaxDocuments::new(n).unwrap());
        let err = chunk_documents(&input, limit(3)).unwrap_err();
        assert!(
            matches!(err, Error::TooManyDocuments { count: 4, .. }),
            "{err:?}"
        );
        assert_eq!(
            chunk_documents(&input, limit(10_000)).unwrap().len(),
            10_000
        );
        let two = normalized("a\n---\nb\n");
        assert_eq!(chunk_documents(&two, limit(2)).unwrap().len(), 2);
        assert!(chunk_documents(&two, limit(1)).is_err());
    }
}
