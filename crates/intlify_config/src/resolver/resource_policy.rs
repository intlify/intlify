// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's Phase 4, `resource-policy-admission`, through the admission of the
//! selected Resource Limit Policy and its comparison with the capability.
//! Before any policy is admitted, the capability's `policyComparable`
//! `artifactAdmission` bounds are the sole authority over the submitted
//! collection and the selected declaration's references. The Bootstrap
//! Resource Policy Candidate Admission then decides the selected
//! `resourceLimits` reference, and the admitted policy must need no bound past
//! the capability's value. The rechecks under the admitted policy, which make
//! it final and confirm the selection, come after this.

use std::collections::BTreeSet;

use crate::artifacts::collection::FormalCollection;
use crate::artifacts::envelope::{Declaration, FormalMember};
use crate::input_limits::Bound;
use crate::model::{NonEmptyMap, Policies, RequiredNullable, TargetDeclaration, TargetId};
use crate::references::{PolicyReference, TargetProfileReference};

use super::bootstrap::{admit_candidate, AdmittedResourcePolicy, ResourcePolicyVerification};
use super::evaluation::{
    both, Actual, Admitted, Blocking, Causes, Check, Evaluation, EvaluationInvariant, Phase,
    Shortfall, Subject, Unit,
};
use super::limits::{
    ArtifactAdmissionLimits, BoundId, Group, ImplementationCapability, ResolutionLimits,
};
use super::tally::{actual, measure, total, within, Tally};

const fn artifact_bound(name: &'static str) -> BoundId {
    BoundId {
        group: Group::ArtifactAdmission,
        name,
    }
}

const MAX_ARTIFACTS: BoundId = artifact_bound("maxArtifacts");
const MAX_POLICY_ARTIFACTS: BoundId = artifact_bound("maxPolicyArtifacts");
const MAX_TARGET_PROFILE_ARTIFACTS: BoundId = artifact_bound("maxTargetProfileArtifacts");
const MAX_ARTIFACT_REFERENCES: BoundId = artifact_bound("maxArtifactReferences");
const MAX_SINGLE_ARTIFACT_CANONICAL_BYTES: BoundId =
    artifact_bound("maxSingleArtifactCanonicalBytes");
const MAX_TOTAL_ARTIFACT_CANONICAL_BYTES: BoundId =
    artifact_bound("maxTotalArtifactCanonicalBytes");
const MAX_SINGLE_REFERENCE_CANONICAL_BYTES: BoundId =
    artifact_bound("maxSingleReferenceCanonicalBytes");
const MAX_TOTAL_REFERENCE_CANONICAL_BYTES: BoundId =
    artifact_bound("maxTotalReferenceCanonicalBytes");

/// What one Phase 4 unit checks: a bound of 015's `artifactAdmission` table,
/// in the table's declaration order, then the selected reference.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PolicySubject {
    /// Every submitted occurrence, under `maxArtifacts`.
    Artifacts,
    /// The occurrences declaring a Policy kind, under `maxPolicyArtifacts`.
    PolicyArtifacts,
    /// The occurrences declaring the Target Profile kind, under
    /// `maxTargetProfileArtifacts`.
    TargetProfileArtifacts,
    /// The selected declaration's references, under `maxArtifactReferences`.
    References,
    /// One distinct canonical submitted body, by its place in their unsigned
    /// byte order, under `maxSingleArtifactCanonicalBytes`.
    Artifact(usize),
    /// Every occurrence's canonical bytes, under
    /// `maxTotalArtifactCanonicalBytes`.
    ArtifactBytes,
    /// One reference of the selected declaration, under
    /// `maxSingleReferenceCanonicalBytes`.
    Reference(ReferenceField),
    /// Every reference's canonical bytes, under
    /// `maxTotalReferenceCanonicalBytes`.
    ReferenceBytes,
    /// The selected `resourceLimits` reference and the policy it names.
    ResourceLimits,
}

/// Where a reference sits in the selected declaration: a `policies` slot, in
/// the name order of the slots, then each target's `profile`, in Target ID
/// order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ReferenceField {
    Policy(PolicySlot),
    TargetProfile(TargetId),
}

/// The slots of `policies`, declared in the unsigned byte order of their names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PolicySlot {
    Approval,
    GlossarySet,
    ProviderRouting,
    ResourceLimits,
    Selection,
    SourceAdmission,
    Trust,
}

impl PolicySlot {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Approval => "approval",
            Self::GlossarySet => "glossarySet",
            Self::ProviderRouting => "providerRouting",
            Self::ResourceLimits => "resourceLimits",
            Self::Selection => "selection",
            Self::SourceAdmission => "sourceAdmission",
            Self::Trust => "trust",
        }
    }
}

/// The selected declaration's reference-bearing fields as structural
/// admission left them, each admitted or unavailable with the causes behind
/// it.
pub(crate) struct DeclarationReferences<'a> {
    pub(crate) policies: Admitted<&'a Policies<PolicyReference>>,
    pub(crate) target_profiles:
        Admitted<&'a NonEmptyMap<TargetId, TargetDeclaration<TargetProfileReference>>>,
}

