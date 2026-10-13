// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Writes the four marker families from ICU4X's compiled data,
//! `icu_locale_data` 2.2.0 generated from CLDR 48.2.0, into one blob. Only
//! this module links compiled data, and only under the `generate` feature.
//!
//! It writes the complete revision-"0" artifact from that blob and the
//! vendored CLDR records: the conformance payload, and the manifest that
//! names both payloads and the dataset's semantic digest. It also writes the
//! reader's fixture blobs: the four families from small logical payloads, and
//! the same blob with its locale index widened.

use icu_locale::provider::{
    Baked, LocaleAliasesV1, LocaleLikelySubtagsExtendedV1, LocaleLikelySubtagsLanguageV1,
    LocaleLikelySubtagsScriptRegionV1,
};
use icu_provider::dynutil::UpcastDataPayload;
use icu_provider::export::{DataExporter, ExportMarker, FlushMetadata};
use icu_provider::prelude::*;
use icu_provider_blob::export::BlobExporter;

use intlify_shared_json::text;
use serde_json::{json, Value};
use zerovec::vecs::{Index16, Index32, VarZeroSlice, VarZeroVecOwned};

use crate::cldr::{self, CldrFailure, CLDR_REVISION};
use crate::dataset::{
    LocaleCanonicalizationDataset, DATASET_SCHEMA_IDENTITY, DATASET_SCHEMA_REVISION,
};
use crate::manifest::{
    LocaleCanonicalizationDataArtifact, ARTIFACT_KIND, ARTIFACT_SCHEMA_REVISION,
};
use crate::markers::{Marker, MarkerBlob};
use crate::payload::LogicalPayload;
use crate::primitives::FileDigest;
use crate::reader::{CONFORMANCE_FORMAT, MARKERS_FORMAT};

/// The Locale Canonicalization Specification the artifact realizes.
pub const SPECIFICATION: (&str, &str) = ("intlify-locale-canonicalization", "0");

/// The one dataset specification revision "0" admits.
pub const DATASET_IDENTITY: &str = "intlify-locale-canonicalization-cldr-48.2.0";

/// The published checksum of `icu_locale_data` 2.2.0, as `Cargo.lock`
/// records it.
pub const MARKER_DATA_CHECKSUM: &str =
    "sha256:d5fdcc9ac77c6d74ff5cf6e65ef3181d6af32003b16fce3a77fb451d2f695993";

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
    /// The CLDR records do not give revision "0"'s conformance data.
    Cldr(CldrFailure),
    /// The blob just written does not read back, or its payloads and the
    /// conformance data do not make a dataset.
    Dataset,
    /// A digest is beyond the shared encoding's capacity.
    Unframable,
}

/// The committed blob's exact bytes: the four marker families, each written
/// once. ICU4X's exporter orders everything it writes, so the same data
/// always gives the same bytes.
pub fn markers_blob() -> Result<Vec<u8>, GenerateFailure> {
    blob(&Marker::ALL.map(Family::Admitted))
}

/// The complete revision-"0" artifact, file by file, as exact bytes. The
/// manifest names the payloads by these addresses, beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactFiles {
    pub manifest: Vec<u8>,
    pub conformance: Vec<u8>,
    pub markers: Vec<u8>,
}

