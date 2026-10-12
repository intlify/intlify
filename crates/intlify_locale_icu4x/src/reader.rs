// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 017's reader of a Locale Canonicalization Data Artifact, in its six
//! steps. It fetches nothing: the caller reads the manifest, learns from it
//! where each payload is, and hands over the exact bytes.
//!
//! [`VerifiedManifest::read`] takes the manifest through 015's pre-invocation
//! boundary (017 steps 1 and 2), checks the limits a manifest decides
//! (payload count and each payload's declared length), and verifies its
//! integrity (step 3). [`VerifiedManifest::admit`] checks each payload's
//! length and file digest (step 4), decodes each under its format (step 5),
//! and rebuilds the dataset to compare its semantic digest (step 6).
//!
//! Admitting data is not admitting a specification. Whether the
//! specification is supported, and whether the dataset is the one it admits,
//! are the owner's checks, which come after this.

use std::fmt;

use intlify_shared_json::encoding::EncodingFailure;
use intlify_shared_json::token::VersionedIdentity;

use crate::conformance::{ConformanceFailure, ConformanceLimits, LocaleConformanceData};
use crate::dataset::{self, DATASET_SCHEMA_IDENTITY, DATASET_SCHEMA_REVISION};
use crate::manifest::{self, EnvelopeFailure, LocaleCanonicalizationDataArtifact, Payload, Role};
use crate::markers::{BlobFailure, MarkerBlob};
use crate::payload::{LogicalPayload, PayloadFailure, Within};
use crate::primitives::{FileDigest, LogicalAddress};

/// The format of a conformance payload: canonical JSON text revision "0".
pub const CONFORMANCE_FORMAT: (&str, &str) = ("intlify-canonical-json-text", "0");

/// The format of a markers payload, which is the ICU4X provider schema: a
/// blob of ICU4X 2.2.0.
pub const MARKERS_FORMAT: (&str, &str) = ("icu4x-blob", "2.2");

/// The caller's finite limits, all of them checked before the work they
/// bound. There are no defaults: 017 brings no Resource Limit Policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadLimits {
    /// The manifest's bytes, checked before it is decoded.
    pub manifest_bytes: u64,
    /// The manifest's nesting of objects and arrays.
    pub manifest_depth: u64,
    /// The manifest's payload entries.
    pub payloads: u64,
    /// The declared byte length of each payload, checked before any is
    /// fetched.
    pub payload_bytes: u64,
    /// The four logical marker payloads as compact JSON text, counted before
    /// any of them is built.
    pub marker_bytes: u64,
    pub conformance: ConformanceLimits,
}

/// Why a Locale Canonicalization Data Artifact was not admitted. No variant
/// keeps any of its bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadFailure {
    /// 015's `locale-canonicalization-data-artifact` pre-invocation boundary;
    /// the resolver is not invoked.
    Envelope(EnvelopeFailure),
    /// Everything after the boundary, which 015 reports as
    /// `project-profile-locale-data-not-admitted` during specification
    /// admission.
    Data(DataFailure),
}

/// Why data behind a well-formed manifest was not admitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataFailure {
    /// A count or length is over the caller's limit.
    Limit(ArtifactLimit),
    /// A value is beyond the shared encoding's own capacity, so its digest
    /// cannot be computed, which is not the same as a digest that disagrees.
    Unframable,
    /// The manifest's integrity digest is not the digest of the rest of it.
    Integrity,
    /// A payload's bytes are not the ones the manifest names.
    Payload(Role, PayloadCheck),
    /// The manifest names a dataset schema this reader does not implement.
    DatasetSchema,
    /// A payload's format is not the one its role admits.
    Format(Role),
    Conformance(ConformanceFailure),
    Markers(BlobFailure),
    /// A marker family's logical payload could not be taken. With ICU4X 2.2's
    /// four data structs this does not happen after the blob has decoded.
    MarkerPayload(PayloadFailure),
    /// The rebuilt dataset's semantic digest is not the manifest's.
    SemanticDigest,
}

/// What a data limit bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactLimit {
    Payloads,
    PayloadBytes,
    MarkerBytes,
}