/// The unit Phase 4 records for `check` over `subject`.
pub(crate) const fn unit(subject: PolicySubject, check: Check) -> Unit {
    Unit {
        phase: Phase::ResourcePolicyAdmission,
        subject: Subject::ResourcePolicy(subject),
        check,
    }
}

/// Phase 4 up to the comparison with the capability. The collection bounds
/// need nothing earlier, so they are checked whatever the selection; the
/// reference bounds need the selected declaration's fields; the decision needs
/// the `resourceLimits` reference, every collection bound, and that
/// reference's own bytes; and the comparison needs the admitted policy.
pub(crate) fn admit_resource_policy(
    evaluation: &mut Evaluation,
    capability: &ImplementationCapability,
    collection: &FormalCollection,
    resource_limits: Admitted<&PolicyReference>,
    references: &DeclarationReferences<'_>,
    verification: &impl ResourcePolicyVerification,
) -> Result<Admitted<AdmittedResourcePolicy>, EvaluationInvariant> {
    let limits = &capability.policy_comparable().artifact_admission;
    let members = collection.members();
    let inspected = collection_bounds(evaluation, limits, members)?;
    let own_bytes = reference_bounds(evaluation, limits, &resource_limits, references)?;
    let decided = evaluation.check(
        unit(PolicySubject::ResourceLimits, Check::BootstrapCandidate),
        both(both(resource_limits, inspected), own_bytes).map(|((reference, ()), ())| reference),
        |reference| admit_candidate(&reference.tuple(), members, verification),
    )?;
    evaluation.check(
        unit(PolicySubject::ResourceLimits, Check::CompatiblePolicy),
        decided,
        |policy| match first_shortfall(policy.limits(), capability.policy_comparable()) {
            None => Ok(policy),
            Some(shortfall) => Err(Blocking::shortfall(shortfall)),
        },
    )
}

/// One bound's unit over a count whose every part is available.
fn bounded(
    evaluation: &mut Evaluation,
    subject: PolicySubject,
    bound: BoundId,
    limit: Bound,
    tally: Tally,
) -> Result<Admitted<()>, EvaluationInvariant> {
    evaluation.check(unit(subject, Check::WithinCapability), Ok(()), |()| {
        within(bound, limit, actual(tally, limit))
    })
}

/// Whether both hold, or the causes of each that does not.
fn and(first: Admitted<()>, second: Admitted<()>) -> Admitted<()> {
    both(first, second).map(|((), ())| ())
}

/// The five bounds over the collection, one unit each but one per distinct
/// canonical body, and whether all of them hold.
fn collection_bounds(
    evaluation: &mut Evaluation,
    limits: &ArtifactAdmissionLimits,
    members: &[FormalMember],
) -> Result<Admitted<()>, EvaluationInvariant> {
    // A member declaring another kind or schema revision is unsupported and
    // counts only as an artifact.
    let declared = |policy: bool| {
        members
            .iter()
            .filter(|member| {
                matches!(member.declaration(), Declaration::Admitted(kind) if kind.is_policy() == policy)
            })
            .count()
    };
    let mut holds = bounded(
        evaluation,
        PolicySubject::Artifacts,
        MAX_ARTIFACTS,
        limits.max_artifacts,
        Tally::of(members.len()),
    )?;
    for (subject, bound, limit, count) in [
        (
            PolicySubject::PolicyArtifacts,
            MAX_POLICY_ARTIFACTS,
            limits.max_policy_artifacts,
            declared(true),
        ),
        (
            PolicySubject::TargetProfileArtifacts,
            MAX_TARGET_PROFILE_ARTIFACTS,
            limits.max_target_profile_artifacts,
            declared(false),
        ),
    ] {
        let held = bounded(evaluation, subject, bound, limit, Tally::of(count))?;
        holds = and(holds, held);
    }
    // Equal occurrences have one canonical body and commute, so they share
    // its unit; every occurrence still counts toward the total.
    let bodies: BTreeSet<&[u8]> = members.iter().map(FormalMember::canonical_body).collect();
    for (ordinal, body) in bodies.into_iter().enumerate() {
        let held = bounded(
            evaluation,
            PolicySubject::Artifact(ordinal),
            MAX_SINGLE_ARTIFACT_CANONICAL_BYTES,
            limits.max_single_artifact_canonical_bytes,
            Tally::of(body.len()),
        )?;
        holds = and(holds, held);
    }
    let held = bounded(
        evaluation,
        PolicySubject::ArtifactBytes,
        MAX_TOTAL_ARTIFACT_CANONICAL_BYTES,
        limits.max_total_artifact_canonical_bytes,
        total(
            members
                .iter()
                .map(|member| Tally::exactly(member.canonical_bytes())),
        ),
    )?;
    Ok(and(holds, held))
}

