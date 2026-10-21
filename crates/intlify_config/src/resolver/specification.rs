// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's Profile Resolution Specification Input and its admission. Phase 0
//! checks the `project-profile` assertion against the Active Project Profile
//! Specification; Phase 5 checks the other seven slots against what the
//! resolver supports, admits the vocabulary artifact against its slot, and
//! binds the canonicalization data to the admitted specification. Not a wire
//! decoder: 017 defines no wire form, and an adapter that reads one rejects a
//! duplicate, unknown, malformed, or over-bound member before invocation. Not
//! construction authority: until 015 Phase 5 constructs the supported values,
//! tests supply them.

use crate::artifacts::vocabulary::FormalVocabulary;
use crate::locale::{
    Canonicalizer, Provider, SemanticPins, SpecificationNotAdmitted, Supported, VersionedIdentity,
};

use super::evaluation::{
    both, Admitted, Blocking, Causes, Check, Code, Evaluation, EvaluationInvariant, Phase, Reason,
    Subject, Unit,
};
use super::limits::ConfirmedPolicy;
use super::vocabulary::{admit_vocabulary, SurfaceVocabulary, ASSERTED, INTACT};

/// 015's eight fixed slots, declared in their canonical order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Slot {
    ProjectProfile,
    LocaleCanonicalization,
    LocaleNegotiation,
    MessageLocaleFallback,
    Coverage,
    IntentSurfaceClassVocabulary,
    DeliveryGraph,
    DeliveryPlacement,
}

impl Slot {
    /// The slot's name, which `details.slot` carries.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::ProjectProfile => "project-profile",
            Self::LocaleCanonicalization => "locale-canonicalization",
            Self::LocaleNegotiation => "locale-negotiation",
            Self::MessageLocaleFallback => "message-locale-fallback",
            Self::Coverage => "coverage",
            Self::IntentSurfaceClassVocabulary => "intent-surface-class-vocabulary",
            Self::DeliveryGraph => "delivery-graph",
            Self::DeliveryPlacement => "delivery-placement",
        }
    }
}

/// The vocabulary slot's assertion: the exact Intent Surface-Class Vocabulary
/// Artifact the invocation submits, which is admitted against it later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VocabularyAssertion<Id> {
    pub(crate) identity: Id,
    pub(crate) revision: Id,
    pub(crate) semantic_digest: Id,
}

/// 015's typed input: eight fixed optional fields. A missing field is the
/// resolver's Finding; the type has no room for an unknown or repeated one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SpecificationInput<Id> {
    pub(crate) project_profile: Option<VersionedIdentity<Id>>,
    pub(crate) locale_canonicalization: Option<VersionedIdentity<Id>>,
    pub(crate) locale_negotiation: Option<VersionedIdentity<Id>>,
    pub(crate) message_locale_fallback: Option<VersionedIdentity<Id>>,
    pub(crate) coverage: Option<VersionedIdentity<Id>>,
    pub(crate) intent_surface_class_vocabulary: Option<VocabularyAssertion<Id>>,
    pub(crate) delivery_graph: Option<VersionedIdentity<Id>>,
    pub(crate) delivery_placement: Option<VersionedIdentity<Id>>,
}

/// What one resolver admits in each slot: the Active Project Profile
/// Specification, the canonicalization specifications with the dataset each
/// admits, and one value for every other specification slot. The vocabulary
/// slot has no fixed value. Construction fixes these in 015 Phase 5; until then
/// only tests build them.
pub(crate) struct SupportedSpecifications<Id> {
    project_profile: VersionedIdentity<Id>,
    locale_canonicalization: Supported<Id>,
    locale_negotiation: VersionedIdentity<Id>,
    message_locale_fallback: VersionedIdentity<Id>,
    coverage: VersionedIdentity<Id>,
    delivery_graph: VersionedIdentity<Id>,
    delivery_placement: VersionedIdentity<Id>,
}

