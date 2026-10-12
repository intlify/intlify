// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The four marker families and the check that a blob holds exactly them.
//! Reading a blob proves which markers it carries and that each decodes. It
//! is not an integrity check: ICU4X decodes damaged but well-formed data as
//! other data, and in a debug build zerovec asserts on a map whose keys are
//! out of order. The 017 reader therefore checks a payload's file digest
//! before reading it here, and the dataset's semantic digest after.

use icu_locale::provider::{
    LocaleAliasesV1, LocaleLikelySubtagsExtendedV1, LocaleLikelySubtagsLanguageV1,
    LocaleLikelySubtagsScriptRegionV1,
};
use icu_locale::LocaleCanonicalizer;
use icu_provider::prelude::*;
use icu_provider_blob::BlobDataProvider;

/// One of the four ICU4X marker families revision "0" admits, named as 017
/// records it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Marker {
    LocaleAliasesV1,
    LocaleLikelySubtagsExtendedV1,
    LocaleLikelySubtagsLanguageV1,
    LocaleLikelySubtagsScriptRegionV1,
}

impl Marker {
    /// Every marker, in 017's order: by name.
    pub const ALL: [Self; 4] = [
        Self::LocaleAliasesV1,
        Self::LocaleLikelySubtagsExtendedV1,
        Self::LocaleLikelySubtagsLanguageV1,
        Self::LocaleLikelySubtagsScriptRegionV1,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::LocaleAliasesV1 => "LocaleAliasesV1",
            Self::LocaleLikelySubtagsExtendedV1 => "LocaleLikelySubtagsExtendedV1",
            Self::LocaleLikelySubtagsLanguageV1 => "LocaleLikelySubtagsLanguageV1",
            Self::LocaleLikelySubtagsScriptRegionV1 => "LocaleLikelySubtagsScriptRegionV1",
        }
    }

    pub(crate) const fn info(self) -> DataMarkerInfo {
        match self {
            Self::LocaleAliasesV1 => LocaleAliasesV1::INFO,
            Self::LocaleLikelySubtagsExtendedV1 => LocaleLikelySubtagsExtendedV1::INFO,
            Self::LocaleLikelySubtagsLanguageV1 => LocaleLikelySubtagsLanguageV1::INFO,
            Self::LocaleLikelySubtagsScriptRegionV1 => LocaleLikelySubtagsScriptRegionV1::INFO,
        }
    }
}

/// How a blob lays out its index. Both are ICU4X 2.2's blob schema; its
/// exporter chooses the wide index only when the data needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Narrow,
    Wide,
}

/// Why a blob cannot supply the four marker families. No variant keeps any
/// of the blob's bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlobFailure {
    /// The bytes are not exactly one ICU4X 2.2 blob.
    Malformed,
    /// The blob is in a schema from before ICU4X 2.0.
    UnsupportedSchema,
    /// A required marker family is absent.
    MissingMarker(Marker),
    /// The blob carries a marker family other than the four.
    AdditionalMarker,
    /// A required marker family is present but its payload does not decode.
    Undecodable(Marker),
}

/// A blob that holds exactly the four marker families, each of which
/// decodes, with ICU4X's extended canonicalizer built from them.
pub struct MarkerBlob {
    provider: BlobDataProvider,
    layout: Layout,
    canonicalizer: LocaleCanonicalizer,
}

impl MarkerBlob {
    /// Checks the blob's index before ICU4X reads it, then decodes every
    /// marker family, so a blob that fails here never builds a canonicalizer.
    pub fn read(bytes: Box<[u8]>) -> Result<Self, BlobFailure> {
        let (layout, hashes) = index(&bytes)?;
        check(&hashes)?;
        let provider =
            BlobDataProvider::try_new_from_blob(bytes).map_err(|_| BlobFailure::Malformed)?;
        for marker in Marker::ALL {
            decodes(&provider, marker)?;
        }
        // Every family the extended canonicalizer loads has just decoded.
        let canonicalizer = LocaleCanonicalizer::try_new_extended_with_buffer_provider(&provider)
            .map_err(|_| BlobFailure::Malformed)?;
        Ok(Self {
            provider,
            layout,
            canonicalizer,
        })
    }

