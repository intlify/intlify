// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Invalid snapshot fixtures — each test mutates a known-valid
//! baseline in exactly one place and asserts that
//! [`decode_snapshot`] returns the matching [`DecodeErrorCode`]
//! without panicking.

use ox_mf2_parser::snapshot::{
    decode_snapshot, parse_source_to_snapshot, DecodeErrorCode, SectionKind, SnapshotOptions,
    HEADER_SIZE, SECTION_RECORD_SIZE,
};
use ox_mf2_parser::{ParseOptions, SourceFileInput, SourceStore};

fn baseline() -> Vec<u8> {
    let mut sources = SourceStore::new();
    let id = sources.add(SourceFileInput {
        source: "Hi",
        ..Default::default()
    });
    parse_source_to_snapshot(
        &sources,
        id,
        ParseOptions::default(),
        SnapshotOptions::default(),
    )
    .unwrap()
    .bytes
}

fn section_record_offset(bytes: &[u8], kind: SectionKind) -> Option<usize> {
    let count = u16::from_le_bytes(bytes[24..26].try_into().unwrap()) as usize;
    for i in 0..count {
        let off = HEADER_SIZE as usize + i * SECTION_RECORD_SIZE as usize;
        let k = u16::from_le_bytes(bytes[off..off + 2].try_into().unwrap());
        if k == kind.as_u16() {
            return Some(off);
        }
    }
    None
}

fn section_payload_offset(bytes: &[u8], kind: SectionKind) -> usize {
    let rec = section_record_offset(bytes, kind).expect("section present");
    u32::from_le_bytes(bytes[rec + 4..rec + 8].try_into().unwrap()) as usize
}

#[test]
fn bad_magic_is_rejected() {
    let mut bytes = baseline();
    bytes[0] = b'X';
    let err = decode_snapshot(&bytes).unwrap_err();
    assert_eq!(err.code, DecodeErrorCode::InvalidMagic);
}

#[test]
fn unsupported_major_version_is_rejected() {
    let mut bytes = baseline();
    bytes[8] = 0x99;
    let err = decode_snapshot(&bytes).unwrap_err();
    assert_eq!(err.code, DecodeErrorCode::UnsupportedMajorVersion);
}

#[test]
fn unsupported_minor_version_is_rejected() {
    let mut bytes = baseline();
    bytes[10] = 0x99;
    let err = decode_snapshot(&bytes).unwrap_err();
    assert_eq!(err.code, DecodeErrorCode::UnsupportedMinorVersion);
}

#[test]
fn nonzero_feature_flags_are_rejected() {
    let mut bytes = baseline();
    bytes[12] = 1;
    let err = decode_snapshot(&bytes).unwrap_err();
    assert_eq!(err.code, DecodeErrorCode::InvalidFeatureFlags);
}

#[test]
fn nonzero_header_reserved_is_rejected() {
    let mut bytes = baseline();
    bytes[26] = 1; // reserved u16 low byte
    let err = decode_snapshot(&bytes).unwrap_err();
    assert_eq!(err.code, DecodeErrorCode::InvalidReservedField);
}

#[test]
fn nonzero_header_reserved_tail_is_rejected() {
    let mut bytes = baseline();
    bytes[28] = 1; // reserved_tail u32 low byte
    let err = decode_snapshot(&bytes).unwrap_err();
    assert_eq!(err.code, DecodeErrorCode::InvalidReservedField);
}

#[test]
fn wrong_header_length_is_rejected() {
    let mut bytes = baseline();
    bytes[16] = 33;
    let err = decode_snapshot(&bytes).unwrap_err();
    assert_eq!(err.code, DecodeErrorCode::InvalidHeaderLength);
}

#[test]
fn buffer_shorter_than_header_is_rejected() {
    let bytes = vec![0u8; 16];
    let err = decode_snapshot(&bytes).unwrap_err();
    assert_eq!(err.code, DecodeErrorCode::BufferTooShort);
}

#[test]
fn missing_required_section_is_rejected() {
    // Drop the last entry from the section table — that entry is one
    // of the required core sections in the baseline (StringData),
    // because the canonical writer order ends with the string data
    // section in the no-trivia, no-diagnostics baseline.
    let mut bytes = baseline();
    let count = u16::from_le_bytes(bytes[24..26].try_into().unwrap());
    bytes[24..26].copy_from_slice(&(count - 1).to_le_bytes());
    let err = decode_snapshot(&bytes).unwrap_err();
    assert_eq!(err.code, DecodeErrorCode::MissingRequiredSection);
}

