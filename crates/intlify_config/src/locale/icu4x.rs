// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The ICU4X 2.2.0 reference adapter as this boundary's provider, behind the
//! non-default `icu4x` feature. It admits one Locale Canonicalization Data
//! Artifact in 017's six steps, from bytes its caller acquired, and
//! canonicalizes through 015's revision "0" conformance layer around ICU4X.
//! It fetches nothing, embeds no data, and never falls back to other locale
//! behavior.
//!
//! [`Icu4xData::read`] is 015's `locale-canonicalization-data-artifact`
//! pre-invocation boundary, the manifest's envelope alone; specification
//! admission reads the rest through [`FormalData`] (D15:3170).
//! [`Icu4xProvider::admit`] does both at once, for callers outside an
//! invocation.

use std::collections::BTreeMap;
use std::sync::Arc;

use intlify_locale_icu4x::{
    AdmittedData, ConformanceCounts, DecodedManifest, EnvelopeFailure, LayerFailure, Measured,
    ReadFailure, ReadLimits, Role, VerifiedManifest, ENGINE, MARKERS_FORMAT,
};

use crate::input_limits::Bound;

use super::{
    Admission, AdmissionFacts, DataCount, DataCounts, DataFacts, DataNotAdmitted, DataReading,
    DatasetPin, EnvelopeReason, FormalData, LocaleDataLimits, PayloadFacts, Provider,
    ProviderBinding, ProviderFailure, SemanticPins, VersionedIdentity,
};

/// One artifact past 015's `locale-canonicalization-data-artifact` boundary:
/// its manifest within the capability's `inputBoundary` bounds and in 017's
/// closed shape, and the exact bytes the caller supplied for each payload the
/// manifest names, `None` for one it supplied none for (D17:1065). That is
/// its value: a manifest spelled with other bytes but decoding the same is the
/// same data, and a payload that differs in one byte is other data. Only
/// [`Icu4xData::read`] makes one.
#[derive(PartialEq, Eq)]
pub(crate) struct Icu4xData {
    manifest: DecodedManifest,
    conformance: Option<Arc<[u8]>>,
    markers: Option<Arc<[u8]>>,
}

impl Icu4xData {
    /// The boundary: an absent manifest is `missing-input`, and then its
    /// bytes and depth, under `bytes` and `depth`, before anything is decoded,
    /// and its closed decoding. Of `payloads`, the bytes the caller supplied
    /// by address, the data shares those at the addresses the manifest names
    /// for specification admission to read, and the boundary reads none.
    pub(crate) fn read(
        manifest: Option<&[u8]>,
        payloads: &BTreeMap<String, Arc<[u8]>>,
        bytes: Bound,
        depth: Bound,
    ) -> Result<Self, EnvelopeReason> {
        let manifest = manifest.ok_or(EnvelopeReason::MissingInput)?;
        let manifest =
            DecodedManifest::decode(manifest, bytes.get(), depth.get()).map_err(envelope_reason)?;
        let supplied = |role| payloads.get(manifest.address(role).as_str()).cloned();
        Ok(Self {
            conformance: supplied(Role::Conformance),
            markers: supplied(Role::Markers),
            manifest,
        })
    }
}

impl FormalData for Icu4xData {
    type Provider = Icu4xProvider;

    /// The rest of 017's steps under the capability's `localeData` bounds, in
    /// the stages the resolver's checks judge: the bounds the manifest
    /// decides, the manifest's read, each payload's decoding and counts, and
    /// the rest, only of data within every bound.
    ///
    /// Nothing is fetched: each payload is the bytes kept at the address the
    /// manifest names. They are read only once the manifest is verified, and
    /// a payload kept as `None` is missing. The marker blob owns its bytes,
    /// so they are copied only once the manifest's read has held.
    fn read(&self, limits: &LocaleDataLimits) -> DataReading<Icu4xProvider> {
        let payloads = data_count(self.manifest.payload_count());
        let payload_bytes_over = self.manifest.declares_over(limits.max_payload_bytes.get());
        let (read, provider) = if self
            .manifest
            .payload_count()
            .over(limits.max_payloads.get())
            || payload_bytes_over
        {
            (Err(DataNotAdmitted), Err(DataNotAdmitted))
        } else {
            self.read_payloads(limits)
        };
        DataReading {
            facts: DataFacts {
                payloads,
                payload_bytes_over,
                read,
            },
            provider,
        }
    }
}

impl Icu4xData {
    /// The manifest's read, each payload's own, and the provider of data
    /// within the bounds of what the payloads count.
    fn read_payloads(
        &self,
        limits: &LocaleDataLimits,
    ) -> (
        Result<PayloadFacts, DataNotAdmitted>,
        Result<Icu4xProvider, DataNotAdmitted>,
    ) {
        let conformance = self.conformance.as_deref();
        let markers = self.markers.as_deref();
        let Ok(manifest) = self
            .manifest
            .clone()
            .verify_integrity()
            .and_then(|manifest| {
                manifest
                    .check_payloads(conformance, markers)
                    .map(|()| manifest)
            })
        else {
            return (Err(DataNotAdmitted), Err(DataNotAdmitted));
        };
        // The manifest's read has just refused a missing payload.
        let markers = manifest.decode_markers(markers.map(Box::from).unwrap_or_default());
        let conformance = manifest.decode_conformance(conformance.unwrap_or_default());
        let facts = PayloadFacts {
            markers: markers
                .as_ref()
                .map(|markers| data_count(markers.written_bytes()))
                .map_err(|_| DataNotAdmitted),
            conformance: conformance
                .as_ref()
                .map(|conformance| data_counts(conformance.counts()))
                .map_err(|_| DataNotAdmitted),
        };
        let provider = match (markers, conformance) {
            (Ok(markers), Ok(conformance))
                if within(markers.written_bytes(), conformance.counts(), limits) =>
            {
                manifest
                    .finish(conformance, markers)
                    .map(Icu4xProvider::of)
                    .map_err(|_| DataNotAdmitted)
            }
            _ => Err(DataNotAdmitted),
        };
        (Ok(facts), provider)
    }
}