/// The references present in `policies`, in the name order of their slots.
/// An explicit `null` is no reference.
fn present(
    policies: &Policies<PolicyReference>,
) -> impl Iterator<Item = (PolicySlot, &PolicyReference)> {
    fn nullable(value: &RequiredNullable<PolicyReference>) -> Option<&PolicyReference> {
        match value {
            RequiredNullable::Reference(reference) => Some(reference),
            RequiredNullable::Null => None,
        }
    }
    [
        (PolicySlot::Approval, Some(&policies.approval)),
        (PolicySlot::GlossarySet, nullable(&policies.glossary_set)),
        (
            PolicySlot::ProviderRouting,
            nullable(&policies.provider_routing),
        ),
        (PolicySlot::ResourceLimits, Some(&policies.resource_limits)),
        (PolicySlot::Selection, Some(&policies.selection)),
        (
            PolicySlot::SourceAdmission,
            Some(&policies.source_admission),
        ),
        (PolicySlot::Trust, Some(&policies.trust)),
    ]
    .into_iter()
    .filter_map(|(slot, reference)| reference.map(|reference| (slot, reference)))
}

fn join(earlier: Option<Causes>, causes: &Causes) -> Causes {
    match earlier {
        Some(earlier) => earlier.union(causes.clone()),
        None => causes.clone(),
    }
}

/// The three bounds over the selected declaration's references, and whether
/// the `resourceLimits` reference's own bytes hold. A field structural
/// admission did not admit has no references to count: a count is proved only
/// when what is visible already exceeds the bound.
fn reference_bounds(
    evaluation: &mut Evaluation,
    limits: &ArtifactAdmissionLimits,
    resource_limits: &Admitted<&PolicyReference>,
    references: &DeclarationReferences<'_>,
) -> Result<Admitted<()>, EvaluationInvariant> {
    let mut visible: Vec<(ReferenceField, u64)> = Vec::new();
    let mut unavailable: Option<Causes> = None;
    match references.policies {
        Ok(policies) => {
            visible.extend(present(policies).map(|(slot, reference)| {
                (ReferenceField::Policy(slot), reference.canonical_bytes())
            }));
        }
        Err(ref causes) => unavailable = Some(join(unavailable, causes)),
    }
    match references.target_profiles {
        Ok(targets) => visible.extend(targets.iter().map(|(id, target)| {
            (
                ReferenceField::TargetProfile(id.clone()),
                target.profile.canonical_bytes(),
            )
        })),
        Err(ref causes) => unavailable = Some(join(unavailable, causes)),
    }
    // The declaration's count and total gate the resolution of its other
    // references, not the decision for the selected one.
    let count_limit = limits.max_artifact_references;
    let _ = evaluation.check(
        unit(PolicySubject::References, Check::WithinCapability),
        measure(Tally::of(visible.len()), unavailable.clone(), count_limit),
        |actual| within(MAX_ARTIFACT_REFERENCES, count_limit, actual),
    )?;
    // The selected reference is checked from what selection admitted, even
    // when the rest of `policies` was not.
    let single_limit = limits.max_single_reference_canonical_bytes;
    let resource_limits_slot = ReferenceField::Policy(PolicySlot::ResourceLimits);
    let own_bytes = evaluation.check(
        unit(
            PolicySubject::Reference(resource_limits_slot.clone()),
            Check::WithinCapability,
        ),
        resource_limits
            .as_ref()
            .map(|reference| reference.canonical_bytes())
            .map_err(Causes::clone),
        |bytes| {
            within(
                MAX_SINGLE_REFERENCE_CANONICAL_BYTES,
                single_limit,
                Actual::Exact(bytes),
            )
        },
    )?;
    for (field, bytes) in &visible {
        if *field == resource_limits_slot {
            continue;
        }
        let _ = evaluation.check(
            unit(
                PolicySubject::Reference(field.clone()),
                Check::WithinCapability,
            ),
            Ok(*bytes),
            |bytes| {
                within(
                    MAX_SINGLE_REFERENCE_CANONICAL_BYTES,
                    single_limit,
                    Actual::Exact(bytes),
                )
            },
        )?;
    }
    let total_limit = limits.max_total_reference_canonical_bytes;
    let _ = evaluation.check(
        unit(PolicySubject::ReferenceBytes, Check::WithinCapability),
        measure(
            total(visible.iter().map(|&(_, bytes)| Tally::exactly(bytes))),
            unavailable,
            total_limit,
        ),
        |actual| within(MAX_TOTAL_REFERENCE_CANONICAL_BYTES, total_limit, actual),
    )?;
    Ok(own_bytes)
}

