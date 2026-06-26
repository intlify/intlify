// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Snapshot decoder.
//!
//! Eagerly validates header, section table, every section payload, and
//! cross-section references so that [`crate::snapshot::SnapshotView`]
//! accessors can stay allocation-free. Malformed bytes return a
//! [`crate::snapshot::DecodeError`] rather than panicking.

use std::sync::Arc;

use crate::diagnostic::DiagnosticCode;
use crate::snapshot::error::{DecodeError, DecodeErrorCode};
use crate::snapshot::format::{
    read_u16_le, read_u32_le, SectionKind, DIAGNOSTIC_LABEL_RECORD_SIZE, DIAGNOSTIC_RECORD_SIZE,
    EDGE_KIND_NODE, EDGE_KIND_TOKEN, EDGE_RECORD_SIZE, HEADER_SIZE, NODE_RECORD_SIZE, NONE_REF,
    ROOT_RECORD_SIZE, SECTION_ALIGNMENT, SECTION_FLAG_REQUIRED, SECTION_RECORD_SIZE,
    SNAPSHOT_FEATURE_FLAGS, SNAPSHOT_MAGIC, SNAPSHOT_MAJOR_VERSION, SNAPSHOT_MINOR_VERSION,
    SOURCE_RECORD_SIZE, STRING_OFFSET_RECORD_SIZE, TOKEN_RECORD_SIZE, TRIVIA_RECORD_SIZE,
};
use crate::snapshot::view::{SectionIndex, SectionSlice, SnapshotView, SnapshotViewOwned};
use crate::syntax_kind::SyntaxKind;

/// Decode and validate a borrowed snapshot buffer.
pub fn decode_snapshot(bytes: &[u8]) -> Result<SnapshotView<'_>, DecodeError> {
    let sections = validate_and_build_index(bytes)?;
    Ok(SnapshotView::from_validated(bytes, sections))
}

/// Decode an owned snapshot buffer, sharing ownership via [`Arc<[u8]>`].
pub fn decode_snapshot_owned(
    bytes: impl Into<Arc<[u8]>>,
) -> Result<SnapshotViewOwned, DecodeError> {
    let arc = bytes.into();
    let sections = validate_and_build_index(&arc)?;
    Ok(SnapshotViewOwned::from_validated(arc, sections))
}

