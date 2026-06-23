// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Snapshot string table builder.
//!
//! v0.1 string interning is first-seen-order. Only metadata and
//! diagnostic strings are interned here; source-derived text uses
//! `source_id + span` and is never copied into the string table.
//!
//! The lookup table is a `Vec<u64>` of pre-hashed keys parallel to
//! the offset records, scanned linearly at intern time and verified
//! with a byte comparison against the data buffer. This avoids the
//! per-unique-string `String` allocation a
//! `HashMap<String, StringId>` would require, and stays
//! cache-friendly for the v0.1 workload (metadata + a small static
//! diagnostic catalog, typically << 1k unique strings per
//! snapshot). If a real workload ever pushes unique string counts
//! above the linear-scan crossover, swap this for an
//! `FxHashMap<u64, StringId>` with a collision-fallback list.

use crate::snapshot::error::SnapshotWriteError;
use crate::snapshot::format::{checked_u32, StringId, NONE_REF};

/// First-seen-order string interner used by
/// [`crate::snapshot::writer`].
#[derive(Debug, Default)]
pub(crate) struct StringTableBuilder {
    data: Vec<u8>,
    offsets: Vec<StringOffsetEntry>,
    /// Parallel pre-hashed lookup keys, one per entry in `offsets`.
    /// `hashes[i]` is the FNV-1a hash of the bytes at
    /// `data[offsets[i].offset..][..offsets[i].len]`.
    hashes: Vec<u64>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct StringOffsetEntry {
    pub offset: u32,
    pub len: u32,
}

impl StringTableBuilder {
    /// Pre-allocate room for `string_hint` distinct strings and
    /// roughly `data_hint` bytes of concatenated payload. Both
    /// hints are advisory: any oversized batch grows normally.
    pub fn with_capacity(string_hint: usize, data_hint: usize) -> Self {
        Self {
            data: Vec::with_capacity(data_hint),
            offsets: Vec::with_capacity(string_hint),
            hashes: Vec::with_capacity(string_hint),
        }
    }

    /// Intern an optional string. `None` returns [`StringId::NONE`].
    pub fn intern_optional(&mut self, value: Option<&str>) -> Result<StringId, SnapshotWriteError> {
        match value {
            Some(s) => self.intern(s),
            None => Ok(StringId::NONE),
        }
    }

    /// Intern a required string. Empty strings are ordinary
    /// entries; the none sentinel is only reachable through
    /// [`Self::intern_optional`].
    pub fn intern(&mut self, value: &str) -> Result<StringId, SnapshotWriteError> {
        let bytes = value.as_bytes();
        let hash = fnv1a_hash(bytes);
        // Linear scan over pre-hashed keys. On a hash match, verify
        // the bytes against the data buffer to rule out collisions.
        for (i, &existing) in self.hashes.iter().enumerate() {
            if existing != hash {
                continue;
            }
            let entry = self.offsets[i];
            let start = entry.offset as usize;
            let end = start + entry.len as usize;
            if self.data[start..end] == *bytes {
                return Ok(StringId::new(i as u32));
            }
        }
        // Miss: append data, offset record, and hash key. The
        // string data section's `byte_len` must fit in `u32`, so
        // reject the append when the post-append cumulative length
        // would overflow — checking `offset` and `len` in isolation
        // is not enough because each can fit while their sum
        // crosses `u32::MAX`.
        let start_len = self.data.len();
        let end_len = start_len
            .checked_add(bytes.len())
            .ok_or(SnapshotWriteError::SectionTooLarge)?;
        let offset = checked_u32(start_len).ok_or(SnapshotWriteError::SectionTooLarge)?;
        let len = checked_u32(bytes.len()).ok_or(SnapshotWriteError::SectionTooLarge)?;
        let _ = checked_u32(end_len).ok_or(SnapshotWriteError::SectionTooLarge)?;
        let id_raw = checked_u32(self.offsets.len()).ok_or(SnapshotWriteError::TooManyStrings)?;
        if id_raw == NONE_REF {
            return Err(SnapshotWriteError::TooManyStrings);
        }
        self.data.extend_from_slice(bytes);
        self.offsets.push(StringOffsetEntry { offset, len });
        self.hashes.push(hash);
        Ok(StringId::new(id_raw))
    }

    /// Borrowed view of the offset records, in [`StringId`] order.
    #[inline]
    pub fn offsets(&self) -> &[StringOffsetEntry] {
        &self.offsets
    }

    /// Concatenated UTF-8 byte buffer, ready to copy into the
    /// string data section.
    #[inline]
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

/// FNV-1a 64-bit hash. Deterministic across builds and platforms,
/// no dependency, and constant per-byte cost — exactly what the
/// linear-scan intern path needs.
#[inline]
fn fnv1a_hash(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interning_is_first_seen() {
        let mut builder = StringTableBuilder::default();
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
        let mut builder = StringTableBuilder::default();
        assert!(builder.intern_optional(None).unwrap().is_none());
        let id = builder.intern_optional(Some("foo")).unwrap();
        assert_eq!(id, StringId::new(0));
    }

    #[test]
    fn empty_string_is_an_ordinary_entry() {
        let mut builder = StringTableBuilder::default();
        let id = builder.intern("").unwrap();
        assert_eq!(id, StringId::new(0));
        assert_eq!(builder.offsets().len(), 1);
        assert_eq!(builder.offsets()[0].len, 0);
        assert_eq!(builder.data().len(), 0);
    }

    #[test]
    fn offsets_match_data_layout() {
        let mut builder = StringTableBuilder::default();
        builder.intern("hello").unwrap();
        builder.intern("world!").unwrap();
        let offsets = builder.offsets();
        assert_eq!(offsets[0].offset, 0);
        assert_eq!(offsets[0].len, 5);
        assert_eq!(offsets[1].offset, 5);
        assert_eq!(offsets[1].len, 6);
        assert_eq!(builder.data(), b"helloworld!");
    }

    #[test]
    fn intern_does_not_allocate_per_lookup_after_first_seen() {
        // Smoke test: 1000 repeated intern calls on a few unique
        // strings must keep `offsets.len()` bounded and never grow
        // `data` past the first-seen payload.
        let mut builder = StringTableBuilder::default();
        for i in 0..1000 {
            let s = match i % 3 {
                0 => "alpha",
                1 => "beta",
                _ => "gamma",
            };
            let _ = builder.intern(s).unwrap();
        }
        assert_eq!(builder.offsets().len(), 3);
        assert_eq!(builder.data(), b"alphabetagamma");
    }

    #[test]
    fn with_capacity_does_not_change_observable_behaviour() {
        let mut builder = StringTableBuilder::with_capacity(8, 64);
        let a = builder.intern("alpha").unwrap();
        let a2 = builder.intern("alpha").unwrap();
        assert_eq!(a, a2);
        assert_eq!(builder.offsets().len(), 1);
    }

    #[test]
    fn fnv1a_separates_common_metadata_strings() {
        // Sanity: locale tags, paths, and small message ids must
        // map to distinct hashes under FNV-1a so the linear-scan
        // collision path stays cheap.
        let inputs = ["en", "en-US", "ja", "greeting.mf2", "hello"];
        let mut seen = std::collections::HashSet::new();
        for input in inputs {
            assert!(
                seen.insert(fnv1a_hash(input.as_bytes())),
                "FNV-1a collision on {input:?}"
            );
        }
    }
}
