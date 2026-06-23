// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Snapshot encode-time and decode-time errors.
//!
//! `SnapshotWriteError` covers cases where trusted parser output cannot
//! be encoded into the v0.1 format (overflow, missing root, invalid
//! source id). `DecodeError` covers validating untrusted snapshot bytes
//! and never panics on malformed input.

use crate::snapshot::format::SectionKind;

/// Encode-time failure produced by the snapshot writer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotWriteError {
    /// Source span exceeded the `u32` byte-offset domain.
    SourceTooLarge,
    /// Root count exceeded `u32::MAX`.
    TooManyRoots,
    /// Source count exceeded `u32::MAX`.
    TooManySources,
    /// Interned string count exceeded `u32::MAX`.
    TooManyStrings,
    /// Node count exceeded `u32::MAX`.
    TooManyNodes,
    /// Edge count exceeded `u32::MAX`.
    TooManyEdges,
    /// Token count exceeded `u32::MAX`.
    TooManyTokens,
    /// Trivia count exceeded `u32::MAX`.
    TooManyTrivia,
    /// Diagnostic count exceeded `u32::MAX`.
    TooManyDiagnostics,
    /// Diagnostic label count exceeded `u32::MAX`.
    TooManyDiagnosticLabels,
    /// A section byte length or final buffer length exceeded the `u32`
    /// domain.
    SectionTooLarge,
    /// Parser produced no root node — writer refuses to synthesise one.
    MissingRoot,
    /// A record referenced a Phase 1 `SourceId` that does not exist in
    /// the supplied `SourceStore`.
    InvalidSourceId,
}

impl core::fmt::Display for SnapshotWriteError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::SourceTooLarge => "source length exceeds u32::MAX byte offsets",
            Self::TooManyRoots => "root count exceeds u32::MAX",
            Self::TooManySources => "source count exceeds u32::MAX",
            Self::TooManyStrings => "interned string count exceeds u32::MAX",
            Self::TooManyNodes => "node count exceeds u32::MAX",
            Self::TooManyEdges => "edge count exceeds u32::MAX",
            Self::TooManyTokens => "token count exceeds u32::MAX",
            Self::TooManyTrivia => "trivia count exceeds u32::MAX",
            Self::TooManyDiagnostics => "diagnostic count exceeds u32::MAX",
            Self::TooManyDiagnosticLabels => "diagnostic label count exceeds u32::MAX",
            Self::SectionTooLarge => "snapshot section byte length exceeds u32::MAX",
            Self::MissingRoot => "parse result has no root node",
            Self::InvalidSourceId => "record references a SourceId that is not in SourceStore",
        })
    }
}

impl std::error::Error for SnapshotWriteError {}

/// Programmatic decode failure code.
///
/// Code values are stable across the v0.1 surface so tests, fixture
/// validators, and language bindings can match on them without
/// parsing human-readable messages. The `#[repr(u16)]` with explicit
/// discriminants is the enforcement mechanism: reordering or
/// inserting a variant in the wrong place would change a stable
/// numeric value, and the `snapshot_compat.rs` guard test catches
/// it. When adding a new variant, append it at the end with the next
/// unused number and update the guard test in the same commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum DecodeErrorCode {
    BufferTooShort = 1,
    InvalidMagic = 2,
    UnsupportedMajorVersion = 3,
    UnsupportedMinorVersion = 4,
    InvalidHeaderLength = 5,
    InvalidFeatureFlags = 6,
    InvalidReservedField = 7,
    SectionTableOutOfBounds = 8,
    DuplicateSection = 9,
    MissingRequiredSection = 10,
    UnknownSection = 11,
    UnknownRequiredSection = 12,
    InvalidSectionFlags = 13,
    InvalidSectionAlignment = 14,
    InvalidSectionBounds = 15,
    InvalidRecordSize = 16,
    InvalidSectionCount = 17,
    OverlappingSection = 18,
    InvalidPadding = 19,
    TrailingPadding = 20,
    InvalidStringOffset = 21,
    InvalidUtf8 = 22,
    InvalidStringRef = 23,
    InvalidSourceRef = 24,
    InvalidRootRef = 25,
    InvalidNodeRef = 26,
    InvalidTokenRef = 27,
    InvalidTriviaRef = 28,
    UnknownSyntaxKind = 29,
    InvalidDiagnosticSeverity = 30,
    UnknownDiagnosticCode = 31,
    InvalidDiagnosticRange = 32,
    InvalidSourceTextRange = 33,
    InvalidExtendedData = 34,
    InvalidEdgeKind = 35,
    InvalidSpan = 36,
}

impl DecodeErrorCode {
    /// Stable numeric wire value used by tests, fixture validators,
    /// and language bindings. The mapping is locked by the
    /// compatibility guard in `tests/snapshot_compat.rs`.
    #[inline]
    pub const fn as_u16(self) -> u16 {
        self as u16
    }
}

