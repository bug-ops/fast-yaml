//! Recovery of `%YAML` / `%TAG` directive lines from the source text.
//!
//! The parser emits no directive events, so the formatter re-reads them from the lines
//! preceding each explicit document start.

use memchr::{memchr2, memrchr2};

/// Start of a source line: byte offset and 1-based line number.
#[derive(Clone, Copy)]
struct LinePos {
    byte: usize,
    line: usize,
}

const SOURCE_START: LinePos = LinePos { byte: 0, line: 1 };

/// Finds the directives declared before explicit document starts, in O(1) extra memory.
pub(super) struct DirectiveScanner<'a> {
    source: &'a str,
    /// Whether any line can start with `%`; when false no scanning happens.
    may_have_directive: bool,
    /// Start of the last line looked up; document starts arrive in increasing line order.
    cursor: LinePos,
}

impl<'a> DirectiveScanner<'a> {
    /// Creates a scanner over `source`.
    pub(super) fn new(source: &'a str) -> Self {
        Self {
            source,
            may_have_directive: source.starts_with('%')
                || source.contains("\n%")
                || source.contains("\r%"),
            cursor: SOURCE_START,
        }
    }

    /// Directive block (each line newline-terminated) preceding the `---` on 1-based `marker_line`.
    ///
    /// Blank and comment lines between directives are skipped and reserved directives dropped.
    /// The block must start the stream or follow a `...` document end marker, otherwise its
    /// lines are content; `None` when nothing remains.
    pub(super) fn before(&mut self, marker_line: usize) -> Option<String> {
        if !self.may_have_directive {
            return None;
        }
        let mut pos = self.seek(marker_line);
        let mut directives = Vec::new();
        while let Some((start, end)) = self.previous_line(pos) {
            let line = &self.source[start..end];
            if line.starts_with('%') {
                if is_directive(line) {
                    directives.push(strip_directive_comment(line));
                }
            } else if !(line.trim().is_empty() || line.trim_start().starts_with('#')) {
                if !is_document_end(line) {
                    directives.clear();
                }
                break;
            }
            pos = start;
        }
        if directives.is_empty() {
            return None;
        }
        Some(
            directives
                .iter()
                .rev()
                .fold(String::new(), |mut out, line| {
                    out.push_str(line);
                    out.push('\n');
                    out
                }),
        )
    }

    /// Byte offset of the start of `line`, advancing the cursor with saphyr's line breaks.
    fn seek(&mut self, line: usize) -> usize {
        if line < self.cursor.line {
            self.cursor = SOURCE_START;
        }
        let bytes = self.source.as_bytes();
        while self.cursor.line < line {
            let Some(rel) = memchr2(b'\n', b'\r', &bytes[self.cursor.byte..]) else {
                self.cursor.byte = bytes.len();
                break;
            };
            let at = self.cursor.byte + rel;
            let crlf = bytes[at] == b'\r' && bytes.get(at + 1) == Some(&b'\n');
            self.cursor.byte = at + 1 + usize::from(crlf);
            self.cursor.line += 1;
        }
        self.cursor.byte
    }

    /// Byte range (terminator excluded) of the line ending just before the line starting at `pos`.
    fn previous_line(&self, pos: usize) -> Option<(usize, usize)> {
        let bytes = self.source.as_bytes();
        let mut end = pos.checked_sub(1)?;
        if bytes[end] == b'\n' && end > 0 && bytes[end - 1] == b'\r' {
            end -= 1;
        }
        let start = memrchr2(b'\n', b'\r', &bytes[..end]).map_or(0, |i| i + 1);
        Some((start, end))
    }
}

/// Whether `line` is a `%YAML` or `%TAG` directive (`%` in column 0, exact name).
fn is_directive(line: &str) -> bool {
    line.strip_prefix("%YAML")
        .or_else(|| line.strip_prefix("%TAG"))
        .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t']))
}

/// Whether `line` is a `...` document end marker.
fn is_document_end(line: &str) -> bool {
    line.strip_prefix("...")
        .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t']))
}

/// Cuts a trailing comment; a `#` not preceded by a blank belongs to the token.
fn strip_directive_comment(line: &str) -> &str {
    let cut = line
        .match_indices('#')
        .find(|&(i, _)| line[..i].ends_with([' ', '\t']))
        .map_or(line.len(), |(i, _)| i);
    line[..cut].trim_end()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn before(source: &str, marker_line: usize) -> Option<String> {
        DirectiveScanner::new(source).before(marker_line)
    }

    #[test]
    fn comment_cut_keeps_hash_inside_token() {
        assert_eq!(strip_directive_comment("%YAML 1.2 # c"), "%YAML 1.2");
        assert_eq!(
            strip_directive_comment("%TAG !e! tag:x#y # c"),
            "%TAG !e! tag:x#y"
        );
        assert_eq!(
            strip_directive_comment("%TAG !e! tag:x#y"),
            "%TAG !e! tag:x#y"
        );
    }

    #[test]
    fn block_must_follow_stream_start_or_document_end() {
        assert_eq!(before("a\n%YAML 1.2\n---\nb\n", 3), None);
        assert_eq!(
            before("a\n...\n%YAML 1.2\n---\nb\n", 4).as_deref(),
            Some("%YAML 1.2\n")
        );
    }

    #[test]
    fn reserved_directive_is_skipped_not_block_clearing() {
        assert_eq!(
            before("a\n...\n%FOO x\n%YAML 1.1\n---\nb\n", 5).as_deref(),
            Some("%YAML 1.1\n")
        );
        assert_eq!(before("%FOO x\n---\nb\n", 2), None);
    }

    #[test]
    fn line_breaks_lf_crlf_and_lone_cr() {
        for nl in ["\n", "\r\n", "\r"] {
            let src = format!("a{nl}...{nl}%YAML 1.2{nl}---{nl}b{nl}");
            assert_eq!(before(&src, 4).as_deref(), Some("%YAML 1.2\n"), "{nl:?}");
        }
    }

    #[test]
    fn comment_line_between_document_end_and_directive() {
        assert_eq!(
            before("a\n...\n# c\n%YAML 1.2\n---\nb\n", 5).as_deref(),
            Some("%YAML 1.2\n")
        );
    }

    #[test]
    fn tab_separated_directive() {
        assert_eq!(
            before("a\n...\n%YAML\t1.2\n---\nb\n", 4).as_deref(),
            Some("%YAML\t1.2\n")
        );
    }

    #[test]
    fn successive_lookups_advance_the_cursor() {
        let mut scanner = DirectiveScanner::new("%YAML 1.2\n---\na\n...\n%YAML 1.1\n---\nb\n");
        assert_eq!(scanner.before(2).as_deref(), Some("%YAML 1.2\n"));
        assert_eq!(scanner.before(6).as_deref(), Some("%YAML 1.1\n"));
    }

    #[test]
    fn no_percent_line_skips_scanning() {
        assert_eq!(before("a: 1\n---\nb: 2\n", 2), None);
    }
}
