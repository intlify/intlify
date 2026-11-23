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
//! which confirms the provisional selection. A bound's check under the
//! capability and its recheck are units of different subject kinds, ranked on
//! either side of the decision, so every unit follows the units it waits on.
//! Recounting the diagnostics under the policy waits for 015 Implementation
//! Phase 5.
//!
//! The checks under the capability observe their Findings beside the
//! capability's bound: those of the collection at its root, or at each member
//! past one artifact's bound under a key that keeps only its JSON type
//! (D15:834, 836; plan 025 interpretation #200), and those of the selected
//! declaration's references at the declaration, or at each reference's value,
//! in the file Phase 1 read (D15:790, 1918, 1921-1922). After selection, the
//! work bounds and the body are observed at the chosen candidate, beside the
//! selected reference and, for a work bound, the capability's bootstrap bound
//! or, for the body, the pins where they do not authenticate the candidate;
//! the comparison is observed at the admitted Policy, beside the reference and
//! the capability's bound it needs more of (D15:2249, 2355). Candidate
//! selection observes a reference that names nothing admissible at the
//! reference, beside what it compared the reference with, and each occurrence
//! of a selected group at the member, beside the reference (D15:2249). The
//! rechecks are observed beside the admitted Policy that supplies the bound
//! (D15:2249): the `artifactAdmission` ones where their checks under the
//! capability are, and the `configurationInput` ones at the materialized
//! root, each string past the bound, the declarations, each declared Profile
//! ID's key, or the selector under the policy's bound (D15:790, 812, 816,
//! 1884). Every Finding of Phase 4 carries occurrence Evidence.

use std::collections::BTreeMap;

use crate::artifacts::collection::FormalCollection;
use crate::artifacts::envelope::{ArtifactKind, Declaration, FormalMember, MemberKey};
use crate::boundary::capability::ImplementationCapability;
use crate::boundary::verification::VerificationInput;
use crate::input_limits::{Bound, ValueCounts};
use crate::model::{
    NonEmptyMap, Policies, ProfileId, RequiredNullable, TargetDeclaration, TargetId,
};
use crate::references::{PolicyReference, TargetProfileReference};
use crate::structural::selection::{SelectionInvariant, SelectorInput, StringLengthFact};

use super::artifact_resolution::{admitted, collection_root, unadmitted};
use super::bootstrap::work::{self, WorkBound};
use super::bootstrap::{admit_body, select_candidate, AdmittedResourcePolicy, Compared, Rejected};
use super::capability::CapabilityBound;
use super::declaration_observer::{DeclarationObserver, ObserverInvariant};
use super::evaluation::{
    and, both, Actual, Admitted, Blocking, Causes, Check, Evaluation, EvaluationInvariant, Found,
    Observation, Phase, Shortfall, Subject, Unit,
};
use super::evidence::EvidenceProjection;
use super::evidence_component::{ComponentInvariant, ComponentPath, Key, VerificationRole};
use super::evidence_token::JsonType;
use super::limits::{
    ArtifactAdmissionLimits, BoundId, ConfigurationInputLimits, ConfirmedPolicy, DiagnosticsLimits,
    Group, ResolutionLimits,
};
use super::tally::{actual, first_over, total, within, Tally};

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

/// What one Phase 4 unit checks, in rank order: each bound of 015's
/// `artifactAdmission` table under the capability's value, which needs no
/// policy; the selected `resourceLimits` reference and the policy it names;
/// and then, under the admitted policy, each bound of the `configurationInput`
/// table and again of the `artifactAdmission` table, in the tables'
/// declaration order and over their scopes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PolicySubject {
    /// An `artifactAdmission` bound under the capability's value.
    Capability(ArtifactSubject),
    /// The selected `resourceLimits` reference and the policy it names.
    ResourceLimits,
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
    /// One declared Profile ID, its own key, under `maxProfileIdBytes`.
    ProfileId(ProfileId),
    /// A string External Profile Selector Input, under `maxProfileIdBytes`.
    Selector,
    /// The completed structural analysis, under `maxStructuralAnalysisUnits`.
    StructuralAnalysis,
    /// An `artifactAdmission` bound under the admitted policy's value.
    Recheck(ArtifactSubject),
}

/// What one `artifactAdmission` unit checks, in the table's declaration order
/// and over the table's scope.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ArtifactSubject {
    /// Every submitted occurrence, under `maxArtifacts`.
    Artifacts,
    /// The occurrences declaring a Policy kind, under `maxPolicyArtifacts`.
    PolicyArtifacts,
    /// The occurrences declaring the Target Profile kind, under
    /// `maxTargetProfileArtifacts`.
    TargetProfileArtifacts,
    /// The selected declaration's references, under `maxArtifactReferences`.
    References,
    /// The submitted bodies of one member key, under
    /// `maxSingleArtifactCanonicalBytes`.
    Artifact(MemberKey),
    /// Every occurrence's canonical bytes, under
    /// `maxTotalArtifactCanonicalBytes`.
    ArtifactBytes,
    /// One reference of the selected declaration, under
    /// `maxSingleReferenceCanonicalBytes`.
    Reference(ReferenceField),
    /// Every reference's canonical bytes, under
    /// `maxTotalReferenceCanonicalBytes`.
    ReferenceBytes,
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
    /// Every slot, in the unsigned byte order of their names.
    pub(crate) const ALL: [Self; 7] = [
        Self::Approval,
        Self::GlossarySet,
        Self::ProviderRouting,
        Self::ResourceLimits,
        Self::Selection,
        Self::SourceAdmission,
        Self::Trust,
    ];

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

    /// The kind 017 assigns to the slot's configuration role.
    pub(crate) const fn kind(self) -> ArtifactKind {
        match self {
            Self::Approval => ArtifactKind::ApprovalPolicy,
            Self::GlossarySet => ArtifactKind::GlossarySet,
            Self::ProviderRouting => ArtifactKind::ProviderRoutingPolicy,
            Self::ResourceLimits => ArtifactKind::ResourceLimitPolicy,
            Self::Selection => ArtifactKind::SelectionPolicy,
            Self::SourceAdmission => ArtifactKind::SourceAdmissionPolicy,
            Self::Trust => ArtifactKind::TrustPolicy,
        }
    }
}

impl ReferenceField {
    /// The kind 017 assigns to the field's configuration role. A target's
    /// `profile` takes `target-profile`, the only kind its reference spells.
    pub(crate) const fn role_kind(&self) -> ArtifactKind {
        match self {
            Self::Policy(slot) => slot.kind(),
            Self::TargetProfile(_) => ArtifactKind::TargetProfile,
        }
    }
}

/// A reference of the selected declaration as Phase 4's bounds read it, whether
/// or not structural admission admitted it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReferenceOccurrence {
    /// Where it is, which names its byte check: none under a Target ID key
    /// without the identifier syntax, which has no later subject (D15:2184).
    pub(crate) field: Option<ReferenceField>,
    /// Whether it occurs: not for a required reference the declaration
    /// lacks, whose byte check still waits on that Finding (D15:1918, 2203).
    pub(crate) occurs: bool,
    /// Its canonical bytes, or why it has none (D15:1924).
    pub(crate) bytes: Admitted<u64>,
}

/// The canonical bytes of the references that have them: an exact total,
/// since one that failed structural admission has none to add (D15:1924,
/// 1997, 2006). Not evaluated only when every value is excluded, with the
/// causes of each (D15:2184, 3354).
fn reference_total(references: &[ReferenceOccurrence]) -> Admitted<Tally> {
    let admitted = references
        .iter()
        .filter_map(|each| each.bytes.as_ref().ok());
    let excluded = references
        .iter()
        .filter_map(|each| each.bytes.as_ref().err());
    match excluded.cloned().reduce(Causes::union) {
        Some(causes) if admitted.clone().next().is_none() => Err(causes),
        _ => Ok(total(admitted.map(|&bytes| Tally::exactly(bytes)))),
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

impl DeclarationReferences<'_> {
    /// Each present reference by its field, in the fields' logical order, as
    /// `policy` or `target` projects it; and the causes behind each field
    /// structural admission did not admit, which shows no references.
    pub(crate) fn visible<T>(
        &self,
        policy: impl Fn(&PolicyReference) -> T,
        target: impl Fn(&TargetProfileReference) -> T,
    ) -> (Vec<(ReferenceField, T)>, Option<Causes>) {
        let mut visible = Vec::new();
        let mut unavailable: Option<Causes> = None;
        match self.policies {
            Ok(policies) => visible.extend(
                present(policies)
                    .map(|(slot, reference)| (ReferenceField::Policy(slot), policy(reference))),
            ),
            Err(ref causes) => unavailable = Some(join(unavailable, causes)),
        }
        match self.target_profiles {
            Ok(targets) => visible.extend(targets.iter().map(|(id, declaration)| {
                (
                    ReferenceField::TargetProfile(id.clone()),
                    target(&declaration.profile),
                )
            })),
            Err(ref causes) => unavailable = Some(join(unavailable, causes)),
        }
        (visible, unavailable)
    }

    /// The references of fields structural admission admitted whole, each
    /// occurring with its bytes, or the causes of a field it did not.
    #[cfg(any(test, feature = "benchmark"))]
    pub(crate) fn occurrences(&self) -> Admitted<Vec<ReferenceOccurrence>> {
        let (visible, unavailable) = self.visible(
            PolicyReference::canonical_bytes,
            TargetProfileReference::canonical_bytes,
        );
        let occurrence = |(field, bytes)| ReferenceOccurrence {
            field: Some(field),
            occurs: true,
            bytes: Ok(bytes),
        };
        unavailable.map_or_else(|| Ok(visible.into_iter().map(occurrence).collect()), Err)
    }
}

/// What Phases 1 to 3 measured of the configuration and the selector under
/// the capability, which the admitted policy rechecks. Each part is admitted
/// or unavailable with the causes behind it.
pub(crate) struct ConfigurationMeasures<'a> {
    /// The materialized root's value counts.
    pub(crate) value: Admitted<ValueCounts>,
    /// How many declarations the root's `profiles` object holds: every
    /// declaration counts, one whose key lacks the identifier syntax included
    /// (D15:1885).
    pub(crate) declarations: Admitted<u64>,
    /// Every declared Profile ID, in their unsigned byte order: a declaration
    /// whose key lacks the identifier syntax has no subject in a later phase
    /// (D15:1886, 2184).
    pub(crate) profile_ids: Admitted<&'a [ProfileId]>,
    /// The normalized External Profile Selector Input.
    pub(crate) selector: &'a SelectorInput,
    /// The units the completed structural analysis evaluated.
    pub(crate) structural_units: Admitted<u64>,
}

/// What Phase 4 settles for the phases after it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PolicyOutcome {
    /// The selector's final projection authority: the admitted compatible
    /// policy's `maxProfileIdBytes`, or the capability's without such a
    /// policy (D15:816-817).
    pub(crate) authority: Bound,
    /// The selector's String Length Fact under the final projection
    /// authority, or none for an absent or non-string selector.
    pub(crate) selector_length: Option<StringLengthFact>,
    /// The diagnostic bounds of the admitted policy once it is compatible with
    /// the capability, which then take over ordinary reporting from the
    /// capability's whatever its rechecks find (D15:2132, 2391), or none
    /// without such a policy.
    pub(crate) diagnostics: Option<DiagnosticsLimits>,
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
    /// A bound or a member the Evidence names has no component path.
    Component(ComponentInvariant),
    /// A value of the selected declaration a check looked at has no
    /// Evidence.
    Observer(ObserverInvariant),
}

impl From<EvaluationInvariant> for PolicyInvariant {
    fn from(invariant: EvaluationInvariant) -> Self {
        Self::Evaluation(invariant)
    }
}

impl From<ComponentInvariant> for PolicyInvariant {
    fn from(invariant: ComponentInvariant) -> Self {
        Self::Component(invariant)
    }
}

impl From<ObserverInvariant> for PolicyInvariant {
    fn from(invariant: ObserverInvariant) -> Self {
        Self::Observer(invariant)
    }
}

/// The selected declaration's references, or the causes of its absence, and
/// where a check of it looks.
type Declared<'r, 'o> = (
    Result<&'r [ReferenceOccurrence], &'r Causes>,
    Option<&'r DeclarationObserver<'o>>,
);

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

/// The `artifactAdmission` units of one stage: under the capability's value,
/// before a policy is admitted, or as the admitted policy's recheck, with the
/// policy's Evidence once one is admitted.
#[derive(Debug, Clone, Copy)]
enum Stage<'p> {
    Capability,
    Policy(Option<&'p EvidenceProjection>),
}