    pub const fn layout(&self) -> Layout {
        self.layout
    }

    pub(crate) const fn provider(&self) -> &BlobDataProvider {
        &self.provider
    }

    pub(crate) const fn canonicalizer(&self) -> &LocaleCanonicalizer {
        &self.canonicalizer
    }
}

/// The serialized layout of an ICU4X blob, mirrored because ICU4X keeps its
/// schema types private: the schema variant, then the marker hashes, the
/// locale index, and the payload buffers. The variants from before ICU4X 2.0
/// are listed only so the variant numbers line up; none is read.
#[derive(serde::Deserialize)]
enum Schema<'data> {
    V001(Unread),
    V002(Unread),
    V002Bigger(Unread),
    #[serde(borrow)]
    V003(Index<'data>),
    #[serde(borrow)]
    V003Bigger(Index<'data>),
}

#[derive(serde::Deserialize)]
struct Unread;

#[derive(serde::Deserialize)]
struct Index<'data> {
    markers: &'data [u8],
    _locales: &'data [u8],
    _buffers: &'data [u8],
}

const HASH_BYTES: usize = 4;

fn index(bytes: &[u8]) -> Result<(Layout, Vec<[u8; HASH_BYTES]>), BlobFailure> {
    let (schema, rest) =
        postcard::take_from_bytes::<Schema<'_>>(bytes).map_err(|_| BlobFailure::Malformed)?;
    let (layout, index) = match schema {
        Schema::V001(_) | Schema::V002(_) | Schema::V002Bigger(_) => {
            return Err(BlobFailure::UnsupportedSchema);
        }
        Schema::V003(index) => (Layout::Narrow, index),
        Schema::V003Bigger(index) => (Layout::Wide, index),
    };
    if !rest.is_empty() || index.markers.len() % HASH_BYTES != 0 {
        return Err(BlobFailure::Malformed);
    }
    // The remainder is empty: the length was just checked.
    let (hashes, _) = index.markers.as_chunks::<HASH_BYTES>();
    Ok((layout, hashes.to_vec()))
}

fn check(hashes: &[[u8; HASH_BYTES]]) -> Result<(), BlobFailure> {
    // ICU4X writes the index in ascending order without repeats and searches
    // it that way, so any other index is not one it wrote.
    if hashes.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(BlobFailure::Malformed);
    }
    for marker in Marker::ALL {
        if !hashes.contains(&marker.info().id.hashed().to_bytes()) {
            return Err(BlobFailure::MissingMarker(marker));
        }
    }
    if hashes.len() > Marker::ALL.len() {
        return Err(BlobFailure::AdditionalMarker);
    }
    Ok(())
}

fn decodes(provider: &BlobDataProvider, marker: Marker) -> Result<(), BlobFailure> {
    let provider = provider.as_deserializing();
    let request = DataRequest::default();
    let loaded = match marker {
        Marker::LocaleAliasesV1 => {
            DataProvider::<LocaleAliasesV1>::load(&provider, request).map(drop)
        }
        Marker::LocaleLikelySubtagsExtendedV1 => {
            DataProvider::<LocaleLikelySubtagsExtendedV1>::load(&provider, request).map(drop)
        }
        Marker::LocaleLikelySubtagsLanguageV1 => {
            DataProvider::<LocaleLikelySubtagsLanguageV1>::load(&provider, request).map(drop)
        }
        Marker::LocaleLikelySubtagsScriptRegionV1 => {
            DataProvider::<LocaleLikelySubtagsScriptRegionV1>::load(&provider, request).map(drop)
        }
    };
    loaded.map_err(|_| BlobFailure::Undecodable(marker))
}

#[cfg(test)]
pub(crate) mod tests {
    use icu_provider::marker::DataMarkerId;

