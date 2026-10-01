//! Comments located from parser events.

use std::ops::Range;

use crate::{SourceContext, Span, source::offset::ByteOffset};

/// Where a comment sits on its line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentKind {
    /// `#!` comment at the very start of the source.
    Shebang,
    /// Comment preceded only by whitespace on its line.
    FullLine,
    /// Comment following content on its line.
    Inline,
}

/// A comment in YAML source.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{CommentKind, LintContext};
///
/// let context = LintContext::new("# top\nkey: value  # note\n");
/// let comments = context.comments();
/// assert_eq!(comments[0].kind, CommentKind::FullLine);
/// assert_eq!(comments[1].kind, CommentKind::Inline);
/// assert_eq!(comments[1].text, " note");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Comment<'a> {
    /// Comment text without the leading `#`.
    pub text: &'a str,
    /// Location of the comment, from `#` to the end of its line.
    pub span: Span,
    /// Where the comment sits on its line.
    pub kind: CommentKind,
}

impl<'a> Comment<'a> {
    /// Builds a comment from the byte range of `#`..end of line in `source`.
    pub(crate) fn from_range(
        source: &'a str,
        context: &SourceContext<'_>,
        range: Range<usize>,
    ) -> Option<Self> {
        let text = source.get(range.start + 1..range.end)?;
        let span = Span::new(
            context.location_at(ByteOffset::new(range.start)),
            context.location_at(ByteOffset::new(range.end)),
        );
        let before = source.get(context.get_line_offset(span.start.line)..range.start)?;
        let kind = if range.start == 0 && text.starts_with('!') {
            CommentKind::Shebang
        } else if before.trim().is_empty() {
            CommentKind::FullLine
        } else {
            CommentKind::Inline
        };
        Some(Self { text, span, kind })
    }

    /// Whether the comment is alone on its line (a shebang counts as alone).
    #[must_use]
    pub const fn is_full_line(&self) -> bool {
        !matches!(self.kind, CommentKind::Inline)
    }
}