/// How a payload's bytes failed to be the ones the manifest names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadCheck {
    /// No bytes were supplied.
    Missing,
    /// The bytes are shorter or longer than the declared length.
    Length,
    /// The bytes have another file digest.
    Digest,
}

/// A manifest past the pre-invocation boundary, within the limits it decides,
/// and with a verified integrity digest. Only [`VerifiedManifest::read`]
/// makes one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedManifest {
    manifest: LocaleCanonicalizationDataArtifact,
}

/// The data of an admitted artifact: its manifest, the conformance data, and
/// the marker blob with ICU4X's canonicalizer built from it.
pub struct AdmittedData {
    manifest: LocaleCanonicalizationDataArtifact,
    conformance: LocaleConformanceData,
    markers: MarkerBlob,
}

impl VerifiedManifest {
    /// 017 steps 1 to 3 for the manifest: its limits, its strict, closed
    /// decoding, the limits it decides, and its integrity.
    pub fn read(bytes: &[u8], limits: &ReadLimits) -> Result<Self, ReadFailure> {
        let manifest = manifest::decode(bytes, limits.manifest_bytes, limits.manifest_depth)
            .map_err(ReadFailure::Envelope)?;
        if count(manifest.payloads().len()) > limits.payloads {
            return Err(data(DataFailure::Limit(ArtifactLimit::Payloads)));
        }
        if manifest
            .payloads()
            .iter()
            .any(|payload| payload.byte_length() > limits.payload_bytes)
        {
            return Err(data(DataFailure::Limit(ArtifactLimit::PayloadBytes)));
        }
        if !framed(manifest.verify_integrity())? {
            return Err(data(DataFailure::Integrity));
        }
        Ok(Self { manifest })
    }

    pub const fn manifest(&self) -> &LocaleCanonicalizationDataArtifact {
        &self.manifest
    }

    /// Where the caller finds the payload of `role`, relative to the
    /// manifest's location.
    pub fn address(&self, role: Role) -> &LogicalAddress {
        self.payload(role).address()
    }

    /// 017 steps 4 to 6: every payload's bytes, then each payload decoded
    /// under its format, then the dataset rebuilt and its semantic digest
    /// compared. `None` is a payload the caller could not supply.
    pub fn admit(
        self,
        conformance: Option<&[u8]>,
        markers: Option<Box<[u8]>>,
        limits: &ReadLimits,
    ) -> Result<AdmittedData, ReadFailure> {
        // Step 4, for every payload before any is decoded.
        let conformance = supplied(self.payload(Role::Conformance), conformance)?;
        supplied(self.payload(Role::Markers), markers.as_deref())?;
        // `supplied` has just refused a missing payload.
        let markers = markers.unwrap_or_default();

        // Step 5. The dataset schema decides what the payloads must hold.
        let schema = self.manifest.dataset().schema();
        if !named(schema, (DATASET_SCHEMA_IDENTITY, DATASET_SCHEMA_REVISION)) {
            return Err(data(DataFailure::DatasetSchema));
        }
        let conformance = self.conformance(conformance, &limits.conformance)?;
        let (markers, payloads) = self.markers(markers, limits.marker_bytes)?;

        // Step 6.
        let digest = framed(dataset::digest_of(&payloads, &conformance))?;
        if digest != *self.manifest.dataset().semantic_digest() {
            return Err(data(DataFailure::SemanticDigest));
        }
        Ok(AdmittedData {
            manifest: self.manifest,
            conformance,
            markers,
        })
    }

    /// The conformance payload decoded under the one format its role admits.
    fn conformance(
        &self,
        bytes: &[u8],
        limits: &ConformanceLimits,
    ) -> Result<LocaleConformanceData, ReadFailure> {
        if !named(self.payload(Role::Conformance).format(), CONFORMANCE_FORMAT) {
            return Err(data(DataFailure::Format(Role::Conformance)));
        }
        LocaleConformanceData::read(bytes, limits)
            .map_err(|failure| data(DataFailure::Conformance(failure)))
    }

