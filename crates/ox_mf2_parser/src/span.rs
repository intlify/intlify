// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! ID newtypes, [`Span`], and the optional-reference sentinel.
//!
//! The Phase 1 identifier model uses `u32` indexes. `0` is a valid index for
//! every table, so [`NONE_U32`] (=`u32::MAX`) is reserved for optional
//! references. Required references must not use this sentinel value.

/// Sentinel value used for optional table references.
pub const NONE_U32: u32 = u32::MAX;

#[inline]
pub(crate) fn usize_to_u32(value: usize, label: &str) -> u32 {
    u32::try_from(value).unwrap_or_else(|_| panic!("{label} exceeds u32::MAX"))
}

#[inline]
pub(crate) fn usize_to_id_u32(value: usize, label: &str) -> u32 {
    let value = usize_to_u32(value, label);
    assert!(
        value != NONE_U32,
        "{label} exceeds maximum allocatable id (u32::MAX - 1)"
    );
    value
}

macro_rules! define_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[repr(transparent)]
        pub struct $name(pub u32);

        impl $name {
            /// Sentinel value: this identifier does not point at any record.
            pub const NONE: Self = Self(NONE_U32);

            #[inline]
            pub const fn new(value: u32) -> Self {
                Self(value)
            }

            #[inline]
            pub const fn raw(self) -> u32 {
                self.0
            }

            #[inline]
            pub const fn is_none(self) -> bool {
                self.0 == NONE_U32
            }

            #[inline]
            pub const fn index(self) -> usize {
                self.0 as usize
            }
        }

        impl From<u32> for $name {
            #[inline]
            fn from(value: u32) -> Self {
                Self(value)
            }
        }

        impl From<$name> for u32 {
            #[inline]
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

define_id!(
    /// Identifier for a node in [`crate::CstTables`].
    NodeId
);
define_id!(
    /// Identifier for an edge entry in [`crate::CstTables`].
    EdgeId
);
define_id!(
    /// Identifier for a token record in [`crate::CstTables`].
    TokenId
);
define_id!(
    /// Identifier for a trivia record in [`crate::CstTables`].
    TriviaId
);
define_id!(
    /// Identifier for a source file inside [`crate::SourceStore`].
    SourceId
);

/// UTF-8 byte span `[start, end)` into the source text.
///
/// `Span` does not carry the source identity. Spans live next to the record
/// that produced them, or alongside an explicit `SourceId`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    /// Empty span `[0, 0)`.
    pub const EMPTY: Self = Self { start: 0, end: 0 };

    #[inline]
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    /// Construct a single-point span `[offset, offset)` at the given offset.
    #[inline]
    pub const fn at(offset: u32) -> Self {
        Self {
            start: offset,
            end: offset,
        }
    }

    /// Length in UTF-8 bytes.
    #[inline]
    pub const fn len(self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    #[inline]
    pub const fn is_empty(self) -> bool {
        self.end <= self.start
    }

    #[inline]
    pub const fn contains(self, offset: u32) -> bool {
        self.start <= offset && offset < self.end
    }

    /// Inclusive cover of two spans (smallest span that contains both).
    #[inline]
    #[must_use]
    pub fn cover(self, other: Self) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_is_a_valid_id() {
        let id = NodeId::new(0);
        assert!(!id.is_none());
        assert_eq!(id.raw(), 0);
        assert_eq!(id.index(), 0);
    }

    #[test]
    fn none_sentinel_is_u32_max() {
        assert_eq!(NodeId::NONE.raw(), u32::MAX);
        assert!(NodeId::NONE.is_none());
        assert!(TokenId::NONE.is_none());
        assert!(TriviaId::NONE.is_none());
        assert!(EdgeId::NONE.is_none());
        assert!(SourceId::NONE.is_none());
        assert_eq!(NONE_U32, u32::MAX);
    }

    #[test]
    fn id_conversion_reserves_none_sentinel() {
        assert_eq!(usize_to_id_u32(0, "test id"), 0);
        assert_eq!(usize_to_u32(u32::MAX as usize, "test count"), u32::MAX);

        let panic = std::panic::catch_unwind(|| {
            let _ = usize_to_id_u32(u32::MAX as usize, "test id");
        });
        assert!(panic.is_err());
    }

    #[test]
    fn span_basics() {
        let span = Span::new(2, 7);
        assert_eq!(span.len(), 5);
        assert!(!span.is_empty());
        assert!(span.contains(2));
        assert!(span.contains(6));
        assert!(!span.contains(7));

        let empty = Span::EMPTY;
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);
    }

    #[test]
    fn span_cover_picks_outer_bounds() {
        let a = Span::new(2, 5);
        let b = Span::new(4, 10);
        let cov = a.cover(b);
        assert_eq!(cov, Span::new(2, 10));
    }

    #[test]
    fn id_conversions_are_lossless() {
        let id = NodeId::from(42u32);
        assert_eq!(u32::from(id), 42);
    }
}