impl<Id: From<&'static str>> SupportedSpecifications<Id> {
    /// Revision "0": 015-135 names the toolchain-supplied specifications, all at
    /// revision "0", around the active profile specification and the
    /// canonicalization specifications construction supplies.
    pub(crate) fn revision_zero(
        project_profile: VersionedIdentity<Id>,
        locale_canonicalization: Supported<Id>,
    ) -> Self {
        let revision_zero = |identity: &'static str| VersionedIdentity {
            identity: identity.into(),
            revision: "0".into(),
        };
        Self {
            project_profile,
            locale_canonicalization,
            locale_negotiation: revision_zero("intlify-locale-negotiation"),
            message_locale_fallback: revision_zero("intlify-message-locale-fallback"),
            coverage: revision_zero("intlify-localization-coverage"),
            delivery_graph: revision_zero("intlify-delivery-graph"),
            delivery_placement: revision_zero("intlify-delivery-placement"),
        }
    }
}

const fn unit(phase: Phase, slot: Slot) -> Unit {
    Unit {
        phase,
        subject: Subject::Slot(slot),
        check: Check::SupportedAssertion,
    }
}

/// The unit Phase 5 binds the canonicalization data in.
pub(crate) const DATA: Unit = Unit {
    phase: Phase::SpecificationAdmission,
    subject: Subject::CanonicalizationData,
    check: Check::MatchingData,
};

/// The units Phase 5 records, in their canonical order.
pub(crate) const PHASE_5: [Unit; 10] = [
    unit(Phase::SpecificationAdmission, Slot::LocaleCanonicalization),
    unit(Phase::SpecificationAdmission, Slot::LocaleNegotiation),
    unit(Phase::SpecificationAdmission, Slot::MessageLocaleFallback),
    unit(Phase::SpecificationAdmission, Slot::Coverage),
    unit(
        Phase::SpecificationAdmission,
        Slot::IntentSurfaceClassVocabulary,
    ),
    unit(Phase::SpecificationAdmission, Slot::DeliveryGraph),
    unit(Phase::SpecificationAdmission, Slot::DeliveryPlacement),
    INTACT,
    ASSERTED,
    DATA,
];

fn present<V>(asserted: Option<&V>) -> Result<&V, Blocking> {
    asserted.ok_or(Blocking {
        code: Code::SpecificationInputIncomplete,
        reason: Some(Reason::MissingRequiredSlot),
        limit: None,
    })
}

const fn not_admitted(code: Code, reason: SpecificationNotAdmitted) -> Blocking {
    Blocking {
        code,
        reason: Some(match reason {
            SpecificationNotAdmitted::Unsupported => Reason::Unsupported,
            SpecificationNotAdmitted::Incompatible => Reason::Incompatible,
        }),
        limit: None,
    }
}

/// One slot whose supported value is a single specification identity.
fn specification_slot<Id: Clone + Eq>(
    evaluation: &mut Evaluation,
    unit: Unit,
    code: Code,
    prerequisite: Admitted<()>,
    asserted: Option<&VersionedIdentity<Id>>,
    supported: &VersionedIdentity<Id>,
) -> Result<Admitted<VersionedIdentity<Id>>, EvaluationInvariant> {
    evaluation.check(unit, prerequisite, |()| {
        let asserted = present(asserted)?;
        match asserted.admit(supported) {
            Ok(()) => Ok(asserted.clone()),
            Err(reason) => Err(not_admitted(code, reason)),
        }
    })
}

/// Phase 0: the `project-profile` assertion must be the Active Project Profile
/// Specification. The assertion never selects semantics and no later check
/// depends on it, so a failure blocks the outcome without suppressing anything.
pub(crate) fn admit_project_profile<Id: Clone + Eq>(
    evaluation: &mut Evaluation,
    supported: &SupportedSpecifications<Id>,
    input: &SpecificationInput<Id>,
) -> Result<Admitted<VersionedIdentity<Id>>, EvaluationInvariant> {
    specification_slot(
        evaluation,
        unit(Phase::ResolverBootstrapAdmission, Slot::ProjectProfile),
        Code::SpecificationNotAdmitted,
        Ok(()),
        input.project_profile.as_ref(),
        &supported.project_profile,
    )
}

