// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Which checks the resolver evaluated, which it could not, and why. Not 015's
//! Check or Finding Registry, a Finding Key, a Redacted subject token, or an
//! evaluation-status record: 015 Phase 5 owns those representations. A unit
//! here is one check over one subject, an issue key names the blocking result a
//! unit found, and a unit whose prerequisite is unavailable is recorded as not
//! evaluated with the keys behind it instead of reporting a cascade issue. A
//! resource issue keeps the details 015 reports with it, and an evaluated unit
//! may also find a non-blocking result, which suppresses nothing.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use crate::input_limits::Bound;

use super::artifact_resolution::CollectionSubject;
use super::coverage::CoverageSubject;
use super::fallback::FallbackSubject;
use super::groups::GroupSubject;
use super::hydration::HydrationSubject;
use super::identity::DeclarationField;
use super::limits::BoundId;
use super::locales::LocaleSubject;
use super::negotiation::NegotiationSubject;
use super::resource_policy::{PolicySubject, ReferenceField};
use super::specification::Slot;
use super::targets::TargetSubject;

/// 015's resolution phases, declared in rank order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Phase {
    ResolverBootstrapAdmission,
    EntryAdmission,
    StructuralAdmission,
    ProfileSelection,
    ResourcePolicyAdmission,
    SpecificationAdmission,
    ProjectIdentityResolution,
    LocaleResolution,
    LocalePolicyResolution,
    ArtifactReferenceResolution,
    DeliveryResolution,
    TargetResolution,
    GroupResolution,
    HydrationValidation,
    ProfileProjection,
    ReportingFinalization,
}

impl Phase {
    /// 015's stable phase ID.
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::ResolverBootstrapAdmission => "resolver-bootstrap-admission",
            Self::EntryAdmission => "entry-admission",
            Self::StructuralAdmission => "structural-admission",
            Self::ProfileSelection => "profile-selection",
            Self::ResourcePolicyAdmission => "resource-policy-admission",
            Self::SpecificationAdmission => "specification-admission",
            Self::ProjectIdentityResolution => "project-identity-resolution",
            Self::LocaleResolution => "locale-resolution",
            Self::LocalePolicyResolution => "locale-policy-resolution",
            Self::ArtifactReferenceResolution => "artifact-reference-resolution",
            Self::DeliveryResolution => "delivery-resolution",
            Self::TargetResolution => "target-resolution",
            Self::GroupResolution => "group-resolution",
            Self::HydrationValidation => "hydration-validation",
            Self::ProfileProjection => "profile-projection",
            Self::ReportingFinalization => "reporting-finalization",
        }
    }
}

/// What one unit checks: 015's subject kind and Logical subject key. Kinds are
/// declared in rank order, so subjects order by kind and then by key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Subject {
    /// One slot of the Profile Resolution Specification Input.
    Slot(Slot),
    /// The Intent Surface-Class Vocabulary Artifact, a singleton.
    Vocabulary,
    /// The Locale Canonicalization Data Artifact, a singleton.
    CanonicalizationData,
    /// A Resource Limit Policy bound over the configuration, the collection,
    /// or the selected declaration's references, or the selected
    /// `resourceLimits` reference.
    ResourcePolicy(PolicySubject),
    /// One field of the selected profile declaration.
    DeclarationField(DeclarationField),
    /// A locale, or a locale bound or set, of the selected declaration.
    Locale(LocaleSubject),
    /// The declaration's negotiation aliases, or one alias key or destination.
    Negotiation(NegotiationSubject),
    /// The declaration's message fallback, or one source or its candidates.
    Fallback(FallbackSubject),
    /// The declaration's coverage rules, a rule's selector item, a normalized
    /// domain, or the decision table and one of its cells.
    Coverage(CoverageSubject),
    /// One distinct submitted body, or one group of tuples differing only in
    /// their digest, of the complete artifact collection.
    ArtifactCollection(CollectionSubject),
    /// One present reference of the selected declaration, resolved against
    /// the Profile Resolution Artifact Set.
    ArtifactReference(ReferenceField),
    /// The declaration's delivery placement, a singleton.
    DeliveryPlacement,
    /// The declaration's Target ID map, or one Target ID.
    Target(TargetSubject),
    /// The declaration's groups and their members, one group or member, or a
    /// Target ID's place among the groups.
    Group(GroupSubject),
    /// The declaration's hydration relations, one relation, or a Target ID's
    /// roles within one group.
    Hydration(HydrationSubject),
}

