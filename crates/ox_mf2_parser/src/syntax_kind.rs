//! `SyntaxKind` enum shared by parser, tables, diagnostics, and snapshots.
//!
//! Numeric values stabilise as the Phase 2 Binary AST snapshot wire format, so
//! once they ship they must not be reordered or reused. The full kind catalog
//! lands in Milestone 1.

/// Shared classification for nodes, tokens, trivia, errors, and missing nodes.
///
/// `SyntaxKind` has a compact `u16` representation that is stored directly in
/// node/token/trivia records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
#[non_exhaustive]
pub enum SyntaxKind {
    // Sentinel — never emitted by the parser. Reserved as the "uninitialized"
    // discriminant so that table builders can detect forgotten initialisation.
    Tombstone = 0,

    // Root.
    Root = 1,

    // Error / Missing / Unknown — populated during recovery.
    Error = 2,
    Missing = 3,
    Unknown = 4,
}

impl Default for SyntaxKind {
    fn default() -> Self {
        Self::Tombstone
    }
}

impl SyntaxKind {
    #[inline]
    pub const fn as_u16(self) -> u16 {
        self as u16
    }

    #[inline]
    pub const fn is_error(self) -> bool {
        matches!(self, Self::Error | Self::Missing | Self::Unknown)
    }

    #[inline]
    pub const fn is_root(self) -> bool {
        matches!(self, Self::Root)
    }
}