/// What Phase 5 admitted: each remaining slot's member, the surface classes of
/// the vocabulary artifact admitted against its slot, and the canonicalizer
/// bound to the admitted canonicalization specification's dataset. A later
/// phase depends only on the members it uses.
pub(crate) struct AdmittedSpecifications<Id, P: Provider> {
    pub(crate) locale_canonicalization: Admitted<VersionedIdentity<Id>>,
    pub(crate) locale_negotiation: Admitted<VersionedIdentity<Id>>,
    pub(crate) message_locale_fallback: Admitted<VersionedIdentity<Id>>,
    pub(crate) coverage: Admitted<VersionedIdentity<Id>>,
    pub(crate) intent_surface_class_vocabulary: Admitted<VocabularyAssertion<Id>>,
    pub(crate) delivery_graph: Admitted<VersionedIdentity<Id>>,
    pub(crate) delivery_placement: Admitted<VersionedIdentity<Id>>,
    pub(crate) vocabulary: Admitted<SurfaceVocabulary>,
    pub(crate) canonicalizer: Admitted<Canonicalizer<P>>,
}

/// Phase 5's slots, vocabulary, and canonicalization data, which run only under
/// the Resource Limit Policy Phase 4 confirmed. `vocabulary` and `data` are the
/// artifacts the pre-invocation boundary already admitted as envelopes; here
/// the vocabulary must be intact and the one its slot asserts, and the data
/// must carry the dataset the admitted specification admits.
pub(crate) fn admit_specifications<Id, P>(
    evaluation: &mut Evaluation,
    supported: &SupportedSpecifications<Id>,
    input: &SpecificationInput<Id>,
    policy: Result<&ConfirmedPolicy, &Causes>,
    vocabulary: &FormalVocabulary,
    data: P,
) -> Result<AdmittedSpecifications<Id, P>, EvaluationInvariant>
where
    Id: Clone + Eq + AsRef<str>,
    P: Provider<Identity = Id>,
{
    let [canonicalization, negotiation, fallback, coverage, asserted_vocabulary, graph, placement, ..] =
        PHASE_5;
    let confirmed = || policy.map(|_| ()).map_err(Causes::clone);
    let pins = evaluation.check(canonicalization, confirmed(), |()| {
        let asserted = present(input.locale_canonicalization.as_ref())?;
        match supported.locale_canonicalization.expected(asserted) {
            Ok(pins) => Ok(pins.clone()),
            Err(reason) => Err(not_admitted(Code::LocaleSpecificationNotAdmitted, reason)),
        }
    })?;
    let locale_negotiation = specification_slot(
        evaluation,
        negotiation,
        Code::NegotiationSpecificationNotAdmitted,
        confirmed(),
        input.locale_negotiation.as_ref(),
        &supported.locale_negotiation,
    )?;
    let message_locale_fallback = specification_slot(
        evaluation,
        fallback,
        Code::MessageFallbackSpecificationNotAdmitted,
        confirmed(),
        input.message_locale_fallback.as_ref(),
        &supported.message_locale_fallback,
    )?;
    let coverage = specification_slot(
        evaluation,
        coverage,
        Code::CoverageSpecificationNotAdmitted,
        confirmed(),
        input.coverage.as_ref(),
        &supported.coverage,
    )?;
    // The slot has no fixed value, so it only has to be present: the
    // vocabulary artifact is admitted against what it asserts.
    let intent_surface_class_vocabulary =
        evaluation.check(asserted_vocabulary, confirmed(), |()| {
            present(input.intent_surface_class_vocabulary.as_ref()).cloned()
        })?;
    let delivery_graph = specification_slot(
        evaluation,
        graph,
        Code::DeliverySpecificationNotAdmitted,
        confirmed(),
        input.delivery_graph.as_ref(),
        &supported.delivery_graph,
    )?;
    let delivery_placement = specification_slot(
        evaluation,
        placement,
        Code::DeliverySpecificationNotAdmitted,
        confirmed(),
        input.delivery_placement.as_ref(),
        &supported.delivery_placement,
    )?;
    let vocabulary = admit_vocabulary(
        evaluation,
        confirmed(),
        intent_surface_class_vocabulary.as_ref(),
        vocabulary,
    )?;
    let canonicalizer = evaluation.check(
        DATA,
        both(pins.clone(), policy.map_err(Causes::clone)),
        |(pins, policy): (SemanticPins<Id>, &ConfirmedPolicy)| {
            // The boundary already admitted the data, so the only failure left
            // is data that disagrees with the admitted specification.
            Canonicalizer::bind(
                &pins,
                Some(data),
                policy
                    .limits()
                    .locale_resolution
                    .max_locale_identifier_bytes,
            )
            .map_err(|_| Blocking {
                code: Code::LocaleDataNotAdmitted,
                reason: None,
                limit: None,
            })
        },
    )?;
    Ok(AdmittedSpecifications {
        locale_canonicalization: pins.map(|pins| pins.specification),
        locale_negotiation,
        message_locale_fallback,
        coverage,
        intent_surface_class_vocabulary,
        delivery_graph,
        delivery_placement,
        vocabulary,
        canonicalizer,
    })
}

