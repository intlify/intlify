// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's Phase 4, `resource-policy-admission`. Before any policy is admitted,
//! the capability's `policyComparable` `artifactAdmission` bounds are the sole
//! authority over the submitted collection and the selected declaration's
//! references. The Bootstrap Resource Policy Candidate Admission then decides
//! the selected `resourceLimits` reference, and the admitted policy must need
//! no bound past the capability's value. That policy becomes the final
//! selector-projection authority, its `configurationInput` and
//! `artifactAdmission` bounds are reapplied to what the earlier phases and the
//! bounds above measured, and only when every recheck holds is it confirmed,
//! which confirms the provisional selection. Recounting the diagnostics under
//! it waits for 015 Implementation Phase 5.

use std::collections::BTreeSet;

use crate::artifacts::collection::FormalCollection;
use crate::artifacts::envelope::{Declaration, FormalMember};
use crate::input_limits::{Bound, ValueCounts};
use crate::model::{NonEmptyMap, Policies, RequiredNullable, TargetDeclaration, TargetId};
use crate::references::{PolicyReference, TargetProfileReference};
use crate::structural::selection::{SelectionInvariant, SelectorInput, StringLengthFact};

use super::bootstrap::{admit_candidate, AdmittedResourcePolicy, ResourcePolicyVerification};
use super::evaluation::{
    and, both, Actual, Admitted, Blocking, Causes, Check, Evaluation, EvaluationInvariant, Phase,
    Shortfall, Subject, Unit,
};
use super::limits::{
    ArtifactAdmissionLimits, BoundId, ConfigurationInputLimits, ConfirmedPolicy, Group,
    ImplementationCapability, ResolutionLimits,
};
use super::tally::{actual, measure, total, within, Tally};

const fn configuration_bound(name: &'static str) -> BoundId {
    BoundId {
        group: Group::ConfigurationInput,
        name,
    }
}

const MAX_NODES: BoundId = configuration_bound("maxNodes");
const MAX_DEPTH: BoundId = configuration_bound("maxDepth");
const MAX_COLLECTION_ENTRIES: BoundId = configuration_bound("maxCollectionEntries");
const MAX_TOTAL_STRING_BYTES: BoundId = configuration_bound("maxTotalStringBytes");
const MAX_SINGLE_STRING_BYTES: BoundId = configuration_bound("maxSingleStringBytes");
const MAX_PROFILES: BoundId = configuration_bound("maxProfiles");
const MAX_PROFILE_ID_BYTES: BoundId = configuration_bound("maxProfileIdBytes");
const MAX_STRUCTURAL_ANALYSIS_UNITS: BoundId = configuration_bound("maxStructuralAnalysisUnits");

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

/// What one Phase 4 unit checks: a bound of 015's `configurationInput` table,
/// then of its `artifactAdmission` table, each in the table's declaration
/// order and over the table's scope, then the selected reference.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PolicySubject {
    /// The materialized root's logical values, under `maxNodes`.
    Nodes,
    /// The materialized root's deepest value level, under `maxDepth`.
    Depth,
    /// The materialized root's object members and array elements, under
    /// `maxCollectionEntries`.
    CollectionEntries,
    /// The bytes of the materialized root's string values and object keys,
    /// under `maxTotalStringBytes`.
    StringBytes,
    /// The materialized root's longest string value or object key, under
    /// `maxSingleStringBytes`: when it is within the bound, so is every other.
    LongestString,
    /// The root `profiles` object's declarations, under `maxProfiles`.
    Profiles,
    /// One declared Profile ID, by its place in the IDs' unsigned byte order,
    /// under `maxProfileIdBytes`.
    ProfileId(usize),
    /// A string External Profile Selector Input, under `maxProfileIdBytes`.
    Selector,
    /// The completed structural analysis, under `maxStructuralAnalysisUnits`.
    StructuralAnalysis,
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

/// What Phases 1 to 3 measured of the configuration and the selector under
/// the capability, which the admitted policy rechecks. Each part is admitted
/// or unavailable with the causes behind it.
pub(crate) struct ConfigurationMeasures<'a> {
    /// The materialized root's value counts.
    pub(crate) value: Admitted<ValueCounts>,
    /// The UTF-8 bytes of every declared Profile ID, in the IDs' unsigned
    /// byte order.
    pub(crate) profile_ids: Admitted<&'a [u64]>,
    /// The normalized External Profile Selector Input.
    pub(crate) selector: &'a SelectorInput,
    /// The units the completed structural analysis evaluated.
    pub(crate) structural_units: Admitted<u64>,
}