#[test]
fn duplicate_section_is_rejected() {
    let mut bytes = baseline();
    // Rewrite a non-Roots section record's kind to Roots.
    let off = section_record_offset(&bytes, SectionKind::Sources).expect("sources");
    bytes[off] = SectionKind::Roots.as_u16() as u8;
    bytes[off + 1] = 0;
    let err = decode_snapshot(&bytes).unwrap_err();
    assert_eq!(err.code, DecodeErrorCode::DuplicateSection);
}

#[test]
fn unknown_section_kind_is_rejected() {
    let mut bytes = baseline();
    // Change one section's kind to a value not in v0.1.
    let off = section_record_offset(&bytes, SectionKind::Edges).expect("edges");
    bytes[off] = 99; // not a known kind
    bytes[off + 1] = 0;
    let err = decode_snapshot(&bytes).unwrap_err();
    assert_eq!(err.code, DecodeErrorCode::UnknownSection);
}

#[test]
fn nonzero_section_reserved_is_rejected() {
    let mut bytes = baseline();
    let off = section_record_offset(&bytes, SectionKind::Roots).expect("roots");
    bytes[off + 19] = 1; // reserved byte
    let err = decode_snapshot(&bytes).unwrap_err();
    assert_eq!(err.code, DecodeErrorCode::InvalidReservedField);
}

#[test]
fn wrong_section_alignment_is_rejected() {
    let mut bytes = baseline();
    let off = section_record_offset(&bytes, SectionKind::Roots).expect("roots");
    bytes[off + 18] = 4; // alignment must be 8
    let err = decode_snapshot(&bytes).unwrap_err();
    assert_eq!(err.code, DecodeErrorCode::InvalidSectionAlignment);
}

#[test]
fn wrong_record_size_is_rejected() {
    let mut bytes = baseline();
    let off = section_record_offset(&bytes, SectionKind::Nodes).expect("nodes");
    bytes[off + 16] = 32; // record_size low byte (was 24)
    let err = decode_snapshot(&bytes).unwrap_err();
    assert_eq!(err.code, DecodeErrorCode::InvalidRecordSize);
}

#[test]
fn core_section_without_required_flag_is_rejected() {
    let mut bytes = baseline();
    let off = section_record_offset(&bytes, SectionKind::Roots).expect("roots");
    bytes[off + 2] = 0; // flags low byte (clear required)
    bytes[off + 3] = 0;
    let err = decode_snapshot(&bytes).unwrap_err();
    assert_eq!(err.code, DecodeErrorCode::InvalidSectionFlags);
}

#[test]
fn nonzero_padding_is_rejected() {
    let mut bytes = baseline();
    // The padding between the section table and the first section.
    // For the baseline with 7 sections, the section table ends at
    // 32 + 7*20 = 172 and the first section starts at 176 — so
    // bytes [172..176) are padding.
    let count = u16::from_le_bytes(bytes[24..26].try_into().unwrap()) as usize;
    let table_end = HEADER_SIZE as usize + count * SECTION_RECORD_SIZE as usize;
    assert!(bytes[table_end] == 0);
    bytes[table_end] = 0xFF;
    let err = decode_snapshot(&bytes).unwrap_err();
    assert_eq!(err.code, DecodeErrorCode::InvalidPadding);
}

#[test]
fn trailing_padding_is_rejected() {
    let mut bytes = baseline();
    bytes.push(0);
    let err = decode_snapshot(&bytes).unwrap_err();
    assert_eq!(err.code, DecodeErrorCode::TrailingPadding);
}