/// The check a unit runs, declared in rank order within a subject kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Check {
    /// The slot holds an assertion the resolver supports.
    SupportedAssertion,
    /// The data is the dataset the admitted specification admits.
    MatchingData,
    /// The artifact's integrity holds and its members are valid, unique, and
    /// sorted.
    IntactArtifact,
    /// The artifact is the one its slot asserts.
    AssertedArtifact,
    /// The field is admitted as an opaque semantic identity.
    OpaqueIdentity,
    /// A counted or measured total stays within the capability's value of its
    /// bound, before a Resource Limit Policy is admitted.
    WithinCapability,
    /// A counted or measured total stays within its bound.
    WithinBound,
    /// The spelling is a valid locale and has a canonical form.
    CanonicalLocale,
    /// No other spelling in the same set names the same locale.
    UniqueLocale,
    /// The locale belongs to the project's requested locales.
    RequestedMember,
    /// A literal fallback candidate is not its own sequence's source.
    DistinctFromSource,
    /// A fallback candidate occurs once in its sequence.
    UniqueCandidate,
    /// The placement, or a coverage cell, resolves to its effective mode.
    EffectiveMode,
    /// The surface class is a member of the admitted vocabulary.
    VocabularyMember,
    /// No other item in the same selector names the same surface class.
    UniqueSurfaceClass,
    /// No other rule normalizes to the same domain.
    UniqueDomain,
    /// Exactly one submitted artifact is the Resource Limit Policy the
    /// reference names, and its body is admitted.
    BootstrapCandidate,
    /// The admitted policy needs no bound past the capability's value.
    CompatiblePolicy,
    /// The member declares an admitted kind, establishes its tuple, and its
    /// body is admitted.
    AdmissibleMember,
    /// The member was submitted once.
    SingleOccurrence,
    /// The group's members have one canonical body.
    ConsistentContent,
    /// The reference is of its role's kind and a supported specification
    /// revision, and names an artifact of the Set.
    ResolvedReference,
    /// Every requested locale of the target is one of the project's.
    ProjectSubset,
    /// The target's effective default is one of its requested locales.
    SupportedDefault,
    /// The member names a declared Target ID.
    KnownMember,
    /// The member occurs once in its group.
    UniqueMember,
    /// The Target ID is a member of exactly one group.
    SingleGroup,
    /// Both endpoints of the relation name declared Target IDs.
    KnownEndpoints,
    /// The relation's endpoints are two Target IDs.
    DistinctEndpoints,
    /// Both endpoints are members of the relation's group.
    SameGroupEndpoints,
    /// The relation occurs once in its group.
    UniqueRelation,
    /// The server endpoint's Target Profile renders server-side.
    ServerRole,
    /// The client endpoint's Target Profile hydrates in the browser.
    ClientRole,
    /// Both endpoints request the same locales.
    SameRequestedLocales,
    /// Both endpoints have the same effective default.
    SameEffectiveDefault,
    /// Nothing statically known makes the two Target Profiles incompatible.
    StaticCompatibility,
    /// The Target ID takes one role within its group's relations.
    SingleRole,
}

/// One atomic evaluation unit. Units order as 015 evaluates and commits them:
/// phase, subject kind, subject key, then check.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Unit {
    pub(crate) phase: Phase,
    pub(crate) subject: Subject,
    pub(crate) check: Check,
}

/// The 015 Finding codes these checks can report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Code {
    SpecificationInputIncomplete,
    SpecificationNotAdmitted,
    LocaleSpecificationNotAdmitted,
    LocaleDataNotAdmitted,
    NegotiationSpecificationNotAdmitted,
    MessageFallbackSpecificationNotAdmitted,
    CoverageSpecificationNotAdmitted,
    DeliverySpecificationNotAdmitted,
    IntentSurfaceClassVocabularyNotAdmitted,
    LocaleInvalid,
    LocaleNonCanonical,
    LocaleDuplicate,
    DefaultRequestedLocaleNotInProject,
    TargetLocalesNotSubset,
    TargetDefaultLocaleNotSupported,
    NegotiationAliasDuplicate,
    NegotiationAliasDestinationNotRequested,
    MessageFallbackSourceNotRequested,
    MessageFallbackSequenceInvalid,
    CoverageRuleInvalid,
    CoverageRuleDuplicate,
    CoverageRuleConflict,
    ArtifactReferenceNotAdmitted,
    ArtifactCollectionNotAdmitted,
    GroupMemberUnknown,
    GroupMemberDuplicate,
    TargetUnassigned,
    TargetAssignedToMultipleGroups,
    HydrationRelationInvalid,
    HydrationLocalePolicyIncompatible,
    HydrationCapabilityIncompatible,
    ResourcePolicyNotAdmitted,
    ResourcePolicyCapabilityIncompatible,
    ResourceLimitExceeded,
}

impl Code {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::SpecificationInputIncomplete => "project-profile-specification-input-incomplete",
            Self::SpecificationNotAdmitted => "project-profile-specification-not-admitted",
            Self::LocaleSpecificationNotAdmitted => {
                "project-profile-locale-specification-not-admitted"
            }
            Self::LocaleDataNotAdmitted => "project-profile-locale-data-not-admitted",
            Self::NegotiationSpecificationNotAdmitted => {
                "project-profile-negotiation-specification-not-admitted"
            }
            Self::MessageFallbackSpecificationNotAdmitted => {
                "project-profile-message-fallback-specification-not-admitted"
            }
            Self::CoverageSpecificationNotAdmitted => {
                "project-profile-coverage-specification-not-admitted"
            }
            Self::DeliverySpecificationNotAdmitted => {
                "project-profile-delivery-specification-not-admitted"
            }
            Self::IntentSurfaceClassVocabularyNotAdmitted => {
                "project-profile-intent-surface-class-vocabulary-not-admitted"
            }
            Self::LocaleInvalid => "project-profile-locale-invalid",
            Self::LocaleNonCanonical => "project-profile-locale-non-canonical",
            Self::LocaleDuplicate => "project-profile-locale-duplicate",
            Self::DefaultRequestedLocaleNotInProject => {
                "project-profile-default-requested-locale-not-in-project"
            }
            Self::TargetLocalesNotSubset => "project-profile-target-locales-not-subset",
            Self::TargetDefaultLocaleNotSupported => {
                "project-profile-target-default-locale-not-supported"
            }
            Self::NegotiationAliasDuplicate => "project-profile-negotiation-alias-duplicate",
            Self::NegotiationAliasDestinationNotRequested => {
                "project-profile-negotiation-alias-destination-not-requested"
            }
            Self::MessageFallbackSourceNotRequested => {
                "project-profile-message-fallback-source-not-requested"
            }
            Self::MessageFallbackSequenceInvalid => {
                "project-profile-message-fallback-sequence-invalid"
            }
            Self::CoverageRuleInvalid => "project-profile-coverage-rule-invalid",
            Self::CoverageRuleDuplicate => "project-profile-coverage-rule-duplicate",
            Self::CoverageRuleConflict => "project-profile-coverage-rule-conflict",
            Self::ArtifactReferenceNotAdmitted => "project-profile-artifact-reference-not-admitted",
            Self::ArtifactCollectionNotAdmitted => {
                "project-profile-artifact-collection-not-admitted"
            }
            Self::GroupMemberUnknown => "project-profile-group-member-unknown",
            Self::GroupMemberDuplicate => "project-profile-group-member-duplicate",
            Self::TargetUnassigned => "project-profile-target-unassigned",
            Self::TargetAssignedToMultipleGroups => {
                "project-profile-target-assigned-to-multiple-groups"
            }
            Self::HydrationRelationInvalid => "project-profile-hydration-relation-invalid",
            Self::HydrationLocalePolicyIncompatible => {
                "project-profile-hydration-locale-policy-incompatible"
            }
            Self::HydrationCapabilityIncompatible => {
                "project-profile-hydration-capability-incompatible"
            }
            Self::ResourcePolicyNotAdmitted => "project-profile-resource-policy-not-admitted",
            Self::ResourcePolicyCapabilityIncompatible => {
                "project-profile-resource-policy-capability-incompatible"
            }
            Self::ResourceLimitExceeded => "project-profile-resource-limit-exceeded",
        }
    }
}