/// 015's Profile Resolution Specification Set: each slot's admitted member
/// exactly once. It exists only when all eight were admitted; otherwise its
/// causes are those of every member that was not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SpecificationSet<Id> {
    pub(crate) project_profile: VersionedIdentity<Id>,
    pub(crate) locale_canonicalization: VersionedIdentity<Id>,
    pub(crate) locale_negotiation: VersionedIdentity<Id>,
    pub(crate) message_locale_fallback: VersionedIdentity<Id>,
    pub(crate) coverage: VersionedIdentity<Id>,
    pub(crate) intent_surface_class_vocabulary: VocabularyAssertion<Id>,
    pub(crate) delivery_graph: VersionedIdentity<Id>,
    pub(crate) delivery_placement: VersionedIdentity<Id>,
}

impl<Id: Clone> SpecificationSet<Id> {
    pub(crate) fn new<P: Provider>(
        project_profile: &Admitted<VersionedIdentity<Id>>,
        admitted: &AdmittedSpecifications<Id, P>,
    ) -> Admitted<Self> {
        // The members in slot order, four at a time.
        let first = both(
            both(
                project_profile.clone(),
                admitted.locale_canonicalization.clone(),
            ),
            both(
                admitted.locale_negotiation.clone(),
                admitted.message_locale_fallback.clone(),
            ),
        );
        let second = both(
            both(
                admitted.coverage.clone(),
                admitted.intent_surface_class_vocabulary.clone(),
            ),
            both(
                admitted.delivery_graph.clone(),
                admitted.delivery_placement.clone(),
            ),
        );
        both(first, second).map(
            |(
                (
                    (project_profile, locale_canonicalization),
                    (locale_negotiation, message_locale_fallback),
                ),
                ((coverage, intent_surface_class_vocabulary), (delivery_graph, delivery_placement)),
            )| Self {
                project_profile,
                locale_canonicalization,
                locale_negotiation,
                message_locale_fallback,
                coverage,
                intent_surface_class_vocabulary,
                delivery_graph,
                delivery_placement,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::artifacts::vectors::{self, formal};
    use crate::locale::fixtures::{fixture_pins, FixtureProvider};
    use crate::locale::DatasetPin;
    use crate::model::CONFIGURATION_SCHEMA_VERSION;
    use crate::resolver::evaluation::Status;
    use crate::resolver::limits::fixture_limits;
    use crate::resolver::locales::fixtures::units;

    type Id = &'static str;

    const fn identity(identity: Id, revision: Id) -> VersionedIdentity<Id> {
        VersionedIdentity { identity, revision }
    }

    const ACTIVE: VersionedIdentity<Id> = identity("test-only-profile-specification", "0");

    /// Each specification slot, the phase that admits it, and the code that
    /// reports an assertion it does not support.
    const SPECIFICATION_SLOTS: [(Slot, Phase, &str); 7] = [
        (
            Slot::ProjectProfile,
            Phase::ResolverBootstrapAdmission,
            "project-profile-specification-not-admitted",
        ),
        (
            Slot::LocaleCanonicalization,
            Phase::SpecificationAdmission,
            "project-profile-locale-specification-not-admitted",
        ),
        (
            Slot::LocaleNegotiation,
            Phase::SpecificationAdmission,
            "project-profile-negotiation-specification-not-admitted",
        ),
        (
            Slot::MessageLocaleFallback,
            Phase::SpecificationAdmission,
            "project-profile-message-fallback-specification-not-admitted",
        ),
        (
            Slot::Coverage,
            Phase::SpecificationAdmission,
            "project-profile-coverage-specification-not-admitted",
        ),
        (
            Slot::DeliveryGraph,
            Phase::SpecificationAdmission,
            "project-profile-delivery-specification-not-admitted",
        ),
        (
            Slot::DeliveryPlacement,
            Phase::SpecificationAdmission,
            "project-profile-delivery-specification-not-admitted",
        ),
    ];

    fn supported_with(canonicalization: Vec<SemanticPins<Id>>) -> SupportedSpecifications<Id> {
        SupportedSpecifications::revision_zero(ACTIVE, Supported::new(canonicalization).unwrap())
    }

    fn supported() -> SupportedSpecifications<Id> {
        supported_with(vec![fixture_pins()])
    }

    fn vocabulary() -> VocabularyAssertion<Id> {
        VocabularyAssertion {
            identity: vectors::IDENTITY,
            revision: vectors::REVISION,
            semantic_digest: vectors::SEMANTIC_DIGEST,
        }
    }

    fn complete() -> SpecificationInput<Id> {
        SpecificationInput {
            project_profile: Some(ACTIVE),
            locale_canonicalization: Some(fixture_pins().specification),
            locale_negotiation: Some(identity("intlify-locale-negotiation", "0")),
            message_locale_fallback: Some(identity("intlify-message-locale-fallback", "0")),
            coverage: Some(identity("intlify-localization-coverage", "0")),
            intent_surface_class_vocabulary: Some(vocabulary()),
            delivery_graph: Some(identity("intlify-delivery-graph", "0")),
            delivery_placement: Some(identity("intlify-delivery-placement", "0")),
        }
    }

    fn assertion(
        input: &mut SpecificationInput<Id>,
        slot: Slot,
    ) -> &mut Option<VersionedIdentity<Id>> {
        match slot {
            Slot::ProjectProfile => &mut input.project_profile,
            Slot::LocaleCanonicalization => &mut input.locale_canonicalization,
            Slot::LocaleNegotiation => &mut input.locale_negotiation,
            Slot::MessageLocaleFallback => &mut input.message_locale_fallback,
            Slot::Coverage => &mut input.coverage,
            Slot::IntentSurfaceClassVocabulary => unreachable!("not a specification slot"),
            Slot::DeliveryGraph => &mut input.delivery_graph,
            Slot::DeliveryPlacement => &mut input.delivery_placement,
        }
    }

    fn policy() -> ConfirmedPolicy {
        ConfirmedPolicy::fixture_owned(fixture_limits(100))
    }

    struct Admission {
        evaluation: Evaluation,
        project_profile: Admitted<VersionedIdentity<Id>>,
        specifications: AdmittedSpecifications<Id, FixtureProvider>,
    }

    impl Admission {
        fn run(
            supported: &SupportedSpecifications<Id>,
            input: &SpecificationInput<Id>,
            policy: Result<&ConfirmedPolicy, &Causes>,
        ) -> Self {
            let mut evaluation = Evaluation::default();
            let project_profile = admit_project_profile(&mut evaluation, supported, input).unwrap();
            let specifications = admit_specifications(
                &mut evaluation,
                supported,
                input,
                policy,
                &formal(),
                FixtureProvider::new(),
            )
            .unwrap();
            Self {
                evaluation,
                project_profile,
                specifications,
            }
        }

        fn confirmed(input: &SpecificationInput<Id>) -> Self {
            Self::run(&supported(), input, Ok(&policy()))
        }

        fn issues(&self) -> Vec<(Unit, &'static str, Option<&'static str>)> {
            self.evaluation
                .issues()
                .map(|key| {
                    (
                        key.unit().clone(),
                        key.code().as_str(),
                        key.reason().map(Reason::as_str),
                    )
                })
                .collect()
        }

        fn set(&self) -> Admitted<SpecificationSet<Id>> {
            SpecificationSet::new(&self.project_profile, &self.specifications)
        }

        fn not_evaluated(&self) -> Vec<(Unit, Causes)> {
            self.evaluation
                .statuses()
                .filter_map(|(unit, status)| match status {
                    Status::Evaluated => None,
                    Status::NotEvaluated(causes) => Some((unit.clone(), causes.clone())),
                })
                .collect()
        }
    }

    /// The causes of the one issue an admission found.
    fn only_cause(admission: &Admission) -> Causes {
        let keys: Vec<_> = admission.evaluation.issues().cloned().collect();
        assert_eq!(keys.len(), 1);
        admission
            .set()
            .err()
            .filter(|causes| causes.keys().eq(keys.iter()))
            .expect("the set names the issue as its only cause")
    }

    #[test]
    fn slots_are_named_and_ordered_as_015_lists_them() {
        let slots = [
            Slot::ProjectProfile,
            Slot::LocaleCanonicalization,
            Slot::LocaleNegotiation,
            Slot::MessageLocaleFallback,
            Slot::Coverage,
            Slot::IntentSurfaceClassVocabulary,
            Slot::DeliveryGraph,
            Slot::DeliveryPlacement,
        ];
        assert_eq!(
            slots.map(Slot::name),
            [
                "project-profile",
                "locale-canonicalization",
                "locale-negotiation",
                "message-locale-fallback",
                "coverage",
                "intent-surface-class-vocabulary",
                "delivery-graph",
                "delivery-placement",
            ]
        );
        assert!(slots.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(
            PHASE_5.map(|unit| unit.subject),
            [
                Subject::Slot(Slot::LocaleCanonicalization),
                Subject::Slot(Slot::LocaleNegotiation),
                Subject::Slot(Slot::MessageLocaleFallback),
                Subject::Slot(Slot::Coverage),
                Subject::Slot(Slot::IntentSurfaceClassVocabulary),
                Subject::Slot(Slot::DeliveryGraph),
                Subject::Slot(Slot::DeliveryPlacement),
                Subject::Vocabulary,
                Subject::Vocabulary,
                Subject::CanonicalizationData,
            ]
        );
        assert!(PHASE_5.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn a_complete_input_admits_every_slot_and_forms_the_set() {
        let admission = Admission::confirmed(&complete());
        assert_eq!(admission.issues(), []);
        assert_eq!(admission.not_evaluated(), []);
        assert_eq!(
            admission
                .evaluation
                .statuses()
                .map(|(unit, _)| unit.clone())
                .collect::<Vec<_>>(),
            [
                [unit(
                    Phase::ResolverBootstrapAdmission,
                    Slot::ProjectProfile
                )]
                .as_slice(),
                &PHASE_5
            ]
            .concat()
        );
        let input = complete();
        assert_eq!(
            admission.set(),
            Ok(SpecificationSet {
                project_profile: input.project_profile.unwrap(),
                locale_canonicalization: input.locale_canonicalization.unwrap(),
                locale_negotiation: input.locale_negotiation.unwrap(),
                message_locale_fallback: input.message_locale_fallback.unwrap(),
                coverage: input.coverage.unwrap(),
                intent_surface_class_vocabulary: input.intent_surface_class_vocabulary.unwrap(),
                delivery_graph: input.delivery_graph.unwrap(),
                delivery_placement: input.delivery_placement.unwrap(),
            })
        );
        assert_eq!(
            admission.specifications.vocabulary,
            Ok(SurfaceVocabulary::fixture_owned(&vectors::MEMBERS))
        );
        let canonicalizer = admission.specifications.canonicalizer.as_ref().unwrap();
        assert_eq!(canonicalizer.binding().pins, fixture_pins());
    }

    #[test]
    fn a_missing_slot_is_incomplete_and_names_its_slot() {
        for (slot, phase, _) in SPECIFICATION_SLOTS {
            let mut input = complete();
            *assertion(&mut input, slot) = None;
            let admission = Admission::confirmed(&input);
            assert_eq!(
                admission.issues(),
                [(
                    unit(phase, slot),
                    "project-profile-specification-input-incomplete",
                    Some("missing-required-slot")
                )],
                "{}",
                slot.name()
            );
        }
        let mut input = complete();
        input.intent_surface_class_vocabulary = None;
        let admission = Admission::confirmed(&input);
        assert_eq!(
            admission.issues(),
            [(
                unit(
                    Phase::SpecificationAdmission,
                    Slot::IntentSurfaceClassVocabulary
                ),
                "project-profile-specification-input-incomplete",
                Some("missing-required-slot")
            )]
        );
        let causes = only_cause(&admission);
        // The artifact is still read; only its comparison waits on the slot.
        assert_eq!(admission.not_evaluated(), [(ASSERTED, causes.clone())]);
        assert_eq!(admission.specifications.vocabulary, Err(causes));
    }

    #[test]
    fn another_identity_is_unsupported_and_another_revision_incompatible_in_every_specification_slot(
    ) {
        for (slot, phase, code) in SPECIFICATION_SLOTS {
            for (change, reason) in [
                (
                    (|asserted| asserted.identity = "another-specification")
                        as fn(&mut VersionedIdentity<Id>),
                    "unsupported",
                ),
                (|asserted| asserted.revision = "1", "incompatible"),
            ] {
                let mut input = complete();
                change(assertion(&mut input, slot).as_mut().unwrap());
                let admission = Admission::confirmed(&input);
                assert_eq!(
                    admission.issues(),
                    [(unit(phase, slot), code, Some(reason))],
                    "{} {reason}",
                    slot.name()
                );
                only_cause(&admission);
            }
        }
    }

    #[test]
    fn the_vocabulary_slot_admits_any_assertion_and_the_artifact_answers_it() {
        let mut input = complete();
        input.intent_surface_class_vocabulary = Some(VocabularyAssertion {
            identity: "any-vocabulary",
            revision: "7",
            semantic_digest: "sha256:any",
        });
        let admission = Admission::confirmed(&input);
        // The slot has no fixed value, so the Set forms; the artifact is not
        // the one it asserts.
        assert_eq!(
            admission.issues(),
            [(
                ASSERTED,
                "project-profile-intent-surface-class-vocabulary-not-admitted",
                Some("identity-mismatch")
            )]
        );
        assert_eq!(
            admission.specifications.intent_surface_class_vocabulary,
            Ok(input.intent_surface_class_vocabulary.unwrap())
        );
        assert!(admission.set().is_ok());
        let causes = admission.specifications.vocabulary.unwrap_err();
        assert_eq!(units(&causes), [ASSERTED]);
    }

    #[test]
    fn a_set_missing_several_members_names_every_cause() {
        let mut input = complete();
        input.project_profile = None;
        input.message_locale_fallback = Some(identity("intlify-message-locale-fallback", "1"));
        input.delivery_placement = None;
        let admission = Admission::confirmed(&input);
        let keys: Vec<_> = admission.evaluation.issues().cloned().collect();
        assert_eq!(keys.len(), 3);
        let causes = admission.set().unwrap_err();
        assert!(causes.keys().eq(keys.iter()));
    }

    #[test]
    fn phase_0_suppresses_nothing_but_keeps_the_set_from_forming() {
        let mut input = complete();
        input.project_profile = Some(identity("test-only-profile-specification", "1"));
        let admission = Admission::confirmed(&input);
        assert_eq!(admission.not_evaluated(), []);
        assert!(admission.specifications.canonicalizer.is_ok());
        assert_eq!(admission.set().err(), admission.project_profile.err());
    }

    #[test]
    fn the_canonicalization_data_waits_for_its_slot() {
        for asserted in [None, Some(identity("another-specification", "0"))] {
            let mut input = complete();
            input.locale_canonicalization = asserted;
            let admission = Admission::confirmed(&input);
            let causes = only_cause(&admission);
            assert_eq!(admission.not_evaluated(), [(DATA, causes.clone())]);
            assert_eq!(admission.specifications.canonicalizer.err(), Some(causes));
        }
    }

    #[test]
    fn data_naming_another_specification_or_dataset_is_not_admitted() {
        let mut stale = fixture_pins();
        stale.dataset.semantic_digest = "sha256:another-dataset";
        for (supported, asserted) in [
            // The slot admits a specification the provider's data does not name.
            (
                vec![SemanticPins {
                    specification: identity("another-specification", "0"),
                    dataset: fixture_pins().dataset,
                }],
                identity("another-specification", "0"),
            ),
            // The data names the specification but not the dataset it admits.
            (vec![stale], fixture_pins().specification),
        ] {
            let mut input = complete();
            input.locale_canonicalization = Some(asserted.clone());
            let admission = Admission::run(&supported_with(supported), &input, Ok(&policy()));
            assert_eq!(
                admission.issues(),
                [(DATA, "project-profile-locale-data-not-admitted", None)]
            );
            assert_eq!(
                admission.specifications.locale_canonicalization,
                Ok(asserted)
            );
            assert_eq!(admission.not_evaluated(), []);
            // The Set holds the specification members only; the data is a
            // separate input admitted against its member.
            assert!(admission.set().is_ok());
        }
    }

    #[test]
    fn an_unconfirmed_policy_leaves_every_phase_5_unit_not_evaluated() {
        // Phase 4 supplies these keys once it exists. Phase 5 only carries what
        // it is given, so a Phase 0 key stands in for them here.
        let mut input = complete();
        input.project_profile = None;
        let mut stand_in = Evaluation::default();
        let causes = admit_project_profile(&mut stand_in, &supported(), &input)
            .unwrap()
            .unwrap_err();
        let admission = Admission::run(&supported(), &complete(), Err(&causes));
        assert_eq!(admission.issues(), []);
        assert_eq!(
            admission.not_evaluated(),
            PHASE_5.map(|unit| (unit, causes.clone()))
        );
        assert_eq!(admission.set().err(), Some(causes.clone()));
        assert_eq!(admission.specifications.canonicalizer.err(), Some(causes));
    }

    #[test]
    fn the_canonicalizer_takes_the_confirmed_identifier_bound() {
        let policy = ConfirmedPolicy::fixture_owned(fixture_limits(40));
        let admission = Admission::run(&supported(), &complete(), Ok(&policy));
        assert_eq!(
            admission
                .specifications
                .canonicalizer
                .unwrap()
                .max_identifier_bytes(),
            policy
                .limits()
                .locale_resolution
                .max_locale_identifier_bytes
        );
    }

    #[test]
    fn the_configuration_version_and_the_specification_revisions_are_separate_domains() {
        // The configuration schema pins its own version and names none of the
        // specifications a slot asserts.
        let schema = crate::schema::project_profile_config_schema().unwrap();
        assert_eq!(
            schema["definitions"]["ConfigurationVersion"]["enum"],
            json!([CONFIGURATION_SCHEMA_VERSION])
        );
        let text = schema.to_string();
        let input = complete();
        for asserted in [
            input.project_profile.unwrap(),
            input.locale_canonicalization.unwrap(),
            input.locale_negotiation.unwrap(),
            input.message_locale_fallback.unwrap(),
            input.coverage.unwrap(),
            input.delivery_graph.unwrap(),
            input.delivery_placement.unwrap(),
        ] {
            assert!(!text.contains(asserted.identity), "{}", asserted.identity);
        }
        // Revising the canonicalization specification revises the supported
        // values and the slot; nothing about the configuration takes part.
        let revised = identity("test-only-locale-specification", "1");
        let mut input = complete();
        input.locale_canonicalization = Some(revised.clone());
        let admission = Admission::run(
            &supported_with(vec![SemanticPins {
                specification: revised.clone(),
                dataset: DatasetPin {
                    identity: "test-only-finite-locale-data",
                    semantic_digest: "test-only-dataset-content-pin",
                },
            }]),
            &input,
            Ok(&policy()),
        );
        assert_eq!(
            admission.specifications.locale_canonicalization,
            Ok(revised)
        );
    }
}