impl Stage<'_> {
    fn unit(self, subject: ArtifactSubject) -> Unit {
        match self {
            Self::Capability => unit(PolicySubject::Capability(subject), Check::WithinCapability),
            Self::Policy(_) => unit(PolicySubject::Recheck(subject), Check::WithinBound),
        }
    }

    /// What supplies this stage's `bound`, which a Finding past it relates
    /// (D15:2249): the capability's bound, or the admitted policy. None
    /// before a policy is admitted, when no recheck is evaluated.
    fn beside(self, bound: BoundId) -> Result<Option<EvidenceProjection>, ComponentInvariant> {
        match self {
            Self::Capability => {
                capability_bound(CapabilityBound::PolicyComparable(bound)).map(Some)
            }
            Self::Policy(policy) => Ok(policy.cloned()),
        }
    }

    /// What this stage's unit of the collection's `bound` observes: the
    /// collection at its root, beside what supplies the bound (D15:834, 836).
    fn at_collection(self, bound: BoundId) -> Result<Option<Observation>, ComponentInvariant> {
        self.beside(bound)?
            .map(|limit| {
                Ok(Observation {
                    primary: collection_root()?,
                    related: vec![limit],
                })
            })
            .transpose()
    }

    /// What this stage's unit of a member key observes past one artifact's
    /// `bound`: each member under the key, with its canonical bytes, at the
    /// member no Set admitted, beside what supplies the bound (D15:834, 836;
    /// plan 025 interpretation #200).
    fn at_members(
        self,
        members: &[&FormalMember],
        bound: BoundId,
    ) -> Result<Vec<(u64, Observation)>, ComponentInvariant> {
        let Some(limit) = self.beside(bound)? else {
            return Ok(Vec::new());
        };
        members
            .iter()
            .map(|member| {
                Ok((
                    member.canonical_bytes(),
                    Observation {
                        primary: unadmitted(member)?,
                        related: vec![limit.clone()],
                    },
                ))
            })
            .collect()
    }

    /// What this stage's unit of a bound over the selected declaration's
    /// references observes: the value at `path` under the declaration, beside
    /// what supplies the bound (D15:790, 836). Nothing is observed without the
    /// declaration's file.
    fn at_declaration(
        self,
        observer: Option<&DeclarationObserver<'_>>,
        path: &[&str],
        bound: BoundId,
    ) -> Result<Option<Observation>, PolicyInvariant> {
        match (observer, self.beside(bound)?) {
            (Some(observer), Some(limit)) => Ok(Some(Observation {
                primary: observer.value(path)?,
                related: vec![limit],
            })),
            _ => Ok(None),
        }
    }
}

/// The path of a reference field's value under the declaration.
pub(super) fn reference_path(field: &ReferenceField) -> Vec<&str> {
    match field {
        ReferenceField::Policy(slot) => vec!["policies", slot.name()],
        ReferenceField::TargetProfile(id) => vec!["targetProfiles", id.as_str(), "profile"],
    }
}

/// Resolver-input Evidence of the Implementation Capability's `bound`
/// (D15:836).
fn capability_bound(bound: CapabilityBound) -> Result<EvidenceProjection, ComponentInvariant> {
    ComponentPath::capability(bound).map(EvidenceProjection::resolver_input)
}

/// The candidate `blocking`, with what it observed when there is one.
fn found(blocking: Blocking, observed: Option<Observation>) -> Found {
    observed.map_or_else(
        || Found::from(blocking),
        |observation| Found::observed(blocking, observation),
    )
}

/// The collection as the `artifactAdmission` bounds count it: every
/// occurrence, and each member key with its members. Members with one key
/// share its unit, which holds exactly when its largest canonical body does.
struct Submitted<'a> {
    members: &'a [FormalMember],
    keyed: BTreeMap<MemberKey, Vec<&'a FormalMember>>,
}

impl<'a> Submitted<'a> {
    fn of(collection: &'a FormalCollection) -> Self {
        let members = collection.members();
        let mut keyed: BTreeMap<MemberKey, Vec<&'a FormalMember>> = BTreeMap::new();
        for member in members {
            keyed.entry(member.key()).or_default().push(member);
        }
        Self { members, keyed }
    }
}

/// What one stage's `artifactAdmission` units settle: whether the
/// collection's bounds hold and whether the `resourceLimits` reference's own
/// bytes do, which the decision needs, and whether every unit does, which the
/// confirmation needs.
struct Inspected {
    collection: Admitted<()>,
    own_bytes: Admitted<()>,
    every: Admitted<()>,
}

/// Phase 4. The bounds over the collection need nothing earlier, so they are
/// checked whatever the selection; the reference bounds need the selected
/// declaration's fields; the decision needs the `resourceLimits` reference,
/// every collection bound, and that reference's own bytes; the comparison
/// needs the admitted policy; and each recheck needs the compatible policy and
/// what it measures. A reference that failed structural admission has no bytes
/// the bounds protect, so it holds no confirmation back; a recheck's Finding
/// does, and so does a complete input it cannot measure (D15:1736, 1924, 2151,
/// 2367; plan 025 decision #342).
pub(crate) fn admit_resource_policy(
    evaluation: &mut Evaluation,
    capability: &ImplementationCapability,
    collection: &FormalCollection,
    resource_limits: Result<&PolicyReference, &Causes>,
    declared: Declared<'_, '_>,
    measures: &ConfigurationMeasures<'_>,
    verification: &VerificationInput,
) -> Result<PolicyOutcome, PolicyInvariant> {
    let submitted = Submitted::of(collection);
    let compatible = admit_and_compare(
        evaluation,
        capability,
        &submitted,
        resource_limits,
        declared,
        verification,
    )?;
    // The admitted policy supplies every recheck's bound, and every bound a
    // later phase checks under it (D15:2249).
    let compatible = match compatible {
        Ok(policy) => {
            let evidence = admitted(policy.tuple())?;
            Ok((policy, evidence))
        }
        Err(causes) => Err(causes),
    };
    // An admitted, compatible policy is the projection authority whatever its
    // rechecks find; without one, the capability stays the authority.
    let authority = compatible
        .as_ref()
        .map_or(capability.policy_comparable(), |(policy, _)| {
            policy.limits()
        })
        .configuration_input
        .structural
        .max_profile_id_bytes;
    let selector_length = measures
        .selector
        .length_fact(authority)
        .map_err(PolicyInvariant::Selection)?;
    let limits = compatible.as_ref().map(|(policy, _)| policy.limits());
    let policy = compatible.as_ref().ok().map(|(_, evidence)| evidence);
    let configured = configuration_bounds(
        evaluation,
        limits.map(|limits| &limits.configuration_input),
        (measures, selector_length),
        (policy, declared.1),
    )?;
    let inspected = artifact_bounds(
        evaluation,
        Stage::Policy(policy),
        limits.map(|limits| &limits.artifact_admission),
        &submitted,
        resource_limits,
        declared,
    )?;
    let rechecked = and(configured, inspected.every);
    Ok(PolicyOutcome {
        authority,
        selector_length,
        diagnostics: limits.ok().map(|limits| limits.diagnostics),
        confirmed: both(compatible, rechecked).map(|((policy, evidence), ())| {
            ConfirmedPolicy::confirmed(&policy, evidence, Rechecked(()))
        }),
    })
}

/// Phase 4 up to the comparison with the capability: the `artifactAdmission`
/// bounds under the capability's values, then the bootstrap decision with each
/// `artifactBootstrap` bound before the work it protects (Decision 015-164),
/// each check waiting on the one before it: the metadata scan, candidate
/// selection, the chosen candidate's envelope depth, decoded allocation, and
/// validation work, its body admission, and the comparison.
fn admit_and_compare(
    evaluation: &mut Evaluation,
    capability: &ImplementationCapability,
    submitted: &Submitted<'_>,
    resource_limits: Result<&PolicyReference, &Causes>,
    declared: Declared<'_, '_>,
    verification: &VerificationInput,
) -> Result<Admitted<AdmittedResourcePolicy>, PolicyInvariant> {
    let inspected = artifact_bounds(
        evaluation,
        Stage::Capability,
        Ok(&capability.policy_comparable().artifact_admission),
        submitted,
        resource_limits,
        declared,
    )?;
    let limits = &capability.bootstrap_only().artifact_bootstrap;
    // The scan reads the whole collection, not through the reference (D15:834,
    // 836).
    let scan = Observation {
        primary: collection_root()?,
        related: vec![capability_bound(CapabilityBound::BootstrapOnly(
            WorkBound::MetadataScan.id(),
        ))?],
    };
    let scanned = evaluation.check_found(
        unit(PolicySubject::ResourceLimits, Check::WithinMetadataScan),
        both(resource_limits.map_err(Causes::clone), inspected.collection),
        |(reference, ())| {
            let total = work::metadata_scan_units(submitted.members);
            work::within(WorkBound::MetadataScan, limits, total)
                .map(|()| reference)
                .map_err(|blocking| Found::observed(blocking, scan))
        },
    )?;
    // Every check from candidate selection on compares what it judges with
    // the selected reference, through which it reaches the candidates, so
    // nothing is observed without the declaration's file (D15:790, 2249).
    let reference = match (declared.1, resource_limits) {
        (Some(observer), Ok(_)) => {
            Some(observer.value(&["policies", PolicySlot::ResourceLimits.name()])?)
        }
        _ => None,
    };
    // A member that selection compares establishes a tuple, so it is an
    // envelope, a JSON object, which no Set admitted (D15:834; plan 025
    // interpretation #200).
    let envelope = ComponentPath::artifact_member(Key::Unsafe(JsonType::Object))
        .map(EvidenceProjection::resolver_input)?;
    let root = collection_root()?;
    let selected = evaluation.check_found(
        unit(PolicySubject::ResourceLimits, Check::BootstrapCandidate),
        both(scanned, inspected.own_bytes).map(|(reference, ())| reference),
        |named| {
            select_candidate(&named.tuple(), submitted.members)
                .map_err(|rejected| rejection(rejected, reference.as_ref(), (&envelope, &root)))
        },
    )?;
    // The chosen candidate, which no Set admitted yet (D15:834; plan 025
    // interpretation #200).
    let chosen = selected
        .as_ref()
        .ok()
        .map(|envelope| unadmitted(envelope.member()))
        .transpose()?;
    // A check of the candidate observes it beside the selected reference and
    // whatever else it compares the candidate with.
    let beside = |others: Vec<EvidenceProjection>| {
        Some(Observation {
            primary: chosen.clone()?,
            related: [reference.clone()?].into_iter().chain(others).collect(),
        })
    };
    let mut measured = selected;
    for (check, bound) in [
        (Check::WithinEnvelopeDepth, WorkBound::EnvelopeDepth),
        (Check::WithinDecodedAllocation, WorkBound::DecodedAllocation),
        (Check::WithinValidationWork, WorkBound::ValidationWork),
    ] {
        let observed = beside(vec![capability_bound(CapabilityBound::BootstrapOnly(
            bound.id(),
        ))?]);
        measured = evaluation.check_found(
            unit(PolicySubject::ResourceLimits, check),
            measured,
            |candidate| {
                let total = match bound {
                    WorkBound::EnvelopeDepth => work::envelope_depth(candidate.member().value()),
                    WorkBound::DecodedAllocation => {
                        work::decoded_allocation_bytes(candidate.content())
                    }
                    WorkBound::ValidationWork => work::validation_work_units(candidate.content()),
                    WorkBound::MetadataScan => unreachable!("the scan comes before selection"),
                };
                work::within(bound, limits, total)
                    .map(|()| candidate)
                    .map_err(|blocking| found(blocking, observed))
            },
        )?;
    }
    // The pins are compared only where they do not authenticate the
    // candidate (D15:2249).
    let unpinned = beside(vec![EvidenceProjection::resolver_input(
        ComponentPath::verification(VerificationRole::Pins),
    )]);
    let pinned = beside(Vec::new());
    let decided = evaluation.check_found(
        unit(PolicySubject::ResourceLimits, Check::CandidateBody),
        measured,
        |candidate| {
            let observed = if verification.authenticates(&candidate) {
                pinned
            } else {
                unpinned
            };
            admit_body(&candidate, verification).map_err(|blocking| found(blocking, observed))
        },
    )?;
    // The admitted Policy needs more of the first bound in fixed declaration
    // order the capability lacks (D15:2355). It is observed at the Policy,
    // beside the selected reference and the capability's bound (D15:2249).
    let shortfall = decided
        .as_ref()
        .ok()
        .and_then(|policy| first_shortfall(policy.limits(), capability.policy_comparable()));
    let compared = match (&decided, &reference, shortfall) {
        (Ok(policy), Some(reference), Some(shortfall)) => Some(Observation {
            primary: admitted(policy.tuple())?,
            related: vec![
                reference.clone(),
                capability_bound(CapabilityBound::PolicyComparable(shortfall.bound))?,
            ],
        }),
        _ => None,
    };
    Ok(evaluation.check_found(
        unit(PolicySubject::ResourceLimits, Check::CompatiblePolicy),
        decided,
        |policy| match shortfall {
            None => Ok(policy),
            Some(shortfall) => Err(found(Blocking::shortfall(shortfall), compared)),
        },
    )?)
}

