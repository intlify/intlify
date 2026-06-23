// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Binary AST snapshot wire format constants and little-endian helpers.
//!
//! This module pins the v0.1 format contract: magic bytes, version, fixed
//! record sizes, section kinds, alignment, and the canonical none sentinel
//! values. Wire encoding helpers are intentionally tiny and explicit so the
//! format never accidentally adopts Rust struct layout, padding, or
//! platform alignment.

use core::convert::TryFrom;

/// Magic prefix written at offset 0 of every v0.1 snapshot buffer.
pub const SNAPSHOT_MAGIC: [u8; 8] = *b"OXMF2AST";

/// v0.1 major version.
pub const SNAPSHOT_MAJOR_VERSION: u16 = 0;

/// v0.1 minor version. v0.x uses exact version matching, so this is the
/// only minor accepted by the v0.1 decoder.
pub const SNAPSHOT_MINOR_VERSION: u16 = 1;

/// v0.1 `feature_flags` value — fully reserved, only `0` is allowed.
pub const SNAPSHOT_FEATURE_FLAGS: u32 = 0;

/// Wire size of [`crate::snapshot`] `SnapshotHeader`, in bytes.
pub const HEADER_SIZE: u32 = 32;

/// Wire size of one `SectionRecord` entry, in bytes.
pub const SECTION_RECORD_SIZE: u32 = 20;

/// Wire size of one `RootRecord`, in bytes.
pub const ROOT_RECORD_SIZE: u32 = 16;

/// Wire size of one `StringOffsetRecord`, in bytes.
pub const STRING_OFFSET_RECORD_SIZE: u32 = 8;

/// Wire size of one `SourceRecord`, in bytes.
pub const SOURCE_RECORD_SIZE: u32 = 32;

/// Wire size of one `NodeRecord`, in bytes.
pub const NODE_RECORD_SIZE: u32 = 24;

/// Wire size of one `EdgeRecord`, in bytes.
pub const EDGE_RECORD_SIZE: u32 = 8;

/// Wire size of one `TokenRecord`, in bytes.
pub const TOKEN_RECORD_SIZE: u32 = 36;

/// Wire size of one `TriviaRecord`, in bytes.
pub const TRIVIA_RECORD_SIZE: u32 = 16;

/// Wire size of one `DiagnosticRecord`, in bytes.
pub const DIAGNOSTIC_RECORD_SIZE: u32 = 28;

/// Wire size of one `DiagnosticLabelRecord`, in bytes.
pub const DIAGNOSTIC_LABEL_RECORD_SIZE: u32 = 16;

/// Wire size of one `ExtendedDataHeader`, in bytes.
pub const EXTENDED_DATA_HEADER_SIZE: u32 = 8;

/// Section alignment in bytes. Every emitted section starts at an offset
/// that is a multiple of this constant.
pub const SECTION_ALIGNMENT: u32 = 8;

/// Sentinel value for optional `u32` references in the snapshot wire
/// format. Required `RootId`/`NodeId`/`TokenId`/`TriviaId`/`SourceId`
/// indexes never use this value.
pub const NONE_REF: u32 = u32::MAX;

/// `SectionRecord.flags` bit indicating that this section is required
/// for the snapshot to be interpretable.
pub const SECTION_FLAG_REQUIRED: u16 = 1;

/// Edge kind: child reference points at a `NodeRecord`.
pub const EDGE_KIND_NODE: u16 = 0;
/// Edge kind: child reference points at a `TokenRecord`.
pub const EDGE_KIND_TOKEN: u16 = 1;

/// Stable numeric `SectionKind` identifiers.
///
/// Numbers are part of the v0.1 wire contract. Once assigned, a kind
/// number is not reused. Changing the meaning of a section incompatibly
/// requires a major version bump.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum SectionKind {
    Roots = 1,
    Sources = 2,
    Nodes = 3,
    Edges = 4,
    Tokens = 5,
    Trivia = 6,
    Diagnostics = 7,
    DiagnosticLabels = 8,
    StringOffsets = 9,
    StringData = 10,
    SourceTextData = 11,
    ExtendedData = 12,
}

impl SectionKind {
    /// Numeric wire value.
    #[inline]
    pub const fn as_u16(self) -> u16 {
        self as u16
    }

    /// Returns the matching `SectionKind` for a wire value, or `None`
    /// when the value is `0` (reserved) or otherwise unknown to v0.1.
    pub const fn from_u16(value: u16) -> Option<Self> {
        Some(match value {
            1 => Self::Roots,
            2 => Self::Sources,
            3 => Self::Nodes,
            4 => Self::Edges,
            5 => Self::Tokens,
            6 => Self::Trivia,
            7 => Self::Diagnostics,
            8 => Self::DiagnosticLabels,
            9 => Self::StringOffsets,
            10 => Self::StringData,
            11 => Self::SourceTextData,
            12 => Self::ExtendedData,
            _ => return None,
        })
    }