    /// The markers payload decoded under the one format its role admits, with
    /// its logical payloads taken within the limit.
    fn markers(
        &self,
        bytes: Box<[u8]>,
        limit: u64,
    ) -> Result<(MarkerBlob, Vec<LogicalPayload>), ReadFailure> {
        if !named(self.payload(Role::Markers).format(), MARKERS_FORMAT) {
            return Err(data(DataFailure::Format(Role::Markers)));
        }
        let blob =
            MarkerBlob::read(bytes).map_err(|failure| data(DataFailure::Markers(failure)))?;
        let payloads = blob.payloads_within(limit).map_err(|failure| {
            data(match failure {
                Within::Over => DataFailure::Limit(ArtifactLimit::MarkerBytes),
                Within::Failed(failure) => DataFailure::MarkerPayload(failure),
            })
        })?;
        Ok((blob, payloads))
    }

    /// The payload of `role`. The closed manifest has one per role, in role
    /// order.
    fn payload(&self, role: Role) -> &Payload {
        let index = match role {
            Role::Conformance => 0,
            Role::Markers => 1,
        };
        &self.manifest.payloads()[index]
    }
}

impl AdmittedData {
    pub const fn manifest(&self) -> &LocaleCanonicalizationDataArtifact {
        &self.manifest
    }

    pub const fn conformance(&self) -> &LocaleConformanceData {
        &self.conformance
    }

    pub const fn markers(&self) -> &MarkerBlob {
        &self.markers
    }
}

impl fmt::Debug for AdmittedData {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdmittedData")
            .field("manifest", &self.manifest)
            .field("conformance", &self.conformance)
            .finish_non_exhaustive()
    }
}

/// The supplied bytes of one payload, if they are the ones its entry names:
/// present, of the declared length, and with the declared file digest.
fn supplied<'a>(payload: &Payload, bytes: Option<&'a [u8]>) -> Result<&'a [u8], ReadFailure> {
    let failure = |check| data(DataFailure::Payload(payload.role(), check));
    let bytes = bytes.ok_or_else(|| failure(PayloadCheck::Missing))?;
    if count(bytes.len()) != payload.byte_length() {
        return Err(failure(PayloadCheck::Length));
    }
    if FileDigest::of(bytes) != *payload.digest() {
        return Err(failure(PayloadCheck::Digest));
    }
    Ok(bytes)
}

/// Whether `identity` is exactly `(identity, revision)`.
fn named(identity: &VersionedIdentity, (name, revision): (&str, &str)) -> bool {
    identity.identity().as_str() == name && identity.revision().as_str() == revision
}

fn data(failure: DataFailure) -> ReadFailure {
    ReadFailure::Data(failure)
}

/// A digest the shared encoding could not compute is its own failure.
fn framed<T>(result: Result<T, EncodingFailure>) -> Result<T, ReadFailure> {
    result.map_err(|_| data(DataFailure::Unframable))
}

