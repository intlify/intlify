// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! v0.1 snapshot compatibility guard tests.
//!
//! These tests lock in the surface that
//! `design/003-ox-mf2-binary-ast-format-changelog.md` calls out as
//! requiring a changelog entry on intentional change. A failure here
//! is a signal to update both the format design doc and the
//! changelog.

use std::fs;
use std::path::{Path, PathBuf};

use ox_mf2_parser::error::{
    SourceTextErrorCode, OX_MF2_API_ERROR_MIN, OX_MF2_DECODE_ERROR_MAX, OX_MF2_DECODE_ERROR_MIN,
    OX_MF2_SNAPSHOT_WRITE_ERROR_MAX, OX_MF2_SNAPSHOT_WRITE_ERROR_MIN, OX_MF2_SOURCE_TEXT_ERROR_MAX,
    OX_MF2_SOURCE_TEXT_ERROR_MIN,
};
use ox_mf2_parser::snapshot::format::{
    SectionKind, DIAGNOSTIC_LABEL_RECORD_SIZE, DIAGNOSTIC_RECORD_SIZE, EDGE_KIND_NODE,
    EDGE_KIND_TOKEN, EDGE_RECORD_SIZE, EXTENDED_DATA_HEADER_SIZE, HEADER_SIZE, NODE_RECORD_SIZE,
    ROOT_RECORD_SIZE, SECTION_ALIGNMENT, SECTION_RECORD_SIZE, SNAPSHOT_FEATURE_FLAGS,
    SNAPSHOT_MAGIC, SNAPSHOT_MAJOR_VERSION, SNAPSHOT_MINOR_VERSION, SOURCE_RECORD_SIZE,
    STRING_OFFSET_RECORD_SIZE, TOKEN_RECORD_SIZE, TRIVIA_RECORD_SIZE,
};
use ox_mf2_parser::snapshot::{
    decode_snapshot, DecodeErrorCode, SnapshotOptions, SnapshotWriteError, SnapshotWriteErrorCode,
};
use ox_mf2_parser::{parse_message_to_snapshot, ParseOptions};

#[test]
fn snapshot_default_options_match_design() {
    let opts = SnapshotOptions::default();
    assert!(opts.include_diagnostics);
    assert!(!opts.include_source_text);
    assert!(opts.include_trivia);
}

#[test]
fn v01_header_and_record_sizes_are_locked() {
    assert_eq!(SNAPSHOT_MAGIC, *b"OXMF2AST");
    assert_eq!(SNAPSHOT_MAJOR_VERSION, 0);
    assert_eq!(SNAPSHOT_MINOR_VERSION, 1);
    assert_eq!(SNAPSHOT_FEATURE_FLAGS, 0);
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
    assert_eq!(SECTION_ALIGNMENT, 8);
}