pub(crate) fn validate_and_build_index(bytes: &[u8]) -> Result<SectionIndex, DecodeError> {
    if (bytes.len() as u64) < HEADER_SIZE as u64 {
        return Err(DecodeError::new(DecodeErrorCode::BufferTooShort));
    }
    if bytes[0..8] != SNAPSHOT_MAGIC {
        return Err(DecodeError::new(DecodeErrorCode::InvalidMagic));
    }
    let major = read_u16_le(bytes, 8);
    let minor = read_u16_le(bytes, 10);
    if major != SNAPSHOT_MAJOR_VERSION {
        return Err(DecodeError::new(DecodeErrorCode::UnsupportedMajorVersion));
    }
    if minor != SNAPSHOT_MINOR_VERSION {
        return Err(DecodeError::new(DecodeErrorCode::UnsupportedMinorVersion));
    }
    let feature_flags = read_u32_le(bytes, 12);
    if feature_flags != SNAPSHOT_FEATURE_FLAGS {
        return Err(DecodeError::new(DecodeErrorCode::InvalidFeatureFlags));
    }
    let header_len = read_u32_le(bytes, 16);
    if header_len != HEADER_SIZE {
        return Err(DecodeError::new(DecodeErrorCode::InvalidHeaderLength));
    }
    let section_table_offset = read_u32_le(bytes, 20);
    if section_table_offset != HEADER_SIZE {
        return Err(DecodeError::new(DecodeErrorCode::InvalidHeaderLength));
    }
    let section_count = read_u16_le(bytes, 24);
    let reserved = read_u16_le(bytes, 26);
    if reserved != 0 {
        return Err(DecodeError::new(DecodeErrorCode::InvalidReservedField));
    }
    let reserved_tail = read_u32_le(bytes, 28);
    if reserved_tail != 0 {
        return Err(DecodeError::new(DecodeErrorCode::InvalidReservedField));
    }

    let section_table_byte_len = (section_count as u64) * (SECTION_RECORD_SIZE as u64);
    let section_table_end = (HEADER_SIZE as u64).saturating_add(section_table_byte_len);
    if section_table_end > bytes.len() as u64 {
        return Err(DecodeError::new(DecodeErrorCode::SectionTableOutOfBounds));
    }

    // Decode every section record.
    let mut records: Vec<DecodedSection> = Vec::with_capacity(section_count as usize);
    let mut seen_kinds: u32 = 0;
    for i in 0..section_count {
        let rec_offset = HEADER_SIZE as usize + (i as usize) * (SECTION_RECORD_SIZE as usize);
        let kind_raw = read_u16_le(bytes, rec_offset);
        let kind = SectionKind::from_u16(kind_raw).ok_or_else(|| {
            DecodeError::new(DecodeErrorCode::UnknownSection).with_index(i as u32)
        })?;
        let bit = 1u32 << (kind.as_u16() as u32);
        if (seen_kinds & bit) != 0 {
            return Err(DecodeError::new(DecodeErrorCode::DuplicateSection)
                .with_section(kind)
                .with_index(i as u32));
        }
        seen_kinds |= bit;

        let flags = read_u16_le(bytes, rec_offset + 2);
        let offset = read_u32_le(bytes, rec_offset + 4);
        let byte_len = read_u32_le(bytes, rec_offset + 8);
        let count = read_u32_le(bytes, rec_offset + 12);
        let record_size = read_u16_le(bytes, rec_offset + 14 + 2);
        let alignment = bytes[rec_offset + 18];
        let reserved = bytes[rec_offset + 19];

        if reserved != 0 {
            return Err(DecodeError::new(DecodeErrorCode::InvalidReservedField)
                .with_section(kind)
                .with_index(i as u32));
        }
        if alignment != SECTION_ALIGNMENT as u8 {
            return Err(DecodeError::new(DecodeErrorCode::InvalidSectionAlignment)
                .with_section(kind)
                .with_index(i as u32));
        }
        if record_size != kind.record_size() {
            return Err(DecodeError::new(DecodeErrorCode::InvalidRecordSize)
                .with_section(kind)
                .with_index(i as u32));
        }
        // Section flags: only `required` bit is allowed in v0.1.
        if (flags & !SECTION_FLAG_REQUIRED) != 0 {
            return Err(DecodeError::new(DecodeErrorCode::InvalidSectionFlags)
                .with_section(kind)
                .with_index(i as u32));
        }
        let required = (flags & SECTION_FLAG_REQUIRED) != 0;
        if kind.is_core() && !required {
            return Err(DecodeError::new(DecodeErrorCode::InvalidSectionFlags)
                .with_section(kind)
                .with_index(i as u32));
        }
        if !kind.is_core() && required {
            return Err(DecodeError::new(DecodeErrorCode::InvalidSectionFlags)
                .with_section(kind)
                .with_index(i as u32));
        }

        // Alignment.
        if (offset % SECTION_ALIGNMENT) != 0 {
            return Err(DecodeError::new(DecodeErrorCode::InvalidSectionAlignment)
                .with_section(kind)
                .with_offset(offset));
        }
        // Bounds.
        let end = (offset as u64).saturating_add(byte_len as u64);
        if end > bytes.len() as u64 {
            return Err(DecodeError::new(DecodeErrorCode::InvalidSectionBounds)
                .with_section(kind)
                .with_offset(offset));
        }
        // Fixed-record section consistency.
        if record_size != 0 {
            let expected = (count as u64) * (record_size as u64);
            if expected != byte_len as u64 {
                return Err(DecodeError::new(DecodeErrorCode::InvalidSectionCount)
                    .with_section(kind)
                    .with_offset(offset));
            }
        }
        records.push(DecodedSection {
            kind,
            offset,
            byte_len,
            count,
        });
    }

    // Required core sections must all be present.
    for kind in [
        SectionKind::Roots,
        SectionKind::Sources,
        SectionKind::Nodes,
        SectionKind::Edges,
        SectionKind::Tokens,
        SectionKind::StringOffsets,
        SectionKind::StringData,
    ] {
        let bit = 1u32 << (kind.as_u16() as u32);
        if (seen_kinds & bit) == 0 {
            return Err(
                DecodeError::new(DecodeErrorCode::MissingRequiredSection).with_section(kind)
            );
        }
    }

    // Sort by offset; break ties by placing empty payloads before
    // non-empty ones at the same aligned offset. Without the tie
    // breaker, a section table that lists a non-empty section before
    // an empty section that shares its offset would advance `cursor`
    // past the empty section's offset and then reject the empty
    // section as overlapping. The format contract permits the section
    // table to enumerate sections in any order, so this normalisation
    // keeps decode independent of physical table order.
    let mut sorted_indexes: Vec<usize> = (0..records.len()).collect();
    sorted_indexes.sort_by_key(|i| {
        let rec = records[*i];
        (rec.offset, rec.byte_len != 0)
    });
    let mut cursor = HEADER_SIZE + (section_count as u32) * SECTION_RECORD_SIZE;
    for (sorted_pos, src_index) in sorted_indexes.iter().enumerate() {
        let rec = records[*src_index];
        if rec.offset < cursor {
            // Either overlaps the previous section or the section table.
            if sorted_pos == 0 {
                return Err(DecodeError::new(DecodeErrorCode::SectionTableOutOfBounds)
                    .with_section(rec.kind));
            }
            return Err(
                DecodeError::new(DecodeErrorCode::OverlappingSection).with_section(rec.kind)
            );
        }
        // Padding bytes between sections must be zero.
        for byte in &bytes[cursor as usize..rec.offset as usize] {
            if *byte != 0 {
                return Err(DecodeError::new(DecodeErrorCode::InvalidPadding)
                    .with_section(rec.kind)
                    .with_offset(cursor));
            }
        }
        cursor = rec
            .offset
            .checked_add(rec.byte_len)
            .ok_or_else(|| DecodeError::new(DecodeErrorCode::InvalidSectionBounds))?;
    }
    // No trailing padding after the last section.
    if (cursor as usize) != bytes.len() {
        return Err(DecodeError::new(DecodeErrorCode::TrailingPadding).with_offset(cursor));
    }

    // Look up section slices by kind.
    let mut roots = None;
    let mut sources = None;
    let mut nodes = None;
    let mut edges = None;
    let mut tokens = None;
    let mut trivia = None;
    let mut diagnostics = None;
    let mut diagnostic_labels = None;
    let mut string_offsets = None;
    let mut string_data = None;
    let mut source_text_data = None;
    let mut extended_data = None;
    for rec in &records {
        let slice = SectionSlice {
            offset: rec.offset,
            byte_len: rec.byte_len,
            count: rec.count,
        };
        match rec.kind {
            SectionKind::Roots => roots = Some(slice),
            SectionKind::Sources => sources = Some(slice),
            SectionKind::Nodes => nodes = Some(slice),
            SectionKind::Edges => edges = Some(slice),
            SectionKind::Tokens => tokens = Some(slice),
            SectionKind::Trivia => trivia = Some(slice),
            SectionKind::Diagnostics => diagnostics = Some(slice),
            SectionKind::DiagnosticLabels => diagnostic_labels = Some(slice),
            SectionKind::StringOffsets => string_offsets = Some(slice),
            SectionKind::StringData => string_data = Some(slice),
            SectionKind::SourceTextData => source_text_data = Some(slice),
            SectionKind::ExtendedData => extended_data = Some(slice),
        }
    }
    let roots = roots.expect("required Roots section validated above");
    let sources = sources.expect("required Sources section validated above");
    let nodes = nodes.expect("required Nodes section validated above");
    let edges = edges.expect("required Edges section validated above");
    let tokens = tokens.expect("required Tokens section validated above");
    let string_offsets = string_offsets.expect("required StringOffsets validated above");
    let string_data = string_data.expect("required StringData validated above");

    // Core-section minimum counts.
    if roots.count < 1 {
        return Err(
            DecodeError::new(DecodeErrorCode::InvalidSectionCount).with_section(SectionKind::Roots)
        );
    }
    if sources.count < 1 {
        return Err(DecodeError::new(DecodeErrorCode::InvalidSectionCount)
            .with_section(SectionKind::Sources));
    }
    if nodes.count < 1 {
        return Err(
            DecodeError::new(DecodeErrorCode::InvalidSectionCount).with_section(SectionKind::Nodes)
        );
    }

    // ── String table validation ────────────────────────────────────
    validate_string_offsets(bytes, string_offsets, string_data)?;

    // ── Source records: StringRef + base_offset + SourceTextRef ────
    validate_source_records(bytes, sources, string_offsets.count, source_text_data)?;

    // ── Root records ───────────────────────────────────────────────
    validate_root_records(
        bytes,
        roots,
        nodes.count,
        sources.count,
        diagnostics.map_or(0, |s| s.count),
    )?;

    // ── Node records (kind, span, child range, data_ref) ───────────
    validate_node_records(bytes, nodes, edges.count)?;

    // ── Edge records (kind, ref_id) ────────────────────────────────
    validate_edge_records(bytes, edges, nodes.count, tokens.count)?;

    // ── Token records ──────────────────────────────────────────────
    validate_token_records(bytes, tokens, sources.count, trivia.map_or(0, |s| s.count))?;

    // ── Trivia records ─────────────────────────────────────────────
    if let Some(t) = trivia {
        validate_trivia_records(bytes, t, sources.count)?;
    }

    // ── Diagnostic + label records ────────────────────────────────
    if let Some(d) = diagnostics {
        validate_diagnostic_records(
            bytes,
            d,
            sources.count,
            string_offsets.count,
            diagnostic_labels.map_or(0, |s| s.count),
        )?;
    }
    if let Some(l) = diagnostic_labels {
        validate_diagnostic_label_records(bytes, l, sources.count, string_offsets.count)?;
    }

    // ── Extended data ─────────────────────────────────────────────
    if let Some(ed) = extended_data {
        // v0.1 writer does not emit extended data, but decoders must
        // still accept a zero-byte section that exists for forward
        // compatibility.
        if ed.byte_len > 0 {
            // Without a valid ExtendedDataHeader the decoder cannot
            // walk the section; v0.1 treats non-empty extended data
            // as InvalidExtendedData unless every NodeRecord that
            // references it has been validated. Defensive default:
            // reject non-empty extended data in v0.1 writer output.
            return Err(DecodeError::new(DecodeErrorCode::InvalidExtendedData)
                .with_section(SectionKind::ExtendedData)
                .with_offset(ed.offset));
        }
    }

    Ok(SectionIndex {
        roots,
        sources,
        nodes,
        edges,
        tokens,
        trivia,
        diagnostics,
        diagnostic_labels,
        string_offsets,
        string_data,
        source_text_data,
        extended_data,
    })
}

