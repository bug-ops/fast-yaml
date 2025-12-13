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
/// Handles edge cases:
/// - Implicit first document (no leading `---`)
/// - Trailing whitespace and comments
/// - Empty documents (creates empty chunk)
/// - Document end markers (`...`)
///
/// # Algorithm
///
/// 1. Find all `---` markers at line start
/// 2. Split input at these positions
/// 3. Assign sequential indices to chunks
/// 4. Preserve byte offsets for error reporting
///
/// # Performance
///
/// - Time complexity: O(n) where n = input length
/// - Space complexity: O(d) where d = document count
/// - Uses zero-copy slicing (no allocations)
pub(crate) fn chunk_documents(input: &str) -> Vec<Chunk<'_>> {
    if input.is_empty() {
        return Vec::new();
    }

    // Find all `---` document separators at line boundaries
    let separator_positions = find_document_separators(input);

    // Pre-allocate based on separator count (may have implicit first doc + separator docs)
    let estimated_chunks = separator_positions.len() + 1;
    let mut chunks = Vec::with_capacity(estimated_chunks);

    if separator_positions.is_empty() {
        // Single document (no separators)
        // Skip if only whitespace
        if !input.trim().is_empty() {
            chunks.push(Chunk {
                index: 0,
                content: input,
                offset: 0,
            });
        }
        return chunks;
    }

    // Handle implicit first document (before first `---`)
    if separator_positions[0] > 0 {
        let content = &input[0..separator_positions[0]];
        if !content.trim().is_empty() {
            chunks.push(Chunk {
                index: 0,
                content,
                offset: 0,
            });
        }
    }

    // Process documents between separators
    for (i, &start) in separator_positions.iter().enumerate() {
        let end = separator_positions
            .get(i + 1)
            .copied()
            .unwrap_or(input.len());

        let content = &input[start..end];

        // Skip empty documents (e.g., `---\n---`)
        // Check if content after the separator line is empty
        if let Some(first_newline) = content.find('\n') {
            let content_after_separator = &content[first_newline + 1..];
            if content_after_separator.trim().is_empty() {
                continue;
            }
        }

        chunks.push(Chunk {
            index: chunks.len(),
            content,
            offset: start,
        });
    }

    chunks
}

/// Finds byte positions of all `---` document separators.
///
/// Returns sorted vector of byte offsets where separators occur.
fn find_document_separators(input: &str) -> Vec<usize> {
    // Estimate: ~1 separator per 1KB in typical multi-doc files
    let estimated_separators = (input.len() / 1024).max(1);
    let mut positions = Vec::with_capacity(estimated_separators);

    for (line_start, line) in LineOffsets::new(input) {
        // Fast path: only trim if line starts with whitespace
        let trimmed = if line.starts_with(|c: char| c.is_whitespace()) {
            line.trim_start()
        } else {
            line
        };

        // Check for document separator at line start
        if let Some(after_dashes) = trimmed.strip_prefix("---") {
            // Verify it's not part of a scalar (e.g., "key: ---value")
            if after_dashes.is_empty() || after_dashes.starts_with(|c: char| c.is_whitespace()) {
                positions.push(line_start);
            }
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
        assert_eq!(chunks.len(), 1); // Empty doc skipped
        assert!(chunks[0].content.contains("valid"));
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
        // All empty documents are filtered out
        assert_eq!(chunks.len(), 0);
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
