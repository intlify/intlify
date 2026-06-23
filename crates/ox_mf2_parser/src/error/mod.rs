// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Unified ox-mf2 API error code namespace.
//!
//! API failure codes start at [`OX_MF2_API_ERROR_MIN`]. Parser and snapshot
//! classification enums such as [`crate::DiagnosticCode`] and
//! [`crate::SyntaxKind`] are separate namespaces and are not managed here.
//!
//! Range policy: `design/appendix-ox-mf2-error-code.md`.

pub mod binding;
pub mod source_text;

pub use binding::{BindingValidationErrorCode, InitializationErrorCode};
pub use source_text::SourceTextErrorCode;

/// Unified numeric API error code exposed to language bindings.
pub type OxMf2ErrorCode = u32;

/// Minimum value for ox-mf2 API error codes. Values below this are reserved.
pub const OX_MF2_API_ERROR_MIN: OxMf2ErrorCode = 1000;

pub const OX_MF2_DECODE_ERROR_MIN: OxMf2ErrorCode = 1000;
pub const OX_MF2_DECODE_ERROR_MAX: OxMf2ErrorCode = 1999;

pub const OX_MF2_SNAPSHOT_WRITE_ERROR_MIN: OxMf2ErrorCode = 2000;
pub const OX_MF2_SNAPSHOT_WRITE_ERROR_MAX: OxMf2ErrorCode = 2999;

pub const OX_MF2_SOURCE_TEXT_ERROR_MIN: OxMf2ErrorCode = 3000;
pub const OX_MF2_SOURCE_TEXT_ERROR_MAX: OxMf2ErrorCode = 3999;

pub const OX_MF2_INITIALIZATION_ERROR_MIN: OxMf2ErrorCode = 10_000;
pub const OX_MF2_INITIALIZATION_ERROR_MAX: OxMf2ErrorCode = 10_999;

pub const OX_MF2_BINDING_VALIDATION_ERROR_MIN: OxMf2ErrorCode = 11_000;
pub const OX_MF2_BINDING_VALIDATION_ERROR_MAX: OxMf2ErrorCode = 11_999;

/// Domain classification for a unified API error code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OxMf2ErrorDomain {
    Decode,
    SnapshotWrite,
    SourceText,
    Initialization,
    BindingValidation,
    Unknown,
}

/// Classify an API error code into its domain.
#[inline]
pub const fn ox_mf2_error_domain(code: OxMf2ErrorCode) -> OxMf2ErrorDomain {
    if code >= OX_MF2_DECODE_ERROR_MIN && code <= OX_MF2_DECODE_ERROR_MAX {
        OxMf2ErrorDomain::Decode
    } else if code >= OX_MF2_SNAPSHOT_WRITE_ERROR_MIN && code <= OX_MF2_SNAPSHOT_WRITE_ERROR_MAX {
        OxMf2ErrorDomain::SnapshotWrite
    } else if code >= OX_MF2_SOURCE_TEXT_ERROR_MIN && code <= OX_MF2_SOURCE_TEXT_ERROR_MAX {
        OxMf2ErrorDomain::SourceText
    } else if code >= OX_MF2_INITIALIZATION_ERROR_MIN && code <= OX_MF2_INITIALIZATION_ERROR_MAX {
        OxMf2ErrorDomain::Initialization
    } else if code >= OX_MF2_BINDING_VALIDATION_ERROR_MIN
        && code <= OX_MF2_BINDING_VALIDATION_ERROR_MAX
    {
        OxMf2ErrorDomain::BindingValidation
    } else {
        OxMf2ErrorDomain::Unknown
    }
}

/// Stable programmatic name for an API error code.
///
/// Returns `"unknown"` for codes outside the managed namespaces.
pub fn ox_mf2_error_code_name(code: OxMf2ErrorCode) -> &'static str {
    if let Some(name) = decode_error_code_name(code) {
        return name;
    }
    if let Some(name) = snapshot_write_error_code_name(code) {
        return name;
    }
    if let Some(name) = source_text_error_code_name(code) {
        return name;
    }
    if let Some(name) = initialization_error_code_name(code) {
        return name;
    }
    if let Some(name) = binding_validation_error_code_name(code) {
        return name;
    }
    "unknown"
}