#[derive(Debug, Clone, Copy)]
struct DecodedSection {
    kind: SectionKind,
    offset: u32,
    byte_len: u32,
    count: u32,
}

fn validate_string_offsets(
    bytes: &[u8],
    offsets: SectionSlice,
    data: SectionSlice,
) -> Result<(), DecodeError> {
    // Validate the concatenated buffer first so per-slice checks
    // below are guaranteed to land on a UTF-8 boundary check (not a
    // bytes-not-in-bounds check).
    let data_start = data.offset as usize;
    let data_end = data_start + data.byte_len as usize;
    if core::str::from_utf8(&bytes[data_start..data_end]).is_err() {
        return Err(
            DecodeError::new(DecodeErrorCode::InvalidUtf8).with_section(SectionKind::StringData)
        );
    }
    let mut prev_end: u32 = 0;
    for i in 0..offsets.count {
        let rec_offset =
            offsets.offset as usize + (i as usize) * STRING_OFFSET_RECORD_SIZE as usize;
        let off = read_u32_le(bytes, rec_offset);
        let len = read_u32_le(bytes, rec_offset + 4);
        if off < prev_end {
            return Err(DecodeError::new(DecodeErrorCode::InvalidStringOffset)
                .with_section(SectionKind::StringOffsets)
                .with_index(i));
        }
        let end = off
            .checked_add(len)
            .ok_or_else(|| DecodeError::new(DecodeErrorCode::InvalidStringOffset))?;
        if end > data.byte_len {
            return Err(DecodeError::new(DecodeErrorCode::InvalidStringOffset)
                .with_section(SectionKind::StringOffsets)
                .with_index(i));
        }
        // Per-slice UTF-8 boundary check: the full StringData buffer
        // can be valid UTF-8 even when an offset record splits a
        // multibyte scalar. Catching that here means
        // `SnapshotView::string` never silently returns `None` for an
        // in-range reference.
        let abs_start = data_start + off as usize;
        let abs_end = data_start + end as usize;
        if core::str::from_utf8(&bytes[abs_start..abs_end]).is_err() {
            return Err(DecodeError::new(DecodeErrorCode::InvalidUtf8)
                .with_section(SectionKind::StringOffsets)
                .with_index(i));
        }
        prev_end = end;
    }
    Ok(())
}