const fn data_count(count: Measured) -> DataCount {
    match count {
        Measured::Exact(count) => DataCount::Exact(count),
        Measured::BeyondU64 => DataCount::BeyondU64,
    }
}

const fn data_counts(counts: &ConformanceCounts) -> DataCounts {
    DataCounts {
        validity_groups: data_count(counts.validity_groups),
        codes: data_count(counts.codes),
        extension_keys: data_count(counts.extension_keys),
        types: data_count(counts.types),
        corrections: data_count(counts.corrections),
        validation_work: data_count(counts.work),
    }
}

/// Whether what the payloads count is within the capability's bounds of it.
const fn within(
    marker_bytes: Measured,
    counts: &ConformanceCounts,
    limits: &LocaleDataLimits,
) -> bool {
    !(marker_bytes.over(limits.max_marker_bytes.get())
        || counts
            .validity_groups
            .over(limits.max_validity_groups.get())
        || counts.codes.over(limits.max_codes.get())
        || counts.extension_keys.over(limits.max_extension_keys.get())
        || counts.types.over(limits.max_types.get())
        || counts.corrections.over(limits.max_corrections.get())
        || counts.work.over(limits.max_validation_work_units.get()))
}

/// One admitted artifact and what it binds. Only [`Icu4xProvider::admit`]
/// makes one.
pub(crate) struct Icu4xProvider {
    data: AdmittedData,
    binding: ProviderBinding<String>,
}

impl Icu4xProvider {
    /// Admits the artifact whose manifest is `manifest`. `acquire` returns the
    /// bytes at an address the manifest names, relative to the manifest, or
    /// `None` when it has none. It is asked only once the manifest is within
    /// `limits` and its integrity holds.
    pub(crate) fn admit(
        manifest: &[u8],
        mut acquire: impl FnMut(&str) -> Option<Vec<u8>>,
        limits: &ReadLimits,
    ) -> Result<Self, Admission> {
        let manifest = VerifiedManifest::read(manifest, limits).map_err(admission)?;
        let conformance = acquire(manifest.address(Role::Conformance).as_str());
        let markers = acquire(manifest.address(Role::Markers).as_str());
        Self::from_verified(
            manifest,
            (conformance.as_deref(), markers.map(Vec::into_boxed_slice)),
            limits,
        )
        .map_err(admission)
    }

    /// 017 steps 4 to 6 over a verified manifest, with the conformance and
    /// markers payloads, each `None` when there is none.
    fn from_verified(
        manifest: VerifiedManifest,
        (conformance, markers): (Option<&[u8]>, Option<Box<[u8]>>),
        limits: &ReadLimits,
    ) -> Result<Self, ReadFailure> {
        manifest.admit(conformance, markers, limits).map(Self::of)
    }

    /// The provider of data that has been admitted.
    fn of(data: AdmittedData) -> Self {
        Self {
            binding: binding(&data),
            data,
        }
    }
}

impl Provider for Icu4xProvider {
    type Identity = String;

    fn binding(&self) -> &ProviderBinding<String> {
        &self.binding
    }

    fn canonicalize(&self, input: &str) -> Result<Arc<str>, ProviderFailure> {
        self.data
            .canonicalize(input)
            .map(Arc::from)
            .map_err(provider_failure)
    }
}

/// What the admitted manifest pins, and the facts of its admission: ICU4X's
/// release, the provider schema of the marker blob, and the manifest's
/// integrity digest.
fn binding(data: &AdmittedData) -> ProviderBinding<String> {
    let manifest = data.manifest();
    let specification = manifest.specification();
    let dataset = manifest.dataset();
    ProviderBinding {
        pins: SemanticPins {
            specification: VersionedIdentity {
                identity: specification.identity().as_str().into(),
                revision: specification.revision().as_str().into(),
            },
            dataset: DatasetPin {
                identity: dataset.identity().as_str().into(),
                semantic_digest: dataset.semantic_digest().as_str().into(),
            },
        },
        admission: AdmissionFacts {
            provider: VersionedIdentity {
                identity: ENGINE.0.into(),
                revision: ENGINE.1.into(),
            },
            // The reader admits a marker blob under this provider schema alone.
            provider_schema: VersionedIdentity {
                identity: MARKERS_FORMAT.0.into(),
                revision: MARKERS_FORMAT.1.into(),
            },
            artifact: manifest.integrity_digest().as_str().into(),
        },
    }
}

/// Where 015 reports a reader's failure: a manifest that is no envelope the
/// boundary can read is the pre-invocation boundary's, and everything after
/// it is data that was not admitted.
fn admission(failure: ReadFailure) -> Admission {
    match failure {
        ReadFailure::Envelope(envelope) => Admission::PreInvocation(envelope_reason(envelope)),
        ReadFailure::Data(_) => Admission::Data,
    }
}