    use super::*;

    pub(crate) const COMMITTED: &[u8] = include_bytes!("../data/markers.postcard");

    pub(crate) fn committed() -> MarkerBlob {
        MarkerBlob::read(COMMITTED.into()).unwrap()
    }

    fn hash(marker: Marker) -> [u8; HASH_BYTES] {
        marker.info().id.hashed().to_bytes()
    }

    /// The hashes of `markers` in the order ICU4X writes them.
    fn sorted(markers: &[Marker]) -> Vec<[u8; HASH_BYTES]> {
        let mut hashes: Vec<_> = markers.iter().copied().map(hash).collect();
        hashes.sort_unstable();
        hashes
    }

    /// A postcard-encoded blob holding only an index: the schema variant, the
    /// marker hashes, and empty locale and buffer sections.
    fn encoded(variant: u8, markers: &[u8]) -> Vec<u8> {
        let mut bytes = vec![variant, u8::try_from(markers.len()).unwrap()];
        bytes.extend_from_slice(markers);
        bytes.extend_from_slice(&[0, 0]);
        bytes
    }

    #[test]
    fn the_four_markers_are_named_as_017_records_them_in_name_order() {
        assert_eq!(
            Marker::ALL.map(Marker::name),
            [
                "LocaleAliasesV1",
                "LocaleLikelySubtagsExtendedV1",
                "LocaleLikelySubtagsLanguageV1",
                "LocaleLikelySubtagsScriptRegionV1",
            ]
        );
        assert!(Marker::ALL
            .windows(2)
            .all(|pair| pair[0].name() < pair[1].name()));
        for marker in Marker::ALL {
            assert_eq!(
                DataMarkerId::from_name(marker.name()).unwrap().hashed(),
                marker.info().id.hashed(),
                "{marker:?}"
            );
            assert!(marker.info().is_singleton, "{marker:?}");
        }
    }

    #[test]
    fn the_committed_blob_holds_exactly_the_four_markers() {
        let (layout, hashes) = index(COMMITTED).unwrap();
        assert_eq!(layout, Layout::Narrow);
        assert_eq!(hashes, sorted(&Marker::ALL));
        assert_eq!(committed().layout(), Layout::Narrow);
    }

    #[test]
    fn an_index_is_read_from_either_layout_of_the_current_schema() {
        let hashes = sorted(&Marker::ALL);
        let flat = hashes.concat();
        assert_eq!(
            index(&encoded(3, &flat)),
            Ok((Layout::Narrow, hashes.clone()))
        );
        assert_eq!(index(&encoded(4, &flat)), Ok((Layout::Wide, hashes)));
    }

    #[test]
    fn blob_schemas_from_before_icu4x_2_are_not_read() {
        for variant in [0, 1, 2] {
            assert_eq!(index(&[variant]), Err(BlobFailure::UnsupportedSchema));
            assert_eq!(
                MarkerBlob::read(vec![variant, 9, 9].into()).err(),
                Some(BlobFailure::UnsupportedSchema)
            );
        }
    }

    #[test]
    fn bytes_that_are_not_exactly_one_index_are_malformed() {
        let flat = sorted(&Marker::ALL).concat();
        let mut trailing = encoded(3, &flat);
        trailing.push(0);
        let truncated = &COMMITTED[..COMMITTED.len() - 1];
        for bytes in [
            Vec::new(),
            encoded(5, &flat),
            trailing,
            encoded(3, &flat[..flat.len() - 1]),
            truncated.to_vec(),
        ] {
            assert_eq!(index(&bytes).err(), Some(BlobFailure::Malformed));
            assert_eq!(
                MarkerBlob::read(bytes.into()).err(),
                Some(BlobFailure::Malformed)
            );
        }
    }