/// Reject inverted snapshot spans (`span_start > span_end`).
///
/// All snapshot record spans use UTF-8 byte offsets and the half-
/// open interval `[start, end)`. `start == end` is the canonical
/// empty span, but `start > end` is not representable and would
/// surface as a nonsensical `Span` through view accessors.
#[inline]
fn check_span(start: u32, end: u32, section: SectionKind, index: u32) -> Result<(), DecodeError> {
    if start > end {
        return Err(DecodeError::new(DecodeErrorCode::InvalidSpan)
            .with_section(section)
            .with_index(index));
    }
    Ok(())
}

fn check_string_ref(
    raw: u32,
    string_count: u32,
    section: SectionKind,
    index: u32,
) -> Result<(), DecodeError> {
    if raw == NONE_REF {
        return Ok(());
    }
    if raw >= string_count {
        return Err(DecodeError::new(DecodeErrorCode::InvalidStringRef)
            .with_section(section)
            .with_index(index));
    }
    Ok(())
}

fn validate_source_records(
    bytes: &[u8],
    sources: SectionSlice,
    string_count: u32,
    source_text_data: Option<SectionSlice>,
) -> Result<(), DecodeError> {
    for i in 0..sources.count {
        let rec_offset = sources.offset as usize + (i as usize) * SOURCE_RECORD_SIZE as usize;
        let source_id = read_u32_le(bytes, rec_offset);
        let path = read_u32_le(bytes, rec_offset + 4);
        let locale = read_u32_le(bytes, rec_offset + 8);
        let message = read_u32_le(bytes, rec_offset + 12);
        let _base_offset = read_u32_le(bytes, rec_offset + 16);
        let text_source = read_u32_le(bytes, rec_offset + 20);
        let text_offset = read_u32_le(bytes, rec_offset + 24);
        let text_len = read_u32_le(bytes, rec_offset + 28);

        if source_id != i {
            return Err(DecodeError::new(DecodeErrorCode::InvalidSourceRef)
                .with_section(SectionKind::Sources)
                .with_index(i));
        }
        check_string_ref(path, string_count, SectionKind::Sources, i)?;
        check_string_ref(locale, string_count, SectionKind::Sources, i)?;
        check_string_ref(message, string_count, SectionKind::Sources, i)?;

        if text_source == NONE_REF {
            // Canonical none sentinel must have offset == 0 and len == 0.
            if text_offset != 0 || text_len != 0 {
                return Err(DecodeError::new(DecodeErrorCode::InvalidSourceTextRange)
                    .with_section(SectionKind::Sources)
                    .with_index(i));
            }
        } else {
            // text_source must equal this record's source_id (v0.1 rule)
            if text_source != source_id {
                return Err(DecodeError::new(DecodeErrorCode::InvalidSourceTextRange)
                    .with_section(SectionKind::Sources)
                    .with_index(i));
            }
            let Some(ed) = source_text_data else {
                return Err(DecodeError::new(DecodeErrorCode::InvalidSourceTextRange)
                    .with_section(SectionKind::Sources)
                    .with_index(i));
            };
            let end = text_offset.checked_add(text_len).ok_or_else(|| {
                DecodeError::new(DecodeErrorCode::InvalidSourceTextRange)
                    .with_section(SectionKind::Sources)
                    .with_index(i)
            })?;
            if end > ed.byte_len {
                return Err(DecodeError::new(DecodeErrorCode::InvalidSourceTextRange)
                    .with_section(SectionKind::Sources)
                    .with_index(i));
            }
            let data_start = ed.offset as usize + text_offset as usize;
            let data_end = data_start + text_len as usize;
            if core::str::from_utf8(&bytes[data_start..data_end]).is_err() {
                return Err(DecodeError::new(DecodeErrorCode::InvalidUtf8)
                    .with_section(SectionKind::SourceTextData)
                    .with_index(i));
            }
        }
    }
    Ok(())
}

