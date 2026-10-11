// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use icu_provider::marker::DataMarkerId;

use super::*;

pub(crate) const COMMITTED: &[u8] = include_bytes!("../../data/markers.postcard");

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
