//! Document boundary detection and chunking.

#![allow(clippy::redundant_pub_crate)]

/// Represents a document chunk with metadata.
#[derive(Debug, Clone)]
pub(crate) struct Chunk<'a> {
    /// Zero-based index of this document in the stream.
    pub index: usize,

    /// Source text for this document (includes `---` prefix if present).
    pub content: &'a str,

    /// Byte offset of this chunk in the original input.
    #[allow(dead_code)]
    pub offset: usize,
}

/// Splits YAML input into document chunks at `---` boundaries.
///
/// The chunk list mirrors the document stream of `Parser::parse_all`:
/// - Every `---` marker starts a document, even when its body is empty (parsed as null)
/// - Text before the first `---` is a document only if it has content beyond blank lines,
///   comments, and directives
/// - Input without markers is a single document unless it is completely empty
///
/// # Performance
///
/// O(n) in input length with zero-copy slicing.
pub(crate) fn chunk_documents(input: &str) -> Vec<Chunk<'_>> {
    if input.is_empty() {
        return Vec::new();
    }

    let separator_positions = find_document_separators(input);

    let Some(&first_separator) = separator_positions.first() else {
        return vec![Chunk {
            index: 0,
            content: input,
            offset: 0,
        }];
    };

    let mut chunks = Vec::with_capacity(separator_positions.len() + 1);

    let prefix = &input[..first_separator];
    let prefix_kind = classify_prefix(prefix);
    if prefix_kind == PrefixKind::Content {
        chunks.push(Chunk {
            index: 0,
            content: prefix,
            offset: 0,
        });
    }

    for (i, &separator) in separator_positions.iter().enumerate() {
        let end = separator_positions
            .get(i + 1)
            .copied()
            .unwrap_or(input.len());

        // Directives apply to the next document, so they stay attached to it.
        let start = if i == 0 && prefix_kind == PrefixKind::Directives {
            0
        } else {
            separator
        };

        chunks.push(Chunk {
            index: chunks.len(),
            content: &input[start..end],
            offset: start,
        });
    }

    chunks
}

/// What precedes the first `---` marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrefixKind {
    /// Only blank lines and comments.
    Blank,
    /// Blank lines, comments, and at least one `%` directive.
    Directives,
    /// Node content: an implicit first document.
    Content,
}

fn classify_prefix(prefix: &str) -> PrefixKind {
    let mut kind = PrefixKind::Blank;
    for line in prefix.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if line.starts_with('%') {
            kind = PrefixKind::Directives;
        } else {
            return PrefixKind::Content;
        }
    }
    kind
}

/// Finds byte positions of all `---` document separators.
///
/// Returns sorted vector of byte offsets where separators occur.
fn find_document_separators(input: &str) -> Vec<usize> {
    // Estimate: ~1 separator per 1KB in typical multi-doc files
    let estimated_separators = (input.len() / 1024).max(1);
    let mut positions = Vec::with_capacity(estimated_separators);

    for (line_start, line) in LineOffsets::new(input) {
        // Only a column-0 marker separates documents; indented `---` is scalar content.
        if let Some(after_dashes) = line.strip_prefix("---")
            && (after_dashes.is_empty() || after_dashes.starts_with(|c: char| c.is_whitespace()))
        {
            positions.push(line_start);
        }
    }

    positions
}

/// Iterator over line byte offsets.
struct LineOffsets<'a> {
    input: &'a str,
    offset: usize,
}

impl<'a> LineOffsets<'a> {
    #[inline]
    const fn new(input: &'a str) -> Self {
        Self { input, offset: 0 }
    }
}

