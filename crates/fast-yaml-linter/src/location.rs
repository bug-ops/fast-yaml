//! Source location and span tracking for diagnostics.

use std::fmt;
use std::num::NonZeroUsize;

#[cfg(feature = "json-output")]
use serde::{Deserialize, Serialize};

/// A line or column number, counted from 1.
///
/// Zero is not a value, so a position cannot be built from a 0-based index by mistake: a 0-based
/// index has to be turned into a number with `index + 1` before it can become a `OneBased`.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::OneBased;
///
/// assert_eq!(OneBased::new(3).unwrap().get(), 3);
/// assert_eq!(OneBased::new(0), None);
/// assert_eq!(OneBased::FIRST.get(), 1);
/// assert_eq!(OneBased::from_index(0).get(), 1);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "json-output", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "json-output", serde(transparent))]
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
}

impl fmt::Display for OneBased {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A position in the source file.
///
/// Represents a single point in the YAML source with line, column,
/// and byte offset information for precise error reporting. Line and column count from 1
/// ([`OneBased`]) and the offset from 0; the fields are private, so a location is built with
/// [`Location::new`] or [`Location::try_new`] and read with [`line`](Self::line),
/// [`column`](Self::column) and [`offset`](Self::offset).
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::Location;
///
/// let loc = Location::new(10, 5, 145);
/// assert_eq!(loc.line(), 10);
/// assert_eq!(loc.column(), 5);
/// assert_eq!(loc.offset(), 145);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "json-output", derive(Serialize, Deserialize))]
pub struct Location {
    /// Line number (1-indexed, human-readable).
    line: OneBased,
    /// Column number (1-indexed, human-readable).
    column: OneBased,
    /// Byte offset from the start of the text with document-prefix BOMs removed (0-indexed).
    ///
    /// Line and column refer to the same text, so a location never mixes coordinate systems.
    /// To address the original bytes, map the offset back with
    /// `fast_yaml_core::NormalizedInput::original_offset` on `NormalizedInput::new(source)`.
    offset: usize,
}

impl Location {
    /// Creates a new location from a 1-based `line` and `column` and a 0-based byte `offset`.
    ///
    /// A `line` or `column` of 0 is raised to 1, so no location holds one; use
    /// [`try_new`](Self::try_new) to reject it instead, as for numbers that come from outside.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::Location;
    ///
    /// let loc = Location::new(1, 1, 0);
    /// assert_eq!(loc, Location::start());
    /// assert_eq!(Location::new(0, 0, 7).line(), 1);
    /// ```
    #[must_use]
    pub const fn new(line: usize, column: usize, offset: usize) -> Self {
        Self {
            line: match OneBased::new(line) {
                Some(line) => line,
                None => OneBased::FIRST,
            },
            column: match OneBased::new(column) {
                Some(column) => column,
                None => OneBased::FIRST,
            },
            offset,
        }
    }

    /// Creates a location, or `None` when `line` or `column` is 0.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::Location;
    ///
    /// assert_eq!(Location::try_new(2, 3, 10), Some(Location::new(2, 3, 10)));
    /// assert_eq!(Location::try_new(0, 3, 10), None);
    /// assert_eq!(Location::try_new(2, 0, 10), None);
    /// ```
    #[must_use]
    pub const fn try_new(line: usize, column: usize, offset: usize) -> Option<Self> {
        match (OneBased::new(line), OneBased::new(column)) {
            (Some(line), Some(column)) => Some(Self {
                line,
                column,
                offset,
            }),
            _ => None,
        }
    }

    /// Returns the start of the file (line 1, column 1, offset 0).
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::Location;
    ///
    /// let start = Location::start();
    /// assert_eq!(start.line(), 1);
    /// assert_eq!(start.column(), 1);
    /// assert_eq!(start.offset(), 0);
    /// ```
    #[must_use]
    pub const fn start() -> Self {
        Self::new(1, 1, 0)
    }

