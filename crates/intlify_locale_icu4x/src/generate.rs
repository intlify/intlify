// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Writes the four marker families from ICU4X's compiled data,
//! `icu_locale_data` 2.2.0 generated from CLDR 48.2.0, into one blob. Only
//! this module links compiled data, and only under the `generate` feature.
//!
//! It also writes the reader's fixture blobs: the four families from small
//! logical payloads, and the same blob with its locale index widened.

use icu_locale::provider::{
    Baked, LocaleAliasesV1, LocaleLikelySubtagsExtendedV1, LocaleLikelySubtagsLanguageV1,
    LocaleLikelySubtagsScriptRegionV1,
};
use icu_provider::dynutil::UpcastDataPayload;
use icu_provider::export::{DataExporter, ExportMarker, FlushMetadata};
use icu_provider::prelude::*;
use icu_provider_blob::export::BlobExporter;

use zerovec::vecs::{Index16, Index32, VarZeroSlice, VarZeroVecOwned};

use crate::markers::Marker;
use crate::payload::LogicalPayload;

/// Why the blob could not be written. It keeps no data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerateFailure {
    /// The compiled data has no payload for a marker family.
    Load(Marker),
    /// ICU4X's exporter refused a payload or could not finish the blob.
    Export,
    /// A logical payload is not ICU4X data for its family.
    Payload(Marker),
    /// The blob to widen is not a narrow ICU4X 2.2 blob.
    Layout,
}

/// The committed blob's exact bytes: the four marker families, each written
/// once. ICU4X's exporter orders everything it writes, so the same data
/// always gives the same bytes.
pub fn markers_blob() -> Result<Vec<u8>, GenerateFailure> {
    blob(&Marker::ALL.map(Family::Admitted))
}

/// A marker family the generator can write. Tests also write one the
/// adapter does not admit, to build a blob with an additional family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Family {
    Admitted(Marker),
    #[cfg(test)]
    Parents,
}

macro_rules! write_singleton {
    ($exporter:expr, $marker:ty, $failure:expr) => {{
        let payload: DataPayload<$marker> = Baked
            .load(DataRequest::default())
            .map_err(|_| $failure)?
            .payload;
        $exporter
            .flush_singleton(
                <$marker>::INFO,
                &ExportMarker::upcast(payload),
                FlushMetadata::default(),
            )
            .map_err(|_| GenerateFailure::Export)
    }};
}

pub(crate) fn blob(families: &[Family]) -> Result<Vec<u8>, GenerateFailure> {
    let mut sink = Vec::new();
    {
        let mut exporter = BlobExporter::new_with_sink(Box::new(&mut sink));
        for &family in families {
            write(&exporter, family)?;
        }
        exporter.close().map_err(|_| GenerateFailure::Export)?;
    }
    Ok(sink)
}

fn write(exporter: &BlobExporter<'_>, family: Family) -> Result<(), GenerateFailure> {
    use GenerateFailure::Load;
    match family {
        Family::Admitted(Marker::LocaleAliasesV1) => {
            write_singleton!(exporter, LocaleAliasesV1, Load(Marker::LocaleAliasesV1))
        }
        Family::Admitted(Marker::LocaleLikelySubtagsExtendedV1) => write_singleton!(
            exporter,
            LocaleLikelySubtagsExtendedV1,
            Load(Marker::LocaleLikelySubtagsExtendedV1)
        ),
        Family::Admitted(Marker::LocaleLikelySubtagsLanguageV1) => write_singleton!(
            exporter,
            LocaleLikelySubtagsLanguageV1,
            Load(Marker::LocaleLikelySubtagsLanguageV1)
        ),
        Family::Admitted(Marker::LocaleLikelySubtagsScriptRegionV1) => write_singleton!(
            exporter,
            LocaleLikelySubtagsScriptRegionV1,
            Load(Marker::LocaleLikelySubtagsScriptRegionV1)
        ),
        #[cfg(test)]
        Family::Parents => write_singleton!(
            exporter,
            icu_locale::provider::LocaleParentsV1,
            GenerateFailure::Export
        ),
    }
}