/// The stable `details.reason` values of those codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reason {
    MissingRequiredSlot,
    Unsupported,
    Incompatible,
    BodyIntegrityInvalid,
    MemberInvalid,
    MemberDuplicate,
    NonCanonicalOrder,
    IdentityMismatch,
    RevisionMismatch,
    DigestMismatch,
    SameDestination,
    ConflictingDestination,
    LiteralSelfReference,
    LiteralDuplicate,
    IntentSourceDuplicate,
    LocaleNotRequested,
    SurfaceClassUnknown,
    SurfaceClassDuplicate,
    RoleKindMismatch,
    SpecificationRevisionUnsupported,
    SpecificationRevisionMismatch,
    KindMismatch,
    NotFound,
    ConflictingContent,
    DuplicateEnvelope,
    BodySchemaInvalid,
    ProjectProfileResolutionMissing,
    ProjectProfileResolutionInvalid,
    EnvelopeInvalid,
    KindUnsupported,
    ExplicitOverride,
    InheritedProjectDefault,
    EqualEndpoint,
    UnknownEndpoint,
    CrossGroup,
    DuplicateRelation,
    ServerCapabilityMissing,
    ClientCapabilityMissing,
    DualRole,
    RequestedLocales,
    EffectiveDefault,
}

impl Reason {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::MissingRequiredSlot => "missing-required-slot",
            Self::Unsupported => "unsupported",
            Self::Incompatible => "incompatible",
            Self::BodyIntegrityInvalid => "body-integrity-invalid",
            Self::MemberInvalid => "member-invalid",
            Self::MemberDuplicate => "member-duplicate",
            Self::NonCanonicalOrder => "non-canonical-order",
            Self::IdentityMismatch => "identity-mismatch",
            Self::RevisionMismatch => "revision-mismatch",
            Self::DigestMismatch => "digest-mismatch",
            Self::SameDestination => "same-destination",
            Self::ConflictingDestination => "conflicting-destination",
            Self::LiteralSelfReference => "literal-self-reference",
            Self::LiteralDuplicate => "literal-duplicate",
            Self::IntentSourceDuplicate => "intent-source-duplicate",
            Self::LocaleNotRequested => "locale-not-requested",
            Self::SurfaceClassUnknown => "surface-class-unknown",
            Self::SurfaceClassDuplicate => "surface-class-duplicate",
            Self::RoleKindMismatch => "role-kind-mismatch",
            Self::SpecificationRevisionUnsupported => "specification-revision-unsupported",
            Self::SpecificationRevisionMismatch => "specification-revision-mismatch",
            Self::KindMismatch => "kind-mismatch",
            Self::NotFound => "not-found",
            Self::ConflictingContent => "conflicting-content",
            Self::DuplicateEnvelope => "duplicate-envelope",
            Self::BodySchemaInvalid => "body-schema-invalid",
            Self::ProjectProfileResolutionMissing => "project-profile-resolution-missing",
            Self::ProjectProfileResolutionInvalid => "project-profile-resolution-invalid",
            Self::EnvelopeInvalid => "envelope-invalid",
            Self::KindUnsupported => "kind-unsupported",
            Self::ExplicitOverride => "explicit-override",
            Self::InheritedProjectDefault => "inherited-project-default",
            Self::EqualEndpoint => "equal-endpoint",
            Self::UnknownEndpoint => "unknown-endpoint",
            Self::CrossGroup => "cross-group",
            Self::DuplicateRelation => "duplicate-relation",
            Self::ServerCapabilityMissing => "server-capability-missing",
            Self::ClientCapabilityMissing => "client-capability-missing",
            Self::DualRole => "dual-role",
            Self::RequestedLocales => "requested-locales",
            Self::EffectiveDefault => "effective-default",
        }
    }
}

/// The one blocking result a check found, which leaves the value it would
/// establish unavailable. A resource bound's result, and a policy that needs
/// more than the capability allows, carry the details 015 reports with them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Blocking {
    pub(crate) code: Code,
    pub(crate) reason: Option<Reason>,
    pub(crate) details: Option<Details>,
}

