//! Typed byte offsets into the source text.
//!
//! saphyr reports char-based positions; these newtypes keep byte offsets from being
//! mixed up with char indices or columns. Conversion happens only in `SourceContext`.

/// A byte offset into the source text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteOffset(usize);

impl ByteOffset {
    pub(crate) const ZERO: Self = Self(0);

    pub(crate) const fn new(bytes: usize) -> Self {
        Self(bytes)
    }

    pub(crate) const fn get(self) -> usize {
        self.0
    }

    pub(crate) const fn add_bytes(self, bytes: usize) -> Self {
        Self(self.0 + bytes)
    }
}

/// A half-open byte range `start..end` into the source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    start: ByteOffset,
    end: ByteOffset,
}

impl ByteRange {
    pub(crate) fn new(start: ByteOffset, end: ByteOffset) -> Self {
        debug_assert!(start <= end);
        Self { start, end }
    }

    pub(crate) const fn start(self) -> ByteOffset {
        self.start
    }

    pub(crate) const fn end(self) -> ByteOffset {
        self.end
    }

    pub(crate) fn contains(self, offset: ByteOffset) -> bool {
        self.start <= offset && offset < self.end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_range_is_half_open() {
        let range = ByteRange::new(ByteOffset::new(2), ByteOffset::new(5));
        assert!(!range.contains(ByteOffset::new(1)));
        assert!(range.contains(ByteOffset::new(2)));
        assert!(range.contains(ByteOffset::new(4)));
        assert!(!range.contains(ByteOffset::new(5)));
    }

    #[test]
    fn test_add_bytes() {
        assert_eq!(ByteOffset::ZERO.add_bytes(3).get(), 3);
    }
}