impl<'a> Iterator for LineOffsets<'a> {
    type Item = (usize, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.input.len() {
            return None;
        }

        let remaining = &self.input[self.offset..];
        let line_end = remaining
            .find('\n')
            .map_or(self.input.len(), |pos| self.offset + pos + 1);

        let line = &self.input[self.offset..line_end];
        let offset = self.offset;
        self.offset = line_end;

        Some((offset, line))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_single_document() {
        let yaml = "foo: 1\nbar: 2";
        let chunks = chunk_documents(yaml);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].index, 0);
        assert_eq!(chunks[0].content, yaml);
    }

    #[test]
    fn test_chunk_explicit_multi_document() {
        let yaml = "---\nfoo: 1\n---\nbar: 2";
        let chunks = chunk_documents(yaml);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].index, 0);
        assert_eq!(chunks[1].index, 1);
    }

    #[test]
    fn test_chunk_implicit_first_document() {
        let yaml = "implicit: true\n---\nexplicit: true";
        let chunks = chunk_documents(yaml);
        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].content.contains("implicit"));
    }

    #[test]
    fn test_chunk_empty_documents() {
        let yaml = "---\n\n---\nvalid: true";
        let chunks = chunk_documents(yaml);
        assert_eq!(chunks.len(), 2);
        assert!(chunks[1].content.contains("valid"));
    }

    #[test]
    fn test_chunk_preserves_offsets() {
        let yaml = "first\n---\nsecond";
        let chunks = chunk_documents(yaml);
        assert_eq!(chunks[0].offset, 0);
        assert_eq!(chunks[1].offset, 6); // "first\n" = 6 bytes
    }

    #[test]
    fn test_chunk_empty_input() {
        let yaml = "";
        let chunks = chunk_documents(yaml);
        assert_eq!(chunks.len(), 0);
    }

    #[test]
    fn test_chunk_only_separator() {
        let yaml = "---";
        let chunks = chunk_documents(yaml);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_chunk_separator_with_spaces() {
        let yaml = "---   \nfoo: 1";
        let chunks = chunk_documents(yaml);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_chunk_not_separator_in_value() {
        // "---" in middle of line should not be treated as separator
        let yaml = "key: ---value\n---\nfoo: 1";
        let chunks = chunk_documents(yaml);
        assert_eq!(chunks.len(), 2);
    }

    #[test]
    fn test_line_offsets_iterator() {
        let input = "line1\nline2\nline3";
        let lines: Vec<_> = LineOffsets::new(input).collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], (0, "line1\n"));
        assert_eq!(lines[1], (6, "line2\n"));
        assert_eq!(lines[2], (12, "line3"));
    }

    #[test]
    fn test_chunk_multiple_separators_no_content() {
        let yaml = "---\n---\n---\n";
        let chunks = chunk_documents(yaml);
        // Each marker starts a document
        assert_eq!(chunks.len(), 3);
    }

    #[test]
    fn test_chunk_separator_at_end() {
        let yaml = "foo: 1\n---";
        let chunks = chunk_documents(yaml);
        assert_eq!(chunks.len(), 2);
    }

    #[test]
    fn test_chunk_unicode_separator() {
        let yaml = "---\nключ: значение\n---\n日本語: テスト";
        let chunks = chunk_documents(yaml);
        assert_eq!(chunks.len(), 2);
    }

    #[test]
    fn test_chunk_separator_with_comment() {
        let yaml = "---  # comment\nfoo: 1";
        let chunks = chunk_documents(yaml);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_chunk_indented_separator_not_recognized() {
        let yaml = "  ---\nfoo: 1";
        let chunks = chunk_documents(yaml);
        // Indented --- should not be separator
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].content, yaml);
    }

    #[test]
    fn test_chunk_separator_in_middle_of_line() {
        let yaml = "key: ---\nfoo: 1";
        let chunks = chunk_documents(yaml);
        // --- in middle of line should not be separator
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_chunk_multiple_docs_with_content() {
        let yaml = "---\nfirst: 1\n---\nsecond: 2\n---\nthird: 3";
        let chunks = chunk_documents(yaml);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].index, 0);
        assert_eq!(chunks[1].index, 1);
        assert_eq!(chunks[2].index, 2);
    }

    #[test]
    fn test_chunk_whitespace_before_separator() {
        let yaml = "\n\n---\nfoo: 1";
        let chunks = chunk_documents(yaml);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_chunk_tabs_before_separator() {
        let yaml = "\t---\nfoo: 1";
        let chunks = chunk_documents(yaml);
        // Tab before separator means it's indented
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_find_document_separators_empty() {
        let input = "";
        let positions = find_document_separators(input);
        assert_eq!(positions.len(), 0);
    }

    #[test]
    fn test_find_document_separators_no_separators() {
        let input = "foo: 1\nbar: 2";
        let positions = find_document_separators(input);
        assert_eq!(positions.len(), 0);
    }

    #[test]
    fn test_find_document_separators_single() {
        let input = "---\nfoo: 1";
        let positions = find_document_separators(input);
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0], 0);
    }

    #[test]
    fn test_find_document_separators_multiple() {
        let input = "---\nfoo: 1\n---\nbar: 2\n---\nbaz: 3";
        let positions = find_document_separators(input);
        assert_eq!(positions.len(), 3);
    }

    #[test]
    fn test_line_offsets_empty() {
        let input = "";
        assert_eq!(LineOffsets::new(input).count(), 0);
    }

    #[test]
    fn test_line_offsets_single_line_no_newline() {
        let input = "single line";
        let lines: Vec<_> = LineOffsets::new(input).collect();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], (0, "single line"));
    }

    #[test]
    fn test_line_offsets_single_line_with_newline() {
        let input = "single line\n";
        let lines: Vec<_> = LineOffsets::new(input).collect();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], (0, "single line\n"));
    }

    #[test]
    fn test_chunk_crlf_line_endings() {
        let yaml = "---\r\nfoo: 1\r\n---\r\nbar: 2";
        let chunks = chunk_documents(yaml);
        assert_eq!(chunks.len(), 2);
    }

    #[test]
    fn test_chunk_mixed_line_endings() {
        let yaml = "---\nfoo: 1\r\n---\r\nbar: 2\n---\nbaz: 3";
        let chunks = chunk_documents(yaml);
        assert_eq!(chunks.len(), 3);
    }
}