impl Blocking {
    /// A bound the measured total exceeds.
    pub(crate) const fn limit(exceeded: LimitExceeded) -> Self {
        Self {
            code: Code::ResourceLimitExceeded,
            reason: None,
            details: Some(Details::Limit(exceeded)),
        }
    }

    /// A Resource Limit Policy that needs more of a bound than the capability
    /// makes available.
    pub(crate) const fn shortfall(shortfall: Shortfall) -> Self {
        Self {
            code: Code::ResourcePolicyCapabilityIncompatible,
            reason: None,
            details: Some(Details::Shortfall(shortfall)),
        }
    }
}

/// The details 015 reports with an issue that has them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Details {
    Limit(LimitExceeded),
    Shortfall(Shortfall),
}

/// Whether `count` submitted items stay within `limit`, or the issue 015
/// reports with that exact count when they do not.
pub(crate) fn within(bound: BoundId, limit: Bound, count: usize) -> Result<(), Blocking> {
    let count = u64::try_from(count).expect("an in-memory count fits u64");
    if count <= limit.get() {
        Ok(())
    } else {
        Err(Blocking::limit(LimitExceeded {
            bound,
            limit,
            actual: Actual::Exact(count),
        }))
    }
}

/// 015's details of an exceeded bound: which bound, its value, and the total.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LimitExceeded {
    pub(crate) bound: BoundId,
    pub(crate) limit: Bound,
    pub(crate) actual: Actual,
}

/// 015's details of a policy the capability cannot run: the first bound, in
/// canonical order, that the policy requires more of than the capability's
/// `policyComparable` value makes available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Shortfall {
    pub(crate) bound: BoundId,
    pub(crate) required: Bound,
    pub(crate) available: Bound,
}

/// A measured total: exact, or a witness that the total is at least this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Actual {
    Exact(u64),
    AtLeast(u64),
}

/// The private counterpart of 015's Finding Key: the unit that found a result,
/// its code, and its reason. In 015 Phase 5 the owning specification joins it
/// and a Redacted subject token stands for the unit's subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IssueKey {
    unit: Unit,
    code: Code,
    reason: Option<Reason>,
}

impl IssueKey {
    pub(crate) const fn unit(&self) -> &Unit {
        &self.unit
    }

    pub(crate) const fn code(&self) -> Code {
        self.code
    }

    pub(crate) const fn reason(&self) -> Option<Reason> {
        self.reason
    }
}

impl Ord for IssueKey {
    /// 015's Finding order: the unit, then the unsigned UTF-8 bytes of the code
    /// and of the reason, with an absent reason first.
    fn cmp(&self, other: &Self) -> Ordering {
        self.unit
            .cmp(&other.unit)
            .then_with(|| self.code.as_str().cmp(other.code.as_str()))
            .then_with(|| {
                self.reason
                    .map(Reason::as_str)
                    .cmp(&other.reason.map(Reason::as_str))
            })
    }
}

impl PartialOrd for IssueKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Why a value a check needs is unavailable: the keys of the blocking results
/// behind it, canonical and duplicate-free. Never empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Causes(BTreeSet<IssueKey>);

impl Causes {
    pub(crate) fn union(mut self, other: Self) -> Self {
        self.0.extend(other.0);
        self
    }

    pub(crate) fn keys(&self) -> impl Iterator<Item = &IssueKey> {
        self.0.iter()
    }
}

/// A value a later check needs, or why it is unavailable.
pub(crate) type Admitted<T> = Result<T, Causes>;

/// Both values, or the causes of every one that is unavailable.
pub(crate) fn both<A, B>(first: Admitted<A>, second: Admitted<B>) -> Admitted<(A, B)> {
    match (first, second) {
        (Ok(first), Ok(second)) => Ok((first, second)),
        (Err(causes), Ok(_)) | (Ok(_), Err(causes)) => Err(causes),
        (Err(first), Err(second)) => Err(first.union(second)),
    }
}

/// Whether both hold, or the causes of each that does not.
pub(crate) fn and(first: Admitted<()>, second: Admitted<()>) -> Admitted<()> {
    both(first, second).map(|((), ())| ())
}

/// A unit's evaluation status. A unit that found a blocking result was still
/// evaluated; only an unavailable prerequisite leaves it not evaluated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Status {
    Evaluated,
    NotEvaluated(Causes),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EvaluationInvariant {
    /// A resolver bug: each unit is evaluated at most once.
    UnitRecordedTwice(Unit),
    /// A resolver bug: only an evaluated unit finds a non-blocking result.
    NoticeWithoutEvaluation(Unit),
}

/// The status of every unit recorded so far, the blocking results found with
/// their details, and the non-blocking ones, each in 015's order whatever order
/// the units ran in.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Evaluation {
    statuses: BTreeMap<Unit, Status>,
    issues: BTreeMap<IssueKey, Option<Details>>,
    notices: BTreeSet<IssueKey>,
}

impl Evaluation {
    /// Runs `unit` when its prerequisite is admitted and records its status.
    /// `evaluate` returns the value the unit establishes or the blocking result
    /// it found. An unavailable prerequisite becomes the unit's causes: the unit
    /// reports nothing, and whatever depends on it inherits the same causes.
    pub(crate) fn check<P, T>(
        &mut self,
        unit: Unit,
        prerequisite: Admitted<P>,
        evaluate: impl FnOnce(P) -> Result<T, Blocking>,
    ) -> Result<Admitted<T>, EvaluationInvariant> {
        if self.statuses.contains_key(&unit) {
            return Err(EvaluationInvariant::UnitRecordedTwice(unit));
        }
        let (status, result) = match prerequisite {
            Err(causes) => (Status::NotEvaluated(causes.clone()), Err(causes)),
            Ok(value) => match evaluate(value) {
                Ok(value) => (Status::Evaluated, Ok(value)),
                Err(Blocking {
                    code,
                    reason,
                    details,
                }) => {
                    let key = IssueKey {
                        unit: unit.clone(),
                        code,
                        reason,
                    };
                    self.issues.insert(key.clone(), details);
                    (Status::Evaluated, Err(Causes(BTreeSet::from([key]))))
                }
            },
        };
        self.statuses.insert(unit, status);
        Ok(result)
    }

