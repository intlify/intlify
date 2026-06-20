//! ID newtypes, [`Span`], and the optional-reference sentinel.
//!
//! Full implementation lands in Milestone 1.

/// Sentinel value used for optional table references.
///
/// `0` is a valid index for every table, so [`u32::MAX`] is reserved for "no
/// reference". Required references must not use this value.
pub const NONE_U32: u32 = u32::MAX;

macro_rules! define_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[repr(transparent)]
        pub struct $name(pub u32);

        impl $name {
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
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub const EMPTY: Self = Self { start: 0, end: 0 };

    #[inline]
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    #[inline]
    pub const fn len(self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    #[inline]
    pub const fn is_empty(self) -> bool {
        self.end <= self.start
    }
}