#[test]
fn out_of_range_root_node_is_rejected() {
    let mut bytes = baseline();
    let roots_off = section_payload_offset(&bytes, SectionKind::Roots);
    // root_node field is the first u32 of RootRecord.
    bytes[roots_off..roots_off + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    let err = decode_snapshot(&bytes).unwrap_err();
    assert_eq!(err.code, DecodeErrorCode::InvalidRootRef);
}

#[test]
fn out_of_range_root_source_is_rejected() {
    let mut bytes = baseline();
    let roots_off = section_payload_offset(&bytes, SectionKind::Roots);
    // source_id field is the second u32 of RootRecord.
    bytes[roots_off + 4..roots_off + 8].copy_from_slice(&u32::MAX.to_le_bytes());
    let err = decode_snapshot(&bytes).unwrap_err();
    assert_eq!(err.code, DecodeErrorCode::InvalidSourceRef);
}

#[test]
fn out_of_range_node_child_range_is_rejected() {
    let mut bytes = baseline();
    let nodes_off = section_payload_offset(&bytes, SectionKind::Nodes);
    // NodeRecord layout: kind u16, flags u16, span_start u32,
    // span_end u32, first_child u32, child_count u32, data_ref u32.
    // first_child is at offset 12; child_count at 16. Set first_child
    // to a huge value.
    bytes[nodes_off + 12..nodes_off + 16].copy_from_slice(&u32::MAX.to_le_bytes());
    let err = decode_snapshot(&bytes).unwrap_err();
    assert_eq!(err.code, DecodeErrorCode::InvalidNodeRef);
}

#[test]
fn unknown_syntax_kind_in_node_is_rejected() {
    let mut bytes = baseline();
    let nodes_off = section_payload_offset(&bytes, SectionKind::Nodes);
    // kind is the first u16 of the first NodeRecord.
    bytes[nodes_off..nodes_off + 2].copy_from_slice(&777u16.to_le_bytes());
    let err = decode_snapshot(&bytes).unwrap_err();
    assert_eq!(err.code, DecodeErrorCode::UnknownSyntaxKind);
}

#[test]
fn nonzero_node_flags_is_rejected() {
    let mut bytes = baseline();
    let nodes_off = section_payload_offset(&bytes, SectionKind::Nodes);
    // flags is the second u16.
    bytes[nodes_off + 2..nodes_off + 4].copy_from_slice(&1u16.to_le_bytes());
    let err = decode_snapshot(&bytes).unwrap_err();
    assert_eq!(err.code, DecodeErrorCode::InvalidReservedField);
}

#[test]
fn nonzero_token_reserved_tail_is_rejected() {
    let mut bytes = baseline();
    let tokens_off = section_payload_offset(&bytes, SectionKind::Tokens);
    // TokenRecord: kind u16, flags u16, span_start u32, span_end u32,
    // source_id u32, lead_start u32, lead_count u32, trail_start u32,
    // trail_count u32, reserved_tail u32. reserved_tail is at offset 32.
    bytes[tokens_off + 32..tokens_off + 36].copy_from_slice(&1u32.to_le_bytes());
    let err = decode_snapshot(&bytes).unwrap_err();
    assert_eq!(err.code, DecodeErrorCode::InvalidReservedField);
}

#[test]
fn out_of_range_edge_token_ref_is_rejected() {
    let mut bytes = baseline();
    let edges_off = section_payload_offset(&bytes, SectionKind::Edges);
    // First edge for "Hi" is a token edge. Set its ref_id huge.
    // EdgeRecord: kind u16, flags u16, ref_id u32. ref_id at offset 4.
    bytes[edges_off + 4..edges_off + 8].copy_from_slice(&u32::MAX.to_le_bytes());
    let err = decode_snapshot(&bytes).unwrap_err();
    // The first edge is a token edge — invalid token ref expected.
    assert_eq!(err.code, DecodeErrorCode::InvalidTokenRef);
}

#[test]
fn invalid_edge_kind_is_rejected() {
    let mut bytes = baseline();
    let edges_off = section_payload_offset(&bytes, SectionKind::Edges);
    bytes[edges_off..edges_off + 2].copy_from_slice(&2u16.to_le_bytes());
    let err = decode_snapshot(&bytes).unwrap_err();
    assert_eq!(err.code, DecodeErrorCode::InvalidEdgeKind);
}

#[test]
fn invalid_source_text_sentinel_is_rejected() {
    let mut bytes = baseline();
    let sources_off = section_payload_offset(&bytes, SectionKind::Sources);
    // SourceRecord text: source_id u32 @ 20, offset u32 @ 24,
    // len u32 @ 28. Default has source_id = NONE_REF, offset = 0,
    // len = 0 — flipping len to 1 breaks the canonical sentinel.
    bytes[sources_off + 28..sources_off + 32].copy_from_slice(&1u32.to_le_bytes());
    let err = decode_snapshot(&bytes).unwrap_err();
    assert_eq!(err.code, DecodeErrorCode::InvalidSourceTextRange);
}

#[test]
fn nonzero_node_data_ref_is_rejected() {
    let mut bytes = baseline();
    let nodes_off = section_payload_offset(&bytes, SectionKind::Nodes);
    // data_ref is the last u32 of NodeRecord at offset 20.
    bytes[nodes_off + 20..nodes_off + 24].copy_from_slice(&0u32.to_le_bytes());
    let err = decode_snapshot(&bytes).unwrap_err();
    assert_eq!(err.code, DecodeErrorCode::InvalidExtendedData);
}

#[test]
fn decoder_does_not_panic_on_random_garbage() {
    // 4 KB of garbage shouldn't crash the decoder.
    let bytes: Vec<u8> = (0..4096).map(|i| (i & 0xFF) as u8).collect();
    let err = decode_snapshot(&bytes).unwrap_err();
    // First check that fails for non-magic input is InvalidMagic.
    assert_eq!(err.code, DecodeErrorCode::InvalidMagic);
}
