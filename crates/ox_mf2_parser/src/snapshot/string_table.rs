// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Snapshot string table builder.
//!
//! v0.1 string interning is first-seen-order. Only metadata and
//! diagnostic strings are interned here; source-derived text uses
//! `source_id + span` and is never copied into the string table.

use std::collections::HashMap;

use crate::snapshot::error::SnapshotWriteError;
use crate::snapshot::format::{checked_u32, StringId, NONE_REF};

/// First-seen-order string interner used by [`crate::snapshot::writer`].
#[derive(Debug, Default)]
pub(crate) struct StringTableBuilder {
    data: Vec<u8>,
    offsets: Vec<StringOffsetEntry>,
    by_value: HashMap<String, StringId>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct StringOffsetEntry {
    pub offset: u32,
    pub len: u32,
}

impl StringTableBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern an optional string. `None` returns [`StringId::NONE`].
    pub fn intern_optional(&mut self, value: Option<&str>) -> Result<StringId, SnapshotWriteError> {
        match value {
            Some(s) => self.intern(s),
            None => Ok(StringId::NONE),
        }
    }

    /// Intern a required string. Empty strings are ordinary entries; the
    /// none sentinel is only reachable through [`Self::intern_optional`].
    pub fn intern(&mut self, value: &str) -> Result<StringId, SnapshotWriteError> {
        if let Some(id) = self.by_value.get(value) {
            return Ok(*id);
        }
        let offset = checked_u32(self.data.len()).ok_or(SnapshotWriteError::SectionTooLarge)?;
        let len = checked_u32(value.len()).ok_or(SnapshotWriteError::SectionTooLarge)?;
        let id_raw = checked_u32(self.offsets.len()).ok_or(SnapshotWriteError::TooManyStrings)?;
        if id_raw == NONE_REF {
            return Err(SnapshotWriteError::TooManyStrings);
        }
        self.data.extend_from_slice(value.as_bytes());
        self.offsets.push(StringOffsetEntry { offset, len });
        let id = StringId::new(id_raw);
        self.by_value.insert(value.to_owned(), id);
        Ok(id)
    }

    /// Borrowed view of the offset records, in [`StringId`] order.
    #[inline]
    pub fn offsets(&self) -> &[StringOffsetEntry] {
        &self.offsets
    }

    /// Concatenated UTF-8 byte buffer, ready to copy into the string
    /// data section.
    #[inline]
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interning_is_first_seen() {
        let mut builder = StringTableBuilder::new();
        let a = builder.intern("alpha").unwrap();
        let b = builder.intern("beta").unwrap();
        let a2 = builder.intern("alpha").unwrap();
        assert_eq!(a, StringId::new(0));
        assert_eq!(b, StringId::new(1));
        assert_eq!(a, a2);
        assert_eq!(builder.offsets().len(), 2);
    }

    #[test]
    fn intern_optional_none_returns_sentinel() {
        let mut builder = StringTableBuilder::new();
        assert!(builder.intern_optional(None).unwrap().is_none());
        let id = builder.intern_optional(Some("foo")).unwrap();
        assert_eq!(id, StringId::new(0));
    }

    #[test]
    fn empty_string_is_an_ordinary_entry() {
        let mut builder = StringTableBuilder::new();
        let id = builder.intern("").unwrap();
        assert_eq!(id, StringId::new(0));
        assert_eq!(builder.offsets().len(), 1);
        assert_eq!(builder.offsets()[0].len, 0);
        assert_eq!(builder.data().len(), 0);
    }

    #[test]
    fn offsets_match_data_layout() {
        let mut builder = StringTableBuilder::new();
        builder.intern("hello").unwrap();
        builder.intern("world!").unwrap();
        let offsets = builder.offsets();
        assert_eq!(offsets[0].offset, 0);
        assert_eq!(offsets[0].len, 5);
        assert_eq!(offsets[1].offset, 5);
        assert_eq!(offsets[1].len, 6);
        assert_eq!(builder.data(), b"helloworld!");
    }
}