    /// The line number, counted from 1.
    #[must_use]
    pub const fn line(&self) -> usize {
        self.line.get()
    }

    /// The column number, counted from 1.
    #[must_use]
    pub const fn column(&self) -> usize {
        self.column.get()
    }

    /// The byte offset from the start of the text, counted from 0.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }
}

/// A span of text in the source file.
///
/// Represents a range from a start location to an end location,
/// useful for highlighting specific portions of YAML source in diagnostics.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{Location, Span};
///
/// let start = Location::new(10, 5, 145);
/// let end = Location::new(10, 9, 149);
/// let span = Span::new(start, end);
///
/// assert_eq!(span.len(), 4);
/// assert!(!span.is_empty());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "json-output", derive(Serialize, Deserialize))]
pub struct Span {
    /// Start position (inclusive).
    pub start: Location,
    /// End position (exclusive).
    pub end: Location,
}

impl Span {
    /// Creates a new span.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{Location, Span};
    ///
    /// let span = Span::new(
    ///     Location::new(1, 1, 0),
    ///     Location::new(1, 5, 4)
    /// );
    /// assert_eq!(span.len(), 4);
    /// ```
    #[must_use]
    pub const fn new(start: Location, end: Location) -> Self {
        Self { start, end }
    }

    /// Checks if this span contains a location.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{Location, Span};
    ///
    /// let span = Span::new(
    ///     Location::new(10, 5, 145),
    ///     Location::new(10, 9, 149)
    /// );
    ///
    /// assert!(span.contains(Location::new(10, 7, 147)));
    /// assert!(!span.contains(Location::new(11, 1, 150)));
    /// ```
    #[must_use]
    pub const fn contains(&self, loc: Location) -> bool {
        loc.offset() >= self.start.offset() && loc.offset() < self.end.offset()
    }

    /// Merges two spans into a single span covering both.
    ///
    /// Returns a span from the minimum start to the maximum end.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{Location, Span};
    ///
    /// let span1 = Span::new(
    ///     Location::new(10, 1, 140),
    ///     Location::new(10, 5, 144)
    /// );
    /// let span2 = Span::new(
    ///     Location::new(10, 10, 149),
    ///     Location::new(10, 15, 154)
    /// );
    ///
    /// let merged = span1.union(span2);
    /// assert_eq!(merged.start.offset(), 140);
    /// assert_eq!(merged.end.offset(), 154);
    /// ```
    #[must_use]
    pub fn union(&self, other: Self) -> Self {
        Self {
            start: if self.start < other.start {
                self.start
            } else {
                other.start
            },
            end: if self.end > other.end {
                self.end
            } else {
                other.end
            },
        }
    }

    /// Returns the length in bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{Location, Span};
    ///
    /// let span = Span::new(
    ///     Location::new(1, 1, 0),
    ///     Location::new(1, 10, 9)
    /// );
    /// assert_eq!(span.len(), 9);
    /// ```
    #[must_use]
    pub const fn len(&self) -> usize {
        self.end.offset().saturating_sub(self.start.offset())
    }

    /// Checks if the span is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{Location, Span};
    ///
    /// let empty = Span::new(
    ///     Location::new(1, 5, 4),
    ///     Location::new(1, 5, 4)
    /// );
    /// assert!(empty.is_empty());
    ///
    /// let non_empty = Span::new(
    ///     Location::new(1, 5, 4),
    ///     Location::new(1, 10, 9)
    /// );
    /// assert!(!non_empty.is_empty());
    /// ```
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_location_new() {
        let loc = Location::new(10, 5, 145);
        assert_eq!(loc.line(), 10);
        assert_eq!(loc.column(), 5);
        assert_eq!(loc.offset(), 145);
    }

