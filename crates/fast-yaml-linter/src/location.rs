//! Source location and span tracking for diagnostics.

#[cfg(feature = "json-output")]
use serde::{Deserialize, Serialize};

/// A position in the source file.
///
/// Represents a single point in the YAML source with line, column,
/// and byte offset information for precise error reporting.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::Location;
///
/// let loc = Location::new(10, 5, 145);
/// assert_eq!(loc.line, 10);
/// assert_eq!(loc.column, 5);
/// assert_eq!(loc.offset, 145);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "json-output", derive(Serialize, Deserialize))]
pub struct Location {
    /// Line number (1-indexed, human-readable).
    pub line: usize,
    /// Column number (1-indexed, human-readable).
    pub column: usize,
    /// Byte offset from the start of the file (0-indexed).
    pub offset: usize,
}

impl Location {
    /// Creates a new location.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::Location;
    ///
    /// let loc = Location::new(1, 1, 0);
    /// assert_eq!(loc, Location::start());
    /// ```
    #[must_use]
    pub const fn new(line: usize, column: usize, offset: usize) -> Self {
        Self {
            line,
            column,
            offset,
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
    /// assert_eq!(start.line, 1);
    /// assert_eq!(start.column, 1);
    /// assert_eq!(start.offset, 0);
    /// ```
    #[must_use]
    pub const fn start() -> Self {
        Self::new(1, 1, 0)
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
        loc.offset >= self.start.offset && loc.offset < self.end.offset
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
    /// assert_eq!(merged.start.offset, 140);
    /// assert_eq!(merged.end.offset, 154);
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
        self.end.offset.saturating_sub(self.start.offset)
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
        assert_eq!(loc.line, 10);
        assert_eq!(loc.column, 5);
        assert_eq!(loc.offset, 145);
    }

    #[test]
    fn test_location_start() {
        let start = Location::start();
        assert_eq!(start.line, 1);
        assert_eq!(start.column, 1);
        assert_eq!(start.offset, 0);
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