/// What Phase 4 settles for the phases after it.
#[derive(Debug)]
pub(crate) struct PolicyOutcome {
    /// The selector's String Length Fact under the final projection
    /// authority, or none for an absent or non-string selector.
    pub(crate) selector_length: Option<StringLengthFact>,
    /// The confirmed policy, which also confirms the provisional selection,
    /// or why there is none.
    pub(crate) confirmed: Admitted<ConfirmedPolicy>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PolicyInvariant {
    Evaluation(EvaluationInvariant),
    /// The selector was observed under a bound below the final projection
    /// authority, which never exceeds the capability's: a resolver defect.
    Selection(SelectionInvariant),
}

impl From<EvaluationInvariant> for PolicyInvariant {
    fn from(invariant: EvaluationInvariant) -> Self {
        Self::Evaluation(invariant)
    }
}

/// That every recheck under the admitted policy held. Only Phase 4 states it,
/// and only a confirmed policy takes it.
pub(crate) struct Rechecked(());

/// The unit Phase 4 records for `check` over `subject`.
pub(crate) const fn unit(subject: PolicySubject, check: Check) -> Unit {
    Unit {
        phase: Phase::ResourcePolicyAdmission,
        subject: Subject::ResourcePolicy(subject),
        check,
    }
}

/// Phase 4. The bounds over the collection need nothing earlier, so they are
/// checked whatever the selection; the reference bounds need the selected
/// declaration's fields; the decision needs the `resourceLimits` reference,
/// every collection bound, and that reference's own bytes; the comparison
/// needs the admitted policy; and each recheck needs the compatible policy and
/// what it measures.
pub(crate) fn admit_resource_policy(
    evaluation: &mut Evaluation,
    capability: &ImplementationCapability,
    collection: &FormalCollection,
    resource_limits: Result<&PolicyReference, &Causes>,
    references: &DeclarationReferences<'_>,
    measures: &ConfigurationMeasures<'_>,
    verification: &impl ResourcePolicyVerification,
) -> Result<PolicyOutcome, PolicyInvariant> {
    let compatible = admit_and_compare(
        evaluation,
        capability,
        collection,
        resource_limits,
        references,
        verification,
    )?;
    // An admitted, compatible policy is the projection authority whatever its
    // rechecks find; without one, the capability stays the authority.
    let authority = compatible
        .as_ref()
        .map_or(
            capability.policy_comparable(),
            AdmittedResourcePolicy::limits,
        )
        .configuration_input
        .structural
        .max_profile_id_bytes;
    let selector_length = measures
        .selector
        .length_fact(authority)
        .map_err(PolicyInvariant::Selection)?;
    let limits = compatible.as_ref().map(AdmittedResourcePolicy::limits);
    let configured = configuration_bounds(
        evaluation,
        limits.map(|limits| &limits.configuration_input),
        measures,
        selector_length,
    )?;
    let artifact_limits = limits.map(|limits| &limits.artifact_admission);
    let inspected = collection_bounds(
        evaluation,
        Check::WithinBound,
        artifact_limits,
        collection.members(),
    )?;
    let (_, referenced) = reference_bounds(
        evaluation,
        Check::WithinBound,
        artifact_limits,
        resource_limits,
        references,
    )?;
    let rechecked = and(configured, and(inspected, referenced));
    Ok(PolicyOutcome {
        selector_length,
        confirmed: both(compatible, rechecked)
            .map(|(policy, ())| ConfirmedPolicy::confirmed(&policy, Rechecked(()))),
    })
}

/// Phase 4 up to the comparison with the capability: the collection and
/// reference bounds under the capability's values, the decision, and the
/// comparison.
fn admit_and_compare(
    evaluation: &mut Evaluation,
    capability: &ImplementationCapability,
    collection: &FormalCollection,
    resource_limits: Result<&PolicyReference, &Causes>,
    references: &DeclarationReferences<'_>,
    verification: &impl ResourcePolicyVerification,
) -> Result<Admitted<AdmittedResourcePolicy>, EvaluationInvariant> {
    let limits: Result<_, &Causes> = Ok(&capability.policy_comparable().artifact_admission);
    let members = collection.members();
    let inspected = collection_bounds(evaluation, Check::WithinCapability, limits, members)?;
    let (own_bytes, _) = reference_bounds(
        evaluation,
        Check::WithinCapability,
        limits,
        resource_limits,
        references,
    )?;
    let decided = evaluation.check(
        unit(PolicySubject::ResourceLimits, Check::BootstrapCandidate),
        both(
            both(resource_limits.map_err(Causes::clone), inspected),
            own_bytes,
        )
        .map(|((reference, ()), ())| reference),
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

/// One bound's unit, not evaluated while the bound's value or the count is
/// unavailable.
fn bounded(
    evaluation: &mut Evaluation,
    unit: Unit,
    bound: BoundId,
    limit: Result<Bound, &Causes>,
    tally: Result<Tally, &Causes>,
) -> Result<Admitted<()>, EvaluationInvariant> {
    evaluation.check(
        unit,
        both(limit.map_err(Causes::clone), tally.map_err(Causes::clone)),
        |(limit, tally)| within(bound, limit, actual(tally, limit)),
    )
}

/// One bound's unit over a count part of which may be unavailable: proved
/// only once what is visible exceeds the bound.
fn measured(
    evaluation: &mut Evaluation,
    unit: Unit,
    bound: BoundId,
    limit: Result<Bound, &Causes>,
    visible: Tally,
    unavailable: Option<&Causes>,
) -> Result<Admitted<()>, EvaluationInvariant> {
    let measured = match limit {
        Ok(limit) => measure(visible, unavailable.cloned(), limit).map(|actual| (limit, actual)),
        Err(causes) => Err(join(unavailable.cloned(), causes)),
    };
    evaluation.check(unit, measured, |(limit, actual)| {
        within(bound, limit, actual)
    })
}

/// The eight `configurationInput` bounds over what Phases 1 to 3 measured,
/// under the policy's values: the root's five as Phase 1 summarizes them, the
/// declarations, each declared Profile ID and then a string selector, and the
/// structural analysis. Whether all of them hold.
fn configuration_bounds(
    evaluation: &mut Evaluation,
    limits: Result<&ConfigurationInputLimits, &Causes>,
    measures: &ConfigurationMeasures<'_>,
    selector_length: Option<StringLengthFact>,
) -> Result<Admitted<()>, EvaluationInvariant> {
    let recheck = |subject| unit(subject, Check::WithinBound);
    let value = limits.map(|limits| &limits.value);
    let structural = limits.map(|limits| &limits.structural);
    let counts = measures.value.as_ref();
    let mut holds: Admitted<()> = Ok(());
    for (subject, bound, limit, count) in [
        (
            PolicySubject::Nodes,
            MAX_NODES,
            value.map(|limits| limits.max_nodes),
            counts.map(|counts| counts.nodes),
        ),
        (
            PolicySubject::Depth,
            MAX_DEPTH,
            value.map(|limits| limits.max_depth),
            counts.map(|counts| counts.depth),
        ),
        (
            PolicySubject::CollectionEntries,
            MAX_COLLECTION_ENTRIES,
            value.map(|limits| limits.max_collection_entries),
            counts.map(|counts| counts.collection_entries),
        ),
        (
            PolicySubject::StringBytes,
            MAX_TOTAL_STRING_BYTES,
            value.map(|limits| limits.max_total_string_bytes),
            counts.map(|counts| counts.total_string_bytes),
        ),
        (
            PolicySubject::LongestString,
            MAX_SINGLE_STRING_BYTES,
            value.map(|limits| limits.max_single_string_bytes),
            counts.map(|counts| counts.single_string_bytes),
        ),
    ] {
        let held = bounded(
            evaluation,
            recheck(subject),
            bound,
            limit,
            count.map(Tally::exactly),
        )?;
        holds = and(holds, held);
    }
    let ids = measures.profile_ids.as_ref();
    let held = bounded(
        evaluation,
        recheck(PolicySubject::Profiles),
        MAX_PROFILES,
        structural.map(|limits| limits.max_profiles),
        ids.map(|ids| Tally::of(ids.len())),
    )?;
    holds = and(holds, held);
    let id_limit = structural.map(|limits| limits.max_profile_id_bytes);
    if let Ok(ids) = ids {
        for (ordinal, &bytes) in ids.iter().enumerate() {
            let held = bounded(
                evaluation,
                recheck(PolicySubject::ProfileId(ordinal)),
                MAX_PROFILE_ID_BYTES,
                id_limit,
                Ok(Tally::exactly(bytes)),
            )?;
            holds = and(holds, held);
        }
    }
    // An absent selector and a non-string type carry no ID bytes.
    if let Some(length) = selector_length {
        let held = evaluation.check(
            recheck(PolicySubject::Selector),
            id_limit.map_err(Causes::clone),
            |limit| {
                let actual = match length {
                    StringLengthFact::Exact(bytes) => Actual::Exact(bytes),
                    // An over-limit selector keeps no length, only the
                    // smallest first-over witness.
                    StringLengthFact::OverLimit => Actual::AtLeast(limit.get() + 1),
                };
                within(MAX_PROFILE_ID_BYTES, limit, actual)
            },
        )?;
        holds = and(holds, held);
    }
    let held = bounded(
        evaluation,
        recheck(PolicySubject::StructuralAnalysis),
        MAX_STRUCTURAL_ANALYSIS_UNITS,
        structural.map(|limits| limits.max_structural_analysis_units),
        measures
            .structural_units
            .as_ref()
            .map(|&units| Tally::exactly(units)),
    )?;
    Ok(and(holds, held))
}

/// The five bounds over the collection, one unit each but one per distinct
/// canonical body, and whether all of them hold.
fn collection_bounds(
    evaluation: &mut Evaluation,
    check: Check,
    limits: Result<&ArtifactAdmissionLimits, &Causes>,
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
        unit(PolicySubject::Artifacts, check),
        MAX_ARTIFACTS,
        limits.map(|limits| limits.max_artifacts),
        Ok(Tally::of(members.len())),
    )?;
    for (subject, bound, limit, count) in [
        (
            PolicySubject::PolicyArtifacts,
            MAX_POLICY_ARTIFACTS,
            limits.map(|limits| limits.max_policy_artifacts),
            declared(true),
        ),
        (
            PolicySubject::TargetProfileArtifacts,
            MAX_TARGET_PROFILE_ARTIFACTS,
            limits.map(|limits| limits.max_target_profile_artifacts),
            declared(false),
        ),
    ] {
        let held = bounded(
            evaluation,
            unit(subject, check),
            bound,
            limit,
            Ok(Tally::of(count)),
        )?;
        holds = and(holds, held);
    }
    // Equal occurrences have one canonical body and commute, so they share
    // its unit; every occurrence still counts toward the total.
    let bodies: BTreeSet<&[u8]> = members.iter().map(FormalMember::canonical_body).collect();
    for (ordinal, body) in bodies.into_iter().enumerate() {
        let held = bounded(
            evaluation,
            unit(PolicySubject::Artifact(ordinal), check),
            MAX_SINGLE_ARTIFACT_CANONICAL_BYTES,
            limits.map(|limits| limits.max_single_artifact_canonical_bytes),
            Ok(Tally::of(body.len())),
        )?;
        holds = and(holds, held);
    }
    let held = bounded(
        evaluation,
        unit(PolicySubject::ArtifactBytes, check),
        MAX_TOTAL_ARTIFACT_CANONICAL_BYTES,
        limits.map(|limits| limits.max_total_artifact_canonical_bytes),
        Ok(total(
            members
                .iter()
                .map(|member| Tally::exactly(member.canonical_bytes())),
        )),
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

/// The three bounds over the selected declaration's references: whether the
/// `resourceLimits` reference's own bytes hold, which the decision needs, and
/// whether every unit does, which the confirmation needs. A field structural
/// admission did not admit has no references to count: a count is proved only
/// when what is visible already exceeds the bound.
fn reference_bounds(
    evaluation: &mut Evaluation,
    check: Check,
    limits: Result<&ArtifactAdmissionLimits, &Causes>,
    resource_limits: Result<&PolicyReference, &Causes>,
    references: &DeclarationReferences<'_>,
) -> Result<(Admitted<()>, Admitted<()>), EvaluationInvariant> {
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
    // The declaration's count and total never gate the decision for the
    // selected reference.
    let count = measured(
        evaluation,
        unit(PolicySubject::References, check),
        MAX_ARTIFACT_REFERENCES,
        limits.map(|limits| limits.max_artifact_references),
        Tally::of(visible.len()),
        unavailable.as_ref(),
    )?;
    // The selected reference is checked from what selection admitted, even
    // when the rest of `policies` was not.
    let single_limit = limits.map(|limits| limits.max_single_reference_canonical_bytes);
    let resource_limits_slot = ReferenceField::Policy(PolicySlot::ResourceLimits);
    let own_bytes = bounded(
        evaluation,
        unit(
            PolicySubject::Reference(resource_limits_slot.clone()),
            check,
        ),
        MAX_SINGLE_REFERENCE_CANONICAL_BYTES,
        single_limit,
        resource_limits.map(|reference| Tally::exactly(reference.canonical_bytes())),
    )?;
    let mut holds = and(count, own_bytes.clone());
    for (field, bytes) in &visible {
        if *field == resource_limits_slot {
            continue;
        }
        let held = bounded(
            evaluation,
            unit(PolicySubject::Reference(field.clone()), check),
            MAX_SINGLE_REFERENCE_CANONICAL_BYTES,
            single_limit,
            Ok(Tally::exactly(*bytes)),
        )?;
        holds = and(holds, held);
    }
    let bytes = measured(
        evaluation,
        unit(PolicySubject::ReferenceBytes, check),
        MAX_TOTAL_REFERENCE_CANONICAL_BYTES,
        limits.map(|limits| limits.max_total_reference_canonical_bytes),
        total(visible.iter().map(|&(_, bytes)| Tally::exactly(bytes))),
        unavailable.as_ref(),
    )?;
    Ok((own_bytes, and(holds, bytes)))
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
    use crate::structural::selection::InvalidSelectorType;

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
        chosen_under(&policy_limits())
    }

    /// The selected Resource Limit Policy, setting `limits`.
    fn chosen_under(limits: &ResolutionLimits) -> Value {
        envelope_of(
            ArtifactKind::ResourceLimitPolicy,
            "storefront-limits",
            "1",
            json!({ SECTION: limits.section() }),
        )
    }

    /// A policy within the roomy capability that every recheck of the
    /// default inputs holds under.
    fn ample() -> ResolutionLimits {
        fixture_limits(5_000)
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
        policies_for(&chosen())
    }

    /// The declaration's `policies`, selecting `policy`.
    fn policies_for(policy: &Value) -> Policies<PolicyReference> {
        serde_json::from_value(json!({
            "resourceLimits": reference_value(&tuple_of(policy)),
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

    /// The `maxProfileIdBytes` value Phase 3 observed the selector under.
    fn observed(capability: &ImplementationCapability) -> Bound {
        capability
            .policy_comparable()
            .configuration_input
            .structural
            .max_profile_id_bytes
    }

    /// What Phases 1 to 3 measured, as a test states it.
    struct Measures {
        value: Admitted<ValueCounts>,
        profile_ids: Admitted<Vec<u64>>,
        selector: SelectorInput,
        structural_units: Admitted<u64>,
    }

    impl Measures {
        /// Two declarations, `app` and `storefront`, and the selector `app`.
        fn new(capability: &ImplementationCapability) -> Self {
            Self {
                value: Ok(ValueCounts {
                    nodes: 40,
                    depth: 5,
                    collection_entries: 39,
                    total_string_bytes: 400,
                    single_string_bytes: 30,
                }),
                profile_ids: Ok(vec![3, 10]),
                selector: SelectorInput::string("app", observed(capability)),
                structural_units: Ok(120),
            }
        }
    }

    /// Everything Phase 4 reads, which a test edits before running it.
    struct Inputs {
        capability: ImplementationCapability,
        members: Vec<Value>,
        resource_limits: Admitted<PolicyReference>,
        policies: Admitted<Policies<PolicyReference>>,
        targets: Admitted<NonEmptyMap<TargetId, TargetDeclaration<TargetProfileReference>>>,
        measures: Measures,
    }

    /// The default inputs selecting the ample policy edited by `edit`.
    fn under(edit: impl FnOnce(&mut ResolutionLimits)) -> Inputs {
        let mut limits = ample();
        edit(&mut limits);
        Inputs::selecting(&chosen_under(&limits))
    }

    impl Inputs {
        fn new() -> Self {
            Self::selecting(&chosen())
        }

        /// The default inputs with `policy` as the selected Resource Limit
        /// Policy.
        fn selecting(policy: &Value) -> Self {
            let capability = roomy();
            Self {
                measures: Measures::new(&capability),
                capability,
                members: vec![
                    policy.clone(),
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
                resource_limits: Ok(reference_to(policy)),
                policies: Ok(policies_for(policy)),
                targets: Ok(targets()),
            }
        }

        fn references(&self) -> DeclarationReferences<'_> {
            DeclarationReferences {
                policies: self.policies.as_ref().map_err(Causes::clone),
                target_profiles: self.targets.as_ref().map_err(Causes::clone),
            }
        }

        /// Phase 4 up to the comparison.
        fn run(&self) -> (Evaluation, Admitted<AdmittedResourcePolicy>) {
            let mut evaluation = Evaluation::default();
            let admitted = admit_and_compare(
                &mut evaluation,
                &self.capability,
                &collection(&self.members),
                self.resource_limits.as_ref(),
                &self.references(),
                &Trusting,
            )
            .unwrap();
            (evaluation, admitted)
        }

        /// The whole of Phase 4.
        fn attempt(&self) -> (Evaluation, Result<PolicyOutcome, PolicyInvariant>) {
            let mut evaluation = Evaluation::default();
            let measures = ConfigurationMeasures {
                value: self.measures.value.clone(),
                profile_ids: self.measures.profile_ids.as_deref().map_err(Causes::clone),
                selector: &self.measures.selector,
                structural_units: self.measures.structural_units.clone(),
            };
            let outcome = admit_resource_policy(
                &mut evaluation,
                &self.capability,
                &collection(&self.members),
                self.resource_limits.as_ref(),
                &self.references(),
                &measures,
                &Trusting,
            );
            (evaluation, outcome)
        }

        fn confirm(&self) -> (Evaluation, PolicyOutcome) {
            let (evaluation, outcome) = self.attempt();
            (evaluation, outcome.unwrap())
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

    fn recheck(subject: PolicySubject) -> Unit {
        unit(subject, Check::WithinBound)
    }

    fn keys(causes: &Causes) -> Vec<IssueKey> {
        causes.keys().cloned().collect()
    }

    /// The subjects of the recorded rechecks, in the order they are recorded.
    fn rechecked(evaluation: &Evaluation) -> Vec<PolicySubject> {
        evaluation
            .statuses()
            .filter(|(unit, _)| unit.check == Check::WithinBound)
            .map(|(unit, _)| match &unit.subject {
                Subject::ResourcePolicy(subject) => subject.clone(),
                other => panic!("{other:?}"),
            })
            .collect()
    }

    /// The subjects of the declaration's references, in their logical order.
    fn reference_subjects() -> Vec<PolicySubject> {
        let target =
            |id: &str| ReferenceField::TargetProfile(serde_json::from_value(json!(id)).unwrap());
        present(&policies())
            .map(|(slot, _)| ReferenceField::Policy(slot))
            .chain([target("app"), target("web")])
            .map(PolicySubject::Reference)
            .collect()
    }

    #[test]
    fn a_compatible_policy_every_recheck_holds_under_confirms_the_selection() {
        let (evaluation, outcome) = under(|_| {}).confirm();
        assert_eq!(issues(&evaluation), []);
        assert_eq!(outcome.selector_length, Some(StringLengthFact::Exact(3)));
        assert_eq!(
            outcome.confirmed.unwrap().limits().bounds(),
            ample().bounds()
        );
        // The configuration's bounds in 015's order, each declared ID before
        // the selector, then the collection's and the references' again.
        let mut expected = vec![
            PolicySubject::Nodes,
            PolicySubject::Depth,
            PolicySubject::CollectionEntries,
            PolicySubject::StringBytes,
            PolicySubject::LongestString,
            PolicySubject::Profiles,
            PolicySubject::ProfileId(0),
            PolicySubject::ProfileId(1),
            PolicySubject::Selector,
            PolicySubject::StructuralAnalysis,
            PolicySubject::Artifacts,
            PolicySubject::PolicyArtifacts,
            PolicySubject::TargetProfileArtifacts,
            PolicySubject::References,
        ];
        expected.extend((0..5).map(PolicySubject::Artifact));
        expected.push(PolicySubject::ArtifactBytes);
        expected.extend(reference_subjects());
        expected.push(PolicySubject::ReferenceBytes);
        assert_eq!(rechecked(&evaluation), expected);
        // Every recheck was evaluated, and each artifact and reference bound
        // also keeps its unit under the capability.
        assert!(evaluation
            .statuses()
            .all(|(_, status)| *status == Status::Evaluated));
        let before = evaluation
            .statuses()
            .filter(|(unit, _)| unit.check == Check::WithinCapability)
            .count();
        assert_eq!(before, expected.len() - 10);
    }

    #[test]
    fn each_configuration_bound_admits_its_exact_value_and_reports_one_more() {
        type Edit = fn(&mut ResolutionLimits, Bound);
        let cases: [(PolicySubject, BoundId, u64, Edit); 8] = [
            (PolicySubject::Nodes, MAX_NODES, 40, |limits, value| {
                limits.configuration_input.value.max_nodes = value;
            }),
            (PolicySubject::Depth, MAX_DEPTH, 5, |limits, value| {
                limits.configuration_input.value.max_depth = value;
            }),
            (
                PolicySubject::CollectionEntries,
                MAX_COLLECTION_ENTRIES,
                39,
                |limits, value| limits.configuration_input.value.max_collection_entries = value,
            ),
            (
                PolicySubject::StringBytes,
                MAX_TOTAL_STRING_BYTES,
                400,
                |limits, value| limits.configuration_input.value.max_total_string_bytes = value,
            ),
            (
                PolicySubject::LongestString,
                MAX_SINGLE_STRING_BYTES,
                30,
                |limits, value| limits.configuration_input.value.max_single_string_bytes = value,
            ),
            (PolicySubject::Profiles, MAX_PROFILES, 2, |limits, value| {
                limits.configuration_input.structural.max_profiles = value;
            }),
            // The longer declared ID; the other and the selector are three
            // bytes.
            (
                PolicySubject::ProfileId(1),
                MAX_PROFILE_ID_BYTES,
                10,
                |limits, value| limits.configuration_input.structural.max_profile_id_bytes = value,
            ),
            (
                PolicySubject::StructuralAnalysis,
                MAX_STRUCTURAL_ANALYSIS_UNITS,
                120,
                |limits, value| {
                    limits
                        .configuration_input
                        .structural
                        .max_structural_analysis_units = value;
                },
            ),
        ];
        for (subject, id, exact, edit) in cases {
            let (evaluation, outcome) = under(|limits| edit(limits, bound(exact))).confirm();
            assert_eq!(issues(&evaluation), [], "{}", id.name);
            assert!(outcome.confirmed.is_ok(), "{}", id.name);
            let (evaluation, outcome) = under(|limits| edit(limits, bound(exact - 1))).confirm();
            let found: Vec<IssueKey> = evaluation.issues().cloned().collect();
            assert_eq!(found.len(), 1, "{}", id.name);
            assert_eq!(found[0].unit(), &recheck(subject), "{}", id.name);
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
            // The selection is not confirmed, and nothing else is chosen.
            assert_eq!(keys(&outcome.confirmed.unwrap_err()), found, "{}", id.name);
        }
    }

    #[test]
    fn the_selector_is_rechecked_under_the_policy_and_keeps_only_the_first_over_witness() {
        // 015's example: a 100-byte selector observed under a 200-byte
        // capability, and a 50-byte policy.
        let mut capacity = fixture_limits(10_000);
        capacity.configuration_input.structural.max_profile_id_bytes = bound(200);
        let run = |policy: u64, selector: SelectorInput| {
            let mut inputs = under(|limits| {
                limits.configuration_input.structural.max_profile_id_bytes = bound(policy);
            });
            inputs.capability = ImplementationCapability::fixture_owned(capacity.clone());
            inputs.measures.selector = selector;
            inputs.confirm()
        };
        let string = |bytes: usize| SelectorInput::string(&"p".repeat(bytes), bound(200));
        let selector = recheck(PolicySubject::Selector);
        let (evaluation, outcome) = run(50, string(100));
        let key = key_of(&evaluation, &selector);
        assert_eq!(
            evaluation.limit(&key),
            Some(LimitExceeded {
                bound: MAX_PROFILE_ID_BYTES,
                limit: bound(50),
                actual: Actual::AtLeast(51),
            })
        );
        assert_eq!(issues(&evaluation).len(), 1);
        assert_eq!(outcome.selector_length, Some(StringLengthFact::OverLimit));
        assert_eq!(keys(&outcome.confirmed.unwrap_err()), [key]);
        // At its exact length the selector holds.
        let (evaluation, outcome) = run(100, string(100));
        assert_eq!(issues(&evaluation), []);
        assert_eq!(outcome.selector_length, Some(StringLengthFact::Exact(100)));
        assert!(outcome.confirmed.is_ok());
        // Past the capability, the policy's own witness is reported.
        let (evaluation, outcome) = run(50, string(201));
        let key = key_of(&evaluation, &selector);
        assert_eq!(evaluation.limit(&key).unwrap().actual, Actual::AtLeast(51));
        assert_eq!(outcome.selector_length, Some(StringLengthFact::OverLimit));
        // An absent selector and a non-string type carry no ID bytes.
        for input in [
            SelectorInput::absent(bound(200)),
            SelectorInput::invalid_type(InvalidSelectorType::Number, bound(200)),
        ] {
            let (evaluation, outcome) = run(50, input);
            assert_eq!(evaluation.status(&selector), None);
            assert_eq!(outcome.selector_length, None);
            assert!(outcome.confirmed.is_ok());
        }
    }

    #[test]
    fn without_a_compatible_policy_nothing_is_rechecked_and_the_capability_stays_the_projection_authority(
    ) {
        let mut capacity = fixture_limits(10_000);
        capacity.configuration_input.structural.max_profile_id_bytes = bound(200);
        let selecting = |capacity: ResolutionLimits| {
            let mut inputs = under(|limits| {
                limits.configuration_input.structural.max_profile_id_bytes = bound(50);
            });
            inputs.capability = ImplementationCapability::fixture_owned(capacity);
            inputs.measures.selector = SelectorInput::string(&"p".repeat(100), bound(200));
            inputs
        };
        // The policy would put the selector over its bound, but it needs a
        // deeper root than the capability allows.
        let mut shallow = capacity.clone();
        shallow.configuration_input.value.max_depth = bound(4);
        let (evaluation, outcome) = selecting(shallow).confirm();
        let incompatible = key_of(&evaluation, &comparison());
        assert_eq!(
            evaluation.shortfall(&incompatible).unwrap().bound,
            MAX_DEPTH
        );
        assert_eq!(outcome.selector_length, Some(StringLengthFact::Exact(100)));
        let subjects = rechecked(&evaluation);
        assert_eq!(subjects.len(), 29);
        for subject in subjects {
            assert_eq!(
                waits_on(&evaluation, &recheck(subject)),
                std::slice::from_ref(&incompatible)
            );
        }
        assert_eq!(keys(&outcome.confirmed.unwrap_err()), [incompatible]);
        // Without an admitted policy at all, the capability is the authority
        // too.
        let mut missing = selecting(capacity);
        missing.members.remove(0);
        let (evaluation, outcome) = missing.confirm();
        let undecided = key_of(&evaluation, &decision());
        assert_eq!(outcome.selector_length, Some(StringLengthFact::Exact(100)));
        assert_eq!(
            waits_on(&evaluation, &recheck(PolicySubject::Selector)),
            std::slice::from_ref(&undecided)
        );
        assert_eq!(keys(&outcome.confirmed.unwrap_err()), [undecided]);
    }

    #[test]
    fn the_artifact_bounds_are_rechecked_under_the_policy_as_they_were_counted_before_it() {
        type Edit = fn(&mut ArtifactAdmissionLimits, Bound);
        // A second Target Profile, so every count is past one.
        let with_second = |edit: &dyn Fn(&mut ResolutionLimits)| {
            let mut inputs = under(edit);
            inputs.members.push(envelope_of(
                ArtifactKind::TargetProfile,
                "storefront-app",
                "1",
                json!({}),
            ));
            inputs
        };
        let references: Vec<u64> = present(&policies())
            .map(|(_, reference)| reference.canonical_bytes())
            .chain(
                targets()
                    .values()
                    .map(|target| target.profile.canonical_bytes()),
            )
            .collect();
        let largest_reference = references.iter().copied().max();
        let reference_total = Some(references.iter().sum());
        let cases: [(BoundId, Option<u64>, Edit); 8] = [
            (MAX_ARTIFACTS, Some(6), |limits, value| {
                limits.max_artifacts = value;
            }),
            (MAX_POLICY_ARTIFACTS, Some(2), |limits, value| {
                limits.max_policy_artifacts = value;
            }),
            (MAX_TARGET_PROFILE_ARTIFACTS, Some(2), |limits, value| {
                limits.max_target_profile_artifacts = value;
            }),
            (MAX_ARTIFACT_REFERENCES, Some(8), |limits, value| {
                limits.max_artifact_references = value;
            }),
            // The policy's own body holds its bounds, so its bytes move with
            // them: only the reference bytes have an exact value to set.
            (
                MAX_SINGLE_ARTIFACT_CANONICAL_BYTES,
                None,
                |limits, value| {
                    limits.max_single_artifact_canonical_bytes = value;
                },
            ),
            (MAX_TOTAL_ARTIFACT_CANONICAL_BYTES, None, |limits, value| {
                limits.max_total_artifact_canonical_bytes = value;
            }),
            (
                MAX_SINGLE_REFERENCE_CANONICAL_BYTES,
                largest_reference,
                |limits, value| limits.max_single_reference_canonical_bytes = value,
            ),
            (
                MAX_TOTAL_REFERENCE_CANONICAL_BYTES,
                reference_total,
                |limits, value| limits.max_total_reference_canonical_bytes = value,
            ),
        ];
        for (id, exact, edit) in cases {
            // Under a bound of one, each of the bound's units reports what it
            // counted, and only those.
            let inputs = with_second(&|limits| edit(&mut limits.artifact_admission, bound(1)));
            let (evaluation, outcome) = inputs.confirm();
            let bytes: Vec<u64> = collection(&inputs.members)
                .members()
                .iter()
                .map(FormalMember::canonical_bytes)
                .collect();
            let mut distinct = bytes.clone();
            distinct.sort_unstable();
            distinct.dedup();
            let found: Vec<IssueKey> = evaluation.issues().cloned().collect();
            let actuals: Vec<u64> = found
                .iter()
                .map(|key| {
                    assert_eq!(key.unit().check, Check::WithinBound, "{}", id.name);
                    let exceeded = evaluation.limit(key).unwrap();
                    assert_eq!(exceeded.bound, id);
                    match exceeded.actual {
                        Actual::Exact(count) => count,
                        Actual::AtLeast(_) => panic!("an exact count"),
                    }
                })
                .collect();
            let expected: Vec<u64> = match id.name {
                "maxArtifacts" => vec![6],
                "maxPolicyArtifacts" | "maxTargetProfileArtifacts" => vec![2],
                "maxArtifactReferences" => vec![8],
                "maxSingleArtifactCanonicalBytes" => {
                    // One unit per distinct body, in their byte order.
                    let bodies: BTreeSet<Vec<u8>> = collection(&inputs.members)
                        .members()
                        .iter()
                        .map(|member| member.canonical_body().to_vec())
                        .collect();
                    bodies
                        .iter()
                        .map(|body| u64::try_from(body.len()).unwrap())
                        .collect()
                }
                "maxTotalArtifactCanonicalBytes" => vec![bytes.iter().sum()],
                "maxSingleReferenceCanonicalBytes" => references.clone(),
                _ => vec![references.iter().sum()],
            };
            assert_eq!(actuals, expected, "{}", id.name);
            assert_eq!(keys(&outcome.confirmed.unwrap_err()), found, "{}", id.name);
            // At the exact count the bound holds.
            if let Some(exact) = exact {
                let (evaluation, outcome) =
                    with_second(&|limits| edit(&mut limits.artifact_admission, bound(exact)))
                        .confirm();
                assert_eq!(issues(&evaluation), [], "{}", id.name);
                assert!(outcome.confirmed.is_ok(), "{}", id.name);
            }
        }
    }

    #[test]
    fn a_recheck_whose_input_is_unavailable_leaves_the_selection_unconfirmed() {
        let root = stand_in(Slot::Coverage);
        let declarations = stand_in(Slot::DeliveryGraph);
        let work = stand_in(Slot::DeliveryPlacement);
        let fields = stand_in(Slot::LocaleNegotiation);
        let mut inputs = under(|_| {});
        inputs.measures.value = Err(root.clone());
        inputs.measures.profile_ids = Err(declarations.clone());
        inputs.measures.structural_units = Err(work.clone());
        inputs.policies = Err(fields.clone());
        let (evaluation, outcome) = inputs.confirm();
        assert_eq!(issues(&evaluation), []);
        assert_eq!(evaluation.status(&comparison()), Some(&Status::Evaluated));
        for subject in [
            PolicySubject::Nodes,
            PolicySubject::Depth,
            PolicySubject::CollectionEntries,
            PolicySubject::StringBytes,
            PolicySubject::LongestString,
        ] {
            assert_eq!(waits_on(&evaluation, &recheck(subject)), keys(&root));
        }
        assert_eq!(
            waits_on(&evaluation, &recheck(PolicySubject::Profiles)),
            keys(&declarations)
        );
        // Without the declarations there is no ID to recheck; the selector is
        // measured without them.
        assert!(!rechecked(&evaluation)
            .iter()
            .any(|subject| matches!(subject, PolicySubject::ProfileId(_))));
        assert_eq!(
            evaluation.status(&recheck(PolicySubject::Selector)),
            Some(&Status::Evaluated)
        );
        assert_eq!(
            waits_on(&evaluation, &recheck(PolicySubject::StructuralAnalysis)),
            keys(&work)
        );
        // The visible targets alone prove no count, so the declaration's
        // reference rechecks wait as well.
        for subject in [PolicySubject::References, PolicySubject::ReferenceBytes] {
            assert_eq!(waits_on(&evaluation, &recheck(subject)), keys(&fields));
        }
        // The policy is admitted and compatible, but not confirmed.
        let every = root
            .clone()
            .union(declarations)
            .union(work)
            .union(fields.clone());
        assert_eq!(keys(&outcome.confirmed.unwrap_err()), keys(&every));
        // Without the policy as well, a recheck waits on both.
        let unselected = stand_in(Slot::MessageLocaleFallback);
        inputs.resource_limits = Err(unselected.clone());
        let (evaluation, _) = inputs.confirm();
        assert_eq!(
            waits_on(&evaluation, &recheck(PolicySubject::Nodes)),
            keys(&root.union(unselected.clone()))
        );
        assert_eq!(
            waits_on(&evaluation, &recheck(PolicySubject::References)),
            keys(&fields.union(unselected))
        );
    }

    #[test]
    fn a_selector_observed_under_a_bound_below_the_authority_is_a_resolver_defect() {
        // The capability is the authority, but the selector was observed past
        // three bytes.
        let mut inputs = under(|_| {});
        inputs.members.remove(0);
        inputs.measures.selector = SelectorInput::string("apps", bound(3));
        assert_eq!(
            inputs.attempt().1.unwrap_err(),
            PolicyInvariant::Selection(SelectionInvariant::BootstrapBoundMismatch)
        );
        // A complete string has a length under any authority.
        inputs.measures.selector = SelectorInput::string("app", bound(3));
        assert!(inputs.attempt().1.is_ok());
    }
}