/// Writes one logical payload as its family's data struct.
macro_rules! write_value {
    ($exporter:expr, $marker:ty, $payload:expr) => {{
        let failure = || GenerateFailure::Payload($payload.marker());
        // A data struct borrows from the text it is read from, and the
        // exporter takes one that lives as long as the program, so this
        // developer-only path leaks the few bytes each call reads.
        let text: &'static str = Box::leak(
            serde_json::to_string($payload.value())
                .map_err(|_| failure())?
                .into_boxed_str(),
        );
        let data: <$marker as DynamicDataMarker>::DataStruct =
            serde_json::from_str(text).map_err(|_| failure())?;
        $exporter
            .flush_singleton(
                <$marker>::INFO,
                &ExportMarker::upcast(DataPayload::<$marker>::from_owned(data)),
                FlushMetadata::default(),
            )
            .map_err(|_| GenerateFailure::Export)
    }};
}

/// A blob of the given logical payloads, written by ICU4X's exporter as it
/// writes any other. The reader's fixtures are small payloads cut from
/// ICU4X's data, written this way.
pub fn payload_blob(payloads: &[LogicalPayload]) -> Result<Vec<u8>, GenerateFailure> {
    let mut sink = Vec::new();
    {
        let mut exporter = BlobExporter::new_with_sink(Box::new(&mut sink));
        for payload in payloads {
            match payload.marker() {
                Marker::LocaleAliasesV1 => write_value!(exporter, LocaleAliasesV1, payload),
                Marker::LocaleLikelySubtagsExtendedV1 => {
                    write_value!(exporter, LocaleLikelySubtagsExtendedV1, payload)
                }
                Marker::LocaleLikelySubtagsLanguageV1 => {
                    write_value!(exporter, LocaleLikelySubtagsLanguageV1, payload)
                }
                Marker::LocaleLikelySubtagsScriptRegionV1 => {
                    write_value!(exporter, LocaleLikelySubtagsScriptRegionV1, payload)
                }
            }?;
        }
        exporter.close().map_err(|_| GenerateFailure::Export)?;
    }
    Ok(sink)
}

/// The blob's layout before ICU4X 2.0 and its narrow layout, mirrored as in
/// `markers`. The wide layout is not read: it is what this writes.
#[derive(serde::Deserialize)]
enum Narrow<'data> {
    V001(Unread),
    V002(Unread),
    V002Bigger(Unread),
    #[serde(borrow)]
    V003(Sections<'data>),
}

#[derive(serde::Deserialize)]
struct Unread;

#[derive(serde::Deserialize)]
struct Sections<'data> {
    markers: &'data [u8],
    locales: &'data [u8],
    buffers: &'data [u8],
}

/// The same blob in the wide layout: its locale index rewritten with 32-bit
/// indices, its marker index and payload buffers unchanged. ICU4X's exporter
/// chooses this layout only when the data needs it, so a small blob in it
/// has to be made this way. Both layouts carry the same data.
pub fn widen(narrow: &[u8]) -> Result<Vec<u8>, GenerateFailure> {
    let Ok((Narrow::V003(sections), [])) = postcard::take_from_bytes::<Narrow<'_>>(narrow) else {
        return Err(GenerateFailure::Layout);
    };
    let locales = VarZeroSlice::<[u8], Index16>::parse_bytes(sections.locales)
        .map_err(|_| GenerateFailure::Layout)?;
    let entries: Vec<&[u8]> = locales.iter().collect();
    let locales = VarZeroVecOwned::<[u8], Index32>::try_from_elements(&entries)
        .map_err(|_| GenerateFailure::Layout)?;
    // The wide layout's postcard variant, then each section as a length and
    // its bytes.
    let mut wide = vec![4];
    for section in [sections.markers, locales.as_bytes(), sections.buffers] {
        length(section.len(), &mut wide);
        wide.extend_from_slice(section);
    }
    Ok(wide)
}