/// The candidates of a selection that admitted no candidate, each with what
/// it observed (D15:2249). A reference that names nothing admissible is
/// observed at the reference, beside what it was compared with: nothing when
/// its own checks fail, the collection as one location when no member shares
/// even its kind or identity, or each candidate of the closest tier. A
/// selected group's conflicting or duplicate occurrences are each observed at
/// the member, beside the reference. Every member is the same `envelope`
/// Evidence, so a tier's candidates merge into one related record and a
/// group's occurrences differ only in their ordinals (D15:834). Nothing is
/// observed without the reference's value.
fn rejection(
    rejected: Rejected<'_>,
    reference: Option<&EvidenceProjection>,
    (envelope, root): (&EvidenceProjection, &EvidenceProjection),
) -> Found {
    let Rejected { blocking, compared } = rejected;
    let Some(reference) = reference else {
        return Found::from(blocking);
    };
    let at_reference = |related| {
        Found::observed(
            blocking,
            Observation {
                primary: reference.clone(),
                related,
            },
        )
    };
    let at_member = || {
        Found::observed(
            blocking,
            Observation {
                primary: envelope.clone(),
                related: vec![reference.clone()],
            },
        )
    };
    match compared {
        Compared::Nothing => at_reference(Vec::new()),
        Compared::Collection => at_reference(vec![root.clone()]),
        Compared::Tier(candidates) => {
            at_reference(candidates.iter().map(|_| envelope.clone()).collect())
        }
        // The first member, then each other one.
        Compared::Group(members) => members
            .iter()
            .skip(1)
            .fold(at_member(), |found, _| found.and(at_member())),
    }
}

/// One bound's unit, not evaluated while the bound's value or the count is
/// unavailable, whose Finding is observed through `observed` when there is
/// one.
fn bounded_at(
    evaluation: &mut Evaluation,
    unit: Unit,
    (bound, limit): (BoundId, Result<Bound, &Causes>),
    tally: Result<Tally, &Causes>,
    observed: Option<Observation>,
) -> Result<Admitted<()>, EvaluationInvariant> {
    evaluation.check_found(
        unit,
        both(limit.map_err(Causes::clone), tally.map_err(Causes::clone)),
        |(limit, tally)| {
            within(bound, limit, actual(tally, limit)).map_err(|blocking| found(blocking, observed))
        },
    )
}

/// One bound's unit whose key stands for several values that are checked
/// independently, of which `tally` is the largest: past the bound it reports
/// only the first-over witness, the same for every value the key stands for.
/// `values` are the values the key stands for, each with its size and what
/// it is observed through: one candidate for each value past the bound
/// (D15:2203).
fn witnessed_at(
    evaluation: &mut Evaluation,
    unit: Unit,
    (bound, limit): (BoundId, Result<Bound, &Causes>),
    tally: Result<Tally, &Causes>,
    values: &[(u64, Observation)],
) -> Result<Admitted<()>, EvaluationInvariant> {
    evaluation.check_found(
        unit,
        both(limit.map_err(Causes::clone), tally.map_err(Causes::clone)),
        |(limit, tally)| {
            within(bound, limit, first_over(tally, limit)).map_err(|blocking| {
                values
                    .iter()
                    .filter(|&&(size, _)| size > limit.get())
                    .map(|(_, observation)| Found::observed(blocking, observation.clone()))
                    .reduce(Found::and)
                    .unwrap_or_else(|| Found::from(blocking))
            })
        },
    )
}

/// What a `configurationInput` recheck past the admitted `policy`'s bound
/// observes: where `at` looks in the configuration, beside the policy, which
/// supplies the bound (D15:2249). Nothing is observed without the policy or
/// the declaration's file.
fn recheck_at<'o>(
    (policy, observer): (
        Option<&EvidenceProjection>,
        Option<&DeclarationObserver<'o>>,
    ),
    at: impl FnOnce(&DeclarationObserver<'o>) -> Result<EvidenceProjection, ObserverInvariant>,
) -> Result<Option<Observation>, ObserverInvariant> {
    match (policy, observer) {
        (Some(policy), Some(observer)) => Ok(Some(Observation {
            primary: at(observer)?,
            related: vec![policy.clone()],
        })),
        _ => Ok(None),
    }
}

/// The eight `configurationInput` bounds over what Phases 1 to 3 measured,
/// under the policy's values: the root's five as Phase 1 summarizes them, the
/// declarations, each declared Profile ID and then a string selector, and the
/// structural analysis. Whether all of them hold. Each is observed where its
/// check under the capability was, beside the admitted policy: the root's
/// totals and the analysis at the root, each string past the bound at its
/// token, the declarations at the root's `profiles`, each Profile ID at its
/// key, and the selector under the policy's bound beside the declarations
/// (D15:790, 812, 816, 1884, 2249).
fn configuration_bounds(
    evaluation: &mut Evaluation,
    limits: Result<&ConfigurationInputLimits, &Causes>,
    (measures, selector_length): (&ConfigurationMeasures<'_>, Option<StringLengthFact>),
    observed: (
        Option<&EvidenceProjection>,
        Option<&DeclarationObserver<'_>>,
    ),
) -> Result<Admitted<()>, PolicyInvariant> {
    let recheck = |subject| unit(subject, Check::WithinBound);
    let at_root = recheck_at(observed, DeclarationObserver::root)?;
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
    ] {
        let held = bounded_at(
            evaluation,
            recheck(subject),
            (bound, limit),
            count.map(Tally::exactly),
            at_root.clone(),
        )?;
        holds = and(holds, held);
    }
    // Each string value and object key is checked on its own, so the longest
    // reports only the first-over witness, and each string past the bound is
    // observed at its token, found only when the longest is past it
    // (D15:1884, 2203).
    let longest = value.map(|limits| limits.max_single_string_bytes);
    let past: Vec<(u64, Observation)> = match (observed, longest, counts) {
        ((Some(policy), Some(observer)), Ok(limit), Ok(counts))
            if counts.single_string_bytes > limit.get() =>
        {
            observer
                .strings_past(limit.get())?
                .into_iter()
                .map(|(bytes, primary)| {
                    (
                        bytes,
                        Observation {
                            primary,
                            related: vec![policy.clone()],
                        },
                    )
                })
                .collect()
        }
        _ => Vec::new(),
    };
    let held = witnessed_at(
        evaluation,
        recheck(PolicySubject::LongestString),
        (MAX_SINGLE_STRING_BYTES, longest),
        counts.map(|counts| Tally::exactly(counts.single_string_bytes)),
        &past,
    )?;
    holds = and(holds, held);
    let ids = measures.profile_ids.as_ref();
    let held = bounded_at(
        evaluation,
        recheck(PolicySubject::Profiles),
        (MAX_PROFILES, structural.map(|limits| limits.max_profiles)),
        measures
            .declarations
            .as_ref()
            .map(|&declarations| Tally::exactly(declarations)),
        recheck_at(observed, DeclarationObserver::declarations)?,
    )?;
    holds = and(holds, held);
    let id_limit = structural.map(|limits| limits.max_profile_id_bytes);
    if let Ok(ids) = ids {
        for id in *ids {
            let held = bounded_at(
                evaluation,
                recheck(PolicySubject::ProfileId(id.clone())),
                (MAX_PROFILE_ID_BYTES, id_limit),
                Ok(Tally::of(id.as_str().len())),
                recheck_at(observed, |observer| observer.key(id.as_str()))?,
            )?;
            holds = and(holds, held);
        }
    }
    // An absent selector and a non-string type carry no ID bytes.
    if let Some(length) = selector_length {
        // Past the policy's bound, the selector is projected under it, the
        // final authority (D15:816, 1889).
        let selector = match (observed, id_limit) {
            ((Some(policy), Some(observer)), Ok(limit)) => {
                let mut observation =
                    observer.selector(measures.selector, limit, measures.declarations.is_ok())?;
                observation.related.push(policy.clone());
                Some(observation)
            }
            _ => None,
        };
        let held = evaluation.check_found(
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
                    .map_err(|blocking| found(blocking, selector))
            },
        )?;
        holds = and(holds, held);
    }
    let held = bounded_at(
        evaluation,
        recheck(PolicySubject::StructuralAnalysis),
        (
            MAX_STRUCTURAL_ANALYSIS_UNITS,
            structural.map(|limits| limits.max_structural_analysis_units),
        ),
        measures
            .structural_units
            .as_ref()
            .map(|&units| Tally::exactly(units)),
        at_root,
    )?;
    Ok(and(holds, held))
}

/// The eight `artifactAdmission` bounds of one stage, in 015's table order:
/// the collection's occurrences and its Policy and Target Profile
/// occurrences, the declaration's references, each member key and the
/// collection's bytes, and each reference and the references' bytes. Every
/// reference that occurs counts, one that failed structural admission
/// included, and a value of the wrong type holds none (D15:1918, 1924).
fn artifact_bounds(
    evaluation: &mut Evaluation,
    stage: Stage<'_>,
    limits: Result<&ArtifactAdmissionLimits, &Causes>,
    submitted: &Submitted<'_>,
    resource_limits: Result<&PolicyReference, &Causes>,
    (references, observer): Declared<'_, '_>,
) -> Result<Inspected, PolicyInvariant> {
    let occurrences = occurrence_bounds(evaluation, stage, limits, submitted.members)?;
    // The declaration's count and total never gate the decision for the
    // selected reference. Both count over the whole declaration (D15:1918,
    // 1922).
    let count = bounded_at(
        evaluation,
        stage.unit(ArtifactSubject::References),
        (
            MAX_ARTIFACT_REFERENCES,
            limits.map(|limits| limits.max_artifact_references),
        ),
        references
            .map(|references| Tally::of(references.iter().filter(|each| each.occurs).count())),
        stage.at_declaration(observer, &[], MAX_ARTIFACT_REFERENCES)?,
    )?;
    let collection = and(
        occurrences,
        body_bounds(evaluation, stage, limits, submitted)?,
    );
    let (own_bytes, each) = each_reference(
        evaluation,
        stage,
        limits,
        (resource_limits, references.unwrap_or(&[])),
        observer,
    )?;
    let sum = references.map_err(Causes::clone).and_then(reference_total);
    let bytes = bounded_at(
        evaluation,
        stage.unit(ArtifactSubject::ReferenceBytes),
        (
            MAX_TOTAL_REFERENCE_CANONICAL_BYTES,
            limits.map(|limits| limits.max_total_reference_canonical_bytes),
        ),
        sum.as_ref().copied(),
        stage.at_declaration(observer, &[], MAX_TOTAL_REFERENCE_CANONICAL_BYTES)?,
    )?;
    Ok(Inspected {
        every: and(and(collection.clone(), count), and(each, bytes)),
        collection,
        own_bytes,
    })
}

/// The collection's three occurrence counts, and whether they hold. A member
/// declaring another kind or schema revision is unsupported and counts only
/// as an artifact.
fn occurrence_bounds(
    evaluation: &mut Evaluation,
    stage: Stage<'_>,
    limits: Result<&ArtifactAdmissionLimits, &Causes>,
    members: &[FormalMember],
) -> Result<Admitted<()>, PolicyInvariant> {
    let declared = |policy: bool| {
        members
            .iter()
            .filter(|member| {
                matches!(member.declaration(), Declaration::Admitted(kind) if kind.is_policy() == policy)
            })
            .count()
    };
    let mut holds: Admitted<()> = Ok(());
    for (subject, bound, limit, count) in [
        (
            ArtifactSubject::Artifacts,
            MAX_ARTIFACTS,
            limits.map(|limits| limits.max_artifacts),
            members.len(),
        ),
        (
            ArtifactSubject::PolicyArtifacts,
            MAX_POLICY_ARTIFACTS,
            limits.map(|limits| limits.max_policy_artifacts),
            declared(true),
        ),
        (
            ArtifactSubject::TargetProfileArtifacts,
            MAX_TARGET_PROFILE_ARTIFACTS,
            limits.map(|limits| limits.max_target_profile_artifacts),
            declared(false),
        ),
    ] {
        let held = bounded_at(
            evaluation,
            stage.unit(subject),
            (bound, limit),
            Ok(Tally::of(count)),
            stage.at_collection(bound)?,
        )?;
        holds = and(holds, held);
    }
    Ok(holds)
}

/// Each member key's bodies, equal occurrences among them, under one unit,
/// and then the canonical bytes of every occurrence. Whether they hold.
fn body_bounds(
    evaluation: &mut Evaluation,
    stage: Stage<'_>,
    limits: Result<&ArtifactAdmissionLimits, &Causes>,
    submitted: &Submitted<'_>,
) -> Result<Admitted<()>, PolicyInvariant> {
    let mut holds: Admitted<()> = Ok(());
    for (key, members) in &submitted.keyed {
        let largest = members
            .iter()
            .map(|member| member.canonical_bytes())
            .max()
            .expect("a member key has a member");
        let held = witnessed_at(
            evaluation,
            stage.unit(ArtifactSubject::Artifact(key.clone())),
            (
                MAX_SINGLE_ARTIFACT_CANONICAL_BYTES,
                limits.map(|limits| limits.max_single_artifact_canonical_bytes),
            ),
            Ok(Tally::exactly(largest)),
            &stage.at_members(members, MAX_SINGLE_ARTIFACT_CANONICAL_BYTES)?,
        )?;
        holds = and(holds, held);
    }
    let held = bounded_at(
        evaluation,
        stage.unit(ArtifactSubject::ArtifactBytes),
        (
            MAX_TOTAL_ARTIFACT_CANONICAL_BYTES,
            limits.map(|limits| limits.max_total_artifact_canonical_bytes),
        ),
        Ok(total(
            submitted
                .members
                .iter()
                .map(|member| Tally::exactly(member.canonical_bytes())),
        )),
        stage.at_collection(MAX_TOTAL_ARTIFACT_CANONICAL_BYTES)?,
    )?;
    Ok(and(holds, held))
}