    #[test]
    fn test_location_never_holds_a_zero_line_or_column() {
        let raised = Location::new(0, 0, 9);
        assert_eq!((raised.line(), raised.column(), raised.offset()), (1, 1, 9));
        assert_eq!(Location::try_new(0, 1, 0), None);
        assert_eq!(Location::try_new(1, 0, 0), None);
        assert_eq!(Location::try_new(3, 4, 5), Some(Location::new(3, 4, 5)));
    }

    #[test]
    fn test_one_based() {
        assert_eq!(OneBased::new(0), None);
        assert_eq!(OneBased::new(7).map(OneBased::get), Some(7));
        assert_eq!(OneBased::from_index(0), OneBased::FIRST);
        assert_eq!(OneBased::from_index(4).get(), 5);
        assert_eq!(OneBased::from_index(usize::MAX).get(), usize::MAX);
        assert!(OneBased::FIRST < OneBased::from_index(1));
        assert_eq!(OneBased::from_index(2).to_string(), "3");
    }

    #[cfg(feature = "json-output")]
    #[test]
    fn test_location_serializes_as_plain_numbers() {
        let loc = Location::new(2, 3, 14);
        let json = serde_json::to_string(&loc).unwrap();
        assert_eq!(json, r#"{"line":2,"column":3,"offset":14}"#);
        assert_eq!(serde_json::from_str::<Location>(&json).unwrap(), loc);
        assert!(serde_json::from_str::<Location>(r#"{"line":0,"column":3,"offset":14}"#).is_err());
    }

    #[test]
    fn test_location_start() {
        let start = Location::start();
        assert_eq!(start.line(), 1);
        assert_eq!(start.column(), 1);
        assert_eq!(start.offset(), 0);
    }

    #[test]
    fn test_location_ordering() {
        let loc1 = Location::new(10, 5, 145);
        let loc2 = Location::new(10, 7, 147);
        let loc3 = Location::new(11, 1, 150);

        assert!(loc1 < loc2);
        assert!(loc2 < loc3);
        assert!(loc1 < loc3);
    }

    #[test]
    fn test_span_new() {
        let start = Location::new(10, 5, 145);
        let end = Location::new(10, 9, 149);
        let span = Span::new(start, end);

        assert_eq!(span.start, start);
        assert_eq!(span.end, end);
    }

    #[test]
    fn test_span_contains() {
        let span = Span::new(Location::new(10, 5, 145), Location::new(10, 9, 149));

        assert!(span.contains(Location::new(10, 5, 145)));
        assert!(span.contains(Location::new(10, 7, 147)));
        assert!(!span.contains(Location::new(10, 9, 149)));
        assert!(!span.contains(Location::new(11, 1, 150)));
    }

    #[test]
    fn test_span_union() {
        let span1 = Span::new(Location::new(10, 1, 140), Location::new(10, 5, 144));
        let span2 = Span::new(Location::new(10, 10, 149), Location::new(10, 15, 154));

        let merged = span1.union(span2);
        assert_eq!(merged.start.offset, 140);
        assert_eq!(merged.end.offset, 154);
    }

    #[test]
    fn test_span_len() {
        let span = Span::new(Location::new(1, 1, 0), Location::new(1, 10, 9));
        assert_eq!(span.len(), 9);

        let empty = Span::new(Location::new(1, 5, 4), Location::new(1, 5, 4));
        assert_eq!(empty.len(), 0);
    }

    #[test]
    fn test_span_is_empty() {
        let empty = Span::new(Location::new(1, 5, 4), Location::new(1, 5, 4));
        assert!(empty.is_empty());

        let non_empty = Span::new(Location::new(1, 5, 4), Location::new(1, 10, 9));
        assert!(!non_empty.is_empty());
    }

    #[test]
    fn test_span_edge_cases() {
        let start_of_file = Location::start();
        let eof = Location::new(100, 1, 5000);
        let file_span = Span::new(start_of_file, eof);

        assert!(!file_span.is_empty());
        assert_eq!(file_span.len(), 5000);
        assert!(file_span.contains(Location::new(50, 10, 2500)));
    }
}