fn count(length: usize) -> u64 {
    u64::try_from(length).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use intlify_shared_json::text;
    use serde_json::{json, Value};

    use super::*;
    use crate::conformance::{Collection, Limited};
    use crate::manifest::tests::NARROW;
    use crate::markers::Layout;

    const WIDE: &[u8] = include_bytes!("../fixtures/artifact/manifest-wide.json");
    const VENDORED: &[u8] = include_bytes!("../fixtures/artifact/manifest-vendored.json");
    const CONFORMANCE: &[u8] = include_bytes!("../fixtures/artifact/conformance.json");
    const MARKERS_NARROW: &[u8] = include_bytes!("../fixtures/artifact/markers-narrow.postcard");
    const MARKERS_WIDE: &[u8] = include_bytes!("../fixtures/artifact/markers-wide.postcard");

    const GENEROUS: ReadLimits = ReadLimits {
        manifest_bytes: u64::MAX,
        manifest_depth: u64::MAX,
        payloads: u64::MAX,
        payload_bytes: u64::MAX,
        marker_bytes: u64::MAX,
        conformance: crate::conformance::tests::GENEROUS,
    };

    fn admit_within(
        manifest: &[u8],
        conformance: Option<&[u8]>,
        markers: Option<&[u8]>,
        limits: &ReadLimits,
    ) -> Result<AdmittedData, ReadFailure> {
        VerifiedManifest::read(manifest, limits)?.admit(conformance, markers.map(Box::from), limits)
    }

    fn admit(
        manifest: &[u8],
        conformance: Option<&[u8]>,
        markers: Option<&[u8]>,
    ) -> Result<AdmittedData, ReadFailure> {
        admit_within(manifest, conformance, markers, &GENEROUS)
    }

    /// The narrow manifest, edited and sealed again.
    fn resealed(edit: impl FnOnce(&mut Value)) -> Vec<u8> {
        let mut value: Value = serde_json::from_slice(NARROW).unwrap();
        edit(&mut value);
        let sealed = serde_json::from_value::<LocaleCanonicalizationDataArtifact>(value)
            .unwrap()
            .seal()
            .unwrap();
        text::write(&serde_json::to_value(sealed).unwrap())
            .unwrap()
            .into_bytes()
    }

    /// The narrow manifest pointing at other bytes for one payload.
    fn pointing_at(index: usize, bytes: &[u8]) -> Vec<u8> {
        resealed(|value| {
            value["payloads"][index]["byteLength"] = json!(bytes.len().to_string());
            value["payloads"][index]["digest"] = json!(FileDigest::of(bytes).as_str());
        })
    }

    fn data_failure(result: Result<AdmittedData, ReadFailure>) -> DataFailure {
        match result {
            Err(ReadFailure::Data(failure)) => failure,
            other => panic!("not a data failure: {other:?}"),
        }
    }

    fn dataset_digest() -> Value {
        crate::conformance::tests::vectors()["semanticDigest"].clone()
    }

    #[test]
    fn every_fixture_artifact_is_admitted_with_the_vectors_semantic_digest() {
        for (manifest, markers, layout) in [
            (NARROW, MARKERS_NARROW, Layout::Narrow),
            (WIDE, MARKERS_WIDE, Layout::Wide),
            (VENDORED, MARKERS_NARROW, Layout::Narrow),
        ] {
            let admitted = admit(manifest, Some(CONFORMANCE), Some(markers)).unwrap();
            assert_eq!(
                json!(admitted.manifest().dataset().semantic_digest().as_str()),
                dataset_digest()
            );
            assert_eq!(admitted.markers().layout(), layout);
            assert_eq!(admitted.conformance().to_text().as_bytes(), CONFORMANCE);
            // The blob has no Debug of its own, so it is left out by name.
            let debug = format!("{admitted:?}");
            assert!(debug.starts_with("AdmittedData { manifest: "), "{debug}");
            assert!(
                debug.contains(", conformance: LocaleConformanceData { "),
                "{debug}"
            );
            assert!(debug.ends_with(", .. }"), "{debug}");
        }
    }

    #[test]
    fn two_layouts_or_moved_sources_keep_one_dataset_and_change_the_artifact() {
        let read = |bytes| VerifiedManifest::read(bytes, &GENEROUS).unwrap();
        let (narrow, wide, vendored) = (read(NARROW), read(WIDE), read(VENDORED));
        let digest = |manifest: &VerifiedManifest, index: usize| {
            manifest.manifest().payloads()[index].digest().clone()
        };
        // The same dataset in two blob layouts: one semantic digest, two file
        // digests for the markers payload, two artifacts.
        assert_eq!(narrow.manifest().dataset(), wide.manifest().dataset());
        assert_eq!(digest(&narrow, 0), digest(&wide, 0));
        assert_ne!(digest(&narrow, 1), digest(&wide, 1));
        assert_ne!(
            narrow.manifest().integrity_digest(),
            wide.manifest().integrity_digest()
        );
        // Provenance alone: the same payloads and dataset, another artifact.
        assert_eq!(narrow.manifest().dataset(), vendored.manifest().dataset());
        assert_eq!(narrow.manifest().payloads(), vendored.manifest().payloads());
        assert_ne!(
            narrow.manifest().integrity_digest(),
            vendored.manifest().integrity_digest()
        );
    }

    #[test]
    fn the_caller_learns_each_payload_address_from_the_verified_manifest() {
        let manifest = VerifiedManifest::read(WIDE, &GENEROUS).unwrap();
        assert_eq!(
            manifest.address(Role::Conformance).as_str(),
            "conformance.json"
        );
        assert_eq!(
            manifest.address(Role::Markers).as_str(),
            "markers-wide.postcard"
        );
    }

    #[test]
    fn a_manifest_at_the_boundary_is_an_envelope_failure() {
        for (bytes, failure) in [
            (b"{}".as_slice(), EnvelopeFailure::Malformed),
            (b"\xff".as_slice(), EnvelopeFailure::Malformed),
        ] {
            assert_eq!(
                VerifiedManifest::read(bytes, &GENEROUS),
                Err(ReadFailure::Envelope(failure))
            );
        }
        let limits = ReadLimits {
            manifest_bytes: 1,
            ..GENEROUS
        };
        assert_eq!(
            VerifiedManifest::read(NARROW, &limits),
            Err(ReadFailure::Envelope(EnvelopeFailure::OverBound(
                crate::manifest::EnvelopeLimit::Bytes
            )))
        );
    }

    #[test]
    fn a_manifest_whose_digest_is_not_its_own_is_refused() {
        let mut value: Value = serde_json::from_slice(NARROW).unwrap();
        value["dataset"]["identity"] = json!("another-dataset");
        let tampered = serde_json::to_vec(&value).unwrap();
        value["integrityDigest"] = json!(format!("sha256:{}", "1".repeat(64)));
        let replaced = serde_json::to_vec(&value).unwrap();
        for bytes in [tampered, replaced] {
            assert_eq!(
                VerifiedManifest::read(&bytes, &GENEROUS),
                Err(ReadFailure::Data(DataFailure::Integrity))
            );
        }
    }

    #[test]
    fn a_payload_must_be_supplied_with_its_declared_length_and_digest() {
        let mut altered = CONFORMANCE.to_vec();
        altered[10] ^= 1;
        let mut extended = MARKERS_NARROW.to_vec();
        extended.push(0);
        let mut flipped = MARKERS_NARROW.to_vec();
        flipped[100] ^= 1;
        let truncated = &CONFORMANCE[..CONFORMANCE.len() - 1];
        for (conformance, markers, failure) in [
            (
                None,
                Some(MARKERS_NARROW),
                DataFailure::Payload(Role::Conformance, PayloadCheck::Missing),
            ),
            (
                Some(CONFORMANCE),
                None,
                DataFailure::Payload(Role::Markers, PayloadCheck::Missing),
            ),
            (
                Some(truncated),
                Some(MARKERS_NARROW),
                DataFailure::Payload(Role::Conformance, PayloadCheck::Length),
            ),
            (
                Some(CONFORMANCE),
                Some(extended.as_slice()),
                DataFailure::Payload(Role::Markers, PayloadCheck::Length),
            ),
            (
                Some(altered.as_slice()),
                Some(MARKERS_NARROW),
                DataFailure::Payload(Role::Conformance, PayloadCheck::Digest),
            ),
            (
                Some(CONFORMANCE),
                Some(flipped.as_slice()),
                DataFailure::Payload(Role::Markers, PayloadCheck::Digest),
            ),
            // Each payload is the other's wrong bytes.
            (
                Some(MARKERS_NARROW),
                Some(CONFORMANCE),
                DataFailure::Payload(Role::Conformance, PayloadCheck::Length),
            ),
        ] {
            assert_eq!(data_failure(admit(NARROW, conformance, markers)), failure);
        }
    }

    #[test]
    fn a_declared_length_or_digest_that_disagrees_with_the_bytes_is_refused() {
        let longer = resealed(|value| value["payloads"][1]["byteLength"] = json!("567"));
        assert_eq!(
            data_failure(admit(&longer, Some(CONFORMANCE), Some(MARKERS_NARROW))),
            DataFailure::Payload(Role::Markers, PayloadCheck::Length)
        );
        let other = resealed(|value| {
            value["payloads"][0]["digest"] = json!(FileDigest::of(b"other").as_str());
        });
        assert_eq!(
            data_failure(admit(&other, Some(CONFORMANCE), Some(MARKERS_NARROW))),
            DataFailure::Payload(Role::Conformance, PayloadCheck::Digest)
        );
    }

    #[test]
    fn the_payload_count_and_declared_lengths_are_limited_before_anything_is_fetched() {
        let longest = u64::try_from(CONFORMANCE.len().max(MARKERS_NARROW.len())).unwrap();
        let exact = ReadLimits {
            payloads: 2,
            payload_bytes: longest,
            ..GENEROUS
        };
        assert!(admit_within(NARROW, Some(CONFORMANCE), Some(MARKERS_NARROW), &exact).is_ok());
        for (limits, limit) in [
            (
                ReadLimits {
                    payloads: 1,
                    ..exact
                },
                ArtifactLimit::Payloads,
            ),
            (
                ReadLimits {
                    payload_bytes: longest - 1,
                    ..exact
                },
                ArtifactLimit::PayloadBytes,
            ),
        ] {
            assert_eq!(
                VerifiedManifest::read(NARROW, &limits),
                Err(ReadFailure::Data(DataFailure::Limit(limit)))
            );
        }
    }

    #[test]
    fn the_logical_marker_payloads_are_limited_before_they_are_built() {
        let written: u64 = crate::conformance::tests::vectors()["markers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| {
                u64::try_from(serde_json::to_string(&entry["payload"]).unwrap().len()).unwrap()
            })
            .sum();
        let exact = ReadLimits {
            marker_bytes: written,
            ..GENEROUS
        };
        assert!(admit_within(NARROW, Some(CONFORMANCE), Some(MARKERS_NARROW), &exact).is_ok());
        let over = ReadLimits {
            marker_bytes: written - 1,
            ..GENEROUS
        };
        assert_eq!(
            data_failure(admit_within(
                NARROW,
                Some(CONFORMANCE),
                Some(MARKERS_NARROW),
                &over
            )),
            DataFailure::Limit(ArtifactLimit::MarkerBytes)
        );
        let conformance = ReadLimits {
            conformance: crate::conformance::ConformanceLimits {
                codes: 19,
                ..crate::conformance::tests::GENEROUS
            },
            ..GENEROUS
        };
        assert_eq!(
            data_failure(admit_within(
                NARROW,
                Some(CONFORMANCE),
                Some(MARKERS_NARROW),
                &conformance
            )),
            DataFailure::Conformance(ConformanceFailure::Limit(Limited::Codes))
        );
    }

    #[test]
    fn a_dataset_schema_or_payload_format_this_reader_does_not_implement_is_refused() {
        for (edit, failure) in [
            (
                ("/dataset/schema/revision", json!("1")),
                DataFailure::DatasetSchema,
            ),
            (
                ("/dataset/schema/identity", json!("another-dataset-schema")),
                DataFailure::DatasetSchema,
            ),
            (
                ("/payloads/0/format/revision", json!("1")),
                DataFailure::Format(Role::Conformance),
            ),
            (
                ("/payloads/0/format/identity", json!("icu4x-blob")),
                DataFailure::Format(Role::Conformance),
            ),
            (
                ("/payloads/1/format/revision", json!("2.3")),
                DataFailure::Format(Role::Markers),
            ),
            (
                ("/payloads/1/format/identity", json!("icu4x-baked")),
                DataFailure::Format(Role::Markers),
            ),
        ] {
            let (pointer, value) = edit;
            let manifest = resealed(|manifest| *manifest.pointer_mut(pointer).unwrap() = value);
            assert_eq!(
                data_failure(admit(&manifest, Some(CONFORMANCE), Some(MARKERS_NARROW))),
                failure,
                "{pointer}"
            );
        }
        assert_eq!(CONFORMANCE_FORMAT, ("intlify-canonical-json-text", "0"));
        assert_eq!(MARKERS_FORMAT, ("icu4x-blob", "2.2"));
    }

    #[test]
    fn a_payload_that_does_not_decode_under_its_format_is_refused_with_the_reason() {
        let mut conformance = crate::conformance::tests::sample();
        conformance["validity"].as_array_mut().unwrap().swap(0, 1);
        let unsorted = text::write(&conformance).unwrap().into_bytes();
        assert_eq!(
            data_failure(admit(
                &pointing_at(0, &unsorted),
                Some(&unsorted),
                Some(MARKERS_NARROW)
            )),
            DataFailure::Conformance(ConformanceFailure::Unsorted(Collection::ValidityGroups))
        );
        let old = [0_u8, 0, 0];
        assert_eq!(
            data_failure(admit(&pointing_at(1, &old), Some(CONFORMANCE), Some(&old))),
            DataFailure::Markers(BlobFailure::UnsupportedSchema)
        );
    }

    #[test]
    fn a_semantic_digest_other_than_the_one_of_the_data_is_refused() {
        let edits = crate::conformance::tests::vectors()["edits"].clone();
        let manifest = resealed(|value| {
            value["dataset"]["semanticDigest"] = edits[0]["semanticDigest"].clone();
        });
        assert_eq!(
            data_failure(admit(&manifest, Some(CONFORMANCE), Some(MARKERS_NARROW))),
            DataFailure::SemanticDigest
        );
        // The wide blob carries the same data, so pointing the narrow
        // manifest's entry at it changes nothing but the transport facts.
        assert!(admit(
            &pointing_at(1, MARKERS_WIDE),
            Some(CONFORMANCE),
            Some(MARKERS_WIDE)
        )
        .is_ok());
    }

    #[test]
    fn the_steps_fail_in_017s_order() {
        // An unsupported format behind a payload with the wrong bytes: the
        // bytes are step 4, the format step 5.
        let manifest = resealed(|value| value["payloads"][1]["format"]["revision"] = json!("2.3"));
        assert_eq!(
            data_failure(admit(
                &manifest,
                Some(CONFORMANCE),
                Some(&MARKERS_NARROW[1..])
            )),
            DataFailure::Payload(Role::Markers, PayloadCheck::Length)
        );
        // The dataset schema is decided before either payload is decoded.
        let manifest = resealed(|value| {
            value["dataset"]["schema"]["revision"] = json!("1");
            value["payloads"][0]["format"]["revision"] = json!("1");
        });
        assert_eq!(
            data_failure(admit(&manifest, Some(CONFORMANCE), Some(MARKERS_NARROW))),
            DataFailure::DatasetSchema
        );
        // Every payload's bytes are checked before any is decoded.
        let old = [0_u8, 0, 0];
        let old_blob = pointing_at(1, &old);
        assert_eq!(
            data_failure(admit(&old_blob, Some(CONFORMANCE), Some(&old[1..]))),
            DataFailure::Payload(Role::Markers, PayloadCheck::Length)
        );
    }

    #[test]
    fn a_digest_the_encoding_cannot_compute_is_unframable() {
        assert_eq!(
            framed::<()>(Err(EncodingFailure::Capacity)),
            Err(ReadFailure::Data(DataFailure::Unframable))
        );
        assert_eq!(framed(Ok(7)), Ok(7));
    }

    #[cfg(feature = "generate")]
    #[test]
    fn a_blob_with_a_family_missing_or_added_is_refused_by_name() {
        use crate::generate::{blob, payload_blob, Family};
        use crate::markers::Marker;
        use crate::payload::LogicalPayload;

        let payloads: Vec<LogicalPayload> = crate::conformance::tests::vectors()["markers"]
            .as_array()
            .unwrap()
            .iter()
            .zip(Marker::ALL)
            .map(|(entry, marker)| LogicalPayload::new(marker, entry["payload"].clone()).unwrap())
            .collect();
        for missing in Marker::ALL {
            let present: Vec<_> = payloads
                .iter()
                .filter(|payload| payload.marker() != missing)
                .cloned()
                .collect();
            let bytes = payload_blob(&present).unwrap();
            assert_eq!(
                data_failure(admit(
                    &pointing_at(1, &bytes),
                    Some(CONFORMANCE),
                    Some(&bytes)
                )),
                DataFailure::Markers(BlobFailure::MissingMarker(missing)),
                "{missing:?}"
            );
        }
        let bytes = blob(&[
            Family::Admitted(Marker::LocaleAliasesV1),
            Family::Admitted(Marker::LocaleLikelySubtagsExtendedV1),
            Family::Admitted(Marker::LocaleLikelySubtagsLanguageV1),
            Family::Admitted(Marker::LocaleLikelySubtagsScriptRegionV1),
            Family::Parents,
        ])
        .unwrap();
        assert_eq!(
            data_failure(admit(
                &pointing_at(1, &bytes),
                Some(CONFORMANCE),
                Some(&bytes)
            )),
            DataFailure::Markers(BlobFailure::AdditionalMarker)
        );
    }
}