fn decode_error_code_name(code: OxMf2ErrorCode) -> Option<&'static str> {
    use crate::snapshot::DecodeErrorCode;
    match code {
        1000 => Some(DecodeErrorCode::BufferTooShort.name()),
        1001 => Some(DecodeErrorCode::InvalidMagic.name()),
        1002 => Some(DecodeErrorCode::UnsupportedMajorVersion.name()),
        1003 => Some(DecodeErrorCode::UnsupportedMinorVersion.name()),
        1004 => Some(DecodeErrorCode::InvalidHeaderLength.name()),
        1005 => Some(DecodeErrorCode::InvalidFeatureFlags.name()),
        1006 => Some(DecodeErrorCode::InvalidReservedField.name()),
        1007 => Some(DecodeErrorCode::SectionTableOutOfBounds.name()),
        1008 => Some(DecodeErrorCode::DuplicateSection.name()),
        1009 => Some(DecodeErrorCode::MissingRequiredSection.name()),
        1010 => Some(DecodeErrorCode::UnknownSection.name()),
        1011 => Some(DecodeErrorCode::UnknownRequiredSection.name()),
        1012 => Some(DecodeErrorCode::InvalidSectionFlags.name()),
        1013 => Some(DecodeErrorCode::InvalidSectionAlignment.name()),
        1014 => Some(DecodeErrorCode::InvalidSectionBounds.name()),
        1015 => Some(DecodeErrorCode::InvalidRecordSize.name()),
        1016 => Some(DecodeErrorCode::InvalidSectionCount.name()),
        1017 => Some(DecodeErrorCode::OverlappingSection.name()),
        1018 => Some(DecodeErrorCode::InvalidPadding.name()),
        1019 => Some(DecodeErrorCode::TrailingPadding.name()),
        1020 => Some(DecodeErrorCode::InvalidStringOffset.name()),
        1021 => Some(DecodeErrorCode::InvalidUtf8.name()),
        1022 => Some(DecodeErrorCode::InvalidStringRef.name()),
        1023 => Some(DecodeErrorCode::InvalidSourceRef.name()),
        1024 => Some(DecodeErrorCode::InvalidRootRef.name()),
        1025 => Some(DecodeErrorCode::InvalidNodeRef.name()),
        1026 => Some(DecodeErrorCode::InvalidTokenRef.name()),
        1027 => Some(DecodeErrorCode::InvalidTriviaRef.name()),
        1028 => Some(DecodeErrorCode::UnknownSyntaxKind.name()),
        1029 => Some(DecodeErrorCode::InvalidDiagnosticSeverity.name()),
        1030 => Some(DecodeErrorCode::UnknownDiagnosticCode.name()),
        1031 => Some(DecodeErrorCode::InvalidDiagnosticRange.name()),
        1032 => Some(DecodeErrorCode::InvalidSourceTextRange.name()),
        1033 => Some(DecodeErrorCode::InvalidExtendedData.name()),
        1034 => Some(DecodeErrorCode::InvalidEdgeKind.name()),
        1035 => Some(DecodeErrorCode::InvalidSpan.name()),
        _ => None,
    }
}

fn snapshot_write_error_code_name(code: OxMf2ErrorCode) -> Option<&'static str> {
    use crate::snapshot::SnapshotWriteErrorCode;
    match code {
        2000 => Some(SnapshotWriteErrorCode::SourceTooLarge.name()),
        2001 => Some(SnapshotWriteErrorCode::TooManyRoots.name()),
        2002 => Some(SnapshotWriteErrorCode::TooManySources.name()),
        2003 => Some(SnapshotWriteErrorCode::TooManyStrings.name()),
        2004 => Some(SnapshotWriteErrorCode::TooManyNodes.name()),
        2005 => Some(SnapshotWriteErrorCode::TooManyEdges.name()),
        2006 => Some(SnapshotWriteErrorCode::TooManyTokens.name()),
        2007 => Some(SnapshotWriteErrorCode::TooManyTrivia.name()),
        2008 => Some(SnapshotWriteErrorCode::TooManyDiagnostics.name()),
        2009 => Some(SnapshotWriteErrorCode::TooManyDiagnosticLabels.name()),
        2010 => Some(SnapshotWriteErrorCode::SectionTooLarge.name()),
        2011 => Some(SnapshotWriteErrorCode::MissingRoot.name()),
        2012 => Some(SnapshotWriteErrorCode::InvalidSourceId.name()),
        2013 => Some(SnapshotWriteErrorCode::InconsistentSourceId.name()),
        _ => None,
    }
}

fn source_text_error_code_name(code: OxMf2ErrorCode) -> Option<&'static str> {
    use SourceTextErrorCode::{
        SourceTextCountMismatch, SourceTextNotIncluded, SourceTextSpanOutOfBounds,
        SourceTextTooLarge, SourceTextUnpairedSurrogate,
    };
    match code {
        3000 => Some(SourceTextNotIncluded.name()),
        3001 => Some(SourceTextSpanOutOfBounds.name()),
        3002 => Some(SourceTextTooLarge.name()),
        3003 => Some(SourceTextCountMismatch.name()),
        3004 => Some(SourceTextUnpairedSurrogate.name()),
        _ => None,
    }
}

fn initialization_error_code_name(code: OxMf2ErrorCode) -> Option<&'static str> {
    use InitializationErrorCode::{NativeBindingUnavailable, WasmNotInitialized};
    match code {
        10_000 => Some(WasmNotInitialized.name()),
        10_001 => Some(NativeBindingUnavailable.name()),
        _ => None,
    }
}

fn binding_validation_error_code_name(code: OxMf2ErrorCode) -> Option<&'static str> {
    use BindingValidationErrorCode::InvalidOptions;
    match code {
        11_000 => Some(InvalidOptions.name()),
        _ => None,
    }
}