/// The first bound, in canonical order, that the policy requires more of than
/// the capability makes available. The comparison is always
/// `available >= required`.
fn first_shortfall(policy: &ResolutionLimits, available: &ResolutionLimits) -> Option<Shortfall> {
    policy
        .bounds()
        .into_iter()
        .zip(available.bounds())
        .find_map(|((bound, required), (_, available))| {
            (available.get() < required.get()).then_some(Shortfall {
                bound,
                required,
                available,
            })
        })
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;
    use crate::artifacts::collection::{read_collection, CollectionLimits};
    use crate::artifacts::envelope::{envelope_of, ArtifactKind, Envelope, ExactTuple};
    use crate::resolver::evaluation::{Code, IssueKey, LimitExceeded, Reason, Status};
    use crate::resolver::limits::{fixture_limits, SECTION};
    use crate::resolver::locales::fixtures::stand_in;
    use crate::resolver::specification::Slot;

    /// Limits no test reaches.
    const ROOM: CollectionLimits = CollectionLimits {
        members: u64::MAX,
        member_bytes: u64::MAX,
        total_bytes: u64::MAX,
        depth: u64::MAX,
    };

    /// 018's admission as a test states it: it admits everything.
    struct Trusting;

    impl ResourcePolicyVerification for Trusting {
        fn authenticates(&self, _: &Envelope<'_>) -> bool {
            true
        }

        fn admits_common_body(&self, _: &serde_json::Map<String, Value>) -> bool {
            true
        }
    }

    fn bound(value: u64) -> Bound {
        Bound::new(value).unwrap()
    }

    /// A capability far above the fixture policy, with its `artifactAdmission`
    /// bounds edited by `edit`.
    fn capability(edit: impl FnOnce(&mut ArtifactAdmissionLimits)) -> ImplementationCapability {
        let mut limits = fixture_limits(10_000);
        edit(&mut limits.artifact_admission);
        ImplementationCapability::fixture_owned(limits)
    }

    fn roomy() -> ImplementationCapability {
        capability(|_| {})
    }

    /// The selected policy's bounds: counting up from 1, but 1 for every
    /// `artifactAdmission` bound, so that a test can lower the capability's
    /// value of those bounds without making the policy need more.
    fn policy_limits() -> ResolutionLimits {
        let mut limits = fixture_limits(1);
        let one = bound(1);
        limits.artifact_admission = ArtifactAdmissionLimits {
            max_artifacts: one,
            max_policy_artifacts: one,
            max_target_profile_artifacts: one,
            max_artifact_references: one,
            max_single_artifact_canonical_bytes: one,
            max_total_artifact_canonical_bytes: one,
            max_single_reference_canonical_bytes: one,
            max_total_reference_canonical_bytes: one,
        };
        limits
    }

    /// The selected Resource Limit Policy.
    fn chosen() -> Value {
        envelope_of(
            ArtifactKind::ResourceLimitPolicy,
            "storefront-limits",
            "1",
            json!({ SECTION: policy_limits().section() }),
        )
    }

    fn tuple_of(value: &Value) -> ExactTuple {
        FormalMember::new(value.clone())
            .unwrap()
            .envelope()
            .unwrap()
            .tuple()
            .clone()
    }

    /// The reference a declaration writes for `tuple`.
    fn reference_value(tuple: &ExactTuple) -> Value {
        json!({
            "kind": tuple.kind.spelling(),
            "identity": tuple.identity.as_str(),
            "revision": tuple.revision.as_str(),
            "specificationRevision": tuple.specification_revision.as_str(),
            "semanticDigest": tuple.semantic_digest.as_str(),
        })
    }

    fn reference_to(value: &Value) -> PolicyReference {
        serde_json::from_value(reference_value(&tuple_of(value))).unwrap()
    }

    fn collection(values: &[Value]) -> FormalCollection {
        let texts: Vec<String> = values.iter().map(Value::to_string).collect();
        let bytes: Vec<&[u8]> = texts.iter().map(String::as_bytes).collect();
        read_collection(Some(&bytes), &ROOM).unwrap()
    }

    /// A Policy reference of `kind` the decision never looks at.
    fn other(kind: &str, identity: &str) -> Value {
        json!({
            "kind": kind,
            "identity": identity,
            "revision": "1",
            "specificationRevision": "0",
            "semanticDigest": format!("sha256:{}", "1".repeat(64)),
        })
    }

    /// The declaration's `policies`, with `glossarySet` set and
    /// `providerRouting` an explicit `null`.
    fn policies() -> Policies<PolicyReference> {
        serde_json::from_value(json!({
            "resourceLimits": reference_value(&tuple_of(&chosen())),
            "trust": other("trust-policy", "storefront-trust"),
            "sourceAdmission": other("source-admission-policy", "storefront-source"),
            "approval": other("approval-policy", "storefront-approval"),
            "selection": other("selection-policy", "storefront-selection"),
            "providerRouting": null,
            "glossarySet": other("glossary-set", "storefront-glossary"),
        }))
        .unwrap()
    }

    fn targets() -> NonEmptyMap<TargetId, TargetDeclaration<TargetProfileReference>> {
        let profile = |identity: &str| {
            json!({
                "profile": {
                    "kind": "target-profile",
                    "identity": identity,
                    "revision": "1",
                    "specificationRevision": "0",
                    "semanticDigest": format!("sha256:{}", "a".repeat(64)),
                },
                "requestedLocales": ["en"],
            })
        };
        serde_json::from_value(
            json!({"web": profile("storefront-web"), "app": profile("storefront-app")}),
        )
        .unwrap()
    }

    /// Everything Phase 4 reads, which a test edits before running it.
    struct Inputs {
        capability: ImplementationCapability,
        members: Vec<Value>,
        resource_limits: Admitted<PolicyReference>,
        policies: Admitted<Policies<PolicyReference>>,
        targets: Admitted<NonEmptyMap<TargetId, TargetDeclaration<TargetProfileReference>>>,
    }

    impl Inputs {
        fn new() -> Self {
            Self {
                capability: roomy(),
                members: vec![
                    chosen(),
                    envelope_of(
                        ArtifactKind::TrustPolicy,
                        "storefront-trust",
                        "1",
                        json!({}),
                    ),
                    envelope_of(
                        ArtifactKind::TargetProfile,
                        "storefront-web",
                        "1",
                        json!({}),
                    ),
                    json!({"kind": "localized-message", "schemaRevision": "0"}),
                    json!({"schemaRevision": "0"}),
                ],
                resource_limits: Ok(reference_to(&chosen())),
                policies: Ok(policies()),
                targets: Ok(targets()),
            }
        }

        fn run(&self) -> (Evaluation, Admitted<AdmittedResourcePolicy>) {
            let mut evaluation = Evaluation::default();
            let references = DeclarationReferences {
                policies: self.policies.as_ref().map_err(Causes::clone),
                target_profiles: self.targets.as_ref().map_err(Causes::clone),
            };
            let admitted = admit_resource_policy(
                &mut evaluation,
                &self.capability,
                &collection(&self.members),
                self.resource_limits.as_ref().map_err(Causes::clone),
                &references,
                &Trusting,
            )
            .unwrap();
            (evaluation, admitted)
        }
    }

    fn issues(evaluation: &Evaluation) -> Vec<(Unit, Code, Option<Reason>)> {
        evaluation
            .issues()
            .map(|key| (key.unit().clone(), key.code(), key.reason()))
            .collect()
    }

    fn key_of(evaluation: &Evaluation, unit: &Unit) -> IssueKey {
        evaluation
            .issues()
            .find(|key| key.unit() == unit)
            .unwrap()
            .clone()
    }

    /// The keys a unit left not evaluated waits on.
    fn waits_on(evaluation: &Evaluation, unit: &Unit) -> Vec<IssueKey> {
        match evaluation.status(unit) {
            Some(Status::NotEvaluated(causes)) => causes.keys().cloned().collect(),
            other => panic!("{unit:?} is {other:?}"),
        }
    }

    fn decision() -> Unit {
        unit(PolicySubject::ResourceLimits, Check::BootstrapCandidate)
    }

    fn comparison() -> Unit {
        unit(PolicySubject::ResourceLimits, Check::CompatiblePolicy)
    }

    #[test]
    fn a_policy_within_every_capability_bound_is_admitted_and_compatible() {
        let inputs = Inputs::new();
        let (evaluation, admitted) = inputs.run();
        assert_eq!(issues(&evaluation), []);
        let policy = admitted.unwrap();
        assert_eq!(policy.tuple(), &tuple_of(&chosen()));
        assert_eq!(policy.limits().bounds(), policy_limits().bounds());
        // Every unit was evaluated: five over the collection (one per distinct
        // body), three over the references (one per visible reference), the
        // decision, and the comparison.
        let reference = |field| unit(PolicySubject::Reference(field), Check::WithinCapability);
        let target = |id: &str| {
            reference(ReferenceField::TargetProfile(
                serde_json::from_value(json!(id)).unwrap(),
            ))
        };
        let mut expected: Vec<Unit> = [
            PolicySubject::Artifacts,
            PolicySubject::PolicyArtifacts,
            PolicySubject::TargetProfileArtifacts,
            PolicySubject::References,
        ]
        .into_iter()
        .chain((0..5).map(PolicySubject::Artifact))
        .chain([PolicySubject::ArtifactBytes])
        .map(|subject| unit(subject, Check::WithinCapability))
        .collect();
        expected.extend(
            [
                PolicySlot::Approval,
                PolicySlot::GlossarySet,
                PolicySlot::ResourceLimits,
                PolicySlot::Selection,
                PolicySlot::SourceAdmission,
                PolicySlot::Trust,
            ]
            .map(|slot| reference(ReferenceField::Policy(slot))),
        );
        expected.extend([target("app"), target("web")]);
        expected.push(unit(PolicySubject::ReferenceBytes, Check::WithinCapability));
        expected.extend([decision(), comparison()]);
        let recorded: Vec<(&Unit, &Status)> = evaluation.statuses().collect();
        assert_eq!(
            recorded
                .iter()
                .map(|(unit, _)| (*unit).clone())
                .collect::<Vec<_>>(),
            expected
        );
        assert!(recorded
            .iter()
            .all(|(_, status)| **status == Status::Evaluated));
    }

    /// The exact count Phase 4 measures for `subject` over the default
    /// inputs, from the issue it reports under a bound of one.
    fn measured(subject: PolicySubject, edit: fn(&mut ArtifactAdmissionLimits)) -> u64 {
        let mut inputs = Inputs::new();
        inputs.capability = capability(edit);
        let (evaluation, _) = inputs.run();
        let key = key_of(&evaluation, &unit(subject, Check::WithinCapability));
        match evaluation.limit(&key).unwrap().actual {
            Actual::Exact(count) => count,
            Actual::AtLeast(_) => panic!("an exact count"),
        }
    }

    #[test]
    fn the_collection_is_counted_as_015_counts_it() {
        let inputs = Inputs::new();
        let members = collection(&inputs.members);
        let bytes: Vec<u64> = members
            .members()
            .iter()
            .map(FormalMember::canonical_bytes)
            .collect();
        // Every occurrence, whatever it declares.
        assert_eq!(
            measured(PolicySubject::Artifacts, |limits| limits.max_artifacts =
                bound(1)),
            5
        );
        // The Resource Limit Policy and the Trust Policy; neither the Target
        // Profile, the unsupported kind, nor the member declaring none.
        assert_eq!(
            measured(PolicySubject::PolicyArtifacts, |limits| {
                limits.max_policy_artifacts = bound(1);
            }),
            2
        );
        let target_profiles = |limits: &mut ArtifactAdmissionLimits| {
            limits.max_target_profile_artifacts = bound(1);
        };
        let mut inputs = Inputs::new();
        inputs.members.push(envelope_of(
            ArtifactKind::TargetProfile,
            "storefront-app",
            "1",
            json!({}),
        ));
        inputs.capability = capability(target_profiles);
        let (evaluation, _) = inputs.run();
        let key = key_of(
            &evaluation,
            &unit(
                PolicySubject::TargetProfileArtifacts,
                Check::WithinCapability,
            ),
        );
        assert_eq!(evaluation.limit(&key).unwrap().actual, Actual::Exact(2));
        // The canonical bytes of every occurrence.
        assert_eq!(
            measured(PolicySubject::ArtifactBytes, |limits| {
                limits.max_total_artifact_canonical_bytes = bound(1);
            }),
            bytes.iter().sum::<u64>()
        );
    }

    #[test]
    fn each_collection_bound_admits_its_exact_value_and_reports_one_more() {
        type Edit = Box<dyn Fn(&mut ArtifactAdmissionLimits, u64)>;
        let inputs = Inputs::new();
        let members = collection(&inputs.members);
        let largest = members
            .members()
            .iter()
            .map(FormalMember::canonical_bytes)
            .max()
            .unwrap();
        let total: u64 = members
            .members()
            .iter()
            .map(FormalMember::canonical_bytes)
            .sum();
        let cases: [(BoundId, u64, Edit); 5] = [
            (
                MAX_ARTIFACTS,
                5,
                Box::new(|limits, value| limits.max_artifacts = bound(value)),
            ),
            (
                MAX_POLICY_ARTIFACTS,
                2,
                Box::new(|limits, value| limits.max_policy_artifacts = bound(value)),
            ),
            (
                MAX_TARGET_PROFILE_ARTIFACTS,
                1,
                Box::new(|limits, value| limits.max_target_profile_artifacts = bound(value)),
            ),
            (
                MAX_SINGLE_ARTIFACT_CANONICAL_BYTES,
                largest,
                Box::new(|limits, value| {
                    limits.max_single_artifact_canonical_bytes = bound(value);
                }),
            ),
            (
                MAX_TOTAL_ARTIFACT_CANONICAL_BYTES,
                total,
                Box::new(|limits, value| {
                    limits.max_total_artifact_canonical_bytes = bound(value);
                }),
            ),
        ];
        for (id, exact, edit) in cases {
            let mut at = Inputs::new();
            at.capability = capability(|limits| edit(limits, exact));
            let (evaluation, admitted) = at.run();
            assert_eq!(issues(&evaluation), [], "{}", id.name);
            assert!(admitted.is_ok(), "{}", id.name);
            if exact == 1 {
                continue;
            }
            let mut over = Inputs::new();
            over.capability = capability(|limits| edit(limits, exact - 1));
            let (evaluation, admitted) = over.run();
            let found: Vec<IssueKey> = evaluation.issues().cloned().collect();
            assert_eq!(found.len(), 1, "{}", id.name);
            assert_eq!(found[0].code(), Code::ResourceLimitExceeded);
            assert_eq!(
                evaluation.limit(&found[0]),
                Some(LimitExceeded {
                    bound: id,
                    limit: bound(exact - 1),
                    actual: Actual::Exact(exact),
                }),
                "{}",
                id.name
            );
            // Nothing past an over-bound collection is decided or compared.
            assert_eq!(
                waits_on(&evaluation, &decision()),
                [found[0].clone()],
                "{}",
                id.name
            );
            assert!(matches!(
                evaluation.status(&comparison()),
                Some(Status::NotEvaluated(_))
            ));
            assert!(admitted.is_err());
        }
    }

    fn reference_unit(field: ReferenceField) -> Unit {
        unit(PolicySubject::Reference(field), Check::WithinCapability)
    }

    #[test]
    fn every_present_reference_of_the_declaration_is_counted_once() {
        // Six policy references (the explicit `null` is none) and two targets.
        let inputs = Inputs::new();
        let policies = policies();
        let targets = targets();
        let bytes: u64 = present(&policies)
            .map(|(_, reference)| reference.canonical_bytes())
            .chain(
                targets
                    .values()
                    .map(|target| target.profile.canonical_bytes()),
            )
            .sum();
        let mut counted = Inputs::new();
        counted.capability = capability(|limits| {
            limits.max_artifact_references = bound(7);
            limits.max_total_reference_canonical_bytes = bound(bytes - 1);
        });
        let (evaluation, admitted) = counted.run();
        let count = key_of(
            &evaluation,
            &unit(PolicySubject::References, Check::WithinCapability),
        );
        assert_eq!(evaluation.limit(&count).unwrap().actual, Actual::Exact(8));
        let total = key_of(
            &evaluation,
            &unit(PolicySubject::ReferenceBytes, Check::WithinCapability),
        );
        assert_eq!(
            evaluation.limit(&total).unwrap().actual,
            Actual::Exact(bytes)
        );
        // The declaration's counts gate what resolves the other references,
        // not the decision for the selected one.
        assert!(admitted.is_ok());
        assert_eq!(evaluation.status(&decision()), Some(&Status::Evaluated));
        // At their exact values both hold.
        let mut exact = inputs;
        exact.capability = capability(|limits| {
            limits.max_artifact_references = bound(8);
            limits.max_total_reference_canonical_bytes = bound(bytes);
        });
        let (evaluation, _) = exact.run();
        assert_eq!(issues(&evaluation), []);
    }

    #[test]
    fn each_reference_is_checked_alone_and_the_selected_ones_bytes_gate_the_decision() {
        let selected = reference_to(&chosen()).canonical_bytes();
        let largest = present(&policies())
            .map(|(_, reference)| reference.canonical_bytes())
            .chain(
                targets()
                    .values()
                    .map(|target| target.profile.canonical_bytes()),
            )
            .max()
            .unwrap();
        assert!(selected <= largest);
        // A bound below only the larger references blocks them, not the
        // decision.
        let mut others = Inputs::new();
        others.capability = capability(|limits| {
            limits.max_single_reference_canonical_bytes = bound(selected);
        });
        let (evaluation, admitted) = others.run();
        let over: Vec<Unit> = issues(&evaluation)
            .into_iter()
            .map(|(unit, _, _)| unit)
            .collect();
        assert!(!over.is_empty());
        assert!(!over.contains(&reference_unit(ReferenceField::Policy(
            PolicySlot::ResourceLimits
        ))));
        assert!(admitted.is_ok());
        // One below the selected reference's own bytes stops the decision.
        let mut own = Inputs::new();
        own.capability = capability(|limits| {
            limits.max_single_reference_canonical_bytes = bound(selected - 1);
        });
        let (evaluation, admitted) = own.run();
        let key = key_of(
            &evaluation,
            &reference_unit(ReferenceField::Policy(PolicySlot::ResourceLimits)),
        );
        assert_eq!(
            evaluation.limit(&key),
            Some(LimitExceeded {
                bound: MAX_SINGLE_REFERENCE_CANONICAL_BYTES,
                limit: bound(selected - 1),
                actual: Actual::Exact(selected),
            })
        );
        assert!(waits_on(&evaluation, &decision()).contains(&key));
        assert!(admitted.is_err());
    }

    #[test]
    fn a_reference_field_structural_admission_did_not_admit_is_counted_only_past_the_bound() {
        let unavailable = stand_in(Slot::Coverage);
        let mut inputs = Inputs::new();
        inputs.policies = Err(unavailable.clone());
        let (evaluation, admitted) = inputs.run();
        // The two targets alone are within the bounds, so nothing is proved.
        for subject in [PolicySubject::References, PolicySubject::ReferenceBytes] {
            let cause: Vec<IssueKey> = unavailable.keys().cloned().collect();
            assert_eq!(
                waits_on(&evaluation, &unit(subject, Check::WithinCapability)),
                cause
            );
        }
        // The selected reference is still checked from what selection
        // admitted, and the other policy slots have no unit.
        assert_eq!(
            evaluation.status(&reference_unit(ReferenceField::Policy(
                PolicySlot::ResourceLimits
            ))),
            Some(&Status::Evaluated)
        );
        assert_eq!(
            evaluation.status(&reference_unit(ReferenceField::Policy(PolicySlot::Trust))),
            None
        );
        assert!(admitted.is_ok());
        // When the visible targets already exceed the bound, the excess is
        // proved with the smallest witness.
        inputs.capability = capability(|limits| limits.max_artifact_references = bound(1));
        let (evaluation, _) = inputs.run();
        let key = key_of(
            &evaluation,
            &unit(PolicySubject::References, Check::WithinCapability),
        );
        assert_eq!(evaluation.limit(&key).unwrap().actual, Actual::AtLeast(2));
    }

    #[test]
    fn without_the_selected_reference_only_the_collection_and_the_visible_references_are_checked() {
        let unselected = stand_in(Slot::Coverage);
        let mut inputs = Inputs::new();
        inputs.resource_limits = Err(unselected.clone());
        let (evaluation, admitted) = inputs.run();
        let causes: Vec<IssueKey> = unselected.keys().cloned().collect();
        assert_eq!(waits_on(&evaluation, &decision()), causes);
        assert_eq!(waits_on(&evaluation, &comparison()), causes);
        assert_eq!(
            waits_on(
                &evaluation,
                &reference_unit(ReferenceField::Policy(PolicySlot::ResourceLimits))
            ),
            causes
        );
        for subject in [PolicySubject::Artifacts, PolicySubject::ArtifactBytes] {
            assert_eq!(
                evaluation.status(&unit(subject, Check::WithinCapability)),
                Some(&Status::Evaluated)
            );
        }
        assert!(admitted.is_err());
    }

    #[test]
    fn the_decision_records_what_blocks_the_policy_and_nothing_is_compared() {
        // No submitted artifact is the selected policy.
        let mut missing = Inputs::new();
        missing.members.remove(0);
        let (evaluation, admitted) = missing.run();
        let key = key_of(&evaluation, &decision());
        assert_eq!(
            (key.code(), key.reason()),
            (Code::ArtifactReferenceNotAdmitted, Some(Reason::NotFound))
        );
        assert_eq!(waits_on(&evaluation, &comparison()), [key]);
        assert!(admitted.is_err());
        // A second occurrence of the selected policy is a duplicate.
        let mut repeated = Inputs::new();
        repeated.members.push(chosen());
        let (evaluation, _) = repeated.run();
        let key = key_of(&evaluation, &decision());
        assert_eq!(
            (key.code(), key.reason()),
            (
                Code::ArtifactCollectionNotAdmitted,
                Some(Reason::DuplicateEnvelope)
            )
        );
        // Equal occurrences share one artifact unit; both count toward the
        // total.
        let artifacts = evaluation
            .statuses()
            .filter(|(unit, _)| {
                matches!(
                    unit.subject,
                    Subject::ResourcePolicy(PolicySubject::Artifact(_))
                )
            })
            .count();
        assert_eq!(artifacts, 5);
        // Every occurrence still counts toward the occurrences and the bytes.
        let mut counted = Inputs::new();
        counted.members.push(chosen());
        counted.capability = capability(|limits| {
            limits.max_artifacts = bound(1);
            limits.max_total_artifact_canonical_bytes = bound(1);
        });
        let (evaluation, _) = counted.run();
        let occurrences = key_of(
            &evaluation,
            &unit(PolicySubject::Artifacts, Check::WithinCapability),
        );
        assert_eq!(
            evaluation.limit(&occurrences).unwrap().actual,
            Actual::Exact(6)
        );
        let bytes: u64 = collection(&counted.members)
            .members()
            .iter()
            .map(FormalMember::canonical_bytes)
            .sum();
        let total = key_of(
            &evaluation,
            &unit(PolicySubject::ArtifactBytes, Check::WithinCapability),
        );
        assert_eq!(
            evaluation.limit(&total).unwrap().actual,
            Actual::Exact(bytes)
        );
    }

    #[test]
    fn the_first_bound_in_canonical_order_the_capability_lacks_makes_the_policy_incompatible() {
        // The capability offers exactly what the policy needs, and room for
        // the collection and references the bounds count before admission.
        let mut required = policy_limits();
        required.artifact_admission = fixture_limits(10_000).artifact_admission;
        let mut exact = Inputs::new();
        exact.capability = ImplementationCapability::fixture_owned(required.clone());
        let (evaluation, admitted) = exact.run();
        assert_eq!(issues(&evaluation), []);
        assert!(admitted.is_ok());
        // One short on two bounds: the first in canonical order is reported.
        let mut short = required.clone();
        short.locale_resolution.max_requested_locales = bound(10);
        short.configuration_input.value.max_depth = bound(1);
        let mut lacking = Inputs::new();
        lacking.capability = ImplementationCapability::fixture_owned(short);
        let (evaluation, admitted) = lacking.run();
        let key = key_of(&evaluation, &comparison());
        assert_eq!(
            (key.code(), key.reason()),
            (Code::ResourcePolicyCapabilityIncompatible, None)
        );
        assert_eq!(
            evaluation.shortfall(&key),
            Some(Shortfall {
                bound: BoundId {
                    group: Group::ConfigurationInput,
                    name: "maxDepth",
                },
                required: bound(2),
                available: bound(1),
            })
        );
        assert_eq!(evaluation.limit(&key), None);
        assert_eq!(evaluation.status(&decision()), Some(&Status::Evaluated));
        assert!(admitted.is_err());
    }

    #[test]
    fn policy_slots_and_reference_fields_follow_the_declarations_name_order() {
        let slots = [
            PolicySlot::Approval,
            PolicySlot::GlossarySet,
            PolicySlot::ProviderRouting,
            PolicySlot::ResourceLimits,
            PolicySlot::Selection,
            PolicySlot::SourceAdmission,
            PolicySlot::Trust,
        ];
        assert!(slots.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(slots
            .windows(2)
            .all(|pair| pair[0].name().as_bytes() < pair[1].name().as_bytes()));
        // `policies` comes before `targetProfiles`.
        let target = ReferenceField::TargetProfile(serde_json::from_value(json!("a")).unwrap());
        assert!(ReferenceField::Policy(PolicySlot::Trust) < target);
        // Present references in slot order, without the explicit null.
        assert_eq!(
            present(&policies())
                .map(|(slot, _)| slot)
                .collect::<Vec<_>>(),
            [
                PolicySlot::Approval,
                PolicySlot::GlossarySet,
                PolicySlot::ResourceLimits,
                PolicySlot::Selection,
                PolicySlot::SourceAdmission,
                PolicySlot::Trust,
            ]
        );
    }
}