fn validate_root_records(
    bytes: &[u8],
    roots: SectionSlice,
    node_count: u32,
    source_count: u32,
    diag_count: u32,
) -> Result<(), DecodeError> {
    for i in 0..roots.count {
        let rec_offset = roots.offset as usize + (i as usize) * ROOT_RECORD_SIZE as usize;
        let root_node = read_u32_le(bytes, rec_offset);
        let source_id = read_u32_le(bytes, rec_offset + 4);
        let diag_start = read_u32_le(bytes, rec_offset + 8);
        let diag_n = read_u32_le(bytes, rec_offset + 12);
        if root_node >= node_count {
            return Err(DecodeError::new(DecodeErrorCode::InvalidRootRef)
                .with_section(SectionKind::Roots)
                .with_index(i));
        }
        if source_id >= source_count {
            return Err(DecodeError::new(DecodeErrorCode::InvalidSourceRef)
                .with_section(SectionKind::Roots)
                .with_index(i));
        }
        let diag_end = diag_start.checked_add(diag_n).ok_or_else(|| {
            DecodeError::new(DecodeErrorCode::InvalidDiagnosticRange)
                .with_section(SectionKind::Roots)
                .with_index(i)
        })?;
        if diag_end > diag_count {
            return Err(DecodeError::new(DecodeErrorCode::InvalidDiagnosticRange)
                .with_section(SectionKind::Roots)
                .with_index(i));
        }
    }
    Ok(())
}

fn validate_node_records(
    bytes: &[u8],
    nodes: SectionSlice,
    edge_count: u32,
) -> Result<(), DecodeError> {
    for i in 0..nodes.count {
        let rec_offset = nodes.offset as usize + (i as usize) * NODE_RECORD_SIZE as usize;
        let kind = read_u16_le(bytes, rec_offset);
        let flags = read_u16_le(bytes, rec_offset + 2);
        let span_start = read_u32_le(bytes, rec_offset + 4);
        let span_end = read_u32_le(bytes, rec_offset + 8);
        check_span(span_start, span_end, SectionKind::Nodes, i)?;
        let first_child = read_u32_le(bytes, rec_offset + 12);
        let child_count = read_u32_le(bytes, rec_offset + 16);
        let data_ref = read_u32_le(bytes, rec_offset + 20);
        if flags != 0 {
            return Err(DecodeError::new(DecodeErrorCode::InvalidReservedField)
                .with_section(SectionKind::Nodes)
                .with_index(i));
        }
        if syntax_kind_is_unknown(kind) {
            return Err(DecodeError::new(DecodeErrorCode::UnknownSyntaxKind)
                .with_section(SectionKind::Nodes)
                .with_index(i));
        }
        let end = first_child.checked_add(child_count).ok_or_else(|| {
            DecodeError::new(DecodeErrorCode::InvalidNodeRef)
                .with_section(SectionKind::Nodes)
                .with_index(i)
        })?;
        if end > edge_count {
            return Err(DecodeError::new(DecodeErrorCode::InvalidNodeRef)
                .with_section(SectionKind::Nodes)
                .with_index(i));
        }
        if data_ref != NONE_REF {
            // v0.1 writer always uses none sentinel; non-sentinel
            // values would need an extended data section.
            return Err(DecodeError::new(DecodeErrorCode::InvalidExtendedData)
                .with_section(SectionKind::Nodes)
                .with_index(i));
        }
    }
    Ok(())
}

fn validate_edge_records(
    bytes: &[u8],
    edges: SectionSlice,
    node_count: u32,
    token_count: u32,
) -> Result<(), DecodeError> {
    for i in 0..edges.count {
        let rec_offset = edges.offset as usize + (i as usize) * EDGE_RECORD_SIZE as usize;
        let kind = read_u16_le(bytes, rec_offset);
        let flags = read_u16_le(bytes, rec_offset + 2);
        let ref_id = read_u32_le(bytes, rec_offset + 4);
        if flags != 0 {
            return Err(DecodeError::new(DecodeErrorCode::InvalidReservedField)
                .with_section(SectionKind::Edges)
                .with_index(i));
        }
        match kind {
            EDGE_KIND_NODE => {
                if ref_id >= node_count {
                    return Err(DecodeError::new(DecodeErrorCode::InvalidNodeRef)
                        .with_section(SectionKind::Edges)
                        .with_index(i));
                }
            }
            EDGE_KIND_TOKEN => {
                if ref_id >= token_count {
                    return Err(DecodeError::new(DecodeErrorCode::InvalidTokenRef)
                        .with_section(SectionKind::Edges)
                        .with_index(i));
                }
            }
            _ => {
                return Err(DecodeError::new(DecodeErrorCode::InvalidEdgeKind)
                    .with_section(SectionKind::Edges)
                    .with_index(i));
            }
        }
    }
    Ok(())
}

