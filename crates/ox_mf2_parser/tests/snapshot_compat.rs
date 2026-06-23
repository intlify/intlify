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

use ox_mf2_parser::snapshot::format::{
    SectionKind, DIAGNOSTIC_LABEL_RECORD_SIZE, DIAGNOSTIC_RECORD_SIZE, EDGE_KIND_NODE,
    EDGE_KIND_TOKEN, EDGE_RECORD_SIZE, EXTENDED_DATA_HEADER_SIZE, HEADER_SIZE, NODE_RECORD_SIZE,
    ROOT_RECORD_SIZE, SECTION_ALIGNMENT, SECTION_RECORD_SIZE, SNAPSHOT_FEATURE_FLAGS,
    SNAPSHOT_MAGIC, SNAPSHOT_MAJOR_VERSION, SNAPSHOT_MINOR_VERSION, SOURCE_RECORD_SIZE,
    STRING_OFFSET_RECORD_SIZE, TOKEN_RECORD_SIZE, TRIVIA_RECORD_SIZE,
};
use ox_mf2_parser::snapshot::{decode_snapshot, DecodeErrorCode, SnapshotOptions};
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
fn decode_error_code_numeric_values_are_locked() {
    // The numeric values of `DecodeErrorCode` are part of the v0.1
    // surface so language bindings and fixture validators can match
    // on them without parsing display messages. Reordering or
    // inserting a variant must keep existing numbers stable; add a
    // new variant at the next unused number and extend this list.
    let expected: &[(DecodeErrorCode, u16)] = &[
        (DecodeErrorCode::BufferTooShort, 1),
        (DecodeErrorCode::InvalidMagic, 2),
        (DecodeErrorCode::UnsupportedMajorVersion, 3),
        (DecodeErrorCode::UnsupportedMinorVersion, 4),
        (DecodeErrorCode::InvalidHeaderLength, 5),
        (DecodeErrorCode::InvalidFeatureFlags, 6),
        (DecodeErrorCode::InvalidReservedField, 7),
        (DecodeErrorCode::SectionTableOutOfBounds, 8),
        (DecodeErrorCode::DuplicateSection, 9),
        (DecodeErrorCode::MissingRequiredSection, 10),
        (DecodeErrorCode::UnknownSection, 11),
        (DecodeErrorCode::UnknownRequiredSection, 12),
        (DecodeErrorCode::InvalidSectionFlags, 13),
        (DecodeErrorCode::InvalidSectionAlignment, 14),
        (DecodeErrorCode::InvalidSectionBounds, 15),
        (DecodeErrorCode::InvalidRecordSize, 16),
        (DecodeErrorCode::InvalidSectionCount, 17),
        (DecodeErrorCode::OverlappingSection, 18),
        (DecodeErrorCode::InvalidPadding, 19),
        (DecodeErrorCode::TrailingPadding, 20),
        (DecodeErrorCode::InvalidStringOffset, 21),
        (DecodeErrorCode::InvalidUtf8, 22),
        (DecodeErrorCode::InvalidStringRef, 23),
        (DecodeErrorCode::InvalidSourceRef, 24),
        (DecodeErrorCode::InvalidRootRef, 25),
        (DecodeErrorCode::InvalidNodeRef, 26),
        (DecodeErrorCode::InvalidTokenRef, 27),
        (DecodeErrorCode::InvalidTriviaRef, 28),
        (DecodeErrorCode::UnknownSyntaxKind, 29),
        (DecodeErrorCode::InvalidDiagnosticSeverity, 30),
        (DecodeErrorCode::UnknownDiagnosticCode, 31),
        (DecodeErrorCode::InvalidDiagnosticRange, 32),
        (DecodeErrorCode::InvalidSourceTextRange, 33),
        (DecodeErrorCode::InvalidExtendedData, 34),
        (DecodeErrorCode::InvalidEdgeKind, 35),
        (DecodeErrorCode::InvalidSpan, 36),
    ];
    for (code, expected_value) in expected {
        assert_eq!(
            code.as_u16(),
            *expected_value,
            "DecodeErrorCode::{code:?} discriminant changed"
        );
    }
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
