// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's admission of the Common Invocation Input Set in one step (015-168,
//! D15:3156).
//!
//! The set has six components: the Profile Resolution Specification Input,
//! the Intent Surface-Class Vocabulary Artifact, the Locale Canonicalization
//! Data Artifact, the Submitted Profile Resolution Artifact Collection, the
//! Resource Limit Policy Verification Input, and the Admitted Implementation
//! Capability. A binding submits each in its typed input slot, which fixes one
//! Safe Origin Kind, with its candidate for that kind beside the value
//! (D15:200, 3154).
//!
//! The capability is admitted first, because every other value's boundary
//! reads its `inputBoundary` bounds (D15:1106). When it is refused, those
//! boundaries are not evaluated, and their values yield no result. Every
//! origin is admitted whatever happens to the values, since its boundary reads
//! no bound, and every value is admitted whatever happens to its origin. The
//! step yields the complete admitted set, or every failure, each the typed
//! result of one boundary: in the rank of the Safe Origin Kind its slot fixes,
//! and within one slot an origin failure before a value failure. The
//! construction-identity reference and the Entry Admission Input Set are
//! admitted outside this step, and the Verification Input assertion is the
//! trusted setup path's parameter beside the set, not one of its components.

use serde_json::Value;

use crate::artifacts::collection::{read_collection, CollectionLimits, FormalCollection};
use crate::artifacts::vocabulary::{self, FormalVocabulary, VocabularyLimits};
use crate::input_limits::Bound;
use crate::locale::{EnvelopeReason, FormalData, VersionedIdentity};
use crate::resolver::capability::InputBoundaryLimits;
use crate::resolver::origin::SafeOriginKind;
use crate::resolver::specification::SpecificationInput;

use super::capability::{self, ImplementationCapability};
use super::specification;
use super::step::{slot, Component, InputFailure, Refusal};
use super::verification::{self, VerificationAssertion, VerificationInput};

/// The Common Invocation Input Set as submitted, its slots in rank order.
/// Each raw value is `None` when absent. The collection is its members'
/// bytes, since the container's own form is its adapter's. `data` reads the
/// canonicalization data's envelope as its adapter does, such as
/// `Icu4xData::read`, under the two bounds it is given:
/// `inputBoundary.maxLocaleDataManifestBytes` and `maxDepth`.
pub(crate) struct Submitted<'a, R> {
    pub(crate) specification: Component<'a, Option<&'a [u8]>>,
    pub(crate) vocabulary: Component<'a, Option<&'a [u8]>>,
    pub(crate) data: Component<'a, R>,
    pub(crate) collection: Component<'a, Option<&'a [&'a [u8]]>>,
    pub(crate) verification: Component<'a, Option<&'a [u8]>>,
    pub(crate) capability: Component<'a, Option<&'a Value>>,
}

/// The admitted Common Invocation Input Set. Each origin is the Safe Origin
/// Kind its slot fixes, so the set does not repeat it.
#[derive(Debug)]
pub(crate) struct CommonInputs<D> {
    pub(crate) specification: SpecificationInput<String>,
    pub(crate) vocabulary: FormalVocabulary,
    pub(crate) data: D,
    pub(crate) collection: FormalCollection,
    pub(crate) verification: VerificationInput,
    pub(crate) capability: ImplementationCapability,
}