fn validate_token_records(
    bytes: &[u8],
    tokens: SectionSlice,
    source_count: u32,
    trivia_count: u32,
) -> Result<(), DecodeError> {
    for i in 0..tokens.count {
        let rec_offset = tokens.offset as usize + (i as usize) * TOKEN_RECORD_SIZE as usize;
        let kind = read_u16_le(bytes, rec_offset);
        let flags = read_u16_le(bytes, rec_offset + 2);
        let span_start = read_u32_le(bytes, rec_offset + 4);
        let span_end = read_u32_le(bytes, rec_offset + 8);
        check_span(span_start, span_end, SectionKind::Tokens, i)?;
        let source_id = read_u32_le(bytes, rec_offset + 12);
        let lead_start = read_u32_le(bytes, rec_offset + 16);
        let lead_count = read_u32_le(bytes, rec_offset + 20);
        let trail_start = read_u32_le(bytes, rec_offset + 24);
        let trail_count = read_u32_le(bytes, rec_offset + 28);
        let reserved_tail = read_u32_le(bytes, rec_offset + 32);
        if flags != 0 {
            return Err(DecodeError::new(DecodeErrorCode::InvalidReservedField)
                .with_section(SectionKind::Tokens)
                .with_index(i));
        }
        if reserved_tail != 0 {
            return Err(DecodeError::new(DecodeErrorCode::InvalidReservedField)
                .with_section(SectionKind::Tokens)
                .with_index(i));
        }
        if syntax_kind_is_unknown(kind) {
            return Err(DecodeError::new(DecodeErrorCode::UnknownSyntaxKind)
                .with_section(SectionKind::Tokens)
                .with_index(i));
        }
        if source_id >= source_count {
            return Err(DecodeError::new(DecodeErrorCode::InvalidSourceRef)
                .with_section(SectionKind::Tokens)
                .with_index(i));
        }
        for (start, count) in [(lead_start, lead_count), (trail_start, trail_count)] {
            let end = start.checked_add(count).ok_or_else(|| {
                DecodeError::new(DecodeErrorCode::InvalidTriviaRef)
                    .with_section(SectionKind::Tokens)
                    .with_index(i)
            })?;
            if end > trivia_count {
                return Err(DecodeError::new(DecodeErrorCode::InvalidTriviaRef)
                    .with_section(SectionKind::Tokens)
                    .with_index(i));
            }
        }
    }
    Ok(())
}

fn validate_trivia_records(
    bytes: &[u8],
    trivia: SectionSlice,
    source_count: u32,
) -> Result<(), DecodeError> {
    for i in 0..trivia.count {
        let rec_offset = trivia.offset as usize + (i as usize) * TRIVIA_RECORD_SIZE as usize;
        let kind = read_u16_le(bytes, rec_offset);
        let flags = read_u16_le(bytes, rec_offset + 2);
        let span_start = read_u32_le(bytes, rec_offset + 4);
        let span_end = read_u32_le(bytes, rec_offset + 8);
        check_span(span_start, span_end, SectionKind::Trivia, i)?;
        let source_id = read_u32_le(bytes, rec_offset + 12);
        if flags != 0 {
            return Err(DecodeError::new(DecodeErrorCode::InvalidReservedField)
                .with_section(SectionKind::Trivia)
                .with_index(i));
        }
        if syntax_kind_is_unknown(kind) {
            return Err(DecodeError::new(DecodeErrorCode::UnknownSyntaxKind)
                .with_section(SectionKind::Trivia)
                .with_index(i));
        }
        if source_id >= source_count {
            return Err(DecodeError::new(DecodeErrorCode::InvalidSourceRef)
                .with_section(SectionKind::Trivia)
                .with_index(i));
        }
    }
    Ok(())
}

fn validate_diagnostic_records(
    bytes: &[u8],
    diags: SectionSlice,
    source_count: u32,
    string_count: u32,
    label_count: u32,
) -> Result<(), DecodeError> {
    for i in 0..diags.count {
        let rec_offset = diags.offset as usize + (i as usize) * DIAGNOSTIC_RECORD_SIZE as usize;
        let source_id = read_u32_le(bytes, rec_offset);
        let span_start = read_u32_le(bytes, rec_offset + 4);
        let span_end = read_u32_le(bytes, rec_offset + 8);
        check_span(span_start, span_end, SectionKind::Diagnostics, i)?;
        let severity = bytes[rec_offset + 12];
        let reserved = bytes[rec_offset + 13];
        let code = read_u16_le(bytes, rec_offset + 14);
        let message = read_u32_le(bytes, rec_offset + 16);
        let label_start = read_u32_le(bytes, rec_offset + 20);
        let lc = read_u32_le(bytes, rec_offset + 24);
        if reserved != 0 {
            return Err(DecodeError::new(DecodeErrorCode::InvalidReservedField)
                .with_section(SectionKind::Diagnostics)
                .with_index(i));
        }
        if severity > 3 {
            return Err(DecodeError::new(DecodeErrorCode::InvalidDiagnosticSeverity)
                .with_section(SectionKind::Diagnostics)
                .with_index(i));
        }
        if diagnostic_code_is_unknown(code) {
            return Err(DecodeError::new(DecodeErrorCode::UnknownDiagnosticCode)
                .with_section(SectionKind::Diagnostics)
                .with_index(i));
        }
        if source_id >= source_count {
            return Err(DecodeError::new(DecodeErrorCode::InvalidSourceRef)
                .with_section(SectionKind::Diagnostics)
                .with_index(i));
        }
        check_string_ref(message, string_count, SectionKind::Diagnostics, i)?;
        let end = label_start.checked_add(lc).ok_or_else(|| {
            DecodeError::new(DecodeErrorCode::InvalidDiagnosticRange)
                .with_section(SectionKind::Diagnostics)
                .with_index(i)
        })?;
        if end > label_count {
            return Err(DecodeError::new(DecodeErrorCode::InvalidDiagnosticRange)
                .with_section(SectionKind::Diagnostics)
                .with_index(i));
        }
    }
    Ok(())
}