    #[test]
    fn an_index_out_of_order_or_repeated_is_malformed() {
        let hashes = sorted(&Marker::ALL);
        let mut reversed = hashes.clone();
        reversed.reverse();
        let mut repeated = hashes.clone();
        repeated.insert(1, hashes[1]);
        for index in [reversed, repeated] {
            assert_eq!(check(&index), Err(BlobFailure::Malformed));
        }
        assert_eq!(check(&hashes), Ok(()));
    }

    #[test]
    fn each_missing_marker_is_named_before_an_additional_one_is_refused() {
        for missing in Marker::ALL {
            let present: Vec<_> = Marker::ALL
                .into_iter()
                .filter(|marker| *marker != missing)
                .collect();
            assert_eq!(
                check(&sorted(&present)),
                Err(BlobFailure::MissingMarker(missing))
            );
            let mut with_another = sorted(&present);
            with_another.push([0xff; HASH_BYTES]);
            assert_eq!(
                check(&with_another),
                Err(BlobFailure::MissingMarker(missing))
            );
        }
        for extra in [[0x00; HASH_BYTES], [0xff; HASH_BYTES]] {
            let mut hashes = sorted(&Marker::ALL);
            hashes.push(extra);
            hashes.sort_unstable();
            assert_eq!(check(&hashes), Err(BlobFailure::AdditionalMarker));
        }
    }

    #[cfg(feature = "generate")]
    #[test]
    fn icu4x_blobs_without_a_family_or_with_another_are_refused() {
        use crate::generate::{blob, Family};

        for missing in Marker::ALL {
            let families: Vec<_> = Marker::ALL
                .into_iter()
                .filter(|marker| *marker != missing)
                .map(Family::Admitted)
                .collect();
            assert_eq!(
                MarkerBlob::read(blob(&families).unwrap().into()).err(),
                Some(BlobFailure::MissingMarker(missing))
            );
        }
        let mut families = Marker::ALL.map(Family::Admitted).to_vec();
        families.push(Family::Parents);
        assert_eq!(
            MarkerBlob::read(blob(&families).unwrap().into()).err(),
            Some(BlobFailure::AdditionalMarker)
        );
    }

    fn damaged(offset: usize, byte: u8) -> Box<[u8]> {
        let mut bytes = COMMITTED.to_vec();
        assert_ne!(bytes[offset], byte, "the byte at {offset} must change");
        bytes[offset] = byte;
        bytes.into()
    }

    #[test]
    fn a_family_whose_payload_does_not_decode_is_named() {
        // Each offset lies inside one family's payload in the committed blob, and
        // the replacement byte makes that payload fail to decode.
        for (offset, byte, marker) in [
            (77_989, 0xff, Marker::LocaleAliasesV1),
            (5_672, 0x00, Marker::LocaleLikelySubtagsExtendedV1),
            (212, 0x00, Marker::LocaleLikelySubtagsLanguageV1),
            (2_900, 0x00, Marker::LocaleLikelySubtagsScriptRegionV1),
        ] {
            assert_eq!(
                MarkerBlob::read(damaged(offset, byte)).err(),
                Some(BlobFailure::Undecodable(marker)),
                "{offset}"
            );
        }
    }

    #[test]
    fn a_damaged_blob_can_still_read_so_integrity_is_the_file_digests_job() {
        // ICU4X decodes damaged data that is still well-formed as other data
        // rather than refusing it, so reading a blob is no integrity check.
        assert!(MarkerBlob::read(damaged(121, 0xff)).is_ok());
    }

    #[test]
    fn a_blob_whose_index_reads_but_icu4x_cannot_read_is_malformed() {
        // The marker index is well formed and complete, but the locale section
        // is not a vector ICU4X can read.
        let flat = sorted(&Marker::ALL).concat();
        let mut bytes = vec![3, u8::try_from(flat.len()).unwrap()];
        bytes.extend_from_slice(&flat);
        bytes.extend_from_slice(&[1, 0xff, 0]);
        assert!(index(&bytes).is_ok());
        assert_eq!(
            MarkerBlob::read(bytes.into()).err(),
            Some(BlobFailure::Malformed)
        );
    }
}
