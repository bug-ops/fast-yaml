//! Source location and span tracking for diagnostics.

pub use fast_yaml_core::OneBased;

#[cfg(feature = "json-output")]
use serde::{Deserialize, Serialize};

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
/// let loc = Location::try_new(10, 5, 145).unwrap();
/// assert_eq!(loc.line().get(), 10);
/// assert_eq!(loc.column().get(), 5);
/// assert_eq!(loc.offset(), 145);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "json-output", derive(Serialize, Deserialize))]
#[cfg_attr(
    feature = "json-output",
    serde(try_from = "LocationRepr", into = "LocationRepr")
)]
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
    /// Creates a location from a 1-based `line` and `column` and a 0-based byte `offset`.
    ///
    /// Zero cannot be passed: [`OneBased`] has no zero. Use [`try_new`](Self::try_new) for
    /// numbers that come from outside.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{Location, OneBased};
    ///
    /// let loc = Location::new(OneBased::FIRST, OneBased::from_index(4), 4);
    /// assert_eq!((loc.line().get(), loc.column().get(), loc.offset()), (1, 5, 4));
    /// assert_eq!(Location::new(OneBased::FIRST, OneBased::FIRST, 0), Location::start());
    /// ```
    #[must_use]
    pub const fn new(line: OneBased, column: OneBased, offset: usize) -> Self {
        Self {
            line,
            column,
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
    /// assert_eq!(Location::try_new(2, 3, 10).map(|loc| loc.line().get()), Some(2));
    /// assert_eq!(Location::try_new(0, 3, 10), None);
    /// assert_eq!(Location::try_new(2, 0, 10), None);
    /// ```
    #[must_use]
    pub const fn try_new(line: usize, column: usize, offset: usize) -> Option<Self> {
        match (OneBased::new(line), OneBased::new(column)) {
            (Some(line), Some(column)) => Some(Self::new(line, column, offset)),
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
    /// assert_eq!(start.line().get(), 1);
    /// assert_eq!(start.column().get(), 1);
    /// assert_eq!(start.offset(), 0);
    /// ```
    #[must_use]
    pub const fn start() -> Self {
        Self::new(OneBased::FIRST, OneBased::FIRST, 0)
    }

    /// The line number, counted from 1.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::Location;
    ///
    /// assert_eq!(Location::try_new(7, 1, 0).unwrap().line().get(), 7);
    /// ```
    #[must_use]
    pub const fn line(&self) -> OneBased {
        self.line
    }

    /// The column number, counted from 1.
    #[must_use]
    pub const fn column(&self) -> OneBased {
        self.column
    }

    /// The byte offset from the start of the text, counted from 0.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }
}

#[cfg(feature = "json-output")]
#[derive(Serialize, Deserialize)]
struct LocationRepr {
    line: usize,
    column: usize,
    offset: usize,
}

#[cfg(feature = "json-output")]
impl From<Location> for LocationRepr {
    fn from(loc: Location) -> Self {
        Self {
            line: loc.line().get(),
            column: loc.column().get(),
            offset: loc.offset,
        }
    }
}

#[cfg(feature = "json-output")]
impl TryFrom<LocationRepr> for Location {
    type Error = &'static str;

    fn try_from(repr: LocationRepr) -> Result<Self, Self::Error> {
        Self::try_new(repr.line, repr.column, repr.offset)
            .ok_or("line and column are counted from 1")
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
/// let start = Location::try_new(10, 5, 145).unwrap();
/// let end = Location::try_new(10, 9, 149).unwrap();
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
    ///     Location::try_new(1, 1, 0).unwrap(),
    ///     Location::try_new(1, 5, 4).unwrap()
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
    ///     Location::try_new(10, 5, 145).unwrap(),
    ///     Location::try_new(10, 9, 149).unwrap()
    /// );
    ///
    /// assert!(span.contains(Location::try_new(10, 7, 147).unwrap()));
    /// assert!(!span.contains(Location::try_new(11, 1, 150).unwrap()));
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
    ///     Location::try_new(10, 1, 140).unwrap(),
    ///     Location::try_new(10, 5, 144).unwrap()
    /// );
    /// let span2 = Span::new(
    ///     Location::try_new(10, 10, 149).unwrap(),
    ///     Location::try_new(10, 15, 154).unwrap()
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
    ///     Location::try_new(1, 1, 0).unwrap(),
    ///     Location::try_new(1, 10, 9).unwrap()
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
    ///     Location::try_new(1, 5, 4).unwrap(),
    ///     Location::try_new(1, 5, 4).unwrap()
    /// );
    /// assert!(empty.is_empty());
    ///
    /// let non_empty = Span::new(
    ///     Location::try_new(1, 5, 4).unwrap(),
    ///     Location::try_new(1, 10, 9).unwrap()
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
        let loc = Location::try_new(10, 5, 145).unwrap();
        assert_eq!(loc.line().get(), 10);
        assert_eq!(loc.column().get(), 5);
        assert_eq!(loc.offset(), 145);
    }

    #[test]
    fn test_try_new_rejects_a_zero_line_or_column() {
        assert_eq!(Location::try_new(0, 1, 0), None);
        assert_eq!(Location::try_new(1, 0, 0), None);
        assert_eq!(
            Location::try_new(3, 4, 5),
            Some(Location::new(
                OneBased::from_index(2),
                OneBased::from_index(3),
                5
            ))
        );
    }

    #[cfg(feature = "json-output")]
    #[test]
    fn test_location_serializes_as_plain_numbers() {
        let loc = Location::try_new(2, 3, 14).unwrap();
        let json = serde_json::to_string(&loc).unwrap();
        assert_eq!(json, r#"{"line":2,"column":3,"offset":14}"#);
        assert_eq!(serde_json::from_str::<Location>(&json).unwrap(), loc);
        assert!(serde_json::from_str::<Location>(r#"{"line":0,"column":3,"offset":14}"#).is_err());
        assert!(serde_json::from_str::<Location>(r#"{"line":2,"column":0,"offset":14}"#).is_err());
    }

    #[test]
    fn test_location_start() {
        let start = Location::start();
        assert_eq!(start.line().get(), 1);
        assert_eq!(start.column().get(), 1);
        assert_eq!(start.offset(), 0);
    }

    #[test]
    fn test_location_ordering() {
        let loc1 = Location::try_new(10, 5, 145).unwrap();
        let loc2 = Location::try_new(10, 7, 147).unwrap();
        let loc3 = Location::try_new(11, 1, 150).unwrap();

        assert!(loc1 < loc2);
        assert!(loc2 < loc3);
        assert!(loc1 < loc3);
    }

    #[test]
    fn test_span_new() {
        let start = Location::try_new(10, 5, 145).unwrap();
        let end = Location::try_new(10, 9, 149).unwrap();
        let span = Span::new(start, end);

        assert_eq!(span.start, start);
        assert_eq!(span.end, end);
    }

    #[test]
    fn test_span_contains() {
        let span = Span::new(
            Location::try_new(10, 5, 145).unwrap(),
            Location::try_new(10, 9, 149).unwrap(),
        );

        assert!(span.contains(Location::try_new(10, 5, 145).unwrap()));
        assert!(span.contains(Location::try_new(10, 7, 147).unwrap()));
        assert!(!span.contains(Location::try_new(10, 9, 149).unwrap()));
        assert!(!span.contains(Location::try_new(11, 1, 150).unwrap()));
    }

    #[test]
    fn test_span_union() {
        let span1 = Span::new(
            Location::try_new(10, 1, 140).unwrap(),
            Location::try_new(10, 5, 144).unwrap(),
        );
        let span2 = Span::new(
            Location::try_new(10, 10, 149).unwrap(),
            Location::try_new(10, 15, 154).unwrap(),
        );

        let merged = span1.union(span2);
        assert_eq!(merged.start.offset, 140);
        assert_eq!(merged.end.offset, 154);
    }

    #[test]
    fn test_span_len() {
        let span = Span::new(
            Location::try_new(1, 1, 0).unwrap(),
            Location::try_new(1, 10, 9).unwrap(),
        );
        assert_eq!(span.len(), 9);

        let empty = Span::new(
            Location::try_new(1, 5, 4).unwrap(),
            Location::try_new(1, 5, 4).unwrap(),
        );
        assert_eq!(empty.len(), 0);
    }

    #[test]
    fn test_span_is_empty() {
        let empty = Span::new(
            Location::try_new(1, 5, 4).unwrap(),
            Location::try_new(1, 5, 4).unwrap(),
        );
        assert!(empty.is_empty());

        let non_empty = Span::new(
            Location::try_new(1, 5, 4).unwrap(),
            Location::try_new(1, 10, 9).unwrap(),
        );
        assert!(!non_empty.is_empty());
    }

    #[test]
    fn test_span_edge_cases() {
        let start_of_file = Location::start();
        let eof = Location::try_new(100, 1, 5000).unwrap();
        let file_span = Span::new(start_of_file, eof);

        assert!(!file_span.is_empty());
        assert_eq!(file_span.len(), 5000);
        assert!(file_span.contains(Location::try_new(50, 10, 2500).unwrap()));
    }
}