/// A postcard length: unsigned LEB128.
fn length(mut value: usize, output: &mut Vec<u8>) {
    loop {
        let low = u8::try_from(value & 0x7f).expect("seven bits");
        value >>= 7;
        if value == 0 {
            output.push(low);
            return;
        }
        output.push(low | 0x80);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markers::tests::COMMITTED;
    use crate::markers::{Layout, MarkerBlob};

    #[test]
    fn the_committed_blob_is_written_byte_for_byte() {
        // Regenerate with `cargo run -p intlify_locale_icu4x --features generate
        // --example generate_locale_data -- --write`.
        assert!(
            markers_blob().unwrap() == COMMITTED,
            "the marker blob is stale"
        );
    }

    #[test]
    fn writing_the_same_families_twice_gives_the_same_bytes() {
        assert_eq!(markers_blob().unwrap(), markers_blob().unwrap());
        let reordered = blob(&[
            Family::Admitted(Marker::LocaleLikelySubtagsScriptRegionV1),
            Family::Admitted(Marker::LocaleAliasesV1),
            Family::Admitted(Marker::LocaleLikelySubtagsLanguageV1),
            Family::Admitted(Marker::LocaleLikelySubtagsExtendedV1),
        ])
        .unwrap();
        assert_eq!(reordered, markers_blob().unwrap());
    }

    const NARROW: &[u8] = include_bytes!("../fixtures/artifact/markers-narrow.postcard");
    const WIDE: &[u8] = include_bytes!("../fixtures/artifact/markers-wide.postcard");

    fn vector_payloads() -> Vec<LogicalPayload> {
        crate::conformance::tests::vectors()["markers"]
            .as_array()
            .unwrap()
            .iter()
            .zip(Marker::ALL)
            .map(|(entry, marker)| LogicalPayload::new(marker, entry["payload"].clone()).unwrap())
            .collect()
    }

    #[test]
    fn the_committed_fixture_blobs_are_written_byte_for_byte() {
        // Regenerate with `cargo run -p intlify_locale_icu4x --features generate
        // --example generate_artifact_fixtures -- --write`.
        let narrow = payload_blob(&vector_payloads()).unwrap();
        assert!(narrow == NARROW, "the narrow fixture blob is stale");
        assert!(
            widen(&narrow).unwrap() == WIDE,
            "the wide fixture blob is stale"
        );
    }

    #[test]
    fn both_layouts_carry_the_payloads_they_were_written_from() {
        let payloads = vector_payloads();
        for (bytes, layout) in [(NARROW, Layout::Narrow), (WIDE, Layout::Wide)] {
            let blob = MarkerBlob::read(bytes.into()).unwrap();
            assert_eq!(blob.layout(), layout);
            assert_eq!(blob.payloads().unwrap(), payloads);
        }
        // The small aliases still realize the one alias they keep.
        let blob = MarkerBlob::read(WIDE.into()).unwrap();
        assert_eq!(blob.canonicalize_raw("iw-IL").as_deref(), Ok("he-IL"));
    }

    #[test]
    fn a_payload_that_is_not_data_of_its_family_is_not_written() {
        let payload = LogicalPayload::new(
            Marker::LocaleLikelySubtagsLanguageV1,
            serde_json::json!({"und": "en"}),
        )
        .unwrap();
        assert_eq!(
            payload_blob(&[payload]),
            Err(GenerateFailure::Payload(
                Marker::LocaleLikelySubtagsLanguageV1
            ))
        );
    }

    #[test]
    fn only_a_narrow_blob_is_widened() {
        let mut trailing = NARROW.to_vec();
        trailing.push(0);
        for bytes in [
            WIDE.to_vec(),
            vec![0, 0, 0],
            Vec::new(),
            trailing,
            NARROW[..NARROW.len() - 1].to_vec(),
        ] {
            assert_eq!(widen(&bytes), Err(GenerateFailure::Layout));
        }
    }

    #[test]
    fn a_postcard_length_is_unsigned_leb128() {
        for (value, expected) in [
            (0, vec![0x00]),
            (127, vec![0x7f]),
            (128, vec![0x80, 0x01]),
            (300, vec![0xac, 0x02]),
            (16_384, vec![0x80, 0x80, 0x01]),
        ] {
            let mut output = Vec::new();
            length(value, &mut output);
            assert_eq!(output, expected, "{value}");
        }
    }
}