fn validate_diagnostic_label_records(
    bytes: &[u8],
    labels: SectionSlice,
    source_count: u32,
    string_count: u32,
) -> Result<(), DecodeError> {
    for i in 0..labels.count {
        let rec_offset =
            labels.offset as usize + (i as usize) * DIAGNOSTIC_LABEL_RECORD_SIZE as usize;
        let source_id = read_u32_le(bytes, rec_offset);
        let span_start = read_u32_le(bytes, rec_offset + 4);
        let span_end = read_u32_le(bytes, rec_offset + 8);
        check_span(span_start, span_end, SectionKind::DiagnosticLabels, i)?;
        let message = read_u32_le(bytes, rec_offset + 12);
        if source_id >= source_count {
            return Err(DecodeError::new(DecodeErrorCode::InvalidSourceRef)
                .with_section(SectionKind::DiagnosticLabels)
                .with_index(i));
        }
        check_string_ref(message, string_count, SectionKind::DiagnosticLabels, i)?;
    }
    Ok(())
}

fn syntax_kind_is_unknown(value: u16) -> bool {
    matches!(value, 0) || syntax_kind_from_u16(value).is_none()
}

/// Wire-value → [`SyntaxKind`] mapping. Returns `None` for any value
/// not currently emitted by the Phase 1 parser.
pub(crate) fn syntax_kind_from_u16(value: u16) -> Option<SyntaxKind> {
    Some(match value {
        v if v == SyntaxKind::Root.as_u16() => SyntaxKind::Root,
        v if v == SyntaxKind::SimpleMessage.as_u16() => SyntaxKind::SimpleMessage,
        v if v == SyntaxKind::ComplexMessage.as_u16() => SyntaxKind::ComplexMessage,
        v if v == SyntaxKind::Pattern.as_u16() => SyntaxKind::Pattern,
        v if v == SyntaxKind::Text.as_u16() => SyntaxKind::Text,
        v if v == SyntaxKind::QuotedPattern.as_u16() => SyntaxKind::QuotedPattern,
        v if v == SyntaxKind::Placeholder.as_u16() => SyntaxKind::Placeholder,
        v if v == SyntaxKind::LiteralExpression.as_u16() => SyntaxKind::LiteralExpression,
        v if v == SyntaxKind::VariableExpression.as_u16() => SyntaxKind::VariableExpression,
        v if v == SyntaxKind::FunctionExpression.as_u16() => SyntaxKind::FunctionExpression,
        v if v == SyntaxKind::Function.as_u16() => SyntaxKind::Function,
        v if v == SyntaxKind::Option.as_u16() => SyntaxKind::Option,
        v if v == SyntaxKind::Attribute.as_u16() => SyntaxKind::Attribute,
        v if v == SyntaxKind::LocalDeclaration.as_u16() => SyntaxKind::LocalDeclaration,
        v if v == SyntaxKind::InputDeclaration.as_u16() => SyntaxKind::InputDeclaration,
        v if v == SyntaxKind::ComplexBody.as_u16() => SyntaxKind::ComplexBody,
        v if v == SyntaxKind::Matcher.as_u16() => SyntaxKind::Matcher,
        v if v == SyntaxKind::Selector.as_u16() => SyntaxKind::Selector,
        v if v == SyntaxKind::Variant.as_u16() => SyntaxKind::Variant,
        v if v == SyntaxKind::VariantKey.as_u16() => SyntaxKind::VariantKey,
        v if v == SyntaxKind::CatchAllKey.as_u16() => SyntaxKind::CatchAllKey,
        v if v == SyntaxKind::Markup.as_u16() => SyntaxKind::Markup,
        v if v == SyntaxKind::MarkupOpen.as_u16() => SyntaxKind::MarkupOpen,
        v if v == SyntaxKind::MarkupStandalone.as_u16() => SyntaxKind::MarkupStandalone,
        v if v == SyntaxKind::MarkupClose.as_u16() => SyntaxKind::MarkupClose,
        v if v == SyntaxKind::QuotedLiteral.as_u16() => SyntaxKind::QuotedLiteral,
        v if v == SyntaxKind::UnquotedLiteral.as_u16() => SyntaxKind::UnquotedLiteral,
        v if v == SyntaxKind::Name.as_u16() => SyntaxKind::Name,
        v if v == SyntaxKind::Identifier.as_u16() => SyntaxKind::Identifier,
        v if v == SyntaxKind::Variable.as_u16() => SyntaxKind::Variable,
        v if v == SyntaxKind::LeftBraceToken.as_u16() => SyntaxKind::LeftBraceToken,
        v if v == SyntaxKind::RightBraceToken.as_u16() => SyntaxKind::RightBraceToken,
        v if v == SyntaxKind::LeftDoubleBraceToken.as_u16() => SyntaxKind::LeftDoubleBraceToken,
        v if v == SyntaxKind::RightDoubleBraceToken.as_u16() => SyntaxKind::RightDoubleBraceToken,
        v if v == SyntaxKind::DotToken.as_u16() => SyntaxKind::DotToken,
        v if v == SyntaxKind::AtToken.as_u16() => SyntaxKind::AtToken,
        v if v == SyntaxKind::PipeToken.as_u16() => SyntaxKind::PipeToken,
        v if v == SyntaxKind::EqualsToken.as_u16() => SyntaxKind::EqualsToken,
        v if v == SyntaxKind::ColonToken.as_u16() => SyntaxKind::ColonToken,
        v if v == SyntaxKind::DollarToken.as_u16() => SyntaxKind::DollarToken,
        v if v == SyntaxKind::SlashToken.as_u16() => SyntaxKind::SlashToken,
        v if v == SyntaxKind::StarToken.as_u16() => SyntaxKind::StarToken,
        v if v == SyntaxKind::HashToken.as_u16() => SyntaxKind::HashToken,
        v if v == SyntaxKind::InputKeyword.as_u16() => SyntaxKind::InputKeyword,
        v if v == SyntaxKind::LocalKeyword.as_u16() => SyntaxKind::LocalKeyword,
        v if v == SyntaxKind::MatchKeyword.as_u16() => SyntaxKind::MatchKeyword,
        v if v == SyntaxKind::NameToken.as_u16() => SyntaxKind::NameToken,
        v if v == SyntaxKind::TextToken.as_u16() => SyntaxKind::TextToken,
        v if v == SyntaxKind::QuotedTextToken.as_u16() => SyntaxKind::QuotedTextToken,
        v if v == SyntaxKind::EscapeToken.as_u16() => SyntaxKind::EscapeToken,
        v if v == SyntaxKind::WhitespaceTrivia.as_u16() => SyntaxKind::WhitespaceTrivia,
        v if v == SyntaxKind::BidiTrivia.as_u16() => SyntaxKind::BidiTrivia,
        v if v == SyntaxKind::Error.as_u16() => SyntaxKind::Error,
        v if v == SyntaxKind::Missing.as_u16() => SyntaxKind::Missing,
        v if v == SyntaxKind::Unknown.as_u16() => SyntaxKind::Unknown,
        _ => return None,
    })
}