    /// Records a non-blocking result `code` of a unit `check` evaluated. It
    /// is never a cause: nothing waits on it.
    pub(crate) fn notice(&mut self, unit: Unit, code: Code) -> Result<(), EvaluationInvariant> {
        if self.statuses.get(&unit) != Some(&Status::Evaluated) {
            return Err(EvaluationInvariant::NoticeWithoutEvaluation(unit));
        }
        self.notices.insert(IssueKey {
            unit,
            code,
            reason: None,
        });
        Ok(())
    }

    pub(crate) fn status(&self, unit: &Unit) -> Option<&Status> {
        self.statuses.get(unit)
    }

    pub(crate) fn statuses(&self) -> impl Iterator<Item = (&Unit, &Status)> {
        self.statuses.iter()
    }

    pub(crate) fn issues(&self) -> impl Iterator<Item = &IssueKey> {
        self.issues.keys()
    }

    /// The details of a resource issue, which no other issue has.
    pub(crate) fn limit(&self, key: &IssueKey) -> Option<LimitExceeded> {
        match self.issues.get(key) {
            Some(Some(Details::Limit(exceeded))) => Some(*exceeded),
            _ => None,
        }
    }

    /// The details of a policy the capability cannot run.
    pub(crate) fn shortfall(&self, key: &IssueKey) -> Option<Shortfall> {
        match self.issues.get(key) {
            Some(Some(Details::Shortfall(shortfall))) => Some(*shortfall),
            _ => None,
        }
    }

