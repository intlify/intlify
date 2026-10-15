// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The ICU4X 2.2.0 reference adapter as this boundary's provider, behind the
//! non-default `icu4x` feature. It admits one Locale Canonicalization Data
//! Artifact in 017's six steps, from bytes its caller acquired, and
//! canonicalizes through 015's revision "0" conformance layer around ICU4X.
//! It fetches nothing, embeds no data, and never falls back to other locale
//! behavior.

use std::sync::Arc;

use intlify_locale_icu4x::{
    AdmittedData, EnvelopeFailure, LayerFailure, ReadFailure, ReadLimits, Role, VerifiedManifest,
    ENGINE, MARKERS_FORMAT,
};

use super::{
    Admission, AdmissionFacts, DatasetPin, EnvelopeReason, Provider, ProviderBinding,
    ProviderFailure, SemanticPins, VersionedIdentity,
};

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
        let data = manifest
            .admit(
                conformance.as_deref(),
                markers.map(Vec::into_boxed_slice),
                limits,
            )
            .map_err(admission)?;
        Ok(Self {
            binding: binding(&data),
            data,
        })
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
        ReadFailure::Envelope(envelope) => Admission::PreInvocation(match envelope {
            EnvelopeFailure::OverBound(_) => EnvelopeReason::OverBoundEnvelope,
            EnvelopeFailure::Malformed | EnvelopeFailure::Unsupported => {
                EnvelopeReason::MalformedEnvelope
            }
            EnvelopeFailure::DuplicateMember => EnvelopeReason::DuplicateMember,
            EnvelopeFailure::UnknownMember => EnvelopeReason::UnknownMember,
        }),
        ReadFailure::Data(_) => Admission::Data,
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
        ArtifactLimit, Component, ConformanceLimits, DataFailure, EnvelopeLimit, PayloadCheck,
        Rejection,
    };
    use serde_json::Value;

    use super::*;
    use crate::input_limits::Bound;
    use crate::locale::fixtures::{ALIASES, CANONICAL, INVALID};
    use crate::locale::{
        BindingFailure, BindingPart, CanonicalizationFailure, Canonicalizer, Supported,
    };

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
