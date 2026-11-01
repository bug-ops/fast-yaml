//! Typed source coordinates shared by parse errors and the linter.
//!
//! Lines and columns count from 1 ([`OneBased`]), document indices from 0 ([`DocumentIndex`]),
//! and the amount a fragment is shifted by is a count ([`LineCount`], [`DocumentCount`]), so the
//! different numbers cannot be passed in place of one another.

use saphyr_parser::Marker;
use std::fmt;
use std::num::NonZeroUsize;

/// A line or column number, counted from 1.
///
/// Zero is not a value, so a position cannot be built from a 0-based index by mistake: a 0-based
/// index has to go through [`from_index`](Self::from_index) before it can become a `OneBased`.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::OneBased;
///
/// assert_eq!(OneBased::new(3).unwrap().get(), 3);
/// assert_eq!(OneBased::new(0), None);
/// assert_eq!(OneBased::FIRST.get(), 1);
/// assert_eq!(OneBased::from_index(0).get(), 1);
/// assert_eq!(OneBased::from_index(4).index(), 4);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OneBased(NonZeroUsize);

impl OneBased {
    /// The first line or column, 1.
    pub const FIRST: Self = Self(NonZeroUsize::MIN);

    /// The number `value`, or `None` for 0.
    #[must_use]
    pub const fn new(value: usize) -> Option<Self> {
        match NonZeroUsize::new(value) {
            Some(number) => Some(Self(number)),
            None => None,
        }
    }

    /// The number of the 0-based `index`: line index 0 is line 1.
    ///
    /// Saturates at `usize::MAX`.
    #[must_use]
    pub const fn from_index(index: usize) -> Self {
        match NonZeroUsize::new(index.saturating_add(1)) {
            Some(number) => Self(number),
            None => Self::FIRST,
        }
    }

    /// The number, 1 or more.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0.get()
    }

    /// The 0-based index of this number: line 1 is index 0.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0.get() - 1
    }

    /// The number `count` places later, saturating at `usize::MAX`.
    #[must_use]
    pub const fn saturating_add(self, count: usize) -> Self {
        Self(self.0.saturating_add(count))
    }
}

impl fmt::Display for OneBased {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Start of a node in the source text, as reported in error messages.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::{ParseError, Parser};
///
/// let err = Parser::parse_str("m:\n  <<: 1\n").unwrap_err();
/// let ParseError::Merge { at, .. } = err else { panic!("merge error expected") };
/// assert_eq!((at.line().get(), at.column().get()), (2, 3));
/// assert_eq!(at.to_string(), "line 2, column 3");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourcePosition {
    line: OneBased,
    column: OneBased,
}

impl SourcePosition {
    /// The first column of the first line.
    pub const START: Self = Self::new(OneBased::FIRST, OneBased::FIRST);

    /// A position at `line` and `column`, both counted from 1.
    #[must_use]
    pub const fn new(line: OneBased, column: OneBased) -> Self {
        Self { line, column }
    }

    /// Line number, counted from 1.
    #[must_use]
    pub const fn line(self) -> OneBased {
        self.line
    }

    /// Column number, counted from 1, in characters.
    #[must_use]
    pub const fn column(self) -> OneBased {
        self.column
    }

    // Char-based column, 1-indexed like saphyr's own errors; no source text to convert from.
    #[allow(clippy::disallowed_methods)]
    pub(crate) fn from_marker(marker: Marker) -> Self {
        Self {
            // Parsed lines start at 1; line 0 comes from the synthetic default spans of emitter events.
            line: OneBased::new(marker.line()).unwrap_or(OneBased::FIRST),
            column: OneBased::from_index(marker.col()),
        }
    }

    // Marker the scanner or parser reported for real input, which never has line 0.
    pub(crate) fn from_parsed_marker(marker: Marker) -> Self {
        debug_assert!(marker.line() > 0, "parsed markers start at line 1");
        Self::from_marker(marker)
    }

    pub(crate) fn from_parsed_span(span: saphyr_parser::Span) -> Self {
        Self::from_parsed_marker(span.start)
    }

    pub(crate) fn from_span(span: saphyr_parser::Span) -> Self {
        Self::from_marker(span.start)
    }

    pub(crate) fn end_of(span: saphyr_parser::Span) -> Self {
        Self::from_marker(span.end)
    }

    pub(crate) const fn below(self, lines: LineCount) -> Self {
        Self {
            line: self.line.saturating_add(lines.get()),
            column: self.column,
        }
    }
}

impl fmt::Display for SourcePosition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}, column {}", self.line, self.column)
    }
}