    /// `true` for sections that must always be present in a v0.1
    /// snapshot with `SectionFlags.required = true`.
    pub const fn is_core(self) -> bool {
        matches!(
            self,
            Self::Roots
                | Self::Sources
                | Self::Nodes
                | Self::Edges
                | Self::Tokens
                | Self::StringOffsets
                | Self::StringData
        )
    }

    /// Wire `record_size` for fixed-record sections. Raw byte sections
    /// (`StringData`, `SourceTextData`, `ExtendedData`) return `0`.
    pub const fn record_size(self) -> u16 {
        match self {
            Self::Roots => ROOT_RECORD_SIZE as u16,
            Self::Sources => SOURCE_RECORD_SIZE as u16,
            Self::Nodes => NODE_RECORD_SIZE as u16,
            Self::Edges => EDGE_RECORD_SIZE as u16,
            Self::Tokens => TOKEN_RECORD_SIZE as u16,
            Self::Trivia => TRIVIA_RECORD_SIZE as u16,
            Self::Diagnostics => DIAGNOSTIC_RECORD_SIZE as u16,
            Self::DiagnosticLabels => DIAGNOSTIC_LABEL_RECORD_SIZE as u16,
            Self::StringOffsets => STRING_OFFSET_RECORD_SIZE as u16,
            Self::StringData | Self::SourceTextData | Self::ExtendedData => 0,
        }
    }

    /// Stable emission order for the v0.1 writer. The decoder reads
    /// sections through the section table and does not require this
    /// physical order, but the writer emits them in this order so that
    /// binary fixtures and byte-level diffs stay reviewable.
    pub const ALL_IN_ORDER: [Self; 12] = [
        Self::Roots,
        Self::Sources,
        Self::Nodes,
        Self::Edges,
        Self::Tokens,
        Self::Trivia,
        Self::Diagnostics,
        Self::DiagnosticLabels,
        Self::StringOffsets,
        Self::StringData,
        Self::SourceTextData,
        Self::ExtendedData,
    ];
}

/// Snapshot-local root identifier. Indexes into the roots section.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct RootId(pub u32);

impl RootId {
    /// Construct a `RootId` from a raw wire value.
    #[inline]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Raw wire value.
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// `usize` index.
    #[inline]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl From<u32> for RootId {
    #[inline]
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<RootId> for u32 {
    #[inline]
    fn from(value: RootId) -> Self {
        value.0
    }
}

/// Snapshot-local string identifier. Indexes into the string offsets
/// section. `0xFFFF_FFFF` is the optional-reference sentinel and must
/// not be dereferenced.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct StringId(pub u32);

impl StringId {
    /// `0xFFFF_FFFF` — optional string is absent.
    pub const NONE: Self = Self(NONE_REF);

    #[inline]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }

    #[inline]
    pub const fn is_none(self) -> bool {
        self.0 == NONE_REF
    }
}

// ── Little-endian read / write helpers ──────────────────────────────
//
// Every wire field is encoded with explicit little-endian operations.
// Helpers panic only on programmer error (slice shorter than the field
// width) and never read uninitialised memory. Decoders pre-check the
// slice length, so the panic paths are unreachable from validated
// input.

#[inline]
pub(crate) fn write_u8(buf: &mut Vec<u8>, value: u8) {
    buf.push(value);
}

#[inline]
pub(crate) fn write_u16_le(buf: &mut Vec<u8>, value: u16) {
    buf.extend_from_slice(&value.to_le_bytes());
}

#[inline]
pub(crate) fn write_u32_le(buf: &mut Vec<u8>, value: u32) {
    buf.extend_from_slice(&value.to_le_bytes());
}

#[inline]
pub(crate) fn read_u16_le(bytes: &[u8], offset: usize) -> u16 {
    let arr: [u8; 2] = bytes[offset..offset + 2].try_into().unwrap();
    u16::from_le_bytes(arr)
}

#[inline]
pub(crate) fn read_u32_le(bytes: &[u8], offset: usize) -> u32 {
    let arr: [u8; 4] = bytes[offset..offset + 4].try_into().unwrap();
    u32::from_le_bytes(arr)
}

/// Round `offset` up to the next multiple of `SECTION_ALIGNMENT`.
/// Returns `None` on `u32` overflow so callers can surface
/// `SectionTooLarge` instead of panicking on `usize` truncation.
#[inline]
pub(crate) fn align_up(offset: u32) -> Option<u32> {
    let mask = SECTION_ALIGNMENT - 1;
    let extra = offset & mask;
    if extra == 0 {
        Some(offset)
    } else {
        let padding = SECTION_ALIGNMENT - extra;
        offset.checked_add(padding)
    }
}

/// Checked `usize` -> `u32` conversion that returns `None` instead of
/// truncating. v0.1 writer surfaces this as the matching `TooMany*` /
/// `SectionTooLarge` error code.
#[inline]
pub(crate) fn checked_u32(value: usize) -> Option<u32> {
    u32::try_from(value).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_sizes_match_design_table() {
        assert_eq!(HEADER_SIZE, 32);
        assert_eq!(SECTION_RECORD_SIZE, 20);
        assert_eq!(ROOT_RECORD_SIZE, 16);
        assert_eq!(STRING_OFFSET_RECORD_SIZE, 8);
        assert_eq!(SOURCE_RECORD_SIZE, 32);
        assert_eq!(NODE_RECORD_SIZE, 24);
        assert_eq!(EDGE_RECORD_SIZE, 8);
        assert_eq!(TOKEN_RECORD_SIZE, 36);
        assert_eq!(TRIVIA_RECORD_SIZE, 16);
        assert_eq!(DIAGNOSTIC_RECORD_SIZE, 28);
        assert_eq!(DIAGNOSTIC_LABEL_RECORD_SIZE, 16);
        assert_eq!(EXTENDED_DATA_HEADER_SIZE, 8);
    }

    #[test]
    fn section_kind_numeric_values_are_stable() {
        assert_eq!(SectionKind::Roots.as_u16(), 1);
        assert_eq!(SectionKind::Sources.as_u16(), 2);
        assert_eq!(SectionKind::Nodes.as_u16(), 3);
        assert_eq!(SectionKind::Edges.as_u16(), 4);
        assert_eq!(SectionKind::Tokens.as_u16(), 5);
        assert_eq!(SectionKind::Trivia.as_u16(), 6);
        assert_eq!(SectionKind::Diagnostics.as_u16(), 7);
        assert_eq!(SectionKind::DiagnosticLabels.as_u16(), 8);
        assert_eq!(SectionKind::StringOffsets.as_u16(), 9);
        assert_eq!(SectionKind::StringData.as_u16(), 10);
        assert_eq!(SectionKind::SourceTextData.as_u16(), 11);
        assert_eq!(SectionKind::ExtendedData.as_u16(), 12);
    }

    #[test]
    fn section_kind_from_u16_rejects_zero_and_unknown() {
        assert!(SectionKind::from_u16(0).is_none());
        assert!(SectionKind::from_u16(13).is_none());
        assert_eq!(SectionKind::from_u16(1), Some(SectionKind::Roots));
    }

    #[test]
    fn section_kind_emission_order_is_stable() {
        let order = SectionKind::ALL_IN_ORDER.map(SectionKind::as_u16);
        assert_eq!(order, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
    }

    #[test]
    fn core_section_predicate_matches_design() {
        for kind in SectionKind::ALL_IN_ORDER {
            let expected = matches!(
                kind,
                SectionKind::Roots
                    | SectionKind::Sources
                    | SectionKind::Nodes
                    | SectionKind::Edges
                    | SectionKind::Tokens
                    | SectionKind::StringOffsets
                    | SectionKind::StringData
            );
            assert_eq!(kind.is_core(), expected, "{kind:?}");
        }
    }

    #[test]
    fn align_up_rounds_to_eight_byte_boundary() {
        assert_eq!(align_up(0), Some(0));
        assert_eq!(align_up(1), Some(8));
        assert_eq!(align_up(7), Some(8));
        assert_eq!(align_up(8), Some(8));
        assert_eq!(align_up(9), Some(16));
        assert_eq!(align_up(u32::MAX - 6), None);
    }

    #[test]
    fn checked_u32_surfaces_overflow_as_none() {
        assert_eq!(checked_u32(0), Some(0));
        assert_eq!(checked_u32(u32::MAX as usize), Some(u32::MAX));
        if usize::BITS > 32 {
            assert_eq!(checked_u32(u32::MAX as usize + 1), None);
        }
    }

    #[test]
    fn little_endian_helpers_round_trip() {
        let mut buf = Vec::new();
        write_u8(&mut buf, 0xAB);
        write_u16_le(&mut buf, 0x1234);
        write_u32_le(&mut buf, 0xDEAD_BEEF);
        assert_eq!(buf.len(), 7);
        assert_eq!(buf[0], 0xAB);
        assert_eq!(read_u16_le(&buf, 1), 0x1234);
        assert_eq!(read_u32_le(&buf, 3), 0xDEAD_BEEF);
    }

    #[test]
    fn magic_is_oxmf2ast() {
        assert_eq!(&SNAPSHOT_MAGIC, b"OXMF2AST");
    }

    #[test]
    fn header_values_lock_initial_v01() {
        assert_eq!(SNAPSHOT_MAJOR_VERSION, 0);
        assert_eq!(SNAPSHOT_MINOR_VERSION, 1);
        assert_eq!(SNAPSHOT_FEATURE_FLAGS, 0);
        assert_eq!(SECTION_ALIGNMENT, 8);
    }
}