impl ArtifactFiles {
    /// Each file under the name the manifest gives it.
    pub fn files(&self) -> [(&'static str, &[u8]); 3] {
        [
            ("manifest.json", &self.manifest),
            ("conformance.json", &self.conformance),
            ("markers.postcard", &self.markers),
        ]
    }
}

/// The complete artifact from ICU4X's compiled data and the vendored CLDR
/// records.
pub fn artifact() -> Result<ArtifactFiles, GenerateFailure> {
    let sources = cldr::vendored().map_err(|_| GenerateFailure::Cldr(CldrFailure::Unreadable))?;
    artifact_from(markers_blob()?, &sources)
}

pub(crate) fn artifact_from(
    markers: Vec<u8>,
    sources: &[(&str, Vec<u8>)],
) -> Result<ArtifactFiles, GenerateFailure> {
    let blob = MarkerBlob::read(markers.clone().into_boxed_slice())
        .map_err(|_| GenerateFailure::Dataset)?;
    let realizes = |extension: &str, key: &str, from: &str, to: &str| {
        realized(&blob, extension, key, from, to)
    };
    let files: Vec<(&str, &[u8])> = sources
        .iter()
        .map(|(address, bytes)| (*address, bytes.as_slice()))
        .collect();
    let data = cldr::conformance_data(&files, &realizes).map_err(GenerateFailure::Cldr)?;
    let conformance = data.to_text().into_bytes();
    let payloads = blob.payloads().map_err(|_| GenerateFailure::Dataset)?;
    let digest = LocaleCanonicalizationDataset::new(payloads, data)
        .map_err(|_| GenerateFailure::Dataset)?
        .semantic_digest()
        .map_err(|_| GenerateFailure::Unframable)?;
    let manifest = manifest(digest.as_str(), sources, &conformance, &markers)?;
    Ok(ArtifactFiles {
        manifest,
        conformance,
        markers,
    })
}

/// Whether ICU4X already realizes a mapping of one key: it gives the source's
/// canonical form the target's, as the value of that key in its extension.
fn realized(blob: &MarkerBlob, extension: &str, key: &str, from: &str, to: &str) -> bool {
    let canonical = |value: &str| blob.canonicalize_raw(&format!("und-{extension}-{key}-{value}"));
    matches!((canonical(from), canonical(to)), (Ok(a), Ok(b)) if a == b)
}

/// The sealed manifest, as canonical JSON text.
fn manifest(
    semantic_digest: &str,
    sources: &[(&str, Vec<u8>)],
    conformance: &[u8],
    markers: &[u8],
) -> Result<Vec<u8>, GenerateFailure> {
    let identity =
        |(identity, revision): (&str, &str)| json!({"identity": identity, "revision": revision});
    let payload = |role: &str, format: (&str, &str), address: &str, bytes: &[u8]| {
        json!({
            "role": role,
            "format": identity(format),
            "address": address,
            "byteLength": bytes.len().to_string(),
            "digest": FileDigest::of(bytes).as_str(),
        })
    };
    let value = json!({
        "kind": ARTIFACT_KIND,
        "schemaRevision": ARTIFACT_SCHEMA_REVISION,
        "specification": identity(SPECIFICATION),
        "dataset": {
            "identity": DATASET_IDENTITY,
            "schema": identity((DATASET_SCHEMA_IDENTITY, DATASET_SCHEMA_REVISION)),
            "semanticDigest": semantic_digest,
        },
        "generation": {
            "markerData": {
                "package": "icu_locale_data",
                "version": "2.2.0",
                "checksum": MARKER_DATA_CHECKSUM,
            },
            "cldr": {
                "revision": CLDR_REVISION,
                "sources": sources
                    .iter()
                    .map(|(address, bytes)| json!({"address": address, "digest": FileDigest::of(bytes).as_str()}))
                    .collect::<Vec<Value>>(),
            },
            "markers": Marker::ALL.map(Marker::name),
        },
        "payloads": [
            payload("conformance", CONFORMANCE_FORMAT, "conformance.json", conformance),
            payload("markers", MARKERS_FORMAT, "markers.postcard", markers),
        ],
        "integrityDigest": FileDigest::of(b"").as_str(),
    });
    let sealed = serde_json::from_value::<LocaleCanonicalizationDataArtifact>(value)
        .map_err(|_| GenerateFailure::Dataset)?
        .seal()
        .map_err(|_| GenerateFailure::Unframable)?;
    let value = serde_json::to_value(sealed).map_err(|_| GenerateFailure::Dataset)?;
    text::write(&value)
        .map(String::into_bytes)
        .map_err(|_| GenerateFailure::Dataset)
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

    #[test]
    fn the_committed_artifact_is_written_byte_for_byte() {
        // Regenerate with `cargo run -p intlify_locale_icu4x --features generate
        // --example generate_locale_data -- --write`.
        let artifact = artifact().unwrap();
        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        for (name, bytes) in artifact.files() {
            assert!(
                std::fs::read(directory.join(name)).unwrap() == bytes,
                "data/{name} is stale"
            );
        }
        assert!(
            artifact.markers == COMMITTED,
            "the artifact's blob is not the committed blob"
        );
    }

    #[test]
    fn the_marker_data_checksum_is_the_one_cargo_locked() {
        let lock = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock"),
        )
        .unwrap();
        let entry = lock
            .split("[[package]]")
            .find(|entry| entry.contains("name = \"icu_locale_data\"\nversion = \"2.2.0\""))
            .unwrap();
        let checksum = entry
            .lines()
            .find_map(|line| line.strip_prefix("checksum = \""))
            .unwrap()
            .trim_end_matches('"');
        assert_eq!(MARKER_DATA_CHECKSUM, format!("sha256:{checksum}"));
    }

    #[test]
    fn a_mapping_is_realized_when_icu4x_gives_both_one_canonical_form() {
        let blob = MarkerBlob::read(COMMITTED.to_vec().into_boxed_slice()).unwrap();
        // ICU4X replaces the deprecated subdivision `no23` by `no50`.
        assert!(realized(&blob, "u", "sd", "no23", "no50"));
        assert!(!realized(&blob, "u", "ms", "imperial", "uksystem"));
        // `sd` is not a `t` field, so neither tag is a locale.
        assert!(!realized(&blob, "t", "sd", "no23", "no50"));
        assert!(!realized(&blob, "u", "sd", "x", "y"));
    }

    #[test]
    fn a_mapping_icu4x_already_realizes_fails_the_artifact() {
        // ICU4X replaces a deprecated subdivision code by itself, so an alias
        // naming one would be realized.
        let keyword = b"<ldmlBCP47><keyword><key name='sd'><type name='no50' alias='no23'/></key></keyword></ldmlBCP47>";
        let sources = vec![("common/bcp47/variant.xml", keyword.to_vec())];
        assert_eq!(
            artifact_from(COMMITTED.to_vec(), &sources),
            Err(GenerateFailure::Cldr(CldrFailure::Realized))
        );
    }

    #[test]
    fn records_that_do_not_make_conformance_data_fail_the_artifact() {
        let sources = vec![("common/validity/region.xml", b"<unclosed>".to_vec())];
        assert_eq!(
            artifact_from(COMMITTED.to_vec(), &sources),
            Err(GenerateFailure::Cldr(CldrFailure::Unreadable))
        );
        assert_eq!(
            artifact_from(vec![0, 0, 0], &sources),
            Err(GenerateFailure::Dataset)
        );
    }
}