/// Each reference's own bytes: whether the `resourceLimits` reference's hold,
/// and whether every one that has bytes does. The selected reference is
/// checked from what selection admitted, even when the rest of `policies` was
/// not. One without bytes waits on why, and holds nothing back (D15:1924).
/// The units commit in canonical order whatever order they run in.
fn each_reference(
    evaluation: &mut Evaluation,
    stage: Stage<'_>,
    limits: Result<&ArtifactAdmissionLimits, &Causes>,
    (resource_limits, references): (Result<&PolicyReference, &Causes>, &[ReferenceOccurrence]),
    observer: Option<&DeclarationObserver<'_>>,
) -> Result<(Admitted<()>, Admitted<()>), PolicyInvariant> {
    let single_limit = limits.map(|limits| limits.max_single_reference_canonical_bytes);
    let selected = ReferenceField::Policy(PolicySlot::ResourceLimits);
    // Each reference is observed at its value (D15:1921).
    let at = |field: &ReferenceField| {
        // The selected reference is looked at only where selection admitted
        // it, and every other one is visible.
        if *field != selected || resource_limits.is_ok() {
            stage.at_declaration(
                observer,
                &reference_path(field),
                MAX_SINGLE_REFERENCE_CANONICAL_BYTES,
            )
        } else {
            Ok(None)
        }
    };
    let own_bytes = bounded_at(
        evaluation,
        stage.unit(ArtifactSubject::Reference(selected.clone())),
        (MAX_SINGLE_REFERENCE_CANONICAL_BYTES, single_limit),
        resource_limits.map(|reference| Tally::exactly(reference.canonical_bytes())),
        at(&selected)?,
    )?;
    let mut holds = own_bytes.clone();
    for reference in references {
        let Some(field) = reference.field.as_ref().filter(|field| **field != selected) else {
            continue;
        };
        let observed = match reference.bytes {
            Ok(_) => at(field)?,
            Err(_) => None,
        };
        let held = bounded_at(
            evaluation,
            stage.unit(ArtifactSubject::Reference(field.clone())),
            (MAX_SINGLE_REFERENCE_CANONICAL_BYTES, single_limit),
            reference.bytes.as_ref().map(|&bytes| Tally::exactly(bytes)),
            observed,
        )?;
        if reference.bytes.is_ok() {
            holds = and(holds, held);
        }
    }
    Ok((own_bytes, holds))
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
    use intlify_shared_json::encoding::encode;
    use serde_json::{json, Value};

    use std::collections::BTreeSet;
    use std::sync::Arc;

    use super::*;
    use crate::artifacts::collection::{read_collection, CollectionLimits, CollectionReport};
    use crate::artifacts::envelope::{envelope_of, ArtifactKind, ExactTuple};
    use crate::materialize::{materialize_file, NodeKind};
    use crate::materialize_tests::limits as input_limits;
    use crate::resolver::capability::ArtifactBootstrapLimits;
    use crate::resolver::entry::fixtures::FileFixture;
    use crate::resolver::evaluation::{
        Actual, BootstrapLimitExceeded, Code, IssueKey, LimitExceeded, Reason, Status,
    };
    use crate::resolver::evidence::Evidence;
    use crate::resolver::evidence_source::hex;
    use crate::resolver::evidence_token::EvidenceTokens;
    use crate::resolver::limits::{fixture_limits, SECTION};
    use crate::resolver::locales::fixtures::earlier;
    use crate::resolver::occurrence::{occurrences, redacted};
    use crate::resolver::token::{logical_member, Sha256, Specification};
    use crate::structural::selection::InvalidSelectorType;

    /// The Profile ID of the selected declaration.
    const SELECTED: &str = "web";

    /// Limits no test reaches.
    const ROOM: CollectionLimits = CollectionLimits {
        member_bytes: u64::MAX,
        total_bytes: u64::MAX,
        depth: u64::MAX,
    };

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
        read_collection(Some(CollectionReport::Formal(&bytes)), &ROOM).unwrap()
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

    fn profile(id: &str) -> ProfileId {
        serde_json::from_value(json!(id)).unwrap()
    }

    /// What Phases 1 to 3 measured, as a test states it.
    struct Measures {
        value: Admitted<ValueCounts>,
        declarations: Admitted<u64>,
        profile_ids: Admitted<Vec<ProfileId>>,
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
                declarations: Ok(2),
                profile_ids: Ok(vec![profile("app"), profile("storefront")]),
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
        /// References that failed structural admission, with why.
        failed: Vec<(ReferenceField, Causes)>,
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
                failed: Vec::new(),
            }
        }

        /// The references as Phase 4 reads them: those of [`Self::failed`]
        /// without bytes, or the causes of no selection.
        fn references(&self) -> Admitted<Vec<ReferenceOccurrence>> {
            let mut references = DeclarationReferences {
                policies: self.policies.as_ref().map_err(Causes::clone),
                target_profiles: self.targets.as_ref().map_err(Causes::clone),
            }
            .occurrences()?;
            for reference in &mut references {
                let failed = self
                    .failed
                    .iter()
                    .find(|(field, _)| reference.field.as_ref() == Some(field));
                if let Some((_, causes)) = failed {
                    reference.bytes = Err(causes.clone());
                }
            }
            Ok(references)
        }

        /// 018's input as trusted setup supplies it: it pins the selected
        /// reference, or another policy when there is none to pin.
        fn verification(&self) -> VerificationInput {
            let pin = self
                .resource_limits
                .as_ref()
                .ok()
                .map(PolicyReference::tuple)
                .filter(|tuple| tuple.kind == ArtifactKind::ResourceLimitPolicy)
                .unwrap_or_else(|| {
                    tuple_of(&envelope_of(
                        ArtifactKind::ResourceLimitPolicy,
                        "pinned-limits",
                        "1",
                        json!({}),
                    ))
                });
            VerificationInput::fixture_owned([pin])
        }

        /// The configuration a file holds with these inputs' references in
        /// the selected declaration, [`SELECTED`], which a check of Phase 4
        /// looks at: the selected reference even where the rest of `policies`
        /// was not admitted. Every other declared Profile ID, whose key a
        /// recheck looks at, declares nothing.
        fn config(&self) -> Value {
            let mut declaration = json!({});
            if let Ok(policies) = &self.policies {
                declaration["policies"] = serde_json::to_value(policies).unwrap();
            }
            if let Ok(reference) = &self.resource_limits {
                declaration["policies"]["resourceLimits"] =
                    serde_json::to_value(reference).unwrap();
            }
            if let Ok(targets) = &self.targets {
                declaration["targetProfiles"] = serde_json::to_value(targets).unwrap();
            }
            let mut profiles = serde_json::Map::new();
            profiles.insert(SELECTED.to_owned(), declaration);
            for id in self.measures.profile_ids.iter().flatten() {
                profiles
                    .entry(id.as_str().to_owned())
                    .or_insert_with(|| json!({}));
            }
            json!({ "profiles": profiles })
        }

        /// Runs `phase` with these inputs' references and where a check
        /// looks at them in the file holding [`Self::config`].
        fn observing<R>(&self, phase: impl FnOnce(Declared<'_, '_>) -> R) -> R {
            let bytes: Arc<[u8]> = Arc::from(serde_json::to_vec(&self.config()).unwrap());
            let document = materialize_file(Arc::clone(&bytes), input_limits()).unwrap();
            let NodeKind::Object(root) = document.node(document.root()).kind() else {
                unreachable!("the configuration is an object")
            };
            let profiles = root.get("profiles").unwrap().value();
            let fixture = FileFixture::new(&fixture_limits(10_000));
            let file = fixture.file(bytes);
            let observer =
                DeclarationObserver::new(&file, (&document, profiles), SELECTED).unwrap();
            let references = self.references();
            phase((references.as_deref(), Some(&observer)))
        }

        /// Phase 4 up to the comparison.
        fn run(&self) -> (Evaluation, Admitted<AdmittedResourcePolicy>) {
            self.run_pinning(&self.verification())
        }

        /// Phase 4 up to the comparison, under the Verification Input
        /// `verification`.
        fn run_pinning(
            &self,
            verification: &VerificationInput,
        ) -> (Evaluation, Admitted<AdmittedResourcePolicy>) {
            let mut evaluation = Evaluation::default();
            let collection = collection(&self.members);
            let admitted = self
                .observing(|declared| {
                    admit_and_compare(
                        &mut evaluation,
                        &self.capability,
                        &Submitted::of(&collection),
                        self.resource_limits.as_ref(),
                        declared,
                        verification,
                    )
                })
                .unwrap();
            (evaluation, admitted)
        }

        /// The whole of Phase 4.
        fn attempt(&self) -> (Evaluation, Result<PolicyOutcome, PolicyInvariant>) {
            let mut evaluation = Evaluation::default();
            let measures = ConfigurationMeasures {
                value: self.measures.value.clone(),
                declarations: self.measures.declarations.clone(),
                profile_ids: self.measures.profile_ids.as_deref().map_err(Causes::clone),
                selector: &self.measures.selector,
                structural_units: self.measures.structural_units.clone(),
            };
            let outcome = self.observing(|declared| {
                admit_resource_policy(
                    &mut evaluation,
                    &self.capability,
                    &collection(&self.members),
                    self.resource_limits.as_ref(),
                    declared,
                    &measures,
                    &self.verification(),
                )
            });
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

    /// The unit of an `artifactAdmission` bound under the capability.
    fn capability_unit(subject: ArtifactSubject) -> Unit {
        Stage::Capability.unit(subject)
    }

    /// The member keys the units of `values` record, in their order: a tuple
    /// without its semantic digest, or the key every member of one JSON type
    /// without a tuple shares (Decisions 015-174 and 015-177).
    fn keys_of(values: &[Value]) -> Vec<MemberKey> {
        collection(values)
            .members()
            .iter()
            .map(|member| logical_member(&member.key()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    fn comparison() -> Unit {
        unit(PolicySubject::ResourceLimits, Check::CompatiblePolicy)
    }

    /// The bootstrap's checks in their order (Decision 015-164).
    const BOOTSTRAP: [Check; 7] = [
        Check::WithinMetadataScan,
        Check::BootstrapCandidate,
        Check::WithinEnvelopeDepth,
        Check::WithinDecodedAllocation,
        Check::WithinValidationWork,
        Check::CandidateBody,
        Check::CompatiblePolicy,
    ];

    fn bootstrap_unit(check: Check) -> Unit {
        unit(PolicySubject::ResourceLimits, check)
    }

    /// `artifactBootstrap` bounds no test reaches, with `bound` set to `limit`.
    fn bootstrap_limits(bound: WorkBound, limit: u64) -> ArtifactBootstrapLimits {
        let largest = self::bound(u64::MAX - 1);
        let mut limits = ArtifactBootstrapLimits {
            max_envelope_depth: largest,
            max_decoded_allocation_bytes: largest,
            max_metadata_scan_units: largest,
            max_validation_work_units: largest,
        };
        let value = self::bound(limit);
        match bound {
            WorkBound::EnvelopeDepth => limits.max_envelope_depth = value,
            WorkBound::DecodedAllocation => limits.max_decoded_allocation_bytes = value,
            WorkBound::MetadataScan => limits.max_metadata_scan_units = value,
            WorkBound::ValidationWork => limits.max_validation_work_units = value,
        }
        limits
    }

    /// Each bound's check, the bound, and the total the default inputs' bootstrap
    /// counts under it.
    fn bootstrap_totals() -> [(Check, WorkBound, u64); 4] {
        let inputs = Inputs::new();
        let collection = collection(&inputs.members);
        let members = collection.members();
        let reference = inputs.resource_limits.as_ref().unwrap().tuple();
        let candidate = select_candidate(&reference, members).unwrap();
        [
            (
                Check::WithinMetadataScan,
                WorkBound::MetadataScan,
                work::metadata_scan_units(members),
            ),
            (
                Check::WithinEnvelopeDepth,
                WorkBound::EnvelopeDepth,
                work::envelope_depth(candidate.member().value()),
            ),
            (
                Check::WithinDecodedAllocation,
                WorkBound::DecodedAllocation,
                work::decoded_allocation_bytes(candidate.content()),
            ),
            (
                Check::WithinValidationWork,
                WorkBound::ValidationWork,
                work::validation_work_units(candidate.content()),
            ),
        ]
    }

    #[test]
    fn each_bootstrap_bound_admits_its_exact_total_and_reports_one_more() {
        // The policy, two other envelopes, a member with a kind and nothing
        // else, and one with no kind; the policy's six levels; and its 64 rules.
        let totals = bootstrap_totals();
        assert_eq!(totals[0].2, 5 + 5 + 5 + 2 + 1);
        assert_eq!(totals[1].2, 6);
        assert_eq!(totals[3].2, 64);
        for (check, work_bound, total) in totals {
            let mut exact = Inputs::new();
            exact.capability = exact
                .capability
                .clone()
                .with_artifact_bootstrap(bootstrap_limits(work_bound, total));
            let (evaluation, decided) = exact.run();
            assert!(decided.is_ok(), "{check:?}");
            assert_eq!(issues(&evaluation), [], "{check:?}");

            let mut over = Inputs::new();
            over.capability = over
                .capability
                .clone()
                .with_artifact_bootstrap(bootstrap_limits(work_bound, total - 1));
            let (evaluation, decided) = over.run();
            assert!(decided.is_err(), "{check:?}");
            let key = key_of(&evaluation, &bootstrap_unit(check));
            assert_eq!(key.code(), Code::BootstrapLimitExceeded, "{check:?}");
            assert_eq!(
                evaluation.bootstrap_limit(&key),
                Some(BootstrapLimitExceeded {
                    bound: work_bound.id(),
                    limit: bound(total - 1),
                    actual: Actual::Exact(total),
                }),
                "{check:?}"
            );
            // The work it protects, and every check after it, waits on it.
            let rank = BOOTSTRAP.iter().position(|each| *each == check).unwrap();
            for later in &BOOTSTRAP[rank + 1..] {
                assert_eq!(
                    waits_on(&evaluation, &bootstrap_unit(*later)),
                    std::slice::from_ref(&key),
                    "{check:?} {later:?}"
                );
            }
        }
    }

    #[test]
    fn candidate_selection_with_its_reference_checks_waits_on_the_scan() {
        // A reference to the Trust Policy fails the role-kind check, which
        // candidate selection makes after the scan.
        let mut wrong = Inputs::new();
        wrong.resource_limits = Ok(reference_to(&wrong.members[1]));
        let (evaluation, _) = wrong.run();
        assert_eq!(
            issues(&evaluation),
            [(
                decision(),
                Code::ArtifactReferenceNotAdmitted,
                Some(Reason::RoleKindMismatch)
            )]
        );
        let selection = key_of(&evaluation, &decision());
        for later in &BOOTSTRAP[2..] {
            assert_eq!(
                waits_on(&evaluation, &bootstrap_unit(*later)),
                std::slice::from_ref(&selection),
                "{later:?}"
            );
        }
        // Past the scan's bound, the reference's checks are not made at all.
        wrong.capability = wrong
            .capability
            .clone()
            .with_artifact_bootstrap(bootstrap_limits(WorkBound::MetadataScan, 1));
        let (evaluation, _) = wrong.run();
        let scan = key_of(&evaluation, &bootstrap_unit(Check::WithinMetadataScan));
        assert_eq!(evaluation.issues().collect::<Vec<_>>(), [&scan]);
        assert_eq!(waits_on(&evaluation, &decision()), [scan]);
    }

    #[test]
    fn a_policy_within_every_capability_bound_is_admitted_and_compatible() {
        let inputs = Inputs::new();
        let (evaluation, admitted) = inputs.run();
        assert_eq!(issues(&evaluation), []);
        let policy = admitted.unwrap();
        assert_eq!(policy.tuple(), &tuple_of(&chosen()));
        assert_eq!(policy.limits().bounds(), policy_limits().bounds());
        // Every unit was evaluated, in 015's table order: the collection's
        // three counts, the references' count, one unit per member key (three
        // tuples, then the one key that the two members establishing none
        // share, both being objects), the collection's
        // bytes, one unit per visible reference, and the references' bytes;
        // then the bootstrap: the metadata scan, candidate selection, the
        // candidate's depth, allocation, and validation work, its body
        // admission, and the comparison (Decision 015-164).
        let reference = |field| capability_unit(ArtifactSubject::Reference(field));
        let target = |id: &str| {
            reference(ReferenceField::TargetProfile(
                serde_json::from_value(json!(id)).unwrap(),
            ))
        };
        let keys = keys_of(&inputs.members);
        assert_eq!(
            keys.iter()
                .map(|key| matches!(key, MemberKey::Tuple(_)))
                .collect::<Vec<_>>(),
            [true, true, true, false]
        );
        let mut expected: Vec<Unit> = [
            ArtifactSubject::Artifacts,
            ArtifactSubject::PolicyArtifacts,
            ArtifactSubject::TargetProfileArtifacts,
            ArtifactSubject::References,
        ]
        .into_iter()
        .chain(keys.into_iter().map(ArtifactSubject::Artifact))
        .chain([ArtifactSubject::ArtifactBytes])
        .map(capability_unit)
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
        expected.push(capability_unit(ArtifactSubject::ReferenceBytes));
        expected.extend(
            [
                Check::WithinMetadataScan,
                Check::BootstrapCandidate,
                Check::WithinEnvelopeDepth,
                Check::WithinDecodedAllocation,
                Check::WithinValidationWork,
                Check::CandidateBody,
                Check::CompatiblePolicy,
            ]
            .map(|check| unit(PolicySubject::ResourceLimits, check)),
        );
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
    fn measured(subject: ArtifactSubject, edit: fn(&mut ArtifactAdmissionLimits)) -> u64 {
        let mut inputs = Inputs::new();
        inputs.capability = capability(edit);
        let (evaluation, _) = inputs.run();
        let key = key_of(&evaluation, &capability_unit(subject));
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
            measured(ArtifactSubject::Artifacts, |limits| limits.max_artifacts =
                bound(1)),
            5
        );
        // The Resource Limit Policy and the Trust Policy; neither the Target
        // Profile, the unsupported kind, nor the member declaring none.
        assert_eq!(
            measured(ArtifactSubject::PolicyArtifacts, |limits| {
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
            &capability_unit(ArtifactSubject::TargetProfileArtifacts),
        );
        assert_eq!(evaluation.limit(&key).unwrap().actual, Actual::Exact(2));
        // The canonical bytes of every occurrence.
        assert_eq!(
            measured(ArtifactSubject::ArtifactBytes, |limits| {
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
            // A member key stands for every body under it, so its unit reports
            // only the first-over witness.
            let actual = if id == MAX_SINGLE_ARTIFACT_CANONICAL_BYTES {
                Actual::AtLeast(exact)
            } else {
                Actual::Exact(exact)
            };
            assert_eq!(
                evaluation.limit(&found[0]),
                Some(LimitExceeded {
                    bound: id,
                    limit: bound(exact - 1),
                    actual,
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

    #[test]
    fn the_bodies_of_one_tuple_share_its_unit_and_report_only_the_first_over_witness() {
        // A second body of the selected policy's tuple, which only its
        // integrity digest tells apart.
        let mut inputs = Inputs::new();
        let mut tampered = chosen();
        tampered["integrityDigest"] = json!(format!("sha256:{}", "0".repeat(64)));
        inputs.members.push(tampered);
        let members = collection(&inputs.members);
        let bodies: BTreeSet<&[u8]> = members
            .members()
            .iter()
            .map(FormalMember::canonical_body)
            .collect();
        // The two members without a tuple share one key as well.
        assert_eq!((bodies.len(), keys_of(&inputs.members).len()), (6, 4));
        let largest = members
            .members()
            .iter()
            .map(FormalMember::canonical_bytes)
            .max()
            .unwrap();
        inputs.capability = capability(|limits| {
            limits.max_single_artifact_canonical_bytes = bound(largest - 1);
        });
        let (evaluation, _) = inputs.run();
        // One unit for both bodies, and one result for it.
        let tuple = logical_member(&MemberKey::Tuple(std::sync::Arc::new(tuple_of(&chosen()))));
        let crossed = capability_unit(ArtifactSubject::Artifact(tuple));
        assert_eq!(
            issues(&evaluation)
                .into_iter()
                .map(|(unit, _, _)| unit)
                .collect::<Vec<_>>(),
            std::slice::from_ref(&crossed)
        );
        assert_eq!(
            evaluation.limit(&key_of(&evaluation, &crossed)),
            Some(LimitExceeded {
                bound: MAX_SINGLE_ARTIFACT_CANONICAL_BYTES,
                limit: bound(largest - 1),
                actual: Actual::AtLeast(largest),
            })
        );
    }

    const PROFILE: Specification<'static> = Specification {
        identity: "intlify-project-profile",
        revision: "0",
    };

    /// A Finding Occurrence of Phase 4: its unit, the views of its primary
    /// and related Evidence, and its ordinal.
    type Observed = (Unit, Value, Vec<Value>, u64);

    /// Phase 4's Finding Occurrences in `evaluation`, in 015's order, each
    /// checked against its Finding Registry row, and the tokens they took.
    fn occurrences_of(evaluation: &Evaluation) -> (Vec<Observed>, EvidenceTokens) {
        let candidates = evaluation.occurrence_candidates();
        let tokens = EvidenceTokens::assign(PROFILE, redacted(candidates), &Sha256);
        let found = occurrences(candidates, PROFILE, &tokens)
            .unwrap()
            .iter()
            .filter(|occurrence| occurrence.owner().phase == Phase::ResourcePolicyAdmission)
            .map(|occurrence| {
                (
                    occurrence.owner().clone(),
                    occurrence.primary().view(),
                    occurrence.related().iter().map(Evidence::view).collect(),
                    occurrence.ordinal(),
                )
            })
            .collect();
        (found, tokens)
    }

    /// The view `projection` takes under `tokens`.
    fn view(
        projection: Result<EvidenceProjection, ComponentInvariant>,
        tokens: &EvidenceTokens,
    ) -> Value {
        projection
            .unwrap()
            .retained(PROFILE, tokens)
            .unwrap()
            .view()
    }

    /// Each collection bound under the capability is observed at the
    /// collection's root, beside the capability's bound (D15:834, 836).
    #[test]
    fn the_collection_bounds_under_the_capability_are_observed_at_its_root() {
        type Edit = fn(&mut ArtifactAdmissionLimits);
        let cases: [(ArtifactSubject, BoundId, Edit); 4] = [
            (ArtifactSubject::Artifacts, MAX_ARTIFACTS, |limits| {
                limits.max_artifacts = bound(1);
            }),
            (
                ArtifactSubject::PolicyArtifacts,
                MAX_POLICY_ARTIFACTS,
                |limits| {
                    limits.max_policy_artifacts = bound(1);
                },
            ),
            (
                ArtifactSubject::TargetProfileArtifacts,
                MAX_TARGET_PROFILE_ARTIFACTS,
                |limits| limits.max_target_profile_artifacts = bound(1),
            ),
            (
                ArtifactSubject::ArtifactBytes,
                MAX_TOTAL_ARTIFACT_CANONICAL_BYTES,
                |limits| limits.max_total_artifact_canonical_bytes = bound(1),
            ),
        ];
        for (subject, id, edit) in cases {
            let mut inputs = Inputs::new();
            inputs.members.push(envelope_of(
                ArtifactKind::TargetProfile,
                "storefront-app",
                "1",
                json!({}),
            ));
            inputs.capability = capability(edit);
            let (evaluation, _) = inputs.run();
            let (found, tokens) = occurrences_of(&evaluation);
            let at = capability_unit(subject);
            let root = view(collection_root(), &tokens);
            let limit = view(
                capability_bound(CapabilityBound::PolicyComparable(id)),
                &tokens,
            );
            assert_eq!(
                found
                    .into_iter()
                    .filter(|(unit, ..)| *unit == at)
                    .collect::<Vec<_>>(),
                [(at, root, vec![limit], 0)],
                "{}",
                id.name
            );
        }
    }

    /// Each member past one artifact's bound is observed at the member, under
    /// a key that keeps only its JSON type, beside the capability's bound: two
    /// bodies of one tuple are two occurrences of one Finding, told apart by
    /// their ordinals (D15:183, 834, 838, 2203; plan 025 interpretation
    /// #200).
    #[test]
    fn each_member_past_one_artifacts_bound_is_observed_at_the_member() {
        let mut inputs = Inputs::new();
        let mut tampered = chosen();
        tampered["integrityDigest"] = json!(format!("sha256:{}", "0".repeat(64)));
        inputs.members.push(tampered);
        let largest = collection(&inputs.members)
            .members()
            .iter()
            .map(FormalMember::canonical_bytes)
            .max()
            .unwrap();
        inputs.capability = capability(|limits| {
            limits.max_single_artifact_canonical_bytes = bound(largest - 1);
        });
        let (evaluation, _) = inputs.run();
        let (found, tokens) = occurrences_of(&evaluation);
        let tuple = logical_member(&MemberKey::Tuple(std::sync::Arc::new(tuple_of(&chosen()))));
        let at = capability_unit(ArtifactSubject::Artifact(tuple));
        let member = view(
            ComponentPath::artifact_member(Key::Unsafe(JsonType::Object))
                .map(EvidenceProjection::resolver_input),
            &tokens,
        );
        let limit = view(
            capability_bound(CapabilityBound::PolicyComparable(
                MAX_SINGLE_ARTIFACT_CANONICAL_BYTES,
            )),
            &tokens,
        );
        assert_eq!(
            found,
            [
                (at.clone(), member.clone(), vec![limit.clone()], 0),
                (at, member, vec![limit], 1),
            ]
        );
    }

    /// The metadata scan reads the whole collection, not through the
    /// reference, so it is observed at the collection's root, beside the
    /// capability's bootstrap bound (D15:834, 836).
    #[test]
    fn the_metadata_scan_is_observed_at_the_collection_beside_its_bound() {
        let total = bootstrap_totals()[0].2;
        let mut over = Inputs::new();
        over.capability = over
            .capability
            .clone()
            .with_artifact_bootstrap(bootstrap_limits(WorkBound::MetadataScan, total - 1));
        let (evaluation, _) = over.run();
        let (found, tokens) = occurrences_of(&evaluation);
        let limit = view(
            capability_bound(CapabilityBound::BootstrapOnly(WorkBound::MetadataScan.id())),
            &tokens,
        );
        assert_eq!(
            found,
            [(
                bootstrap_unit(Check::WithinMetadataScan),
                view(collection_root(), &tokens),
                vec![limit],
                0
            )]
        );
    }

    /// The declaration's reference count and its references' total bytes are
    /// observed at the selected declaration, beside the capability's bound
    /// (D15:790, 836, 1918, 1922).
    #[test]
    fn the_declarations_reference_bounds_are_observed_at_the_declaration() {
        type Edit = fn(&mut ArtifactAdmissionLimits);
        let cases: [(ArtifactSubject, BoundId, Edit); 2] = [
            (
                ArtifactSubject::References,
                MAX_ARTIFACT_REFERENCES,
                |limits| {
                    limits.max_artifact_references = bound(1);
                },
            ),
            (
                ArtifactSubject::ReferenceBytes,
                MAX_TOTAL_REFERENCE_CANONICAL_BYTES,
                |limits| limits.max_total_reference_canonical_bytes = bound(1),
            ),
        ];
        for (subject, id, edit) in cases {
            let mut inputs = Inputs::new();
            inputs.capability = capability(edit);
            let (evaluation, _) = inputs.run();
            let (found, tokens) = occurrences_of(&evaluation);
            let at = capability_unit(subject);
            let declaration =
                inputs.observing(|(_, observer)| observer.unwrap().value(&[]).unwrap());
            let limit = view(
                capability_bound(CapabilityBound::PolicyComparable(id)),
                &tokens,
            );
            assert_eq!(
                found
                    .into_iter()
                    .filter(|(unit, ..)| *unit == at)
                    .collect::<Vec<_>>(),
                [(at, view(Ok(declaration), &tokens), vec![limit], 0)],
                "{}",
                id.name
            );
        }
    }

    /// Each reference, the selected one included, is observed at its value
    /// under the selected declaration, beside the capability's bound for one
    /// reference (D15:790, 836, 1921).
    #[test]
    fn each_reference_is_observed_at_its_value() {
        let mut inputs = Inputs::new();
        inputs.capability = capability(|limits| {
            limits.max_single_reference_canonical_bytes = bound(1);
        });
        let (evaluation, _) = inputs.run();
        let (found, tokens) = occurrences_of(&evaluation);
        let limit = view(
            capability_bound(CapabilityBound::PolicyComparable(
                MAX_SINGLE_REFERENCE_CANONICAL_BYTES,
            )),
            &tokens,
        );
        // Each reference's value, at the pointer of its path from the root.
        let expected: Vec<(Unit, Value, Vec<Value>, u64)> = inputs.observing(|(references, _)| {
            references
                .unwrap()
                .iter()
                .map(|reference| {
                    let field = reference.field.clone().unwrap();
                    let pointer = match &field {
                        ReferenceField::Policy(slot) => {
                            format!("/profiles/{SELECTED}/policies/{}", slot.name())
                        }
                        ReferenceField::TargetProfile(id) => {
                            format!(
                                "/profiles/{SELECTED}/targetProfiles/{}/profile",
                                id.as_str()
                            )
                        }
                    };
                    (
                        capability_unit(ArtifactSubject::Reference(field)),
                        json!({ "exact-json-pointer": pointer }),
                        vec![limit.clone()],
                        0,
                    )
                })
                .collect()
        });
        // Six present policy references, `providerRouting` being `null`, and
        // two Target Profile references.
        assert_eq!(expected.len(), 8);
        assert_eq!(
            found
                .into_iter()
                .filter(|(unit, ..)| matches!(
                    unit.subject,
                    Subject::ResourcePolicy(PolicySubject::Capability(ArtifactSubject::Reference(
                        _
                    )))
                ))
                .map(|(unit, primary, related, ordinal)| {
                    assert_eq!(primary["origin"], "configuration-file", "{primary}");
                    (unit, primary["value"].clone(), related, ordinal)
                })
                .collect::<Vec<_>>(),
            expected
        );
    }

    /// A key's values past the bound are its candidates, each through what
    /// it is observed through, and its values within the bound are none
    /// (D15:2203).
    #[test]
    fn only_the_values_past_the_bound_are_candidates() {
        let through = |bound| Observation {
            primary: collection_root().unwrap(),
            related: vec![capability_bound(CapabilityBound::PolicyComparable(bound)).unwrap()],
        };
        let values = [
            (5, through(MAX_ARTIFACTS)),
            (9, through(MAX_POLICY_ARTIFACTS)),
            (7, through(MAX_TARGET_PROFILE_ARTIFACTS)),
        ];
        let tuple = logical_member(&MemberKey::Tuple(std::sync::Arc::new(tuple_of(&chosen()))));
        let mut evaluation = Evaluation::default();
        let held = witnessed_at(
            &mut evaluation,
            capability_unit(ArtifactSubject::Artifact(tuple)),
            (MAX_SINGLE_ARTIFACT_CANONICAL_BYTES, Ok(bound(6))),
            Ok(Tally::exactly(9)),
            &values,
        )
        .unwrap();
        assert!(held.is_err());
        let observed: Vec<&EvidenceProjection> = evaluation
            .occurrence_candidates()
            .iter()
            .flat_map(|candidate| &candidate.related)
            .collect();
        assert_eq!(observed, [&values[1].1.related[0], &values[2].1.related[0]]);
    }

    /// That `evidence` is the selected `resourceLimits` reference, at the
    /// pointer of its path from the root, through which a check from candidate
    /// selection on reaches the candidates (D15:790, 2249).
    fn assert_selected_reference(evidence: &Value) {
        assert_eq!(evidence["kind"], "configuration-value", "{evidence}");
        assert_eq!(evidence["origin"], "configuration-file", "{evidence}");
        assert_eq!(
            evidence["value"],
            json!({
                "exact-json-pointer": format!("/profiles/{SELECTED}/policies/resourceLimits")
            }),
            "{evidence}"
        );
    }

    /// The view under `tokens` of the chosen candidate, a collection member
    /// no Set admitted, under a key that keeps only its JSON type.
    fn chosen_view(tokens: &EvidenceTokens) -> Value {
        view(
            ComponentPath::artifact_member(Key::Unsafe(JsonType::Object))
                .map(EvidenceProjection::resolver_input),
            tokens,
        )
    }

    /// Each work bound after selection is observed at the chosen candidate,
    /// beside the selected reference and the capability's bootstrap bound
    /// (D15:834, 836, 2249; plan 025 interpretation #200).
    #[test]
    fn each_work_bound_after_selection_is_observed_at_the_candidate() {
        for (check, work_bound, total) in bootstrap_totals().into_iter().skip(1) {
            let mut over = Inputs::new();
            over.capability = over
                .capability
                .clone()
                .with_artifact_bootstrap(bootstrap_limits(work_bound, total - 1));
            let (evaluation, _) = over.run();
            let (found, tokens) = occurrences_of(&evaluation);
            let limit = view(
                capability_bound(CapabilityBound::BootstrapOnly(work_bound.id())),
                &tokens,
            );
            let [(unit, primary, related, ordinal)] = <[Observed; 1]>::try_from(found).unwrap();
            assert_eq!(unit, bootstrap_unit(check));
            assert_eq!(primary, chosen_view(&tokens), "{check:?}");
            assert_eq!(related.len(), 2, "{check:?}");
            assert_selected_reference(&related[0]);
            assert_eq!(related[1], limit, "{check:?}");
            assert_eq!(ordinal, 0, "{check:?}");
        }
    }

    /// The body is observed at the chosen candidate beside the selected
    /// reference, and beside the pins only where they do not authenticate
    /// the candidate (D15:2249).
    #[test]
    fn the_body_is_observed_beside_the_pins_only_where_they_do_not_authenticate_the_candidate() {
        // The pins authenticate a candidate whose integrity does not hold.
        let mut tampered = Inputs::new();
        tampered.members[0]["integrityDigest"] = json!(format!("sha256:{}", "0".repeat(64)));
        let authenticating = tampered.verification();
        // The pins name another policy than the intact candidate.
        let intact = Inputs::new();
        let elsewhere = VerificationInput::fixture_owned([tuple_of(&envelope_of(
            ArtifactKind::ResourceLimitPolicy,
            "pinned-limits",
            "1",
            json!({}),
        ))]);
        let pins = json!({
            "kind": "verification-input",
            "origin": "resource-limit-policy-verification",
            "component": [{ "fixed-role": "pins" }],
        });
        for (inputs, verification, beside) in [
            (&tampered, &authenticating, Vec::new()),
            (&intact, &elsewhere, vec![pins]),
        ] {
            let (evaluation, admitted) = inputs.run_pinning(verification);
            assert!(admitted.is_err());
            let key = key_of(&evaluation, &bootstrap_unit(Check::CandidateBody));
            assert_eq!(
                (key.code(), key.reason()),
                (
                    Code::ResourcePolicyNotAdmitted,
                    Some(Reason::BodyIntegrityInvalid)
                )
            );
            let (found, tokens) = occurrences_of(&evaluation);
            let [(unit, primary, related, ordinal)] = <[Observed; 1]>::try_from(found).unwrap();
            assert_eq!(unit, bootstrap_unit(Check::CandidateBody));
            assert_eq!(primary, chosen_view(&tokens));
            assert_selected_reference(&related[0]);
            assert_eq!(related[1..], beside[..]);
            assert_eq!(ordinal, 0);
        }
    }

    /// The view of the admitted `policy`, under the exact reference that keys
    /// it: `C` of the reference a declaration writes (D15:824, 834).
    fn policy_view(policy: &Value) -> Value {
        let reference = reference_value(&tuple_of(policy));
        json!({
            "kind": "artifact-input",
            "origin": "profile-resolution-artifact-collection",
            "component": [
                { "fixed-role": "artifact-member" },
                { "safe-key": hex(&encode(&reference).unwrap()) },
            ],
        })
    }

    /// The default inputs selecting the ample policy whose `artifactAdmission`
    /// bounds `edit` lowers within the roomy capability, and the view of the
    /// policy, which supplies each recheck's bound.
    fn rechecking(edit: impl FnOnce(&mut ArtifactAdmissionLimits)) -> (Inputs, Value) {
        let mut limits = ample();
        edit(&mut limits.artifact_admission);
        let policy = chosen_under(&limits);
        (Inputs::selecting(&policy), policy_view(&policy))
    }

    /// The occurrences of Phase 4's whole run over `inputs` at the units
    /// `at` keeps, with the run's tokens.
    fn rechecked_at(
        inputs: &Inputs,
        at: impl Fn(&Unit) -> bool,
    ) -> (Vec<Observed>, EvidenceTokens) {
        let (evaluation, _) = inputs.attempt();
        let (found, tokens) = occurrences_of(&evaluation);
        (
            found.into_iter().filter(|(unit, ..)| at(unit)).collect(),
            tokens,
        )
    }

    /// The default inputs selecting the ample policy whose
    /// `configurationInput` bounds `edit` lowers within the roomy capability,
    /// and the view of the policy, which supplies each recheck's bound.
    fn rechecking_input(edit: impl FnOnce(&mut ConfigurationInputLimits)) -> (Inputs, Value) {
        let mut limits = ample();
        edit(&mut limits.configuration_input);
        let policy = chosen_under(&limits);
        (Inputs::selecting(&policy), policy_view(&policy))
    }

    /// The view of the span `text` occupies in `source`, at its first
    /// occurrence.
    fn token_span(source: &[u8], text: &[u8]) -> Value {
        let start = source
            .windows(text.len())
            .position(|window| window == text)
            .unwrap();
        json!({ "start": start.to_string(), "end": (start + text.len()).to_string() })
    }

    /// The root's totals and the structural analysis are rechecked at the
    /// materialized root, beside the admitted policy (D15:790, 1880-1883,
    /// 1887, 2249).
    #[test]
    fn the_roots_totals_and_the_analysis_are_rechecked_at_the_root() {
        type Edit = fn(&mut ConfigurationInputLimits);
        let cases: [(PolicySubject, Edit); 5] = [
            (PolicySubject::Nodes, |limits| {
                limits.value.max_nodes = bound(1);
            }),
            (PolicySubject::Depth, |limits| {
                limits.value.max_depth = bound(1);
            }),
            (PolicySubject::CollectionEntries, |limits| {
                limits.value.max_collection_entries = bound(1);
            }),
            (PolicySubject::StringBytes, |limits| {
                limits.value.max_total_string_bytes = bound(1);
            }),
            (PolicySubject::StructuralAnalysis, |limits| {
                limits.structural.max_structural_analysis_units = bound(1);
            }),
        ];
        for (subject, edit) in cases {
            let (inputs, policy) = rechecking_input(edit);
            let at = recheck(subject.clone());
            let (found, _) = rechecked_at(&inputs, |unit| *unit == at);
            let [(unit, primary, related, ordinal)] = <[Observed; 1]>::try_from(found).unwrap();
            assert_eq!(unit, at);
            assert_eq!(primary["kind"], "configuration-value", "{subject:?}");
            assert_eq!(
                primary["value"],
                json!({ "exact-json-pointer": "" }),
                "{subject:?}"
            );
            assert_eq!((related, ordinal), (vec![policy], 0), "{subject:?}");
        }
    }

    /// The declarations are rechecked at the root's `profiles`, and each
    /// Profile ID past the policy's bound at its key's token, which keeps the
    /// classification the capability's bound gives it (D15:776, 790, 1885,
    /// 1886, 2249).
    #[test]
    fn the_declarations_and_each_long_profile_id_are_rechecked_at_their_tokens() {
        let (inputs, policy) = rechecking_input(|limits| {
            limits.structural.max_profiles = bound(1);
        });
        let at = recheck(PolicySubject::Profiles);
        let (found, _) = rechecked_at(&inputs, |unit| *unit == at);
        let [(_, primary, related, ordinal)] = <[Observed; 1]>::try_from(found).unwrap();
        assert_eq!(
            primary["value"],
            json!({ "exact-json-pointer": "/profiles" })
        );
        assert_eq!((related, ordinal), (vec![policy], 0));
        // "app" is within three bytes, and "storefront" is past them.
        let (inputs, policy) = rechecking_input(|limits| {
            limits.structural.max_profile_id_bytes = bound(3);
        });
        let (found, _) = rechecked_at(&inputs, |unit| {
            matches!(
                unit.subject,
                Subject::ResourcePolicy(PolicySubject::ProfileId(_))
            )
        });
        let source = serde_json::to_vec(&inputs.config()).unwrap();
        let [(unit, primary, related, ordinal)] = <[Observed; 1]>::try_from(found).unwrap();
        assert_eq!(
            unit,
            recheck(PolicySubject::ProfileId(profile("storefront")))
        );
        assert_eq!(
            primary["value"],
            json!({ "exact-json-pointer": "/profiles/storefront" })
        );
        assert_eq!(primary["span"], token_span(&source, br#""storefront""#));
        assert_eq!((related, ordinal), (vec![policy], 0));
    }

    /// Each string past the policy's single-string bound is rechecked at its
    /// token, beside the policy: here the declaration's semantic digests,
    /// the only strings longer than 64 bytes (D15:790, 1884, 2203, 2249).
    #[test]
    fn each_string_past_the_policy_bound_is_rechecked_at_its_token() {
        let (mut inputs, policy) = rechecking_input(|limits| {
            limits.value.max_single_string_bytes = bound(64);
        });
        let digest = u64::try_from(format!("sha256:{}", "0".repeat(64)).len()).unwrap();
        if let Ok(counts) = &mut inputs.measures.value {
            counts.single_string_bytes = digest;
        }
        let at = recheck(PolicySubject::LongestString);
        let (found, _) = rechecked_at(&inputs, |unit| *unit == at);
        // Six present policy references and two Target Profile references.
        assert_eq!(found.len(), 8);
        for (unit, primary, related, ordinal) in found {
            assert_eq!(unit, at);
            let pointer = primary["value"]["exact-json-pointer"].as_str().unwrap();
            assert!(
                pointer.starts_with(&format!("/profiles/{SELECTED}/"))
                    && pointer.ends_with("/semanticDigest"),
                "{pointer}"
            );
            assert_eq!((related, ordinal), (vec![policy.clone()], 0));
        }
    }

    /// A selector past the policy's bound is rechecked under it, its final
    /// projection authority, as over the limit, beside the declarations when
    /// they are available and the policy (D15:812, 816, 1889, 2249; plan 025
    /// interpretation #215).
    #[test]
    fn a_selector_past_the_policy_bound_is_rechecked_over_the_limit() {
        let at = recheck(PolicySubject::Selector);
        let (available, policy) = rechecking_input(|limits| {
            limits.structural.max_profile_id_bytes = bound(2);
        });
        let mut unavailable = Inputs::selecting(&chosen_under(&{
            let mut limits = ample();
            limits.configuration_input.structural.max_profile_id_bytes = bound(2);
            limits
        }));
        unavailable.measures.declarations = Err(earlier(Code::SpecificationInputIncomplete));
        for (inputs, declarations) in [(&available, true), (&unavailable, false)] {
            let (found, _) = rechecked_at(inputs, |unit| *unit == at);
            let [(_, primary, related, ordinal)] = <[Observed; 1]>::try_from(found).unwrap();
            assert_eq!(primary["kind"], "profile-selector");
            assert_eq!(primary["origin"], "profile-selector-cli");
            // A redacted selector's token stands for its type and length
            // alone.
            let mut kept = primary["selector"].clone();
            assert!(kept["redacted-value"]
                .as_object_mut()
                .unwrap()
                .remove("token")
                .is_some());
            assert_eq!(
                kept,
                json!({ "redacted-value": { "type": "string", "length": "over-limit" } })
            );
            let mut expected = Vec::new();
            if declarations {
                expected.push(json!({ "exact-json-pointer": "/profiles" }));
            }
            let values: Vec<Value> = related
                .iter()
                .filter(|view| view["kind"] == "configuration-value")
                .map(|view| view["value"].clone())
                .collect();
            assert_eq!(values, expected, "{declarations}");
            assert_eq!(related.last(), Some(&policy), "{declarations}");
            assert_eq!(related.len(), expected.len() + 1, "{declarations}");
            assert_eq!(ordinal, 0);
        }
    }

    /// The collection's rechecks past the admitted policy's bounds are
    /// observed at its root, as under the capability, beside the policy that
    /// supplies the bound (D15:834, 2249).
    #[test]
    fn the_collection_rechecks_are_observed_at_its_root_beside_the_policy() {
        type Edit = fn(&mut ArtifactAdmissionLimits);
        let cases: [(ArtifactSubject, Edit); 4] = [
            (ArtifactSubject::Artifacts, |limits| {
                limits.max_artifacts = bound(1);
            }),
            (ArtifactSubject::PolicyArtifacts, |limits| {
                limits.max_policy_artifacts = bound(1);
            }),
            (ArtifactSubject::TargetProfileArtifacts, |limits| {
                limits.max_target_profile_artifacts = bound(1);
            }),
            (ArtifactSubject::ArtifactBytes, |limits| {
                limits.max_total_artifact_canonical_bytes = bound(1);
            }),
        ];
        for (subject, edit) in cases {
            let (mut inputs, policy) = rechecking(edit);
            inputs.members.push(envelope_of(
                ArtifactKind::TargetProfile,
                "storefront-app",
                "1",
                json!({}),
            ));
            let at = artifact_recheck(subject.clone());
            let (found, tokens) = rechecked_at(&inputs, |unit| *unit == at);
            assert_eq!(
                found,
                [(at, view(collection_root(), &tokens), vec![policy], 0)],
                "{subject:?}"
            );
        }
    }

    /// Each member past one artifact's bound under the admitted policy is
    /// observed at the member, as under the capability, beside the policy
    /// (D15:834, 2249; plan 025 interpretation #200).
    #[test]
    fn each_member_past_one_artifacts_recheck_is_observed_at_the_member() {
        let (inputs, policy) = rechecking(|limits| {
            limits.max_single_artifact_canonical_bytes = bound(1);
        });
        let (found, tokens) = rechecked_at(&inputs, |unit| {
            matches!(
                unit.subject,
                Subject::ResourcePolicy(PolicySubject::Recheck(ArtifactSubject::Artifact(_)))
            )
        });
        // Every member is past the bound. The two members without a tuple
        // share one key, so their occurrences differ in their ordinals.
        assert_eq!(found.len(), inputs.members.len());
        let mut ordinals: std::collections::BTreeMap<Unit, Vec<u64>> =
            std::collections::BTreeMap::new();
        for (unit, primary, related, ordinal) in found {
            assert_eq!(primary, chosen_view(&tokens));
            assert_eq!(related, std::slice::from_ref(&policy));
            ordinals.entry(unit).or_default().push(ordinal);
        }
        let mut keys: Vec<Vec<u64>> = ordinals.into_values().collect();
        keys.sort();
        assert_eq!(keys, [vec![0], vec![0], vec![0], vec![0, 1]]);
    }

    /// The declaration's reference count and bytes, and each reference, are
    /// rechecked at the declaration and at each reference's value, as under
    /// the capability, beside the admitted policy (D15:790, 1918, 1921, 1922,
    /// 2249).
    #[test]
    fn the_declarations_reference_rechecks_are_observed_beside_the_policy() {
        type Edit = fn(&mut ArtifactAdmissionLimits);
        let declared: [(ArtifactSubject, Edit); 2] = [
            (ArtifactSubject::References, |limits| {
                limits.max_artifact_references = bound(1);
            }),
            (ArtifactSubject::ReferenceBytes, |limits| {
                limits.max_total_reference_canonical_bytes = bound(1);
            }),
        ];
        for (subject, edit) in declared {
            let (inputs, policy) = rechecking(edit);
            let at = artifact_recheck(subject.clone());
            let (found, _) = rechecked_at(&inputs, |unit| *unit == at);
            let [(unit, primary, related, ordinal)] = <[Observed; 1]>::try_from(found).unwrap();
            assert_eq!(unit, at);
            assert_eq!(
                primary["value"],
                json!({ "exact-json-pointer": format!("/profiles/{SELECTED}") }),
                "{subject:?}"
            );
            assert_eq!((related, ordinal), (vec![policy], 0), "{subject:?}");
        }
        let (inputs, policy) = rechecking(|limits| {
            limits.max_single_reference_canonical_bytes = bound(1);
        });
        let (found, _) = rechecked_at(&inputs, |unit| {
            matches!(
                unit.subject,
                Subject::ResourcePolicy(PolicySubject::Recheck(ArtifactSubject::Reference(_)))
            )
        });
        // Six present policy references and two Target Profile references,
        // each at the pointer of its value.
        assert_eq!(found.len(), 8);
        for (unit, primary, related, ordinal) in found {
            let Subject::ResourcePolicy(PolicySubject::Recheck(ArtifactSubject::Reference(field))) =
                &unit.subject
            else {
                unreachable!()
            };
            let pointer = match field {
                ReferenceField::Policy(slot) => {
                    format!("/profiles/{SELECTED}/policies/{}", slot.name())
                }
                ReferenceField::TargetProfile(id) => {
                    format!(
                        "/profiles/{SELECTED}/targetProfiles/{}/profile",
                        id.as_str()
                    )
                }
            };
            assert_eq!(primary["value"], json!({ "exact-json-pointer": pointer }));
            assert_eq!((related, ordinal), (vec![policy.clone()], 0));
        }
    }

    /// The comparison is observed at the admitted Policy, under the exact
    /// reference that keys it, beside the selected reference and the
    /// capability's bound it needs more of first (D15:824, 834, 2249, 2355).
    #[test]
    fn the_comparison_is_observed_at_the_admitted_policy() {
        let mut short = policy_limits();
        short.artifact_admission = fixture_limits(10_000).artifact_admission;
        short.locale_resolution.max_requested_locales = bound(10);
        short.configuration_input.value.max_depth = bound(1);
        let mut lacking = Inputs::new();
        lacking.capability = ImplementationCapability::fixture_owned(short);
        let (evaluation, _) = lacking.run();
        let (found, tokens) = occurrences_of(&evaluation);
        let limit = view(
            capability_bound(CapabilityBound::PolicyComparable(BoundId {
                group: Group::ConfigurationInput,
                name: "maxDepth",
            })),
            &tokens,
        );
        let [(unit, primary, related, ordinal)] = <[Observed; 1]>::try_from(found).unwrap();
        assert_eq!(unit, comparison());
        assert_eq!(primary, policy_view(&chosen()));
        assert_eq!(related.len(), 2);
        assert_selected_reference(&related[0]);
        assert_eq!(related[1], limit);
        assert_eq!(ordinal, 0);
    }

    /// A reference that names nothing admissible is observed at the
    /// reference, beside what selection compared it with: nothing when its
    /// own checks fail, the collection as one location when no member shares
    /// even its kind or identity, and otherwise the closest tier's candidates
    /// (D15:834, 2249).
    #[test]
    fn a_reference_naming_nothing_admissible_is_observed_at_the_reference() {
        // What a case relates to the reference, under the run's tokens.
        type Related = fn(&EvidenceTokens) -> Vec<Value>;
        // The Trust Policy's reference fails the role-kind check.
        let mut wrong = Inputs::new();
        wrong.resource_limits = Ok(reference_to(&wrong.members[1]));
        // No submitted artifact shares the policy's kind and identity.
        let mut missing = Inputs::new();
        missing.members.remove(0);
        // Only another revision of the policy is submitted.
        let mut revised = Inputs::new();
        revised.members[0] = envelope_of(
            ArtifactKind::ResourceLimitPolicy,
            "storefront-limits",
            "2",
            json!({ SECTION: policy_limits().section() }),
        );
        let cases: [(&Inputs, Reason, Related); 3] = [
            (&wrong, Reason::RoleKindMismatch, |_| Vec::new()),
            (&missing, Reason::NotFound, |tokens| {
                vec![view(collection_root(), tokens)]
            }),
            (&revised, Reason::RevisionMismatch, |tokens| {
                vec![chosen_view(tokens)]
            }),
        ];
        for (inputs, reason, related) in cases {
            let (evaluation, admitted) = inputs.run();
            assert!(admitted.is_err());
            let key = key_of(&evaluation, &decision());
            assert_eq!(
                (key.code(), key.reason()),
                (Code::ArtifactReferenceNotAdmitted, Some(reason))
            );
            let (found, tokens) = occurrences_of(&evaluation);
            let [(unit, primary, observed, ordinal)] = <[Observed; 1]>::try_from(found).unwrap();
            assert_eq!(unit, decision(), "{reason:?}");
            assert_selected_reference(&primary);
            assert_eq!(observed, related(&tokens), "{reason:?}");
            assert_eq!(ordinal, 0, "{reason:?}");
        }
    }

    /// Each occurrence of a selected group that is not one candidate, a
    /// duplicate or a conflicting body, is observed at the member beside the
    /// reference: one redacted member, whose occurrences differ only in their
    /// ordinals (D15:834, 2249).
    #[test]
    fn each_occurrence_of_a_rejected_group_is_observed_at_the_member() {
        let mut repeated = Inputs::new();
        repeated.members.push(chosen());
        let mut conflicting = Inputs::new();
        let mut tampered = chosen();
        tampered["integrityDigest"] = json!(format!("sha256:{}", "0".repeat(64)));
        conflicting.members.push(tampered);
        for (inputs, reason) in [
            (&repeated, Reason::DuplicateEnvelope),
            (&conflicting, Reason::ConflictingContent),
        ] {
            let (evaluation, admitted) = inputs.run();
            assert!(admitted.is_err());
            let key = key_of(&evaluation, &decision());
            assert_eq!(
                (key.code(), key.reason()),
                (Code::ArtifactCollectionNotAdmitted, Some(reason))
            );
            let (found, tokens) = occurrences_of(&evaluation);
            assert_eq!(found.len(), 2, "{reason:?}");
            for (expected, (unit, primary, related, ordinal)) in (0..).zip(found) {
                assert_eq!(unit, decision(), "{reason:?}");
                assert_eq!(primary, chosen_view(&tokens), "{reason:?}");
                assert_eq!(related.len(), 1, "{reason:?}");
                assert_selected_reference(&related[0]);
                assert_eq!(ordinal, expected, "{reason:?}");
            }
        }
    }

    fn reference_unit(field: ReferenceField) -> Unit {
        capability_unit(ArtifactSubject::Reference(field))
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
        let count = key_of(&evaluation, &capability_unit(ArtifactSubject::References));
        assert_eq!(evaluation.limit(&count).unwrap().actual, Actual::Exact(8));
        let total = key_of(
            &evaluation,
            &capability_unit(ArtifactSubject::ReferenceBytes),
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

    /// The total is of the references with bytes, exact, and is not evaluated
    /// only when none has any (D15:1924, 2006, 3354).
    #[test]
    fn the_total_waits_only_when_every_reference_is_excluded() {
        let of = |bytes| ReferenceOccurrence {
            field: None,
            occurs: true,
            bytes,
        };
        let one = earlier(Code::SpecificationInputIncomplete);
        let other = earlier(Code::SpecificationNotAdmitted);
        assert_eq!(reference_total(&[]), Ok(Tally::of(0)));
        assert_eq!(
            reference_total(&[of(Ok(3)), of(Err(one.clone())), of(Ok(4))]),
            Ok(Tally::of(7))
        );
        assert_eq!(
            reference_total(&[of(Err(one.clone())), of(Err(other.clone()))]),
            Err(one.union(other))
        );
    }

    #[test]
    fn a_reference_that_failed_counts_without_its_bytes() {
        let failed = earlier(Code::SpecificationInputIncomplete);
        let trust = ReferenceField::Policy(PolicySlot::Trust);
        let mut inputs = Inputs::new();
        inputs.failed = vec![(trust.clone(), failed.clone())];
        let (evaluation, admitted) = inputs.run();
        assert_eq!(waits_on(&evaluation, &reference_unit(trust)), keys(&failed));
        assert!(admitted.is_ok());
        // It still counts, and the others' bytes are an exact total.
        let references = inputs.references().unwrap();
        let total = references.iter().flat_map(|each| each.bytes.as_ref());
        let (count, total) = (references.len() as u64, total.sum::<u64>());
        inputs.capability = capability(|limits| limits.max_artifact_references = bound(count - 1));
        let (evaluation, _) = inputs.run();
        let key = key_of(&evaluation, &capability_unit(ArtifactSubject::References));
        assert_eq!(evaluation.limit(&key).unwrap().actual, Actual::Exact(count));
        inputs.capability = capability(|limits| {
            limits.max_total_reference_canonical_bytes = bound(total - 1);
        });
        let (evaluation, _) = inputs.run();
        let key = key_of(
            &evaluation,
            &capability_unit(ArtifactSubject::ReferenceBytes),
        );
        assert_eq!(evaluation.limit(&key).unwrap().actual, Actual::Exact(total));
        inputs.capability = capability(|limits| {
            limits.max_total_reference_canonical_bytes = bound(total);
        });
        assert_eq!(issues(&inputs.run().0), []);
    }

    #[test]
    fn without_the_selected_reference_only_the_collection_and_the_visible_references_are_checked() {
        let unselected = earlier(Code::SpecificationInputIncomplete);
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
        for subject in [ArtifactSubject::Artifacts, ArtifactSubject::ArtifactBytes] {
            assert_eq!(
                evaluation.status(&capability_unit(subject)),
                Some(&Status::Evaluated)
            );
        }
        assert!(admitted.is_err());
        inputs.policies = Err(unselected);
        let (evaluation, _) = inputs.run();
        for subject in [ArtifactSubject::References, ArtifactSubject::ReferenceBytes] {
            assert_eq!(waits_on(&evaluation, &capability_unit(subject)), causes);
        }
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
        // total. The two members without a tuple share one unit too.
        let artifacts = evaluation
            .statuses()
            .filter(|(unit, _)| {
                matches!(
                    unit.subject,
                    Subject::ResourcePolicy(PolicySubject::Capability(ArtifactSubject::Artifact(
                        _
                    )))
                )
            })
            .count();
        assert_eq!(artifacts, 4);
        // Every occurrence still counts toward the occurrences and the bytes.
        let mut counted = Inputs::new();
        counted.members.push(chosen());
        counted.capability = capability(|limits| {
            limits.max_artifacts = bound(1);
            limits.max_total_artifact_canonical_bytes = bound(1);
        });
        let (evaluation, _) = counted.run();
        let occurrences = key_of(&evaluation, &capability_unit(ArtifactSubject::Artifacts));
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
            &capability_unit(ArtifactSubject::ArtifactBytes),
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

    /// The Policy recheck of an `artifactAdmission` bound.
    fn artifact_recheck(subject: ArtifactSubject) -> Unit {
        Stage::Policy(None).unit(subject)
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
            .map(|field| PolicySubject::Recheck(ArtifactSubject::Reference(field)))
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
            PolicySubject::ProfileId(profile("app")),
            PolicySubject::ProfileId(profile("storefront")),
            PolicySubject::Selector,
            PolicySubject::StructuralAnalysis,
        ];
        let again = PolicySubject::Recheck;
        expected.extend(
            [
                ArtifactSubject::Artifacts,
                ArtifactSubject::PolicyArtifacts,
                ArtifactSubject::TargetProfileArtifacts,
                ArtifactSubject::References,
            ]
            .map(again),
        );
        let inputs = under(|_| {});
        expected.extend(
            keys_of(&inputs.members)
                .into_iter()
                .map(|key| again(ArtifactSubject::Artifact(key))),
        );
        expected.push(again(ArtifactSubject::ArtifactBytes));
        expected.extend(reference_subjects());
        expected.push(again(ArtifactSubject::ReferenceBytes));
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
                PolicySubject::ProfileId(profile("storefront")),
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
            // Each string is checked on its own, so the longest reports only
            // the first-over witness.
            let actual = if subject == PolicySubject::LongestString {
                Actual::AtLeast(exact)
            } else {
                Actual::Exact(exact)
            };
            assert_eq!(found[0].unit(), &recheck(subject), "{}", id.name);
            assert_eq!(
                evaluation.limit(&found[0]),
                Some(LimitExceeded {
                    bound: id,
                    limit: bound(exact - 1),
                    actual,
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
        assert_eq!(outcome.authority, bound(50));
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
        assert_eq!(outcome.authority, bound(200));
        assert_eq!(outcome.selector_length, Some(StringLengthFact::Exact(100)));
        // The two members without a tuple share one recheck.
        let subjects = rechecked(&evaluation);
        assert_eq!(subjects.len(), 28);
        for subject in subjects {
            assert_eq!(
                waits_on(&evaluation, &recheck(subject)),
                std::slice::from_ref(&incompatible)
            );
        }
        // A policy the capability cannot run takes over no reporting.
        assert_eq!(outcome.diagnostics, None);
        assert_eq!(keys(&outcome.confirmed.unwrap_err()), [incompatible]);
        // Without an admitted policy at all, the capability is the authority
        // too.
        let mut missing = selecting(capacity);
        missing.members.remove(0);
        let (evaluation, outcome) = missing.confirm();
        let undecided = key_of(&evaluation, &decision());
        assert_eq!(outcome.authority, bound(200));
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
            let found: Vec<IssueKey> = evaluation.issues().cloned().collect();
            let actuals: Vec<Actual> = found
                .iter()
                .map(|key| {
                    assert_eq!(key.unit().check, Check::WithinBound, "{}", id.name);
                    let exceeded = evaluation.limit(key).unwrap();
                    assert_eq!(exceeded.bound, id);
                    exceeded.actual
                })
                .collect();
            let exactly = |counts: Vec<u64>| counts.into_iter().map(Actual::Exact).collect();
            let expected: Vec<Actual> = match id.name {
                "maxArtifacts" => exactly(vec![6]),
                "maxPolicyArtifacts" | "maxTargetProfileArtifacts" => exactly(vec![2]),
                "maxArtifactReferences" => exactly(vec![8]),
                // One unit per member key, each with the first-over witness.
                "maxSingleArtifactCanonicalBytes" => keys_of(&inputs.members)
                    .iter()
                    .map(|_| Actual::AtLeast(2))
                    .collect(),
                "maxTotalArtifactCanonicalBytes" => exactly(vec![bytes.iter().sum()]),
                "maxSingleReferenceCanonicalBytes" => exactly(references.clone()),
                _ => exactly(vec![references.iter().sum()]),
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
        let root = earlier(Code::SpecificationInputIncomplete);
        let declarations = earlier(Code::SpecificationNotAdmitted);
        let work = earlier(Code::LocaleSpecificationNotAdmitted);
        let fields = earlier(Code::LocaleDataNotAdmitted);
        let mut inputs = under(|_| {});
        inputs.measures.value = Err(root.clone());
        inputs.measures.declarations = Err(declarations.clone());
        inputs.measures.profile_ids = Err(declarations.clone());
        inputs.measures.structural_units = Err(work.clone());
        let trust = ReferenceField::Policy(PolicySlot::Trust);
        inputs.failed = vec![(trust.clone(), fields.clone())];
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
        // A reference that failed waits alone: the count and the total hold
        // over the others (D15:1918, 1924; plan 025 decision #342).
        assert_eq!(
            waits_on(
                &evaluation,
                &artifact_recheck(ArtifactSubject::Reference(trust.clone()))
            ),
            keys(&fields)
        );
        for subject in [ArtifactSubject::References, ArtifactSubject::ReferenceBytes] {
            assert_eq!(
                evaluation.status(&artifact_recheck(subject)),
                Some(&Status::Evaluated)
            );
        }
        // The policy is admitted and compatible, but not confirmed: its
        // diagnostic bounds take over reporting all the same.
        assert_eq!(outcome.diagnostics, Some(ample().diagnostics));
        let every = root.clone().union(declarations).union(work);
        assert_eq!(keys(&outcome.confirmed.unwrap_err()), keys(&every));
        // Without the policy as well, a recheck waits on both.
        let unselected = earlier(Code::NegotiationSpecificationNotAdmitted);
        inputs.resource_limits = Err(unselected.clone());
        let (evaluation, _) = inputs.confirm();
        assert_eq!(
            waits_on(&evaluation, &recheck(PolicySubject::Nodes)),
            keys(&root.union(unselected.clone()))
        );
        assert_eq!(
            waits_on(&evaluation, &artifact_recheck(ArtifactSubject::References)),
            keys(&unselected)
        );
        assert_eq!(
            waits_on(
                &evaluation,
                &artifact_recheck(ArtifactSubject::Reference(trust.clone()))
            ),
            keys(&fields.clone().union(unselected))
        );
        // A reference that failed holds no confirmation back on its own.
        let mut alone = under(|_| {});
        alone.failed = vec![(trust, fields)];
        assert!(alone.confirm().1.confirmed.is_ok());
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
