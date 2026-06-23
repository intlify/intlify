// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Section-local buffer builders and final-assembly metadata.
//!
//! Each section is built into its own `Vec<u8>` first. After every
//! section is collected, the assembler computes aligned offsets and
//! writes the snapshot header, the section table, padding, and section
//! payloads in [`crate::snapshot::format::SectionKind::ALL_IN_ORDER`].

use crate::snapshot::error::SnapshotWriteError;
use crate::snapshot::format::{
    align_up, checked_u32, write_u16_le, write_u32_le, write_u8, SectionKind, HEADER_SIZE,
    SECTION_ALIGNMENT, SECTION_FLAG_REQUIRED, SECTION_RECORD_SIZE, SNAPSHOT_FEATURE_FLAGS,
    SNAPSHOT_MAGIC, SNAPSHOT_MAJOR_VERSION, SNAPSHOT_MINOR_VERSION,
};

/// One emitted section's wire payload and metadata. `count` is the
/// number of fixed records (or `0` for raw byte sections; the
/// assembler reads `record_size` from `SectionKind` to decide).
#[derive(Debug)]
pub(crate) struct EmittedSection {
    pub kind: SectionKind,
    pub bytes: Vec<u8>,
    pub count: u32,
}

/// Top-level snapshot assembler.
///
/// Sections are pushed in any order; the assembler emits them in
/// [`SectionKind::ALL_IN_ORDER`] regardless of the order they were
/// pushed, then writes the final 32-byte header, the section table,
/// padding (zero bytes), and each section payload.
#[derive(Debug, Default)]
pub(crate) struct SnapshotAssembler {
    sections: Vec<EmittedSection>,
}

impl SnapshotAssembler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push an emitted section. The order of `push` calls does not
    /// affect canonical wire order — [`Self::finish`] sorts on
    /// [`SectionKind::ALL_IN_ORDER`].
    pub fn push(&mut self, section: EmittedSection) {
        self.sections.push(section);
    }

    /// Assemble the final snapshot bytes.
    pub fn finish(mut self) -> Result<Vec<u8>, SnapshotWriteError> {
        // Sort sections into canonical SectionKind order.
        self.sections.sort_by_key(|s| canonical_index_of(s.kind));

        // Plan layout: section table offset is fixed at HEADER_SIZE,
        // section table byte length is section_count * SECTION_RECORD_SIZE,
        // and each emitted section starts at an 8-byte aligned offset
        // after the prior section's end (or the section table end for
        // the first section).
        let section_count =
            checked_u32(self.sections.len()).ok_or(SnapshotWriteError::SectionTooLarge)?;
        let section_table_byte_len = section_count
            .checked_mul(SECTION_RECORD_SIZE)
            .ok_or(SnapshotWriteError::SectionTooLarge)?;
        let mut cursor = HEADER_SIZE
            .checked_add(section_table_byte_len)
            .ok_or(SnapshotWriteError::SectionTooLarge)?;

        let mut plan: Vec<SectionPlan> = Vec::with_capacity(self.sections.len());
        for section in &self.sections {
            let offset = align_up(cursor).ok_or(SnapshotWriteError::SectionTooLarge)?;
            let byte_len =
                checked_u32(section.bytes.len()).ok_or(SnapshotWriteError::SectionTooLarge)?;
            // Note: an empty raw byte section is still emitted at an
            // aligned offset; the next section's offset starts from
            // the same cursor in that case.
            let end = offset
                .checked_add(byte_len)
                .ok_or(SnapshotWriteError::SectionTooLarge)?;
            plan.push(SectionPlan { offset, byte_len });
            cursor = end;
        }

        let total_len = cursor;
        let mut buf = Vec::with_capacity(total_len as usize);

        write_header(&mut buf, section_count);
        debug_assert_eq!(buf.len() as u32, HEADER_SIZE);

        for (section, slot) in self.sections.iter().zip(plan.iter()) {
            write_section_record(&mut buf, section.kind, section.count, slot);
        }
        debug_assert_eq!(buf.len() as u32, HEADER_SIZE + section_table_byte_len);

        for (section, slot) in self.sections.iter().zip(plan.iter()) {
            // Pad up to this section's aligned offset.
            while (buf.len() as u32) < slot.offset {
                buf.push(0);
            }
            buf.extend_from_slice(&section.bytes);
        }
        debug_assert_eq!(buf.len() as u32, total_len);
        Ok(buf)
    }
}