fn diagnostic_code_is_unknown(value: u16) -> bool {
    diagnostic_code_from_u16_strict(value).is_none()
}

/// Strict variant of the Phase 1 diagnostic-code lookup: returns `None`
/// for values that have not been assigned to a known v0.1 diagnostic.
pub(crate) fn diagnostic_code_from_u16_strict(value: u16) -> Option<DiagnosticCode> {
    Some(match value {
        v if v == DiagnosticCode::Unspecified.as_u16() => DiagnosticCode::Unspecified,
        v if v == DiagnosticCode::UnexpectedEndOfInput.as_u16() => {
            DiagnosticCode::UnexpectedEndOfInput
        }
        v if v == DiagnosticCode::UnclosedExpression.as_u16() => DiagnosticCode::UnclosedExpression,
        v if v == DiagnosticCode::UnclosedQuotedLiteral.as_u16() => {
            DiagnosticCode::UnclosedQuotedLiteral
        }
        v if v == DiagnosticCode::UnclosedQuotedPattern.as_u16() => {
            DiagnosticCode::UnclosedQuotedPattern
        }
        v if v == DiagnosticCode::InvalidDeclarationStart.as_u16() => {
            DiagnosticCode::InvalidDeclarationStart
        }
        v if v == DiagnosticCode::InvalidMatcherSyntax.as_u16() => {
            DiagnosticCode::InvalidMatcherSyntax
        }
        v if v == DiagnosticCode::InvalidVariantBoundary.as_u16() => {
            DiagnosticCode::InvalidVariantBoundary
        }
        v if v == DiagnosticCode::InvalidMarkupBoundary.as_u16() => {
            DiagnosticCode::InvalidMarkupBoundary
        }
        v if v == DiagnosticCode::MissingComplexBody.as_u16() => DiagnosticCode::MissingComplexBody,
        v if v == DiagnosticCode::UnexpectedToken.as_u16() => DiagnosticCode::UnexpectedToken,
        v if v == DiagnosticCode::SpanOverflow.as_u16() => DiagnosticCode::SpanOverflow,
        v if v == DiagnosticCode::InvalidEscape.as_u16() => DiagnosticCode::InvalidEscape,
        v if v == DiagnosticCode::AmbiguousMessageMode.as_u16() => {
            DiagnosticCode::AmbiguousMessageMode
        }
        v if v == DiagnosticCode::MissingRequiredWhitespace.as_u16() => {
            DiagnosticCode::MissingRequiredWhitespace
        }
        v if v == DiagnosticCode::MissingIdentifierName.as_u16() => {
            DiagnosticCode::MissingIdentifierName
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression test for the equal-offset section sort: an empty
    /// section that shares an aligned offset with a non-empty
    /// section must sort first so the overlap check accepts both,
    /// regardless of the section-table listing order.
    #[test]
    fn equal_offset_sort_places_empty_before_non_empty() {
        let records = [
            DecodedSection {
                kind: SectionKind::StringData,
                offset: 32,
                byte_len: 16,
                count: 0,
            },
            DecodedSection {
                kind: SectionKind::StringOffsets,
                offset: 32,
                byte_len: 0,
                count: 0,
            },
        ];
        let mut sorted: Vec<usize> = (0..records.len()).collect();
        sorted.sort_by_key(|i| {
            let rec = records[*i];
            (rec.offset, rec.byte_len != 0)
        });
        assert_eq!(records[sorted[0]].kind, SectionKind::StringOffsets);
        assert_eq!(records[sorted[1]].kind, SectionKind::StringData);
    }

    #[test]
    fn check_span_accepts_zero_and_equal_endpoints() {
        assert!(check_span(0, 0, SectionKind::Nodes, 0).is_ok());
        assert!(check_span(5, 5, SectionKind::Nodes, 0).is_ok());
        assert!(check_span(2, 7, SectionKind::Nodes, 0).is_ok());
    }

    #[test]
    fn check_span_rejects_inverted_spans() {
        let err = check_span(5, 4, SectionKind::Tokens, 3).unwrap_err();
        assert_eq!(err.code, DecodeErrorCode::InvalidSpan);
        assert_eq!(err.section, Some(SectionKind::Tokens));
        assert_eq!(err.index, Some(3));
    }
}