/// Index of a document in a stream, counted from 0.
///
/// A [`SourcePosition`] counts from 1, so the two are distinct types: an index cannot be passed
/// where a line or a column is expected, nor the other way round.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::{DocumentIndex, Parser};
///
/// let err = Parser::parse_all("a: 1\n---\nm: {<<: 1}\n").unwrap_err();
/// assert_eq!(err.document_index(), DocumentIndex::new(1));
/// assert_eq!(err.document_index().get(), 1);
/// assert_eq!(err.document_index().number(), 2);
/// assert_eq!(DocumentIndex::FIRST.number(), 1);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocumentIndex(usize);

impl DocumentIndex {
    /// The first document of a stream.
    pub const FIRST: Self = Self(0);

    /// The document at `index`, counted from 0.
    #[must_use]
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    /// The index, counted from 0.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }

    /// The ordinal of the document, counted from 1, as shown in messages.
    #[must_use]
    pub const fn number(self) -> usize {
        self.0.saturating_add(1)
    }

    /// The index `documents` documents later.
    #[must_use]
    pub const fn after(self, documents: DocumentCount) -> Self {
        Self(self.0.saturating_add(documents.get()))
    }
}

/// Number of line breaks that precede a fragment of the input.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::LineCount;
///
/// assert_eq!(LineCount::new(4).get(), 4);
/// assert_eq!(LineCount::ZERO, LineCount::default());
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LineCount(usize);

impl LineCount {
    /// No preceding lines.
    pub const ZERO: Self = Self(0);

    /// `lines` preceding line breaks.
    #[must_use]
    pub const fn new(lines: usize) -> Self {
        Self(lines)
    }

    /// The count.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

/// Number of documents that precede a fragment of the input.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::DocumentCount;
///
/// assert_eq!(DocumentCount::new(2).get(), 2);
/// assert_eq!(DocumentCount::ZERO, DocumentCount::default());
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocumentCount(usize);

impl DocumentCount {
    /// No preceding documents.
    pub const ZERO: Self = Self(0);

    /// `documents` preceding documents.
    #[must_use]
    pub const fn new(documents: usize) -> Self {
        Self(documents)
    }

    /// The count.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_one_based() {
        assert_eq!(OneBased::new(0), None);
        assert_eq!(OneBased::new(7).map(OneBased::get), Some(7));
        assert_eq!(OneBased::from_index(0), OneBased::FIRST);
        assert_eq!(OneBased::from_index(4).get(), 5);
        assert_eq!(OneBased::from_index(4).index(), 4);
        assert_eq!(OneBased::from_index(usize::MAX).get(), usize::MAX);
        assert_eq!(OneBased::FIRST.saturating_add(usize::MAX).get(), usize::MAX);
        assert!(OneBased::FIRST < OneBased::from_index(1));
        assert_eq!(OneBased::from_index(2).to_string(), "3");
    }

    #[test]
    fn position_orders_by_line_then_column() {
        let at = |l, c| SourcePosition::new(OneBased::from_index(l), OneBased::from_index(c));
        assert!(at(0, 9) < at(1, 0));
        assert!(at(1, 0) < at(1, 1));
        assert_eq!(SourcePosition::START, at(0, 0));
    }

    #[test]
    fn line_and_document_counts() {
        assert_eq!(LineCount::new(7).get(), 7);
        assert_eq!(LineCount::ZERO.get(), 0);
        assert_eq!(DocumentCount::new(3).get(), 3);
        assert_eq!(DocumentCount::ZERO.get(), 0);
    }

    #[test]
    fn document_index_after_adds_and_saturates() {
        assert_eq!(
            DocumentIndex::new(1).after(DocumentCount::new(2)),
            DocumentIndex::new(3)
        );
        assert_eq!(
            DocumentIndex::FIRST.after(DocumentCount::ZERO),
            DocumentIndex::FIRST
        );
        let last = DocumentIndex::new(usize::MAX);
        assert_eq!(last.after(DocumentCount::new(1)), last);
    }

    #[test]
    fn from_marker_maps_line_zero_to_the_first_line() {
        assert_eq!(
            SourcePosition::from_marker(Marker::new(0, 0, 0)),
            SourcePosition::START
        );
        let moved = SourcePosition::from_marker(Marker::new(0, 0, 3));
        assert_eq!((moved.line().get(), moved.column().get()), (1, 4));
    }

    #[test]
    fn below_saturates() {
        let moved = SourcePosition::START.below(LineCount::new(usize::MAX));
        assert_eq!(moved.line().get(), usize::MAX);
        assert_eq!(moved.column(), OneBased::FIRST);
    }
}