#[derive(Debug, Clone, Copy)]
struct SectionPlan {
    offset: u32,
    byte_len: u32,
}

fn canonical_index_of(kind: SectionKind) -> usize {
    SectionKind::ALL_IN_ORDER
        .iter()
        .position(|k| *k == kind)
        .expect("SectionKind::ALL_IN_ORDER covers every variant")
}

fn write_header(buf: &mut Vec<u8>, section_count: u32) {
    buf.extend_from_slice(&SNAPSHOT_MAGIC);
    write_u16_le(buf, SNAPSHOT_MAJOR_VERSION);
    write_u16_le(buf, SNAPSHOT_MINOR_VERSION);
    write_u32_le(buf, SNAPSHOT_FEATURE_FLAGS);
    write_u32_le(buf, HEADER_SIZE);
    write_u32_le(buf, HEADER_SIZE);
    let section_count_u16 = section_count as u16;
    write_u16_le(buf, section_count_u16);
    write_u16_le(buf, 0); // reserved
    write_u32_le(buf, 0); // reserved_tail
}

fn write_section_record(buf: &mut Vec<u8>, kind: SectionKind, count: u32, plan: &SectionPlan) {
    write_u16_le(buf, kind.as_u16());
    let flags = if kind.is_core() {
        SECTION_FLAG_REQUIRED
    } else {
        0
    };
    write_u16_le(buf, flags);
    write_u32_le(buf, plan.offset);
    write_u32_le(buf, plan.byte_len);
    write_u32_le(buf, count);
    write_u16_le(buf, kind.record_size());
    write_u8(buf, SECTION_ALIGNMENT as u8);
    write_u8(buf, 0); // reserved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_assembler_writes_only_header() {
        let bytes = SnapshotAssembler::new().finish().unwrap();
        assert_eq!(bytes.len(), HEADER_SIZE as usize);
        assert_eq!(&bytes[..8], b"OXMF2AST");
        // section_table_offset = 32 at bytes [20..24]
        assert_eq!(&bytes[20..24], &HEADER_SIZE.to_le_bytes());
        // section_count = 0 at bytes [24..26]
        assert_eq!(&bytes[24..26], &0u16.to_le_bytes());
        // reserved + reserved_tail are zero
        assert_eq!(&bytes[26..32], &[0u8; 6]);
    }

    #[test]
    fn pushes_in_any_order_emits_canonical_order() {
        let mut a = SnapshotAssembler::new();
        a.push(EmittedSection {
            kind: SectionKind::StringData,
            bytes: vec![],
            count: 0,
        });
        a.push(EmittedSection {
            kind: SectionKind::Roots,
            bytes: vec![],
            count: 0,
        });
        let bytes = a.finish().unwrap();
        // Two section records starting at offset HEADER_SIZE.
        let kind_a = u16::from_le_bytes([bytes[32], bytes[33]]);
        let kind_b = u16::from_le_bytes([
            bytes[32 + SECTION_RECORD_SIZE as usize],
            bytes[33 + SECTION_RECORD_SIZE as usize],
        ]);
        assert_eq!(kind_a, SectionKind::Roots.as_u16());
        assert_eq!(kind_b, SectionKind::StringData.as_u16());
    }

    #[test]
    fn pads_between_sections_with_zero_to_eight_byte_alignment() {
        let mut a = SnapshotAssembler::new();
        a.push(EmittedSection {
            kind: SectionKind::StringData,
            // 3 bytes of payload — next section needs 5 bytes of padding.
            bytes: vec![1, 2, 3],
            count: 0,
        });
        a.push(EmittedSection {
            kind: SectionKind::ExtendedData,
            bytes: vec![9, 9],
            count: 0,
        });
        let bytes = a.finish().unwrap();
        // 32 header + 2 section records (40) = 72; next aligned offset = 72.
        // After 3 bytes payload, cursor = 75; next aligned = 80 → 5 zeros.
        assert_eq!(&bytes[72..75], &[1, 2, 3]);
        assert_eq!(&bytes[75..80], &[0, 0, 0, 0, 0]);
        assert_eq!(&bytes[80..82], &[9, 9]);
        assert_eq!(bytes.len(), 82);
    }
}