/// Admit `submitted` against the constructed Capability Specification
/// `constructed`, with the trusted setup path's Verification Input
/// `assertion`: the admitted set, or every failure in 015's order.
pub(crate) fn admit<R, D, Id>(
    submitted: Submitted<'_, R>,
    constructed: &VersionedIdentity<Id>,
    assertion: &VerificationAssertion,
) -> Result<CommonInputs<D>, Vec<InputFailure>>
where
    R: FnOnce(Bound, Bound) -> Result<D, EnvelopeReason>,
    D: FormalData,
    Id: AsRef<str>,
{
    use SafeOriginKind as Kind;

    let admitted = capability::admit(submitted.capability.value, constructed);
    // Every other value is read under the admitted capability, so without one
    // no other value boundary is evaluated.
    let read = admitted.as_ref().ok();
    let bounds = read.map(|capability| &capability.bootstrap_only().input_boundary);
    let mut failures = Vec::new();
    let specification = slot(
        &mut failures,
        Kind::ProfileResolutionSpecificationInput,
        submitted.specification.origin,
        read.map(|capability| {
            specification::admit(submitted.specification.value, capability)
                .map_err(Refusal::Specification)
        }),
    );
    let vocabulary = slot(
        &mut failures,
        Kind::IntentSurfaceClassVocabulary,
        submitted.vocabulary.origin,
        bounds.map(|bounds| {
            vocabulary::read(submitted.vocabulary.value, &vocabulary_limits(bounds))
                .map_err(Refusal::Vocabulary)
        }),
    );
    let data = slot(
        &mut failures,
        Kind::LocaleCanonicalizationData,
        submitted.data.origin,
        bounds.map(|bounds| {
            (submitted.data.value)(bounds.max_locale_data_manifest_bytes, bounds.max_depth)
                .map_err(Refusal::Data)
        }),
    );
    let collection = slot(
        &mut failures,
        Kind::ProfileResolutionArtifactCollection,
        submitted.collection.origin,
        bounds.map(|bounds| {
            read_collection(submitted.collection.value, &collection_limits(bounds))
                .map_err(Refusal::Collection)
        }),
    );
    let verification = slot(
        &mut failures,
        Kind::ResourceLimitPolicyVerification,
        submitted.verification.origin,
        read.map(|capability| {
            verification::admit(submitted.verification.value, capability, assertion)
                .map_err(Refusal::Verification)
        }),
    );
    let capability = slot(
        &mut failures,
        Kind::ImplementationCapability,
        submitted.capability.origin,
        Some(admitted.map_err(Refusal::Capability)),
    );
    match (
        specification,
        vocabulary,
        data,
        collection,
        verification,
        capability,
    ) {
        (
            Some(specification),
            Some(vocabulary),
            Some(data),
            Some(collection),
            Some(verification),
            Some(capability),
        ) if failures.is_empty() => Ok(CommonInputs {
            specification,
            vocabulary,
            data,
            collection,
            verification,
            capability,
        }),
        _ => Err(failures),
    }
}

/// The vocabulary's limits: the capability's `inputBoundary` bounds on it
/// (D15:1805, 1807-1810).
const fn vocabulary_limits(bounds: &InputBoundaryLimits) -> VocabularyLimits {
    VocabularyLimits {
        artifact_bytes: bounds.max_vocabulary_bytes.get(),
        depth: bounds.max_depth.get(),
        members: bounds.max_vocabulary_members.get(),
        member_bytes: bounds.max_vocabulary_member_bytes.get(),
        total_member_bytes: bounds.max_vocabulary_total_member_bytes.get(),
    }
}