/// 015's closed reason for a manifest the boundary cannot read.
const fn envelope_reason(failure: EnvelopeFailure) -> EnvelopeReason {
    match failure {
        EnvelopeFailure::OverBound(_) => EnvelopeReason::OverBoundEnvelope,
        EnvelopeFailure::Malformed | EnvelopeFailure::Unsupported => {
            EnvelopeReason::MalformedEnvelope
        }
        EnvelopeFailure::DuplicateMember => EnvelopeReason::DuplicateMember,
        EnvelopeFailure::UnknownMember => EnvelopeReason::UnknownMember,
    }
}

/// 015 gives an invalid identifier no reason, so whichever rule the layer
/// names, the identifier is invalid. One the layer admitted and ICU4X could
/// not read, or whose canonical form does not hold, is the adapter's failure
/// and never the author's.
fn provider_failure(failure: LayerFailure) -> ProviderFailure {
    match failure {
        LayerFailure::Rejected(_) => ProviderFailure::InvalidIdentifier,
        LayerFailure::Engine | LayerFailure::NotIdempotent => ProviderFailure::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use intlify_locale_icu4x::{
        ArtifactLimit, Component, ConformanceLimits, DataFailure, EnvelopeLimit, FileDigest,
        LocaleCanonicalizationDataArtifact, PayloadCheck, Rejection,
    };
    use serde_json::{json, Value};

    use super::*;
    use crate::input_limits::Bound;
    use crate::locale::fixtures::{ALIASES, CANONICAL, INVALID};
    use crate::locale::{
        BindingFailure, BindingPart, CanonicalizationFailure, Canonicalizer, Supported,
    };
    use crate::resolver::evaluation::{Actual, BootstrapLimitExceeded, Check, Code, Evaluation};
    use crate::resolver::limits::{fixture_limits, ConfirmedPolicy};
    use crate::resolver::locale_data::tests::{found, slot_causes, waiting};
    use crate::resolver::locale_data::{admit_data, DataBound};

    const MANIFEST: &[u8] = include_bytes!("../../../intlify_locale_icu4x/data/manifest.json");
    const CONFORMANCE: &[u8] =
        include_bytes!("../../../intlify_locale_icu4x/data/conformance.json");
    const MARKERS: &[u8] = include_bytes!("../../../intlify_locale_icu4x/data/markers.postcard");

    // The adapter's fixture artifact: revision "0"'s specification over a
    // small test dataset.
    const OTHER_MANIFEST: &[u8] =
        include_bytes!("../../../intlify_locale_icu4x/fixtures/artifact/manifest-narrow.json");
    const OTHER_CONFORMANCE: &[u8] =
        include_bytes!("../../../intlify_locale_icu4x/fixtures/artifact/conformance.json");
    const OTHER_MARKERS: &[u8] =
        include_bytes!("../../../intlify_locale_icu4x/fixtures/artifact/markers-narrow.postcard");

    // 015's handwritten cases, as the adapter's layer tests read them.
    const CASES: &str = include_str!("../../../intlify_locale_icu4x/fixtures/layer/cases.json");

    /// Finite limits the committed artifact is within, as a host gives them.
    const LIMITS: ReadLimits = ReadLimits {
        manifest_bytes: 16_384,
        manifest_depth: 8,
        payloads: 2,
        payload_bytes: 1 << 20,
        marker_bytes: 1 << 20,
        conformance: ConformanceLimits {
            validity_groups: 64,
            codes: 1 << 15,
            extension_keys: 64,
            types: 2_048,
            corrections: 128,
            work: 1 << 16,
        },
    };

    /// The committed artifact's payloads, at the addresses its manifest names.
    fn payload(address: &str) -> Option<Vec<u8>> {
        match address {
            "conformance.json" => Some(CONFORMANCE.to_vec()),
            "markers.postcard" => Some(MARKERS.to_vec()),
            _ => None,
        }
    }

    /// The committed artifact's payloads by address, as a caller supplies
    /// them with the manifest.
    fn payloads() -> BTreeMap<String, Arc<[u8]>> {
        ["conformance.json", "markers.postcard"]
            .map(|address| (address.to_owned(), payload(address).unwrap().into()))
            .into()
    }

    fn committed() -> Icu4xProvider {
        Icu4xProvider::admit(MANIFEST, payload, &LIMITS).unwrap()
    }

    fn revision_zero() -> VersionedIdentity<String> {
        VersionedIdentity {
            identity: "intlify-locale-canonicalization".into(),
            revision: "0".into(),
        }
    }

    fn bind(provider: Icu4xProvider) -> Result<Canonicalizer<Icu4xProvider>, BindingFailure> {
        let supported = Supported::revision_zero();
        let expected = supported.expected(&revision_zero()).unwrap();
        Canonicalizer::bind(expected, Some(provider), Bound::new(128).unwrap())
    }

    #[test]
    fn the_committed_artifact_is_the_dataset_revision_0_admits() {
        let provider = committed();
        let supported = Supported::revision_zero();
        assert_eq!(
            Some(&provider.binding().pins),
            supported.expected(&revision_zero()).ok()
        );
        assert!(bind(provider).is_ok());
    }

    #[test]
    fn the_binding_reports_the_manifest_and_icu4x() {
        let manifest: Value = serde_json::from_slice(MANIFEST).unwrap();
        let text = |pointer: &str| {
            manifest
                .pointer(pointer)
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned()
        };
        assert_eq!(text("/payloads/1/role"), "markers");
        assert_eq!(
            committed().binding(),
            &ProviderBinding {
                pins: SemanticPins {
                    specification: VersionedIdentity {
                        identity: text("/specification/identity"),
                        revision: text("/specification/revision"),
                    },
                    dataset: DatasetPin {
                        identity: text("/dataset/identity"),
                        semantic_digest: text("/dataset/semanticDigest"),
                    },
                },
                admission: AdmissionFacts {
                    provider: VersionedIdentity {
                        identity: "icu4x".into(),
                        revision: "2.2.0".into(),
                    },
                    provider_schema: VersionedIdentity {
                        identity: text("/payloads/1/format/identity"),
                        revision: text("/payloads/1/format/revision"),
                    },
                    artifact: text("/integrityDigest"),
                },
            }
        );
    }

    #[test]
    fn payloads_are_acquired_at_the_addresses_the_manifest_names_once_it_is_read() {
        let mut asked = Vec::new();
        let admitted = Icu4xProvider::admit(
            MANIFEST,
            |address| {
                asked.push(address.to_owned());
                payload(address)
            },
            &LIMITS,
        );
        assert!(admitted.is_ok());
        assert_eq!(asked, ["conformance.json", "markers.postcard"]);

        let mut asked = Vec::new();
        let refused = Icu4xProvider::admit(
            b"{}",
            |address| {
                asked.push(address.to_owned());
                payload(address)
            },
            &LIMITS,
        );
        assert_eq!(
            refused.err(),
            Some(Admission::PreInvocation(EnvelopeReason::MalformedEnvelope))
        );
        assert!(asked.is_empty());
    }

    #[test]
    fn the_finite_fixtures_answers_are_revision_0s() {
        let canonicalizer = bind(committed()).unwrap();
        for &input in CANONICAL {
            let result = canonicalizer.canonicalize(input).unwrap();
            assert_eq!(result.locale().as_str(), input);
            assert_eq!(result.suggested_replacement(), None);
        }
        for &(input, canonical) in ALIASES {
            let result = canonicalizer.canonicalize(input).unwrap();
            assert_eq!(result.suggested_replacement(), Some(canonical), "{input}");
        }
        for &input in INVALID {
            assert_eq!(
                canonicalizer.canonicalize(input).err(),
                Some(CanonicalizationFailure::Provider(
                    ProviderFailure::InvalidIdentifier
                )),
                "{input}"
            );
        }
    }

    #[test]
    fn the_handwritten_015_cases_hold_at_this_boundary() {
        let canonicalizer = bind(committed()).unwrap();
        let cases: Value = serde_json::from_str(CASES).unwrap();
        let mut count = 0;
        for group in cases["groups"].as_array().unwrap() {
            for case in group["cases"].as_array().unwrap() {
                let input = case["input"].as_str().unwrap();
                let result = canonicalizer.canonicalize(input);
                if let Some(canonical) = case["canonical"].as_str() {
                    let result = result.unwrap();
                    assert_eq!(result.locale().as_str(), canonical, "{input}");
                    assert_eq!(
                        result.suggested_replacement(),
                        (input != canonical).then_some(canonical),
                        "{input}"
                    );
                } else {
                    assert_eq!(
                        result.err(),
                        Some(CanonicalizationFailure::Provider(
                            ProviderFailure::InvalidIdentifier
                        )),
                        "{input}"
                    );
                }
                count += 1;
            }
        }
        assert_eq!(count, 95);
    }

    /// Within a requested-locale list, a wildcard, a language range, and a
    /// protocol token are invalid identifiers, never expanded or parsed. `all`
    /// is no exception: CLDR 48.2.0 lists it as a regular language code, so it
    /// is that language, not every locale. Structural admission refuses `all`
    /// only in place of the list.
    #[test]
    fn wildcards_ranges_and_protocol_tokens_are_no_locales_but_all_is_a_language() {
        let canonicalizer = bind(committed()).unwrap();
        for input in ["*", "en-*", "*-US", "en-US;q=0.8", "en-US,fr"] {
            assert_eq!(
                canonicalizer.canonicalize(input).err(),
                Some(CanonicalizationFailure::Provider(
                    ProviderFailure::InvalidIdentifier
                )),
                "{input}"
            );
        }
        let all = canonicalizer.canonicalize("all").unwrap();
        assert_eq!(all.locale().as_str(), "all");
        assert_eq!(all.suggested_replacement(), None);
    }

    #[test]
    fn an_artifact_that_is_not_admitted_never_becomes_a_provider() {
        let failure =
            |manifest: &[u8], acquire: fn(&str) -> Option<Vec<u8>>, limits: &ReadLimits| {
                Icu4xProvider::admit(manifest, acquire, limits).err()
            };
        let tight = ReadLimits {
            manifest_bytes: 1_024,
            ..LIMITS
        };
        assert_eq!(
            failure(MANIFEST, payload, &tight),
            Some(Admission::PreInvocation(EnvelopeReason::OverBoundEnvelope))
        );
        assert_eq!(
            failure(b"[", payload, &LIMITS),
            Some(Admission::PreInvocation(EnvelopeReason::MalformedEnvelope))
        );
        // The limits reach the payloads too, after the manifest is read.
        let small = ReadLimits {
            marker_bytes: 1_024,
            ..LIMITS
        };
        assert_eq!(failure(MANIFEST, payload, &small), Some(Admission::Data));
        // A payload the host has no bytes for, and bytes that are not the
        // ones the manifest names.
        assert_eq!(
            failure(
                MANIFEST,
                |address| (address != "markers.postcard").then(|| payload(address))?,
                &LIMITS
            ),
            Some(Admission::Data)
        );
        assert_eq!(
            failure(
                MANIFEST,
                |address| {
                    let mut bytes = payload(address)?;
                    bytes[0] ^= 1;
                    Some(bytes)
                },
                &LIMITS
            ),
            Some(Admission::Data)
        );
    }

    /// The capability's `localeData` bounds that `LIMITS` sets.
    fn locale_data() -> LocaleDataLimits {
        let bound = |value| Bound::new(value).unwrap();
        LocaleDataLimits {
            max_payloads: bound(LIMITS.payloads),
            max_payload_bytes: bound(LIMITS.payload_bytes),
            max_marker_bytes: bound(LIMITS.marker_bytes),
            max_validity_groups: bound(LIMITS.conformance.validity_groups),
            max_codes: bound(LIMITS.conformance.codes),
            max_extension_keys: bound(LIMITS.conformance.extension_keys),
            max_types: bound(LIMITS.conformance.types),
            max_corrections: bound(LIMITS.conformance.corrections),
            max_validation_work_units: bound(LIMITS.conformance.work),
        }
    }

    /// The capability's `inputBoundary` bytes and depth that `LIMITS` sets.
    fn envelope() -> (Bound, Bound) {
        (
            Bound::new(LIMITS.manifest_bytes).unwrap(),
            Bound::new(LIMITS.manifest_depth).unwrap(),
        )
    }

    /// The boundary reads the envelope alone, and specification admission
    /// reads the rest into the provider the one-step admission makes, as
    /// often as it is asked to: the data stays its caller's.
    #[test]
    fn the_boundary_reads_the_envelope_and_specification_admission_the_rest() {
        let (bytes, depth) = envelope();
        let data = Icu4xData::read(Some(MANIFEST), &payloads(), bytes, depth).unwrap();
        for _ in 0..2 {
            let provider = data.admit(&locale_data()).unwrap();
            assert_eq!(provider.binding(), committed().binding());
        }
    }

    /// The data's value is its decoded manifest and the bytes of each
    /// payload the manifest names, whatever else the caller supplied: what
    /// invocation staleness compares (D15:2590).
    #[test]
    fn the_data_is_its_decoded_manifest_and_the_payloads_it_names() {
        let (bytes, depth) = envelope();
        let read = |manifest: &[u8], payloads: &BTreeMap<String, Arc<[u8]>>| {
            Icu4xData::read(Some(manifest), payloads, bytes, depth).unwrap()
        };
        let data = read(MANIFEST, &payloads());
        let value: Value = serde_json::from_slice(MANIFEST).unwrap();
        let spaced = serde_json::to_vec_pretty(&value).unwrap();
        assert_ne!(spaced, MANIFEST);
        let mut more = payloads();
        more.insert("unnamed.json".into(), Arc::from(&b"{}"[..]));
        assert!(read(&spaced, &more) == data);
        for address in ["conformance.json", "markers.postcard"] {
            let mut flipped = payloads();
            let mut changed = payload(address).unwrap();
            changed[0] ^= 1;
            flipped.insert(address.into(), changed.into());
            assert!(read(MANIFEST, &flipped) != data, "{address}");
            let mut absent = payloads();
            absent.remove(address);
            assert!(read(MANIFEST, &absent) != data, "{address}");
            assert!(
                read(MANIFEST, &absent) == read(&spaced, &absent),
                "{address}"
            );
        }
        let mut other = value;
        other["dataset"]["identity"] = Value::from("another-dataset");
        assert!(read(&serde_json::to_vec(&other).unwrap(), &payloads()) != data);
    }

    #[test]
    fn a_manifest_the_boundary_cannot_read_is_refused_with_its_reason() {
        let (bytes, depth) = envelope();
        let refused = |manifest: Option<&[u8]>, bytes, depth| {
            Icu4xData::read(manifest, &payloads(), bytes, depth).err()
        };
        assert_eq!(
            refused(None, bytes, depth),
            Some(EnvelopeReason::MissingInput)
        );
        assert_eq!(
            refused(Some(b"["), bytes, depth),
            Some(EnvelopeReason::MalformedEnvelope)
        );
        let shorter = Bound::new(u64::try_from(MANIFEST.len()).unwrap() - 1).unwrap();
        assert_eq!(
            refused(Some(MANIFEST), shorter, depth),
            Some(EnvelopeReason::OverBoundEnvelope)
        );
        assert_eq!(
            refused(Some(MANIFEST), bytes, Bound::new(1).unwrap()),
            Some(EnvelopeReason::OverBoundEnvelope)
        );
    }

    /// What fails after the envelope is the data's and never the boundary's:
    /// a `localeData` bound, the integrity, and a payload the host has no
    /// bytes for.
    #[test]
    fn what_fails_after_the_envelope_is_data_not_admitted() {
        let (bytes, depth) = envelope();
        let read =
            |manifest: &[u8], payloads: &BTreeMap<String, Arc<[u8]>>, limits: &LocaleDataLimits| {
                Icu4xData::read(Some(manifest), payloads, bytes, depth)
                    .map(|data| data.admit(limits).map(|_| ()))
            };
        // The artifact has two payloads.
        let mut one = locale_data();
        one.max_payloads = Bound::new(1).unwrap();
        assert_eq!(read(MANIFEST, &payloads(), &one), Ok(Err(DataNotAdmitted)));
        let mut value: Value = serde_json::from_slice(MANIFEST).unwrap();
        value["dataset"]["identity"] = Value::from("another-dataset");
        let tampered = serde_json::to_vec(&value).unwrap();
        assert_eq!(
            read(&tampered, &payloads(), &locale_data()),
            Ok(Err(DataNotAdmitted))
        );
        let mut without = payloads();
        without.remove("markers.postcard");
        assert_eq!(
            read(MANIFEST, &without, &locale_data()),
            Ok(Err(DataNotAdmitted))
        );
    }

    /// Every `localeData` bound reaches the reader: each alone at one, which
    /// the committed artifact is over, leaves the data not admitted.
    #[test]
    fn each_locale_data_bound_reaches_the_reader() {
        let (bytes, depth) = envelope();
        let one = Bound::new(1).unwrap();
        let narrowed: [fn(&mut LocaleDataLimits, Bound); 9] = [
            |limits, bound| limits.max_payloads = bound,
            |limits, bound| limits.max_payload_bytes = bound,
            |limits, bound| limits.max_marker_bytes = bound,
            |limits, bound| limits.max_validity_groups = bound,
            |limits, bound| limits.max_codes = bound,
            |limits, bound| limits.max_extension_keys = bound,
            |limits, bound| limits.max_types = bound,
            |limits, bound| limits.max_corrections = bound,
            |limits, bound| limits.max_validation_work_units = bound,
        ];
        for (index, narrow) in narrowed.into_iter().enumerate() {
            let mut limits = locale_data();
            narrow(&mut limits, one);
            let data = Icu4xData::read(Some(MANIFEST), &payloads(), bytes, depth).unwrap();
            assert_eq!(
                data.admit(&limits).err(),
                Some(DataNotAdmitted),
                "bound {index}"
            );
        }
    }

    #[test]
    fn a_count_keeps_whether_it_passed_u64() {
        assert_eq!(data_count(Measured::Exact(7)), DataCount::Exact(7));
        assert_eq!(data_count(Measured::BeyondU64), DataCount::BeyondU64);
    }

    /// One `localeData` bound's field.
    type Field = fn(&mut LocaleDataLimits) -> &mut Bound;

    /// Each bound with its field, in the bounds' order.
    const FIELDS: [(DataBound, Field); 9] = [
        (DataBound::Payloads, |l| &mut l.max_payloads),
        (DataBound::PayloadBytes, |l| &mut l.max_payload_bytes),
        (DataBound::MarkerBytes, |l| &mut l.max_marker_bytes),
        (DataBound::ValidityGroups, |l| &mut l.max_validity_groups),
        (DataBound::Codes, |l| &mut l.max_codes),
        (DataBound::ExtensionKeys, |l| &mut l.max_extension_keys),
        (DataBound::Types, |l| &mut l.max_types),
        (DataBound::Corrections, |l| &mut l.max_corrections),
        (DataBound::ValidationWork, |l| {
            &mut l.max_validation_work_units
        }),
    ];

    /// The bounds the committed artifact stands exactly at, as the reader
    /// counts them apart from the adapter's facts.
    fn exact_bounds() -> LocaleDataLimits {
        let manifest = VerifiedManifest::read(MANIFEST, &LIMITS).unwrap();
        let markers = manifest.decode_markers(MARKERS.into()).unwrap();
        let conformance = manifest.decode_conformance(CONFORMANCE).unwrap();
        let counts = conformance.counts();
        let bound = |count| match count {
            Measured::Exact(count) => Bound::new(count).unwrap(),
            Measured::BeyondU64 => panic!("an in-memory count"),
        };
        let longest = u64::try_from(CONFORMANCE.len().max(MARKERS.len())).unwrap();
        LocaleDataLimits {
            max_payloads: Bound::new(2).unwrap(),
            max_payload_bytes: Bound::new(longest).unwrap(),
            max_marker_bytes: bound(markers.written_bytes()),
            max_validity_groups: bound(counts.validity_groups),
            max_codes: bound(counts.codes),
            max_extension_keys: bound(counts.extension_keys),
            max_types: bound(counts.types),
            max_corrections: bound(counts.corrections),
            max_validation_work_units: bound(counts.work),
        }
    }

    /// The Findings of the data's checks over the committed artifact under
    /// `limits`, and whether it bound a canonicalizer.
    fn committed_checks(
        limits: &LocaleDataLimits,
    ) -> (Vec<(Check, Code, Option<BootstrapLimitExceeded>)>, bool) {
        let (bytes, depth) = envelope();
        let data = Icu4xData::read(Some(MANIFEST), &payloads(), bytes, depth).unwrap();
        let pins = committed().binding().pins.clone();
        let policy = ConfirmedPolicy::fixture_owned(fixture_limits(100));
        let mut evaluation = Evaluation::default();
        let bound = admit_data(&mut evaluation, &data, limits, Ok(pins), Ok(&policy)).unwrap();
        let found = evaluation
            .issues()
            .map(|key| {
                (
                    key.unit().check,
                    key.code(),
                    evaluation.bootstrap_limit(key),
                )
            })
            .collect();
        (found, bound.is_ok())
    }

    /// D15:3395 over the committed artifact: each `localeData` bound admits
    /// the artifact's exact value, and one below it is that bound's Finding
    /// alone, with its path, its value, and the exact total, except the
    /// payload length's first-over witness (Decision 015-157).
    #[test]
    fn each_locale_data_bound_admits_its_exact_value_and_reports_the_next() {
        let exact = exact_bounds();
        assert_eq!(committed_checks(&exact), (vec![], true));
        for (bound, field) in FIELDS {
            let mut below = exact;
            let count = field(&mut below).get();
            let limit = Bound::new(count - 1).unwrap();
            *field(&mut below) = limit;
            let actual = if bound == DataBound::PayloadBytes {
                Actual::AtLeast(count)
            } else {
                Actual::Exact(count)
            };
            let reported = BootstrapLimitExceeded {
                bound: bound.id(),
                limit,
                actual,
            };
            let expected = (bound.check(), Code::BootstrapLimitExceeded, Some(reported));
            assert_eq!(
                committed_checks(&below),
                (vec![expected], false),
                "{bound:?}"
            );
        }
    }

    /// The two counts the manifest decides are measured whatever stops the
    /// rest of the read.
    #[test]
    fn the_counts_the_manifest_decides_come_beside_the_provider() {
        let (bytes, depth) = envelope();
        let data = Icu4xData::read(Some(MANIFEST), &payloads(), bytes, depth).unwrap();
        let longest = u64::try_from(CONFORMANCE.len().max(MARKERS.len())).unwrap();
        let bound = |value| Bound::new(value).unwrap();
        for (limits, over, admitted) in [
            (
                LocaleDataLimits {
                    max_payloads: bound(2),
                    max_payload_bytes: bound(longest),
                    ..locale_data()
                },
                false,
                true,
            ),
            (
                LocaleDataLimits {
                    max_payloads: bound(1),
                    ..locale_data()
                },
                false,
                false,
            ),
            (
                LocaleDataLimits {
                    max_payload_bytes: bound(longest - 1),
                    ..locale_data()
                },
                true,
                false,
            ),
        ] {
            let reading = data.read(&limits);
            assert_eq!(reading.facts.payloads, DataCount::Exact(2));
            assert_eq!(reading.facts.payload_bytes_over, over);
            assert_eq!(reading.facts.read.is_ok(), admitted);
            assert_eq!(reading.provider.is_ok(), admitted);
        }
    }

    /// A payload without its bytes, or a manifest whose integrity fails, stops
    /// the manifest's read.
    #[test]
    fn what_fails_the_manifests_read_leaves_no_payload_read() {
        let (bytes, depth) = envelope();
        let mut without = payloads();
        without.remove("markers.postcard");
        let mut value: Value = serde_json::from_slice(MANIFEST).unwrap();
        value["dataset"]["identity"] = Value::from("another-dataset");
        let tampered = serde_json::to_vec(&value).unwrap();
        for (manifest, supplied) in [(MANIFEST, &without), (tampered.as_slice(), &payloads())] {
            let data = Icu4xData::read(Some(manifest), supplied, bytes, depth).unwrap();
            let reading = data.read(&locale_data());
            assert_eq!(reading.facts.read, Err(DataNotAdmitted));
            assert!(reading.provider.is_err());
        }
    }

    /// The committed manifest pointing at `bytes` for the payload at `index`,
    /// sealed again, and the payloads with those bytes at its address.
    fn with_payload(index: usize, bytes: &[u8]) -> (Vec<u8>, BTreeMap<String, Arc<[u8]>>) {
        let mut value: Value = serde_json::from_slice(MANIFEST).unwrap();
        value["payloads"][index]["byteLength"] = json!(bytes.len().to_string());
        value["payloads"][index]["digest"] = json!(FileDigest::of(bytes).as_str());
        let mut supplied = payloads();
        let address = ["conformance.json", "markers.postcard"][index];
        supplied.insert(address.to_owned(), bytes.into());
        (sealed(value), supplied)
    }

    /// The manifest `value`, sealed again.
    fn sealed(value: Value) -> Vec<u8> {
        let sealed = serde_json::from_value::<LocaleCanonicalizationDataArtifact>(value)
            .unwrap()
            .seal()
            .unwrap();
        serde_json::to_vec(&sealed).unwrap()
    }

    /// What the data's checks of `manifest` and `supplied` find, and which
    /// wait, with no value of the `locale-canonicalization` slot.
    fn without_the_slot(
        manifest: &[u8],
        supplied: &BTreeMap<String, Arc<[u8]>>,
    ) -> (Vec<(Check, Code)>, Vec<Check>) {
        let (bytes, depth) = envelope();
        let data = Icu4xData::read(Some(manifest), supplied, bytes, depth).unwrap();
        let policy = ConfirmedPolicy::fixture_owned(fixture_limits(100));
        let mut evaluation = Evaluation::default();
        let pins = Err(slot_causes());
        let bound = admit_data(&mut evaluation, &data, &locale_data(), pins, Ok(&policy));
        assert!(bound.unwrap().is_err());
        (found(&evaluation), waiting(&evaluation))
    }

    /// D15:2366, D17:1071: a conformance value outside the closed structure
    /// needs no value of the slot to prove.
    #[test]
    fn a_closed_structure_that_fails_is_reported_without_the_slot() {
        let text = br#"{"corrections":[],"extensionKeys":[],"validity":"x"}"#;
        let (manifest, supplied) = with_payload(0, text);
        assert_eq!(
            without_the_slot(&manifest, &supplied),
            (
                vec![(Check::IntactDataset, Code::LocaleDataNotAdmitted)],
                vec![Check::MatchingData]
            )
        );
    }

    /// D15:2366, D17:1072: the committed artifact with another semantic digest
    /// needs no value of the slot to prove.
    #[test]
    fn a_semantic_digest_that_differs_is_reported_without_the_slot() {
        let mut value: Value = serde_json::from_slice(MANIFEST).unwrap();
        value["dataset"]["semanticDigest"] = json!(format!("sha256:{}", "0".repeat(64)));
        assert_eq!(
            without_the_slot(&sealed(value), &payloads()),
            (
                vec![(Check::IntactDataset, Code::LocaleDataNotAdmitted)],
                vec![Check::MatchingData]
            )
        );
    }

    /// One payload that does not decode leaves the other's counts measured,
    /// and builds no provider (D15:2366).
    #[test]
    fn one_payload_that_does_not_decode_leaves_the_others_counts() {
        let (bytes, depth) = envelope();
        let reading = |manifest: &[u8], supplied: &BTreeMap<String, Arc<[u8]>>| {
            Icu4xData::read(Some(manifest), supplied, bytes, depth)
                .unwrap()
                .read(&locale_data())
        };
        let whole = reading(MANIFEST, &payloads()).facts.read.unwrap();
        let (manifest, supplied) = with_payload(1, b"not a blob");
        let broken = reading(&manifest, &supplied);
        let read = broken.facts.read.unwrap();
        assert_eq!(read.markers, Err(DataNotAdmitted));
        assert_eq!(read.conformance, whole.conformance);
        assert!(broken.provider.is_err());
        let (manifest, supplied) = with_payload(0, b"not json");
        let broken = reading(&manifest, &supplied);
        let read = broken.facts.read.unwrap();
        assert_eq!(read.conformance, Err(DataNotAdmitted));
        assert_eq!(read.markers, whole.markers);
        assert!(broken.provider.is_err());
    }

    #[test]
    fn an_admitted_artifact_of_another_dataset_is_not_revision_0s() {
        let provider = Icu4xProvider::admit(
            OTHER_MANIFEST,
            |address| match address {
                "conformance.json" => Some(OTHER_CONFORMANCE.to_vec()),
                "markers-narrow.postcard" => Some(OTHER_MARKERS.to_vec()),
                _ => None,
            },
            &LIMITS,
        )
        .unwrap();
        let failure = bind(provider).err().unwrap();
        assert_eq!(
            failure,
            BindingFailure::Mismatch(vec![
                BindingPart::DatasetIdentity,
                BindingPart::DatasetDigest
            ])
        );
        assert_eq!(failure.admission(), Admission::Data);
    }

    #[test]
    fn a_reader_failure_is_reported_where_015_reports_it() {
        // 015's spelling of each pre-invocation reason, to set beside the
        // reader's own.
        let spelled = |place| match place {
            Admission::PreInvocation(EnvelopeReason::MissingInput) => "missing-input",
            Admission::PreInvocation(EnvelopeReason::MalformedEnvelope) => "malformed-envelope",
            Admission::PreInvocation(EnvelopeReason::DuplicateMember) => "duplicate-member",
            Admission::PreInvocation(EnvelopeReason::UnknownMember) => "unknown-member",
            Admission::PreInvocation(EnvelopeReason::OverBoundEnvelope) => "over-bound-envelope",
            Admission::Data => "after-the-boundary",
        };
        for failure in [
            EnvelopeFailure::OverBound(EnvelopeLimit::Bytes),
            EnvelopeFailure::OverBound(EnvelopeLimit::Depth),
            EnvelopeFailure::Malformed,
            EnvelopeFailure::Unsupported,
            EnvelopeFailure::DuplicateMember,
            EnvelopeFailure::UnknownMember,
        ] {
            assert_eq!(
                spelled(admission(ReadFailure::Envelope(failure))),
                failure.reason(),
                "{failure:?}"
            );
        }
        for failure in [
            DataFailure::Limit(ArtifactLimit::PayloadBytes),
            DataFailure::Integrity,
            DataFailure::Payload(Role::Markers, PayloadCheck::Missing),
            DataFailure::DatasetSchema,
            DataFailure::Format(Role::Conformance),
            DataFailure::SemanticDigest,
        ] {
            assert_eq!(
                admission(ReadFailure::Data(failure)),
                Admission::Data,
                "{failure:?}"
            );
        }
    }

    #[test]
    fn a_rejection_is_an_invalid_identifier_and_an_adapter_failure_leaves_it_unavailable() {
        for rejection in [
            Rejection::Syntax,
            Rejection::RepeatedExtension,
            Rejection::OtherExtension,
            Rejection::PrivateUse,
            Rejection::Attribute,
            Rejection::RepeatedKey,
            Rejection::Component(Component::Language),
            Rejection::Component(Component::Script),
            Rejection::Component(Component::Region),
            Rejection::Component(Component::Variant),
            Rejection::Key,
            Rejection::Type,
            Rejection::Deprecated,
        ] {
            assert_eq!(
                provider_failure(LayerFailure::Rejected(rejection)),
                ProviderFailure::InvalidIdentifier,
                "{rejection:?}"
            );
        }
        for failure in [LayerFailure::Engine, LayerFailure::NotIdempotent] {
            assert_eq!(
                provider_failure(failure),
                ProviderFailure::Unavailable,
                "{failure:?}"
            );
        }
    }
}