    pub(crate) fn notices(&self) -> impl Iterator<Item = &IssueKey> {
        self.notices.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MISSING: Blocking = Blocking {
        code: Code::SpecificationInputIncomplete,
        reason: Some(Reason::MissingRequiredSlot),
        details: None,
    };

    fn slot(slot: Slot) -> Unit {
        Unit {
            phase: Phase::SpecificationAdmission,
            subject: Subject::Slot(slot),
            check: Check::SupportedAssertion,
        }
    }

    fn data() -> Unit {
        Unit {
            phase: Phase::SpecificationAdmission,
            subject: Subject::CanonicalizationData,
            check: Check::MatchingData,
        }
    }

    fn field(field: DeclarationField) -> Unit {
        Unit {
            phase: Phase::ProjectIdentityResolution,
            subject: Subject::DeclarationField(field),
            check: Check::OpaqueIdentity,
        }
    }

    /// The causes a missing slot leaves, as a later unit would receive them.
    fn missing(evaluation: &mut Evaluation, unit: Unit) -> Causes {
        evaluation
            .check(unit, Ok(()), |()| Err::<(), _>(MISSING))
            .unwrap()
            .unwrap_err()
    }

    #[test]
    fn phases_keep_015s_ids_in_rank_order() {
        let phases = [
            Phase::ResolverBootstrapAdmission,
            Phase::EntryAdmission,
            Phase::StructuralAdmission,
            Phase::ProfileSelection,
            Phase::ResourcePolicyAdmission,
            Phase::SpecificationAdmission,
            Phase::ProjectIdentityResolution,
            Phase::LocaleResolution,
            Phase::LocalePolicyResolution,
            Phase::ArtifactReferenceResolution,
            Phase::DeliveryResolution,
            Phase::TargetResolution,
            Phase::GroupResolution,
            Phase::HydrationValidation,
            Phase::ProfileProjection,
            Phase::ReportingFinalization,
        ];
        assert_eq!(
            phases.map(Phase::id),
            [
                "resolver-bootstrap-admission",
                "entry-admission",
                "structural-admission",
                "profile-selection",
                "resource-policy-admission",
                "specification-admission",
                "project-identity-resolution",
                "locale-resolution",
                "locale-policy-resolution",
                "artifact-reference-resolution",
                "delivery-resolution",
                "target-resolution",
                "group-resolution",
                "hydration-validation",
                "profile-projection",
                "reporting-finalization",
            ]
        );
        assert!(phases.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn codes_and_reasons_are_spelled_as_015_registers_them() {
        assert_eq!(
            [
                Code::SpecificationInputIncomplete,
                Code::SpecificationNotAdmitted,
                Code::LocaleSpecificationNotAdmitted,
                Code::LocaleDataNotAdmitted,
                Code::NegotiationSpecificationNotAdmitted,
                Code::MessageFallbackSpecificationNotAdmitted,
                Code::CoverageSpecificationNotAdmitted,
                Code::DeliverySpecificationNotAdmitted,
                Code::IntentSurfaceClassVocabularyNotAdmitted,
                Code::LocaleInvalid,
                Code::LocaleNonCanonical,
                Code::LocaleDuplicate,
                Code::DefaultRequestedLocaleNotInProject,
                Code::TargetLocalesNotSubset,
                Code::TargetDefaultLocaleNotSupported,
                Code::NegotiationAliasDuplicate,
                Code::NegotiationAliasDestinationNotRequested,
                Code::MessageFallbackSourceNotRequested,
                Code::MessageFallbackSequenceInvalid,
                Code::CoverageRuleInvalid,
                Code::CoverageRuleDuplicate,
                Code::CoverageRuleConflict,
                Code::ArtifactReferenceNotAdmitted,
                Code::ArtifactCollectionNotAdmitted,
                Code::GroupMemberUnknown,
                Code::GroupMemberDuplicate,
                Code::TargetUnassigned,
                Code::TargetAssignedToMultipleGroups,
                Code::HydrationRelationInvalid,
                Code::HydrationLocalePolicyIncompatible,
                Code::HydrationCapabilityIncompatible,
                Code::ResourcePolicyNotAdmitted,
                Code::ResourcePolicyCapabilityIncompatible,
                Code::ResourceLimitExceeded,
            ]
            .map(Code::as_str),
            [
                "project-profile-specification-input-incomplete",
                "project-profile-specification-not-admitted",
                "project-profile-locale-specification-not-admitted",
                "project-profile-locale-data-not-admitted",
                "project-profile-negotiation-specification-not-admitted",
                "project-profile-message-fallback-specification-not-admitted",
                "project-profile-coverage-specification-not-admitted",
                "project-profile-delivery-specification-not-admitted",
                "project-profile-intent-surface-class-vocabulary-not-admitted",
                "project-profile-locale-invalid",
                "project-profile-locale-non-canonical",
                "project-profile-locale-duplicate",
                "project-profile-default-requested-locale-not-in-project",
                "project-profile-target-locales-not-subset",
                "project-profile-target-default-locale-not-supported",
                "project-profile-negotiation-alias-duplicate",
                "project-profile-negotiation-alias-destination-not-requested",
                "project-profile-message-fallback-source-not-requested",
                "project-profile-message-fallback-sequence-invalid",
                "project-profile-coverage-rule-invalid",
                "project-profile-coverage-rule-duplicate",
                "project-profile-coverage-rule-conflict",
                "project-profile-artifact-reference-not-admitted",
                "project-profile-artifact-collection-not-admitted",
                "project-profile-group-member-unknown",
                "project-profile-group-member-duplicate",
                "project-profile-target-unassigned",
                "project-profile-target-assigned-to-multiple-groups",
                "project-profile-hydration-relation-invalid",
                "project-profile-hydration-locale-policy-incompatible",
                "project-profile-hydration-capability-incompatible",
                "project-profile-resource-policy-not-admitted",
                "project-profile-resource-policy-capability-incompatible",
                "project-profile-resource-limit-exceeded",
            ]
        );
        assert_eq!(
            [
                Reason::MissingRequiredSlot,
                Reason::Unsupported,
                Reason::Incompatible,
                Reason::BodyIntegrityInvalid,
                Reason::MemberInvalid,
                Reason::MemberDuplicate,
                Reason::NonCanonicalOrder,
                Reason::IdentityMismatch,
                Reason::RevisionMismatch,
                Reason::DigestMismatch,
                Reason::SameDestination,
                Reason::ConflictingDestination,
                Reason::LiteralSelfReference,
                Reason::LiteralDuplicate,
                Reason::IntentSourceDuplicate,
                Reason::LocaleNotRequested,
                Reason::SurfaceClassUnknown,
                Reason::SurfaceClassDuplicate,
                Reason::RoleKindMismatch,
                Reason::SpecificationRevisionUnsupported,
                Reason::SpecificationRevisionMismatch,
                Reason::KindMismatch,
                Reason::NotFound,
                Reason::ConflictingContent,
                Reason::DuplicateEnvelope,
                Reason::BodySchemaInvalid,
                Reason::ProjectProfileResolutionMissing,
                Reason::ProjectProfileResolutionInvalid,
                Reason::EnvelopeInvalid,
                Reason::KindUnsupported,
                Reason::ExplicitOverride,
                Reason::InheritedProjectDefault,
                Reason::EqualEndpoint,
                Reason::UnknownEndpoint,
                Reason::CrossGroup,
                Reason::DuplicateRelation,
                Reason::ServerCapabilityMissing,
                Reason::ClientCapabilityMissing,
                Reason::DualRole,
                Reason::RequestedLocales,
                Reason::EffectiveDefault,
            ]
            .map(Reason::as_str),
            [
                "missing-required-slot",
                "unsupported",
                "incompatible",
                "body-integrity-invalid",
                "member-invalid",
                "member-duplicate",
                "non-canonical-order",
                "identity-mismatch",
                "revision-mismatch",
                "digest-mismatch",
                "same-destination",
                "conflicting-destination",
                "literal-self-reference",
                "literal-duplicate",
                "intent-source-duplicate",
                "locale-not-requested",
                "surface-class-unknown",
                "surface-class-duplicate",
                "role-kind-mismatch",
                "specification-revision-unsupported",
                "specification-revision-mismatch",
                "kind-mismatch",
                "not-found",
                "conflicting-content",
                "duplicate-envelope",
                "body-schema-invalid",
                "project-profile-resolution-missing",
                "project-profile-resolution-invalid",
                "envelope-invalid",
                "kind-unsupported",
                "explicit-override",
                "inherited-project-default",
                "equal-endpoint",
                "unknown-endpoint",
                "cross-group",
                "duplicate-relation",
                "server-capability-missing",
                "client-capability-missing",
                "dual-role",
                "requested-locales",
                "effective-default",
            ]
        );
    }

    #[test]
    fn an_admitted_prerequisite_is_evaluated_and_a_blocking_result_becomes_the_cause() {
        let mut evaluation = Evaluation::default();
        let doubled = evaluation
            .check(slot(Slot::Coverage), Ok(2), |value| {
                Ok::<_, Blocking>(value * 2)
            })
            .unwrap();
        assert_eq!(doubled, Ok(4));
        let causes = missing(&mut evaluation, slot(Slot::DeliveryGraph));
        let found: Vec<_> = evaluation.issues().cloned().collect();
        assert_eq!(found.len(), 1);
        assert_eq!(
            (found[0].unit(), found[0].code(), found[0].reason()),
            (
                &slot(Slot::DeliveryGraph),
                Code::SpecificationInputIncomplete,
                Some(Reason::MissingRequiredSlot)
            )
        );
        assert_eq!(evaluation.limit(&found[0]), None);
        assert_eq!(causes.keys().collect::<Vec<_>>(), [&found[0]]);
        for unit in [slot(Slot::Coverage), slot(Slot::DeliveryGraph)] {
            assert_eq!(evaluation.status(&unit), Some(&Status::Evaluated));
        }
        assert_eq!(evaluation.status(&slot(Slot::LocaleNegotiation)), None);
    }

    #[test]
    fn an_unavailable_prerequisite_leaves_the_unit_not_evaluated_and_reports_nothing() {
        let mut evaluation = Evaluation::default();
        let causes = missing(&mut evaluation, slot(Slot::LocaleCanonicalization));
        let mut ran = false;
        let dependant = evaluation
            .check(data(), Err::<(), _>(causes.clone()), |()| {
                ran = true;
                Ok::<(), Blocking>(())
            })
            .unwrap();
        assert!(!ran);
        assert_eq!(dependant, Err(causes.clone()));
        assert_eq!(
            evaluation.status(&data()),
            Some(&Status::NotEvaluated(causes))
        );
        assert_eq!(evaluation.issues().count(), 1);
    }

    #[test]
    fn causes_from_several_prerequisites_form_one_canonical_set() {
        let mut evaluation = Evaluation::default();
        let placement = missing(&mut evaluation, slot(Slot::DeliveryPlacement));
        let coverage = missing(&mut evaluation, slot(Slot::Coverage));
        assert_eq!(both::<_, u8>(Ok(1), Ok(2)), Ok((1, 2)));
        assert_eq!(
            both::<(), u8>(Err(placement.clone()), Ok(2)),
            Err(placement.clone())
        );
        assert_eq!(
            both::<u8, ()>(Ok(1), Err(coverage.clone())),
            Err(coverage.clone())
        );
        let joined = both::<(), ()>(Err(placement.clone()), Err(coverage.clone())).unwrap_err();
        assert_eq!(
            joined
                .keys()
                .map(|key| key.unit().subject.clone())
                .collect::<Vec<_>>(),
            [
                Subject::Slot(Slot::Coverage),
                Subject::Slot(Slot::DeliveryPlacement)
            ]
        );
        assert_eq!(joined.clone().union(placement), joined);
    }

    #[test]
    fn a_unit_is_recorded_once() {
        let mut evaluation = Evaluation::default();
        let causes = missing(&mut evaluation, slot(Slot::Coverage));
        let mut ran = false;
        assert_eq!(
            evaluation
                .check(slot(Slot::Coverage), Ok(()), |()| {
                    ran = true;
                    Ok::<(), Blocking>(())
                })
                .err(),
            Some(EvaluationInvariant::UnitRecordedTwice(slot(Slot::Coverage)))
        );
        assert!(!ran);
        evaluation
            .check(data(), Err::<(), _>(causes.clone()), Ok::<(), Blocking>)
            .unwrap()
            .unwrap_err();
        assert_eq!(
            evaluation
                .check(data(), Err::<(), _>(causes), Ok::<(), Blocking>)
                .err(),
            Some(EvaluationInvariant::UnitRecordedTwice(data()))
        );
        assert_eq!(evaluation.issues().count(), 1);
    }

    #[test]
    fn statuses_and_issues_follow_015s_order_whatever_order_the_units_ran_in() {
        let bootstrap = Unit {
            phase: Phase::ResolverBootstrapAdmission,
            subject: Subject::Slot(Slot::ProjectProfile),
            check: Check::SupportedAssertion,
        };
        let canonical = [
            bootstrap,
            slot(Slot::LocaleCanonicalization),
            slot(Slot::Coverage),
            slot(Slot::DeliveryPlacement),
            data(),
            field(DeclarationField::ProjectId),
            field(DeclarationField::SelectionScope),
        ];
        let mut evaluation = Evaluation::default();
        for unit in canonical.iter().rev() {
            missing(&mut evaluation, unit.clone());
        }
        assert_eq!(
            evaluation
                .statuses()
                .map(|(unit, _)| unit.clone())
                .collect::<Vec<_>>(),
            canonical
        );
        assert_eq!(
            evaluation
                .issues()
                .map(|key| key.unit().clone())
                .collect::<Vec<_>>(),
            canonical
        );
    }

    #[test]
    fn units_order_by_phase_then_subject_then_check() {
        // Combinations no phase records, so that each key is seen to decide
        // before the next.
        let unit = |phase, subject, check| Unit {
            phase,
            subject,
            check,
        };
        let ordered = [
            unit(
                Phase::SpecificationAdmission,
                Subject::DeclarationField(DeclarationField::SelectionScope),
                Check::OpaqueIdentity,
            ),
            unit(
                Phase::ProjectIdentityResolution,
                Subject::Slot(Slot::ProjectProfile),
                Check::OpaqueIdentity,
            ),
            unit(
                Phase::ProjectIdentityResolution,
                Subject::Slot(Slot::Coverage),
                Check::SupportedAssertion,
            ),
            unit(
                Phase::ProjectIdentityResolution,
                Subject::CanonicalizationData,
                Check::SupportedAssertion,
            ),
            unit(
                Phase::ProjectIdentityResolution,
                Subject::CanonicalizationData,
                Check::MatchingData,
            ),
            unit(
                Phase::ProjectIdentityResolution,
                Subject::CanonicalizationData,
                Check::OpaqueIdentity,
            ),
        ];
        assert!(ordered.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn issue_keys_order_by_unit_then_by_the_bytes_of_code_and_reason() {
        let key = |code, reason| IssueKey {
            unit: slot(Slot::Coverage),
            code,
            reason,
        };
        // Declaration order differs from byte order in both enums, so neither
        // can stand in for the spelling.
        let mut keys = [
            key(
                Code::SpecificationNotAdmitted,
                Some(Reason::MissingRequiredSlot),
            ),
            key(Code::SpecificationNotAdmitted, Some(Reason::Unsupported)),
            key(Code::SpecificationNotAdmitted, Some(Reason::Incompatible)),
            key(Code::SpecificationNotAdmitted, None),
            key(Code::LocaleDataNotAdmitted, Some(Reason::Unsupported)),
            IssueKey {
                unit: slot(Slot::LocaleCanonicalization),
                code: Code::SpecificationNotAdmitted,
                reason: Some(Reason::Unsupported),
            },
        ];
        keys.sort();
        assert_eq!(
            keys.iter()
                .map(|key| (
                    key.unit().subject.clone(),
                    key.code().as_str(),
                    key.reason().map(Reason::as_str)
                ))
                .collect::<Vec<_>>(),
            [
                (
                    Subject::Slot(Slot::LocaleCanonicalization),
                    "project-profile-specification-not-admitted",
                    Some("unsupported")
                ),
                (
                    Subject::Slot(Slot::Coverage),
                    "project-profile-locale-data-not-admitted",
                    Some("unsupported")
                ),
                (
                    Subject::Slot(Slot::Coverage),
                    "project-profile-specification-not-admitted",
                    None
                ),
                (
                    Subject::Slot(Slot::Coverage),
                    "project-profile-specification-not-admitted",
                    Some("incompatible")
                ),
                (
                    Subject::Slot(Slot::Coverage),
                    "project-profile-specification-not-admitted",
                    Some("missing-required-slot")
                ),
                (
                    Subject::Slot(Slot::Coverage),
                    "project-profile-specification-not-admitted",
                    Some("unsupported")
                ),
            ]
        );
    }

    #[test]
    fn a_resource_issue_keeps_its_details_beside_its_key() {
        let exceeded = LimitExceeded {
            bound: BoundId {
                group: crate::resolver::limits::Group::LocaleResolution,
                name: "maxLocaleOccurrences",
            },
            limit: Bound::new(3).unwrap(),
            actual: Actual::Exact(4),
        };
        let mut evaluation = Evaluation::default();
        let causes = evaluation
            .check(data(), Ok(()), |()| Err::<(), _>(Blocking::limit(exceeded)))
            .unwrap()
            .unwrap_err();
        let key = causes.keys().next().unwrap().clone();
        assert_eq!(
            (key.unit(), key.code(), key.reason()),
            (&data(), Code::ResourceLimitExceeded, None)
        );
        assert_eq!(evaluation.limit(&key), Some(exceeded));
        // Details belong to issues; a key nothing found has none.
        let other = IssueKey {
            unit: slot(Slot::Coverage),
            ..key
        };
        assert_eq!(evaluation.limit(&other), None);
    }

    #[test]
    fn a_count_holds_at_its_bound_and_one_more_is_reported_exactly() {
        let bound = BoundId {
            group: crate::resolver::limits::Group::LocaleResolution,
            name: "maxFallbackSources",
        };
        let limit = Bound::new(2).unwrap();
        assert_eq!(within(bound, limit, 0), Ok(()));
        assert_eq!(within(bound, limit, 2), Ok(()));
        assert_eq!(
            within(bound, limit, 3),
            Err(Blocking::limit(LimitExceeded {
                bound,
                limit,
                actual: Actual::Exact(3),
            }))
        );
    }

    #[test]
    fn a_notice_belongs_to_an_evaluated_unit_and_is_never_a_cause() {
        let mut evaluation = Evaluation::default();
        evaluation
            .check(slot(Slot::Coverage), Ok(()), Ok::<(), Blocking>)
            .unwrap()
            .unwrap();
        evaluation
            .notice(slot(Slot::Coverage), Code::LocaleNonCanonical)
            .unwrap();
        assert_eq!(
            evaluation
                .notices()
                .map(|key| (key.unit().clone(), key.code(), key.reason()))
                .collect::<Vec<_>>(),
            [(slot(Slot::Coverage), Code::LocaleNonCanonical, None)]
        );
        assert_eq!(evaluation.issues().count(), 0);
        assert_eq!(
            evaluation.status(&slot(Slot::Coverage)),
            Some(&Status::Evaluated)
        );
        // A unit never recorded, or one left not evaluated, found nothing.
        assert_eq!(
            evaluation.notice(slot(Slot::LocaleNegotiation), Code::LocaleNonCanonical),
            Err(EvaluationInvariant::NoticeWithoutEvaluation(slot(
                Slot::LocaleNegotiation
            )))
        );
        let causes = missing(&mut evaluation, slot(Slot::DeliveryGraph));
        evaluation
            .check(data(), Err::<(), _>(causes), Ok::<(), Blocking>)
            .unwrap()
            .unwrap_err();
        assert_eq!(
            evaluation.notice(data(), Code::LocaleNonCanonical),
            Err(EvaluationInvariant::NoticeWithoutEvaluation(data()))
        );
        assert_eq!(evaluation.notices().count(), 1);
    }
}