impl core::fmt::Display for DecodeErrorCode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::BufferTooShort => "snapshot buffer is shorter than the v0.1 header",
            Self::InvalidMagic => "snapshot magic does not match OXMF2AST",
            Self::UnsupportedMajorVersion => "snapshot major version is not 0",
            Self::UnsupportedMinorVersion => "snapshot minor version is not the supported draft",
            Self::InvalidHeaderLength => "snapshot header length is not 32",
            Self::InvalidFeatureFlags => "snapshot feature flags must be 0 in v0.1",
            Self::InvalidReservedField => "reserved field is non-zero",
            Self::SectionTableOutOfBounds => "section table extends past the snapshot buffer",
            Self::DuplicateSection => "section table contains the same SectionKind more than once",
            Self::MissingRequiredSection => "required core section is missing",
            Self::UnknownSection => "section table contains an unknown SectionKind",
            Self::UnknownRequiredSection => "unknown SectionKind has SectionFlags.required = true",
            Self::InvalidSectionFlags => "section flags do not match v0.1 strict rules",
            Self::InvalidSectionAlignment => "section is not 8-byte aligned",
            Self::InvalidSectionBounds => "section offset + byte_len exceeds buffer length",
            Self::InvalidRecordSize => "section record size does not match its SectionKind",
            Self::InvalidSectionCount => "section count is invalid for its SectionKind",
            Self::OverlappingSection => "two sections cover overlapping byte ranges",
            Self::InvalidPadding => "padding between sections contains non-zero bytes",
            Self::TrailingPadding => "snapshot buffer has trailing bytes after the last section",
            Self::InvalidStringOffset => "string offset is out of range",
            Self::InvalidUtf8 => "string data is not valid UTF-8",
            Self::InvalidStringRef => "StringRef points outside the string offsets section",
            Self::InvalidSourceRef => "SourceId points outside the sources section",
            Self::InvalidRootRef => "RootRecord.root_node points outside the nodes section",
            Self::InvalidNodeRef => "NodeId points outside the nodes section",
            Self::InvalidTokenRef => "TokenId points outside the tokens section",
            Self::InvalidTriviaRef => "TriviaId points outside the trivia section",
            Self::UnknownSyntaxKind => "record references an unknown SyntaxKind value",
            Self::InvalidDiagnosticSeverity => "diagnostic severity is not a known v0.1 value",
            Self::UnknownDiagnosticCode => "diagnostic code is not a known v0.1 value",
            Self::InvalidDiagnosticRange => {
                "diagnostic label range extends past the labels section"
            }
            Self::InvalidSourceTextRange => {
                "source text range extends past the source text data section"
            }
            Self::InvalidExtendedData => "extended data section is malformed",
            Self::InvalidEdgeKind => "edge kind is not 0 (node) or 1 (token)",
            Self::InvalidSpan => "record span has span_start > span_end",
        })
    }
}

/// Decode-time failure with optional context fields.
///
/// Public Rust decoder APIs return this type as `Result::Err`; bindings
/// translate it into their own error boundary while preserving `code`,
/// `section`, `offset`, and `index`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeError {
    pub code: DecodeErrorCode,
    pub section: Option<SectionKind>,
    pub offset: Option<u32>,
    pub index: Option<u32>,
}

impl DecodeError {
    /// Construct an error with only a code.
    pub const fn new(code: DecodeErrorCode) -> Self {
        Self {
            code,
            section: None,
            offset: None,
            index: None,
        }
    }

    /// Attach the section the failure was found in.
    #[must_use]
    pub const fn with_section(mut self, section: SectionKind) -> Self {
        self.section = Some(section);
        self
    }

    /// Attach the byte offset the failure was found at.
    #[must_use]
    pub const fn with_offset(mut self, offset: u32) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Attach the record index the failure was found at.
    #[must_use]
    pub const fn with_index(mut self, index: u32) -> Self {
        self.index = Some(index);
        self
    }
}

impl core::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.code)?;
        if let Some(section) = self.section {
            write!(f, " (section {section:?})")?;
        }
        if let Some(offset) = self.offset {
            write!(f, " at offset {offset}")?;
        }
        if let Some(index) = self.index {
            write!(f, " at index {index}")?;
        }
        Ok(())
    }
}

impl std::error::Error for DecodeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_error_carries_context() {
        let err = DecodeError::new(DecodeErrorCode::InvalidStringOffset)
            .with_section(SectionKind::StringOffsets)
            .with_offset(64)
            .with_index(3);
        assert_eq!(err.code, DecodeErrorCode::InvalidStringOffset);
        assert_eq!(err.section, Some(SectionKind::StringOffsets));
        assert_eq!(err.offset, Some(64));
        assert_eq!(err.index, Some(3));
    }

    #[test]
    fn write_errors_have_distinct_messages() {
        use SnapshotWriteError::*;
        let codes = [
            SourceTooLarge,
            TooManyRoots,
            TooManySources,
            TooManyStrings,
            TooManyNodes,
            TooManyEdges,
            TooManyTokens,
            TooManyTrivia,
            TooManyDiagnostics,
            TooManyDiagnosticLabels,
            SectionTooLarge,
            MissingRoot,
            InvalidSourceId,
        ];
        let mut seen = std::collections::HashSet::new();
        for code in codes {
            assert!(
                seen.insert(format!("{code}")),
                "duplicate message for {code:?}"
            );
        }
    }
}
