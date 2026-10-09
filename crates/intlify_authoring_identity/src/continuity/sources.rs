// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The exact source bytes continuity evidence is checked against.
//!
//! An edit names its before and after snapshots, and a snapshot names its
//! bytes by length and digest. Replaying an edit means nothing until the bytes
//! it runs over are the ones those snapshots name, so every retained source is
//! checked against its snapshot once, here, and found later by the exact
//! snapshot: never by unit alone, and never by matching text.

use std::cmp::Ordering;

use intlify_authoring::{SnapshotMismatch, SourceSnapshot};

/// Why retained sources were refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetainedSourceFailure {
    /// Bytes are not the ones their snapshot names.
    Mismatch(SnapshotMismatch),
    /// One snapshot was supplied twice.
    Duplicate,
}

/// The bytes of each snapshot the evidence names, each checked against it.
#[derive(Debug, Clone)]
pub struct RetainedSources<'a> {
    sources: Vec<(SourceSnapshot, &'a [u8])>,
}

impl<'a> RetainedSources<'a> {
    /// Check each snapshot's bytes and keep them.
    pub fn new(
        sources: impl IntoIterator<Item = (SourceSnapshot, &'a [u8])>,
    ) -> Result<Self, RetainedSourceFailure> {
        let mut sources: Vec<(SourceSnapshot, &'a [u8])> = sources.into_iter().collect();
        for (snapshot, bytes) in &sources {
            snapshot
                .verify(bytes)
                .map_err(RetainedSourceFailure::Mismatch)?;
        }
        sources.sort_unstable_by(|left, right| snapshot_order(&left.0, &right.0));
        if sources
            .windows(2)
            .any(|pair| snapshot_order(&pair[0].0, &pair[1].0) == Ordering::Equal)
        {
            return Err(RetainedSourceFailure::Duplicate);
        }
        Ok(Self { sources })
    }

    /// Find the bytes of exactly this snapshot.
    pub(crate) fn bytes(&self, snapshot: &SourceSnapshot) -> Option<&'a [u8]> {
        self.sources
            .binary_search_by(|(retained, _)| snapshot_order(retained, snapshot))
            .ok()
            .map(|index| self.sources[index].1)
    }
}

/// Order snapshots so that exactly equal ones are adjacent.
///
/// The canonical order leaves out the declared length; breaking the tie on it
/// makes equality under this order exact equality.
fn snapshot_order(left: &SourceSnapshot, right: &SourceSnapshot) -> Ordering {
    left.canonical_cmp(right)
        .then_with(|| left.byte_length().cmp(&right.byte_length()))
}

#[cfg(test)]
mod tests {
    use intlify_authoring::{IntegrityDigest, OwnerIdentity, OwnerKind, VersionedIdentity};
    use intlify_shared_json::encoding::digest_bytes;

    use super::*;

    fn snapshot(unit: &str, revision: &str, text: &str) -> SourceSnapshot {
        SourceSnapshot::new(
            OwnerIdentity::new(OwnerKind::Application, "storefront").unwrap(),
            unit,
            revision,
            VersionedIdentity::literal("intlify-grammar-js-module", "0"),
            text.len() as u64,
            IntegrityDigest::from_hash(digest_bytes(text.as_bytes())).as_str(),
        )
        .unwrap()
    }

    #[test]
    fn bytes_are_found_only_by_the_exact_snapshot_that_names_them() {
        let first = snapshot("checkout", "1", "intent('A')\n");
        let second = snapshot("checkout", "2", "intent('B')\n");
        let sources = RetainedSources::new([
            (second.clone(), "intent('B')\n".as_bytes()),
            (first.clone(), "intent('A')\n".as_bytes()),
        ])
        .unwrap();
        assert_eq!(sources.bytes(&first), Some("intent('A')\n".as_bytes()));
        assert_eq!(sources.bytes(&second), Some("intent('B')\n".as_bytes()));
        // The same unit at another revision, or another unit with the same
        // text, names other bytes.
        assert_eq!(
            sources.bytes(&snapshot("checkout", "3", "intent('A')\n")),
            None
        );
        assert_eq!(
            sources.bytes(&snapshot("payment", "1", "intent('A')\n")),
            None
        );
        // A snapshot that agrees on everything but its declared length sits
        // beside the retained one in canonical order, and is not it.
        let mut longer = serde_json::to_value(&first).unwrap();
        longer["byteLength"] = serde_json::json!((first.byte_length() + 1).to_string());
        let longer: SourceSnapshot = serde_json::from_value(longer).unwrap();
        assert_eq!(longer.canonical_cmp(&first), Ordering::Equal);
        assert_eq!(sources.bytes(&longer), None);
    }

    #[test]
    fn bytes_that_are_not_the_snapshot_s_and_repeats_are_refused() {
        let named = snapshot("checkout", "1", "intent('A')\n");
        assert_eq!(
            RetainedSources::new([(named.clone(), "intent('B')\n".as_bytes())]).err(),
            Some(RetainedSourceFailure::Mismatch(
                SnapshotMismatch::Utf8Digest
            ))
        );
        assert_eq!(
            RetainedSources::new([(named.clone(), "intent('A')".as_bytes())]).err(),
            Some(RetainedSourceFailure::Mismatch(
                SnapshotMismatch::ByteLength
            ))
        );
        let bytes = "intent('A')\n".as_bytes();
        assert_eq!(
            RetainedSources::new([(named.clone(), bytes), (named, bytes)]).err(),
            Some(RetainedSourceFailure::Duplicate)
        );
    }
}