/// The collection's limits: the capability's `inputBoundary` bounds on its
/// raw bytes (D15:1805, 1812-1813).
const fn collection_limits(bounds: &InputBoundaryLimits) -> CollectionLimits {
    CollectionLimits {
        member_bytes: bounds.max_collection_member_bytes.get(),
        total_bytes: bounds.max_collection_total_bytes.get(),
        depth: bounds.max_depth.get(),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use serde_json::json;

    use super::*;
    use crate::artifacts::collection::CollectionFailure;
    use crate::artifacts::verification::{sealed, unsealed, VerificationInputArtifact};
    use crate::artifacts::vocabulary::EnvelopeFailure;
    use crate::artifacts::vocabulary::IntentSurfaceClassVocabularyArtifact;
    use crate::boundary::capability::CapabilityRefusal;
    use crate::boundary::origin::OriginRefusal;
    use crate::boundary::specification::SpecificationRefusal;
    use crate::boundary::verification::VerificationRefusal;
    use crate::construction::capability::{
        IDENTITY as CAPABILITY, REVISION as CAPABILITY_REVISION,
    };
    use crate::locale::fixtures::FixtureProvider;
    use crate::locale::{DataNotAdmitted, LocaleDataLimits};
    use crate::resolver::limits::fixture_limits;
    use crate::resolver::origin::SafeOriginKind as Kind;

    /// A Profile Resolution Specification Input with one slot.
    const SPECIFICATION: &[u8] =
        br#"{"coverage":{"identity":"intlify-localization-coverage","revision":"0"}}"#;

    /// A collection of two members, which the boundary keeps whatever they
    /// declare.
    const COLLECTION: [&[u8]; 2] = [br#"{"kind":"resource-limit-policy"}"#, b"[]"];

    /// The six slots in rank order.
    const SLOTS: [Kind; 6] = [
        Kind::ProfileResolutionSpecificationInput,
        Kind::IntentSurfaceClassVocabulary,
        Kind::LocaleCanonicalizationData,
        Kind::ProfileResolutionArtifactCollection,
        Kind::ResourceLimitPolicyVerification,
        Kind::ImplementationCapability,
    ];

    /// Canonicalization data past a test's envelope: the two bounds the
    /// envelope was read under.
    #[derive(Debug, PartialEq, Eq)]
    struct Envelope {
        bytes: Bound,
        depth: Bound,
    }

    impl FormalData for Envelope {
        type Provider = FixtureProvider;

        fn admit(self, _: &LocaleDataLimits) -> Result<FixtureProvider, DataNotAdmitted> {
            Ok(FixtureProvider::new())
        }
    }

    /// An envelope that is always admitted, and keeps its bounds.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "the step takes an envelope reader"
    )]
    fn read(bytes: Bound, depth: Bound) -> Result<Envelope, EnvelopeReason> {
        Ok(Envelope { bytes, depth })
    }

    fn constructed() -> VersionedIdentity<&'static str> {
        VersionedIdentity {
            identity: CAPABILITY,
            revision: CAPABILITY_REVISION,
        }
    }

    /// A capability value with each `inputBoundary` bound set by `bound` from
    /// its name, and every other bound as a fixture owner states it.
    fn capability_value(bound: impl Fn(&str) -> u64) -> Value {
        let mut value = ImplementationCapability::fixture_owned(fixture_limits(1)).normalized();
        let bounds = value["bootstrapOnly"]["inputBoundary"]
            .as_object_mut()
            .unwrap();
        for (name, setting) in bounds.iter_mut() {
            *setting = json!(bound(name).to_string());
        }
        value
    }

    fn failure(slot: Kind, refusal: Refusal) -> InputFailure {
        InputFailure { slot, refusal }
    }

    /// Raw values every boundary admits, and the assertion the Verification
    /// Input makes about itself.
    struct Inputs {
        vocabulary: Vec<u8>,
        verification: Vec<u8>,
        assertion: VerificationAssertion,
        capability: Value,
    }

    impl Inputs {
        fn new() -> Self {
            let vocabulary = IntentSurfaceClassVocabularyArtifact::sealed(
                "storefront-surfaces",
                "1",
                &["button", "heading"],
            )
            .unwrap();
            let pin = json!({
                "kind": "resource-limit-policy",
                "identity": "storefront-limits",
                "revision": "1",
                "specificationRevision": "0",
                "semanticDigest": format!("sha256:{}", "1".repeat(64)),
            });
            let verification = sealed(unsealed("storefront-pins", "1", &[pin])).unwrap();
            let read: VerificationInputArtifact =
                serde_json::from_value(verification.clone()).unwrap();
            Self {
                vocabulary: serde_json::to_vec(&vocabulary).unwrap(),
                verification: serde_json::to_vec(&verification).unwrap(),
                assertion: VerificationAssertion {
                    identity: read.identity().clone(),
                    revision: read.revision().clone(),
                    semantic_digest: read.semantic_digest().unwrap(),
                },
                capability: capability_value(|_| 1 << 20),
            }
        }

        /// These values with `data` for the envelope, each beside its slot's
        /// own Safe Origin Kind.
        fn submitted<R>(&self, data: R) -> Submitted<'_, R> {
            let own = |slot: Kind| Some(slot.id());
            Submitted {
                specification: Component {
                    value: Some(SPECIFICATION),
                    origin: own(Kind::ProfileResolutionSpecificationInput),
                },
                vocabulary: Component {
                    value: Some(&self.vocabulary),
                    origin: own(Kind::IntentSurfaceClassVocabulary),
                },
                data: Component {
                    value: data,
                    origin: own(Kind::LocaleCanonicalizationData),
                },
                collection: Component {
                    value: Some(&COLLECTION),
                    origin: own(Kind::ProfileResolutionArtifactCollection),
                },
                verification: Component {
                    value: Some(&self.verification),
                    origin: own(Kind::ResourceLimitPolicyVerification),
                },
                capability: Component {
                    value: Some(&self.capability),
                    origin: own(Kind::ImplementationCapability),
                },
            }
        }

        fn admit<R>(
            &self,
            submitted: Submitted<'_, R>,
        ) -> Result<CommonInputs<Envelope>, Vec<InputFailure>>
        where
            R: FnOnce(Bound, Bound) -> Result<Envelope, EnvelopeReason>,
        {
            admit(submitted, &constructed(), &self.assertion)
        }
    }

    /// Each component is the value its own boundary admits, read under the
    /// admitted capability's bounds.
    #[test]
    fn a_complete_set_is_admitted_component_by_component() {
        let inputs = Inputs::new();
        let admitted = inputs.admit(inputs.submitted(read)).unwrap();
        let capability = capability::admit(Some(&inputs.capability), &constructed()).unwrap();
        let bounds = &capability.bootstrap_only().input_boundary;
        assert_eq!(
            admitted.specification,
            specification::admit(Some(SPECIFICATION), &capability).unwrap()
        );
        assert_eq!(
            admitted.vocabulary,
            vocabulary::read(Some(&inputs.vocabulary), &vocabulary_limits(bounds)).unwrap()
        );
        assert_eq!(
            admitted.data,
            Envelope {
                bytes: bounds.max_locale_data_manifest_bytes,
                depth: bounds.max_depth,
            }
        );
        assert_eq!(
            admitted.collection,
            read_collection(Some(&COLLECTION), &collection_limits(bounds)).unwrap()
        );
        assert_eq!(admitted.collection.members().len(), 2);
        assert_eq!(
            admitted.verification,
            verification::admit(Some(&inputs.verification), &capability, &inputs.assertion)
                .unwrap()
        );
        assert_eq!(admitted.capability, capability);
    }

    /// Failures come in the rank of their slots, an origin's before its
    /// value's, and an absent or refused value never skips its origin, nor an
    /// absent or refused origin its value.
    #[test]
    fn every_failure_comes_in_slot_rank_with_the_origin_first() {
        const NUMBER: [&[u8]; 1] = [b"1"];
        let inputs = Inputs::new();
        let mut submitted = inputs.submitted(read);
        submitted.specification = Component {
            value: Some(b"{"),
            origin: None,
        };
        submitted.vocabulary.value = None;
        submitted.data.origin = Some(" Locale-Canonicalization-Data");
        submitted.collection = Component {
            value: Some(&NUMBER),
            origin: Some("intent-surface-class-vocabulary"),
        };
        submitted.verification = Component {
            value: None,
            origin: Some("verification"),
        };
        submitted.capability.origin = None;
        assert_eq!(
            inputs.admit(submitted).unwrap_err(),
            [
                failure(
                    Kind::ProfileResolutionSpecificationInput,
                    Refusal::Origin(OriginRefusal::Missing)
                ),
                failure(
                    Kind::ProfileResolutionSpecificationInput,
                    Refusal::Specification(SpecificationRefusal::Malformed(
                        specification::Malformation::Syntax
                    ))
                ),
                failure(
                    Kind::IntentSurfaceClassVocabulary,
                    Refusal::Vocabulary(EnvelopeFailure::Missing)
                ),
                failure(
                    Kind::LocaleCanonicalizationData,
                    Refusal::Origin(OriginRefusal::NonCanonical)
                ),
                failure(
                    Kind::ProfileResolutionArtifactCollection,
                    Refusal::Origin(OriginRefusal::EvidenceKindIncompatible)
                ),
                failure(
                    Kind::ProfileResolutionArtifactCollection,
                    Refusal::Collection(CollectionFailure::Unrepresentable)
                ),
                failure(
                    Kind::ResourceLimitPolicyVerification,
                    Refusal::Origin(OriginRefusal::Unknown)
                ),
                failure(
                    Kind::ResourceLimitPolicyVerification,
                    Refusal::Verification(VerificationRefusal::Missing)
                ),
                failure(
                    Kind::ImplementationCapability,
                    Refusal::Origin(OriginRefusal::Missing)
                ),
            ]
        );
    }

    /// The data's envelope rejected by its adapter is the data slot's failure.
    #[test]
    fn the_adapters_envelope_reason_is_the_data_slots_failure() {
        let inputs = Inputs::new();
        let refused = |_: Bound, _: Bound| Err(EnvelopeReason::DuplicateMember);
        assert_eq!(
            inputs.admit(inputs.submitted(refused)).unwrap_err(),
            [failure(
                Kind::LocaleCanonicalizationData,
                Refusal::Data(EnvelopeReason::DuplicateMember)
            )]
        );
    }

    /// A refused capability leaves every value that reads its bounds without
    /// a result, the data's envelope unread, and every origin still admitted.
    #[test]
    fn a_refused_capability_leaves_the_bound_reading_values_unevaluated() {
        let inputs = Inputs::new();
        let called = Cell::new(false);
        let data = |bytes: Bound, depth: Bound| {
            called.set(true);
            read(bytes, depth)
        };
        let mut submitted = inputs.submitted(data);
        submitted.specification.value = None;
        submitted.vocabulary.value = None;
        submitted.collection.value = None;
        submitted.verification.value = None;
        submitted.vocabulary.origin = Some("profile-selector-cli");
        submitted.capability.value = None;
        assert_eq!(
            inputs.admit(submitted).unwrap_err(),
            [
                failure(
                    Kind::IntentSurfaceClassVocabulary,
                    Refusal::Origin(OriginRefusal::EvidenceKindIncompatible)
                ),
                failure(
                    Kind::ImplementationCapability,
                    Refusal::Capability(CapabilityRefusal::Missing)
                ),
            ]
        );
        assert!(!called.get());
    }

    /// One refused origin refuses the set, whose values were all admitted.
    #[test]
    fn a_refused_origin_alone_refuses_the_set() {
        let inputs = Inputs::new();
        for slot in SLOTS {
            let mut submitted = inputs.submitted(read);
            let origin = match slot {
                Kind::ProfileResolutionSpecificationInput => &mut submitted.specification.origin,
                Kind::IntentSurfaceClassVocabulary => &mut submitted.vocabulary.origin,
                Kind::LocaleCanonicalizationData => &mut submitted.data.origin,
                Kind::ProfileResolutionArtifactCollection => &mut submitted.collection.origin,
                Kind::ResourceLimitPolicyVerification => &mut submitted.verification.origin,
                _ => &mut submitted.capability.origin,
            };
            *origin = Some("configuration-file");
            assert_eq!(
                inputs.admit(submitted).unwrap_err(),
                [failure(
                    slot,
                    Refusal::Origin(OriginRefusal::EvidenceKindIncompatible)
                )],
                "{slot:?}"
            );
        }
    }

    /// Each value boundary reads its own bounds of the capability: with every
    /// bound distinct and too small, each value is over its own, and the
    /// data's envelope is read under the manifest's bytes and the depth.
    #[test]
    fn each_value_is_read_under_its_own_capability_bounds() {
        const DISTINCT: [(&str, u64); 11] = [
            ("maxDepth", 11),
            ("maxSpecificationInputBytes", 12),
            ("maxVocabularyBytes", 13),
            ("maxVocabularyMembers", 14),
            ("maxVocabularyMemberBytes", 15),
            ("maxVocabularyTotalMemberBytes", 16),
            ("maxLocaleDataManifestBytes", 17),
            ("maxCollectionMemberBytes", 18),
            ("maxCollectionTotalBytes", 19),
            ("maxVerificationInputBytes", 20),
            ("maxVerificationPins", 21),
        ];
        let mut inputs = Inputs::new();
        inputs.capability = capability_value(|name| {
            DISTINCT
                .iter()
                .find(|(bound, _)| *bound == name)
                .map(|(_, value)| *value)
                .unwrap()
        });
        let mut envelope = None;
        let data = |bytes: Bound, depth: Bound| {
            envelope = Some(Envelope { bytes, depth });
            read(bytes, depth)
        };
        assert_eq!(
            inputs.admit(inputs.submitted(data)).unwrap_err(),
            [
                failure(
                    Kind::ProfileResolutionSpecificationInput,
                    Refusal::Specification(SpecificationRefusal::OverBound(
                        specification::SpecificationBound::InputBytes
                    ))
                ),
                failure(
                    Kind::IntentSurfaceClassVocabulary,
                    Refusal::Vocabulary(EnvelopeFailure::OverBound(
                        vocabulary::EnvelopeLimit::ArtifactBytes
                    ))
                ),
                failure(
                    Kind::ProfileResolutionArtifactCollection,
                    Refusal::Collection(CollectionFailure::OverBound(
                        crate::artifacts::collection::CollectionLimit::MemberBytes
                    ))
                ),
                failure(
                    Kind::ResourceLimitPolicyVerification,
                    Refusal::Verification(VerificationRefusal::OverBound(
                        verification::VerificationBound::InputBytes
                    ))
                ),
            ]
        );
        let bound = |value| Bound::new(value).unwrap();
        assert_eq!(
            envelope,
            Some(Envelope {
                bytes: bound(17),
                depth: bound(11),
            })
        );
        let bounds = InputBoundaryLimits {
            max_depth: bound(11),
            max_specification_input_bytes: bound(12),
            max_vocabulary_bytes: bound(13),
            max_vocabulary_members: bound(14),
            max_vocabulary_member_bytes: bound(15),
            max_vocabulary_total_member_bytes: bound(16),
            max_locale_data_manifest_bytes: bound(17),
            max_collection_member_bytes: bound(18),
            max_collection_total_bytes: bound(19),
            max_verification_input_bytes: bound(20),
            max_verification_pins: bound(21),
        };
        assert_eq!(
            vocabulary_limits(&bounds),
            VocabularyLimits {
                artifact_bytes: 13,
                depth: 11,
                members: 14,
                member_bytes: 15,
                total_member_bytes: 16,
            }
        );
        assert_eq!(
            collection_limits(&bounds),
            CollectionLimits {
                member_bytes: 18,
                total_bytes: 19,
                depth: 11,
            }
        );
    }

    /// The ICU4X adapter's envelope reader is the data slot's: it reads the
    /// manifest under the capability's bounds on it, and a manifest over them
    /// is the data slot's failure.
    #[cfg(feature = "icu4x")]
    #[test]
    fn the_icu4x_envelope_reader_reads_under_the_capabilitys_bounds() {
        use crate::locale::icu4x::Icu4xData;

        const MANIFEST: &[u8] = include_bytes!("../../../intlify_locale_icu4x/data/manifest.json");
        let data = |bytes, depth| Icu4xData::read(Some(MANIFEST), |_: &str| None, bytes, depth);
        let mut inputs = Inputs::new();
        assert!(admit(inputs.submitted(data), &constructed(), &inputs.assertion).is_ok());
        let size = u64::try_from(MANIFEST.len()).unwrap();
        inputs.capability = capability_value(|name| {
            if name == "maxLocaleDataManifestBytes" {
                size - 1
            } else {
                1 << 20
            }
        });
        assert_eq!(
            admit(inputs.submitted(data), &constructed(), &inputs.assertion).err(),
            Some(vec![failure(
                Kind::LocaleCanonicalizationData,
                Refusal::Data(EnvelopeReason::OverBoundEnvelope)
            )])
        );
    }
}