#[test]
fn section_kind_numeric_order_is_locked() {
    let order: [u16; 12] = [
        SectionKind::Roots.as_u16(),
        SectionKind::Sources.as_u16(),
        SectionKind::Nodes.as_u16(),
        SectionKind::Edges.as_u16(),
        SectionKind::Tokens.as_u16(),
        SectionKind::Trivia.as_u16(),
        SectionKind::Diagnostics.as_u16(),
        SectionKind::DiagnosticLabels.as_u16(),
        SectionKind::StringOffsets.as_u16(),
        SectionKind::StringData.as_u16(),
        SectionKind::SourceTextData.as_u16(),
        SectionKind::ExtendedData.as_u16(),
    ];
    assert_eq!(order, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
}

#[test]
fn edge_kind_numeric_values_are_locked() {
    assert_eq!(EDGE_KIND_NODE, 0);
    assert_eq!(EDGE_KIND_TOKEN, 1);
}

#[test]
fn decode_error_code_numeric_values_are_locked_with_evidence() {
    // The numeric values of `DecodeErrorCode` are part of the v0.1
    // surface so language bindings and fixture validators can match
    // on them without parsing display messages. Reordering or
    // inserting a variant must keep existing numbers stable; add a
    // new variant at the next unused number and extend this list.
    let expected: &[(DecodeErrorCode, u32)] = &[
        (DecodeErrorCode::BufferTooShort, 1000),
        (DecodeErrorCode::InvalidMagic, 1001),
        (DecodeErrorCode::UnsupportedMajorVersion, 1002),
        (DecodeErrorCode::UnsupportedMinorVersion, 1003),
        (DecodeErrorCode::InvalidHeaderLength, 1004),
        (DecodeErrorCode::InvalidFeatureFlags, 1005),
        (DecodeErrorCode::InvalidReservedField, 1006),
        (DecodeErrorCode::SectionTableOutOfBounds, 1007),
        (DecodeErrorCode::DuplicateSection, 1008),
        (DecodeErrorCode::MissingRequiredSection, 1009),
        (DecodeErrorCode::UnknownSection, 1010),
        (DecodeErrorCode::UnknownRequiredSection, 1011),
        (DecodeErrorCode::InvalidSectionFlags, 1012),
        (DecodeErrorCode::InvalidSectionAlignment, 1013),
        (DecodeErrorCode::InvalidSectionBounds, 1014),
        (DecodeErrorCode::InvalidRecordSize, 1015),
        (DecodeErrorCode::InvalidSectionCount, 1016),
        (DecodeErrorCode::OverlappingSection, 1017),
        (DecodeErrorCode::InvalidPadding, 1018),
        (DecodeErrorCode::TrailingPadding, 1019),
        (DecodeErrorCode::InvalidStringOffset, 1020),
        (DecodeErrorCode::InvalidUtf8, 1021),
        (DecodeErrorCode::InvalidStringRef, 1022),
        (DecodeErrorCode::InvalidSourceRef, 1023),
        (DecodeErrorCode::InvalidRootRef, 1024),
        (DecodeErrorCode::InvalidNodeRef, 1025),
        (DecodeErrorCode::InvalidTokenRef, 1026),
        (DecodeErrorCode::InvalidTriviaRef, 1027),
        (DecodeErrorCode::UnknownSyntaxKind, 1028),
        (DecodeErrorCode::InvalidDiagnosticSeverity, 1029),
        (DecodeErrorCode::UnknownDiagnosticCode, 1030),
        (DecodeErrorCode::InvalidDiagnosticRange, 1031),
        (DecodeErrorCode::InvalidSourceTextRange, 1032),
        (DecodeErrorCode::InvalidExtendedData, 1033),
        (DecodeErrorCode::InvalidEdgeKind, 1034),
        (DecodeErrorCode::InvalidSpan, 1035),
    ];
    eprintln!("EVIDENCE compat decode_error_code_numeric_values_are_locked begin");
    for (code, expected_value) in expected {
        eprintln!("EVIDENCE compat DecodeErrorCode::{code:?}={expected_value}");
        assert_eq!(
            code.as_u32(),
            *expected_value,
            "DecodeErrorCode::{code:?} discriminant changed"
        );
        assert!(
            (*expected_value >= OX_MF2_DECODE_ERROR_MIN
                && *expected_value <= OX_MF2_DECODE_ERROR_MAX),
            "DecodeErrorCode::{code:?} outside decode range"
        );
        assert!(
            *expected_value >= OX_MF2_API_ERROR_MIN,
            "DecodeErrorCode::{code:?} uses reserved low range"
        );
    }
    eprintln!("EVIDENCE compat InvalidMagic=1001 InvalidSpan=1035");
    eprintln!("EVIDENCE compat decode_error_code_numeric_values_are_locked end");
}

#[test]
fn snapshot_write_error_code_numeric_values_are_locked() {
    let expected: &[(SnapshotWriteError, SnapshotWriteErrorCode, u32)] = &[
        (
            SnapshotWriteError::SourceTooLarge,
            SnapshotWriteErrorCode::SourceTooLarge,
            2000,
        ),
        (
            SnapshotWriteError::TooManyRoots,
            SnapshotWriteErrorCode::TooManyRoots,
            2001,
        ),
        (
            SnapshotWriteError::TooManySources,
            SnapshotWriteErrorCode::TooManySources,
            2002,
        ),
        (
            SnapshotWriteError::TooManyStrings,
            SnapshotWriteErrorCode::TooManyStrings,
            2003,
        ),
        (
            SnapshotWriteError::TooManyNodes,
            SnapshotWriteErrorCode::TooManyNodes,
            2004,
        ),
        (
            SnapshotWriteError::TooManyEdges,
            SnapshotWriteErrorCode::TooManyEdges,
            2005,
        ),
        (
            SnapshotWriteError::TooManyTokens,
            SnapshotWriteErrorCode::TooManyTokens,
            2006,
        ),
        (
            SnapshotWriteError::TooManyTrivia,
            SnapshotWriteErrorCode::TooManyTrivia,
            2007,
        ),
        (
            SnapshotWriteError::TooManyDiagnostics,
            SnapshotWriteErrorCode::TooManyDiagnostics,
            2008,
        ),
        (
            SnapshotWriteError::TooManyDiagnosticLabels,
            SnapshotWriteErrorCode::TooManyDiagnosticLabels,
            2009,
        ),
        (
            SnapshotWriteError::SectionTooLarge,
            SnapshotWriteErrorCode::SectionTooLarge,
            2010,
        ),
        (
            SnapshotWriteError::MissingRoot,
            SnapshotWriteErrorCode::MissingRoot,
            2011,
        ),
        (
            SnapshotWriteError::InvalidSourceId,
            SnapshotWriteErrorCode::InvalidSourceId,
            2012,
        ),
        (
            SnapshotWriteError::InconsistentSourceId,
            SnapshotWriteErrorCode::InconsistentSourceId,
            2013,
        ),
    ];
    for (err, code, expected_value) in expected {
        assert_eq!(err.code(), *code);
        assert_eq!(code.as_u32(), *expected_value);
        assert_eq!(err.as_ox_mf2_error_code(), *expected_value);
        assert!(
            (*expected_value >= OX_MF2_SNAPSHOT_WRITE_ERROR_MIN
                && *expected_value <= OX_MF2_SNAPSHOT_WRITE_ERROR_MAX),
            "SnapshotWriteErrorCode::{code:?} outside write range"
        );
    }
}

#[test]
fn source_text_error_code_numeric_values_are_locked() {
    use ox_mf2_parser::snapshot::SourceTextUnavailable;
    use ox_mf2_parser::SourceStoreError;

    let cases: &[(u32, SourceTextErrorCode)] = &[
        (3000, SourceTextErrorCode::SourceTextNotIncluded),
        (3001, SourceTextErrorCode::SourceTextSpanOutOfBounds),
        (3002, SourceTextErrorCode::SourceTextTooLarge),
        (3003, SourceTextErrorCode::SourceTextCountMismatch),
        (3004, SourceTextErrorCode::SourceTextUnpairedSurrogate),
    ];
    for (expected_value, code) in cases {
        assert_eq!(code.as_u32(), *expected_value);
        assert!(
            (*expected_value >= OX_MF2_SOURCE_TEXT_ERROR_MIN
                && *expected_value <= OX_MF2_SOURCE_TEXT_ERROR_MAX),
            "SourceTextErrorCode::{code:?} outside source text range"
        );
    }

    assert_eq!(
        SourceTextUnavailable::NotIncluded.as_ox_mf2_error_code(),
        3000
    );
    assert_eq!(
        SourceTextUnavailable::SpanOutOfBounds.as_ox_mf2_error_code(),
        3001
    );
    assert_eq!(
        SourceStoreError::SourceTooLarge.as_ox_mf2_error_code(),
        3002
    );
}

#[test]
fn changelog_documents_v01() {
    let changelog = repo_root().join("design/003-ox-mf2-binary-ast-format-changelog.md");
    let body = fs::read_to_string(&changelog)
        .unwrap_or_else(|_| panic!("changelog missing at {}", changelog.display()));
    assert!(
        body.contains("## v0.1"),
        "format changelog must document v0.1"
    );
    assert!(
        body.contains("OXMF2AST"),
        "changelog must record the snapshot magic"
    );
    assert!(
        body.contains("major_version = 0"),
        "changelog must record the v0.1 major version"
    );
    assert!(
        body.contains("minor_version = 1"),
        "changelog must record the v0.1 minor version"
    );
}

#[test]
fn round_trip_helper_test_remains_stable() {
    // Tiny round-trip to exercise the encode → decode boundary so
    // any unrelated regression that breaks decoding shows up in the
    // compatibility guard as well. Uses `parse_message_to_snapshot`
    // so the test never relies on the `parse_message` + manual
    // `SourceStore` pattern that the API contract forbids.
    let snap = parse_message_to_snapshot(
        "Hi",
        None,
        ParseOptions::default(),
        SnapshotOptions::default(),
    )
    .unwrap();
    let view = decode_snapshot(&snap.bytes).expect("decode succeeds");
    assert_eq!(view.root_count(), 1);
}

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is the parser crate; the repository root is
    // two levels up.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .expect("crate manifest has two parent dirs")
        .to_path_buf()
}
