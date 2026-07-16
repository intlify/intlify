// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::ops::Range;

/// A half-open UTF-8 byte range.
///
/// Resource spans are intentionally independent from parser spans so this
/// crate can remain below every message-level consumer in the dependency
/// graph.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Utf8ByteSpan {
    start: u32,
    end: u32,
}

impl Utf8ByteSpan {
    /// Construct a span without changing the supplied endpoint order.
    ///
    /// APIs that require an ordered span validate it at their own boundary so
    /// they can report the stable `Reversed` error instead of silently fixing
    /// caller input.
    #[must_use]
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    /// Return the inclusive start byte offset.
    #[must_use]
    pub const fn start(self) -> u32 {
        self.start
    }

    /// Return the exclusive end byte offset.
    #[must_use]
    pub const fn end(self) -> u32 {
        self.end
    }

    /// Return whether the span has no bytes.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// Return the byte length when the endpoints are ordered.
    #[must_use]
    pub const fn checked_len(self) -> Option<u32> {
        self.end.checked_sub(self.start)
    }

    /// Return the equivalent standard-library range.
    #[must_use]
    pub const fn as_range(self) -> Range<u32> {
        self.start..self.end
    }
}

impl From<Range<u32>> for Utf8ByteSpan {
    fn from(range: Range<u32>) -> Self {
        Self::new(range.start, range.end)
    }
}

impl From<Utf8ByteSpan> for Range<u32> {
    fn from(span: Utf8ByteSpan) -> Self {
        span.as_range()
    }
}

#[cfg(test)]
mod tests {
    use super::Utf8ByteSpan;

    #[test]
    fn preserves_endpoint_order() {
        let span = Utf8ByteSpan::new(8, 3);

        assert_eq!(span.start(), 8);
        assert_eq!(span.end(), 3);
        assert_eq!(span.checked_len(), None);
    }

    #[test]
    fn reports_ordered_length_and_range() {
        let span = Utf8ByteSpan::new(3, 8);

        assert_eq!(span.checked_len(), Some(5));
        assert_eq!(span.as_range(), 3..8);
        assert!(!span.is_empty());
        assert!(Utf8ByteSpan::new(3, 3).is_empty());
    }
}
