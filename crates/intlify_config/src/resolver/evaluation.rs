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

use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

use crate::input_limits::Bound;

use super::artifact_resolution::CollectionSubject;
use super::capability::BootstrapBoundId;
use super::coverage::CoverageSubject;
use super::dependency::SemanticGroup;
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

/// 015's resolution phases, declared in rank order, each spelled as its ID.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
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
    /// Every phase, in rank order.
    pub(crate) const ALL: [Self; 16] = [
        Self::ResolverBootstrapAdmission,
        Self::EntryAdmission,
        Self::StructuralAdmission,
        Self::ProfileSelection,
        Self::ResourcePolicyAdmission,
        Self::SpecificationAdmission,
        Self::ProjectIdentityResolution,
        Self::LocaleResolution,
        Self::LocalePolicyResolution,
        Self::ArtifactReferenceResolution,
        Self::DeliveryResolution,
        Self::TargetResolution,
        Self::GroupResolution,
        Self::HydrationValidation,
        Self::ProfileProjection,
        Self::ReportingFinalization,
    ];

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

/// What one unit checks: a Logical subject key of one subject kind, which
/// [`Subject::kind`] names ([`SubjectKind`](super::subject::SubjectKind)).
/// Subjects order by their kind's rank, then by key.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// The bodies of one member key, or one conflict group of tuples
    /// differing only in their digest, of the complete artifact collection.
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
    /// One semantic group of the profile view, whose unit owns the group's
    /// semantic dependency record: each group is a singleton kind.
    SemanticDependency(SemanticGroup),
}

/// The check a unit runs, declared in rank order within a subject kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Check {
    /// The slot holds an assertion the resolver supports.
    SupportedAssertion,
    /// The unit records the dependency of each slot whose identity is
    /// established: the owner of entry or materialized dependencies, which
    /// waits on no check.
    EstablishedSlots,
    /// The raw file stays within `bootstrapOnly.rawInput.maxFileBytes`.
    WithinFileBytes,
    /// Strict parsing stays within `bootstrapOnly.rawInput.maxParserTokens`.
    WithinParserTokens,
    /// The file is valid UTF-8 and strict JSON.
    WellFormedJson,
    /// No object of the file repeats a member name.
    UniqueMembers,
    /// Every string, member name, and number lies in the portable
    /// JSON-compatible domain.
    JsonCompatible,
    /// The materialized root's logical values stay within `maxNodes`.
    WithinNodes,
    /// The root's deepest value level stays within `maxDepth`.
    WithinDepth,
    /// The root's object members and array elements stay within
    /// `maxCollectionEntries`.
    WithinCollectionEntries,
    /// The bytes of the root's strings and object keys stay within
    /// `maxTotalStringBytes`.
    WithinTotalStringBytes,
    /// Each string and object key of the root stays within
    /// `maxSingleStringBytes`.
    WithinSingleStringBytes,
    /// The materialized root is an object.
    ObjectRoot,
    /// The root has a `schemaVersion` member.
    PresentSchemaVersion,
    /// The `schemaVersion` member is a string.
    StringSchemaVersion,
    /// The construction authority has a schema for the `schemaVersion`.
    SupportedSchemaVersion,
    /// Every member the schema requires at the path is present.
    RequiredField,
    /// The value at the path has the type the schema declares.
    FieldType,
    /// The value, or the map key, is one the schema admits.
    FieldValue,
    /// The object at the path has no member the schema does not declare.
    KnownField,
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
    /// More than one declaration needs a selector, and one is given.
    RequiredSelector,
    /// The selector is a valid Profile ID.
    ValidSelector,
    /// The selector names a declaration.
    DeclaredSelector,
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
    /// The coverage rule's selectors normalize to its domain: every item
    /// admitted, and an omitted selector standing for every requested locale
    /// or vocabulary member.
    NormalizedDomain,
    /// No other rule normalizes to the same domain.
    UniqueDomain,
    /// Every cell of the coverage decision table is decided, so the table and
    /// its Coverage Decision Basis exist.
    CompleteTable,
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
    /// The target's Target Profile has a direction for each of its requested
    /// locales.
    DirectedLocales,
    /// The member names a declared Target ID.
    KnownMember,
    /// The member occurs once in its group.
    UniqueMember,
    /// The Target ID is a member of exactly one group.
    SingleGroup,
    /// The endpoint names a declared Target ID.
    KnownEndpoint,
    /// The relation's endpoints are two Target IDs.
    DistinctEndpoints,
    /// The endpoint is a member of the relation's group.
    SameGroupEndpoint,
    /// The relation occurs once in its group.
    UniqueRelation,
    /// The endpoint's Target Profile takes the role its field names: the
    /// server's renders server-side and the client's hydrates in the browser.
    EndpointRole,
    /// Both endpoints request the same locales.
    SameRequestedLocales,
    /// Both endpoints have the same effective default.
    SameEffectiveDefault,
    /// Nothing statically known makes the two Target Profiles incompatible.
    StaticCompatibility,
    /// The Target ID takes one role within its group's relations.
    SingleRole,
    /// Every check that establishes the semantic group held, so the unit owns
    /// the group's semantic dependency record. It finds nothing of its own.
    EstablishedGroup,
    /// Each diagnostic domain's reporting stays within its bounds. The check's
    /// status is a domain's terminal state, which its Finding's reason names.
    CompleteReporting,
}

/// One atomic evaluation unit. Units order as 015 evaluates and commits them:
/// phase, subject kind, subject key, then check.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Unit {
    pub(crate) phase: Phase,
    pub(crate) subject: Subject,
    pub(crate) check: Check,
}

/// The 015 Finding codes of the resolver's checks: every code of 015's
/// registry. Structural admission still reports the entry, schema, and
/// selection codes in its own terms; their checks are declared, and their
/// units come with the Findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Code {
    JsonInvalid,
    JsonMemberDuplicate,
    InputNotJsonCompatible,
    SchemaVersionUnsupported,
    RequiredFieldMissing,
    FieldTypeInvalid,
    FieldValueInvalid,
    UnknownField,
    SelectorRequired,
    SelectorInvalid,
    SelectorUnknown,
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
    TargetLocaleDirectionMissing,
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
    BootstrapLimitExceeded,
    ResourceLimitExceeded,
    ReportingLimitExceeded,
}

impl Code {
    /// Every code, in 015's table order.
    pub(crate) const ALL: [Self; 48] = [
        Self::JsonInvalid,
        Self::JsonMemberDuplicate,
        Self::InputNotJsonCompatible,
        Self::SchemaVersionUnsupported,
        Self::RequiredFieldMissing,
        Self::FieldTypeInvalid,
        Self::FieldValueInvalid,
        Self::UnknownField,
        Self::SelectorRequired,
        Self::SelectorInvalid,
        Self::SelectorUnknown,
        Self::SpecificationInputIncomplete,
        Self::SpecificationNotAdmitted,
        Self::LocaleSpecificationNotAdmitted,
        Self::LocaleDataNotAdmitted,
        Self::NegotiationSpecificationNotAdmitted,
        Self::MessageFallbackSpecificationNotAdmitted,
        Self::CoverageSpecificationNotAdmitted,
        Self::DeliverySpecificationNotAdmitted,
        Self::IntentSurfaceClassVocabularyNotAdmitted,
        Self::LocaleInvalid,
        Self::LocaleNonCanonical,
        Self::LocaleDuplicate,
        Self::DefaultRequestedLocaleNotInProject,
        Self::TargetLocalesNotSubset,
        Self::TargetDefaultLocaleNotSupported,
        Self::TargetLocaleDirectionMissing,
        Self::NegotiationAliasDuplicate,
        Self::NegotiationAliasDestinationNotRequested,
        Self::MessageFallbackSourceNotRequested,
        Self::MessageFallbackSequenceInvalid,
        Self::CoverageRuleInvalid,
        Self::CoverageRuleDuplicate,
        Self::CoverageRuleConflict,
        Self::ArtifactReferenceNotAdmitted,
        Self::ArtifactCollectionNotAdmitted,
        Self::GroupMemberUnknown,
        Self::GroupMemberDuplicate,
        Self::TargetUnassigned,
        Self::TargetAssignedToMultipleGroups,
        Self::HydrationRelationInvalid,
        Self::HydrationLocalePolicyIncompatible,
        Self::HydrationCapabilityIncompatible,
        Self::ResourcePolicyNotAdmitted,
        Self::ResourcePolicyCapabilityIncompatible,
        Self::BootstrapLimitExceeded,
        Self::ResourceLimitExceeded,
        Self::ReportingLimitExceeded,
    ];

    /// 015's spelling of the code.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::JsonInvalid => "project-profile-json-invalid",
            Self::JsonMemberDuplicate => "project-profile-json-member-duplicate",
            Self::InputNotJsonCompatible => "project-profile-input-not-json-compatible",
            Self::SchemaVersionUnsupported => "project-profile-schema-version-unsupported",
            Self::RequiredFieldMissing => "project-profile-required-field-missing",
            Self::FieldTypeInvalid => "project-profile-field-type-invalid",
            Self::FieldValueInvalid => "project-profile-field-value-invalid",
            Self::UnknownField => "project-profile-unknown-field",
            Self::SelectorRequired => "project-profile-selector-required",
            Self::SelectorInvalid => "project-profile-selector-invalid",
            Self::SelectorUnknown => "project-profile-selector-unknown",
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
            Self::TargetLocaleDirectionMissing => "project-profile-target-locale-direction-missing",
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
            Self::BootstrapLimitExceeded => "project-profile-bootstrap-limit-exceeded",
            Self::ResourceLimitExceeded => "project-profile-resource-limit-exceeded",
            Self::ReportingLimitExceeded => "project-profile-reporting-limit-exceeded",
        }
    }

    /// The stable reasons 015's tables give the code, in the order they list
    /// them; none for a code without stable reasons.
    pub(crate) const fn reasons(self) -> &'static [Reason] {
        use Reason as R;
        match self {
            Self::InputNotJsonCompatible => &[
                R::UnsupportedType,
                R::NonFiniteNumber,
                R::NonPortableNumber,
                R::InvalidUnicode,
                R::NonStringKey,
                R::DynamicContainer,
                R::DynamicProperty,
                R::SparseArray,
                R::Cycle,
            ],
            Self::SpecificationInputIncomplete => &[R::MissingRequiredSlot],
            Self::SpecificationNotAdmitted
            | Self::LocaleSpecificationNotAdmitted
            | Self::NegotiationSpecificationNotAdmitted
            | Self::MessageFallbackSpecificationNotAdmitted
            | Self::CoverageSpecificationNotAdmitted
            | Self::DeliverySpecificationNotAdmitted => &[R::Unsupported, R::Incompatible],
            Self::IntentSurfaceClassVocabularyNotAdmitted => &[
                R::BodyIntegrityInvalid,
                R::MemberInvalid,
                R::MemberDuplicate,
                R::NonCanonicalOrder,
                R::IdentityMismatch,
                R::RevisionMismatch,
                R::DigestMismatch,
            ],
            Self::TargetDefaultLocaleNotSupported => {
                &[R::ExplicitOverride, R::InheritedProjectDefault]
            }
            Self::NegotiationAliasDuplicate => &[R::SameDestination, R::ConflictingDestination],
            Self::MessageFallbackSequenceInvalid => &[
                R::LiteralSelfReference,
                R::LiteralDuplicate,
                R::IntentSourceDuplicate,
            ],
            Self::CoverageRuleInvalid => &[
                R::LocaleNotRequested,
                R::SurfaceClassUnknown,
                R::SurfaceClassDuplicate,
            ],
            Self::ArtifactReferenceNotAdmitted => &[
                R::NotFound,
                R::KindMismatch,
                R::IdentityMismatch,
                R::RevisionMismatch,
                R::SpecificationRevisionMismatch,
                R::SpecificationRevisionUnsupported,
                R::RoleKindMismatch,
                R::DigestMismatch,
            ],
            Self::ArtifactCollectionNotAdmitted => &[
                R::EnvelopeInvalid,
                R::KindUnsupported,
                R::DuplicateEnvelope,
                R::ConflictingContent,
            ],
            Self::HydrationRelationInvalid => &[
                R::EqualEndpoint,
                R::UnknownEndpoint,
                R::CrossGroup,
                R::DuplicateRelation,
                R::ServerCapabilityMissing,
                R::ClientCapabilityMissing,
                R::DualRole,
            ],
            Self::HydrationLocalePolicyIncompatible => &[R::RequestedLocales, R::EffectiveDefault],
            Self::ResourcePolicyNotAdmitted => &[
                R::BodyIntegrityInvalid,
                R::BodySchemaInvalid,
                R::ProjectProfileResolutionMissing,
                R::ProjectProfileResolutionInvalid,
            ],
            Self::ReportingLimitExceeded => &[R::Entry, R::Shared],
            Self::JsonInvalid
            | Self::JsonMemberDuplicate
            | Self::SchemaVersionUnsupported
            | Self::RequiredFieldMissing
            | Self::FieldTypeInvalid
            | Self::FieldValueInvalid
            | Self::UnknownField
            | Self::SelectorRequired
            | Self::SelectorInvalid
            | Self::SelectorUnknown
            | Self::LocaleDataNotAdmitted
            | Self::LocaleInvalid
            | Self::LocaleNonCanonical
            | Self::LocaleDuplicate
            | Self::DefaultRequestedLocaleNotInProject
            | Self::TargetLocalesNotSubset
            | Self::TargetLocaleDirectionMissing
            | Self::NegotiationAliasDestinationNotRequested
            | Self::MessageFallbackSourceNotRequested
            | Self::CoverageRuleDuplicate
            | Self::CoverageRuleConflict
            | Self::GroupMemberUnknown
            | Self::GroupMemberDuplicate
            | Self::TargetUnassigned
            | Self::TargetAssignedToMultipleGroups
            | Self::HydrationCapabilityIncompatible
            | Self::ResourcePolicyCapabilityIncompatible
            | Self::BootstrapLimitExceeded
            | Self::ResourceLimitExceeded => &[],
        }
    }
}

/// A code reads and writes as 015 spells it.
impl Serialize for Code {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Code {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let spelled = Cow::<str>::deserialize(deserializer)?;
        Self::ALL
            .into_iter()
            .find(|code| code.as_str() == spelled)
            .ok_or_else(|| de::Error::custom(format!("no Finding code {spelled}")))
    }
}

/// A code's schema is the closed list of 015's spellings, in table order.
impl JsonSchema for Code {
    fn schema_name() -> Cow<'static, str> {
        "FindingCode".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        let codes: Vec<&str> = Self::ALL.iter().map(|code| code.as_str()).collect();
        schemars::json_schema!({ "type": "string", "enum": codes })
    }
}

/// The stable `details.reason` values of those codes, in the order of
/// 015's tables, each where it first appears.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reason {
    UnsupportedType,
    NonFiniteNumber,
    NonPortableNumber,
    InvalidUnicode,
    NonStringKey,
    DynamicContainer,
    DynamicProperty,
    SparseArray,
    Cycle,
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
    ExplicitOverride,
    InheritedProjectDefault,
    SameDestination,
    ConflictingDestination,
    LiteralSelfReference,
    LiteralDuplicate,
    IntentSourceDuplicate,
    LocaleNotRequested,
    SurfaceClassUnknown,
    SurfaceClassDuplicate,
    NotFound,
    KindMismatch,
    SpecificationRevisionMismatch,
    SpecificationRevisionUnsupported,
    RoleKindMismatch,
    EnvelopeInvalid,
    KindUnsupported,
    DuplicateEnvelope,
    ConflictingContent,
    EqualEndpoint,
    UnknownEndpoint,
    CrossGroup,
    DuplicateRelation,
    ServerCapabilityMissing,
    ClientCapabilityMissing,
    DualRole,
    RequestedLocales,
    EffectiveDefault,
    BodySchemaInvalid,
    ProjectProfileResolutionMissing,
    ProjectProfileResolutionInvalid,
    Entry,
    Shared,
}

impl Reason {
    /// Every reason, in the order of 015's tables, each where it first
    /// appears.
    pub(crate) const ALL: [Self; 52] = [
        Self::UnsupportedType,
        Self::NonFiniteNumber,
        Self::NonPortableNumber,
        Self::InvalidUnicode,
        Self::NonStringKey,
        Self::DynamicContainer,
        Self::DynamicProperty,
        Self::SparseArray,
        Self::Cycle,
        Self::MissingRequiredSlot,
        Self::Unsupported,
        Self::Incompatible,
        Self::BodyIntegrityInvalid,
        Self::MemberInvalid,
        Self::MemberDuplicate,
        Self::NonCanonicalOrder,
        Self::IdentityMismatch,
        Self::RevisionMismatch,
        Self::DigestMismatch,
        Self::ExplicitOverride,
        Self::InheritedProjectDefault,
        Self::SameDestination,
        Self::ConflictingDestination,
        Self::LiteralSelfReference,
        Self::LiteralDuplicate,
        Self::IntentSourceDuplicate,
        Self::LocaleNotRequested,
        Self::SurfaceClassUnknown,
        Self::SurfaceClassDuplicate,
        Self::NotFound,
        Self::KindMismatch,
        Self::SpecificationRevisionMismatch,
        Self::SpecificationRevisionUnsupported,
        Self::RoleKindMismatch,
        Self::EnvelopeInvalid,
        Self::KindUnsupported,
        Self::DuplicateEnvelope,
        Self::ConflictingContent,
        Self::EqualEndpoint,
        Self::UnknownEndpoint,
        Self::CrossGroup,
        Self::DuplicateRelation,
        Self::ServerCapabilityMissing,
        Self::ClientCapabilityMissing,
        Self::DualRole,
        Self::RequestedLocales,
        Self::EffectiveDefault,
        Self::BodySchemaInvalid,
        Self::ProjectProfileResolutionMissing,
        Self::ProjectProfileResolutionInvalid,
        Self::Entry,
        Self::Shared,
    ];

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::UnsupportedType => "unsupported-type",
            Self::NonFiniteNumber => "non-finite-number",
            Self::NonPortableNumber => "non-portable-number",
            Self::InvalidUnicode => "invalid-unicode",
            Self::NonStringKey => "non-string-key",
            Self::DynamicContainer => "dynamic-container",
            Self::DynamicProperty => "dynamic-property",
            Self::SparseArray => "sparse-array",
            Self::Cycle => "cycle",
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
            Self::ExplicitOverride => "explicit-override",
            Self::InheritedProjectDefault => "inherited-project-default",
            Self::SameDestination => "same-destination",
            Self::ConflictingDestination => "conflicting-destination",
            Self::LiteralSelfReference => "literal-self-reference",
            Self::LiteralDuplicate => "literal-duplicate",
            Self::IntentSourceDuplicate => "intent-source-duplicate",
            Self::LocaleNotRequested => "locale-not-requested",
            Self::SurfaceClassUnknown => "surface-class-unknown",
            Self::SurfaceClassDuplicate => "surface-class-duplicate",
            Self::NotFound => "not-found",
            Self::KindMismatch => "kind-mismatch",
            Self::SpecificationRevisionMismatch => "specification-revision-mismatch",
            Self::SpecificationRevisionUnsupported => "specification-revision-unsupported",
            Self::RoleKindMismatch => "role-kind-mismatch",
            Self::EnvelopeInvalid => "envelope-invalid",
            Self::KindUnsupported => "kind-unsupported",
            Self::DuplicateEnvelope => "duplicate-envelope",
            Self::ConflictingContent => "conflicting-content",
            Self::EqualEndpoint => "equal-endpoint",
            Self::UnknownEndpoint => "unknown-endpoint",
            Self::CrossGroup => "cross-group",
            Self::DuplicateRelation => "duplicate-relation",
            Self::ServerCapabilityMissing => "server-capability-missing",
            Self::ClientCapabilityMissing => "client-capability-missing",
            Self::DualRole => "dual-role",
            Self::RequestedLocales => "requested-locales",
            Self::EffectiveDefault => "effective-default",
            Self::BodySchemaInvalid => "body-schema-invalid",
            Self::ProjectProfileResolutionMissing => "project-profile-resolution-missing",
            Self::ProjectProfileResolutionInvalid => "project-profile-resolution-invalid",
            Self::Entry => "entry",
            Self::Shared => "shared",
        }
    }
}

/// A reason reads and writes as 015 spells it.
impl Serialize for Reason {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Reason {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let spelled = Cow::<str>::deserialize(deserializer)?;
        Self::ALL
            .into_iter()
            .find(|reason| reason.as_str() == spelled)
            .ok_or_else(|| de::Error::custom(format!("no Finding reason {spelled}")))
    }
}

/// A reason's schema is the closed list of 015's spellings, in table order.
impl JsonSchema for Reason {
    fn schema_name() -> Cow<'static, str> {
        "FindingReason".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        let reasons: Vec<&str> = Self::ALL.iter().map(|reason| reason.as_str()).collect();
        schemars::json_schema!({ "type": "string", "enum": reasons })
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

    /// Work under a resolver-side `bootstrapOnly` bound past the capability's
    /// value.
    pub(crate) const fn bootstrap_limit(exceeded: BootstrapLimitExceeded) -> Self {
        Self {
            code: Code::BootstrapLimitExceeded,
            reason: None,
            details: Some(Details::BootstrapLimit(exceeded)),
        }
    }

    /// The same result as a unit whose key can stand for several values
    /// reports it: past its bound only the first-over witness `limit + 1`,
    /// whichever value the unit measured, since every value the key stands
    /// for reports the same (Decision 015-157).
    pub(crate) const fn first_over(self) -> Self {
        match self.details {
            Some(Details::Limit(exceeded)) => Self {
                details: Some(Details::Limit(LimitExceeded {
                    // `Bound` excludes `u64::MAX`, so the witness is
                    // representable.
                    actual: Actual::AtLeast(exceeded.limit.get() + 1),
                    ..exceeded
                })),
                ..self
            },
            _ => self,
        }
    }
}

/// The details 015 reports with an issue that has them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Details {
    Limit(LimitExceeded),
    Shortfall(Shortfall),
    BootstrapLimit(BootstrapLimitExceeded),
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

/// 015's details of work past a resolver-side `bootstrapOnly` bound: the bound,
/// whose capability bound path the Finding names, its value, and the total.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BootstrapLimitExceeded {
    pub(crate) bound: BootstrapBoundId,
    pub(crate) limit: Bound,
    pub(crate) actual: Actual,
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
    /// A resolver bug: a unit's subject is of a kind another phase checks.
    KindOutsidePhase(Unit),
    /// A resolver bug: a unit runs a check its subject's kind does not run.
    CheckOutsideKind(Unit),
    /// A resolver bug: a unit waits on a result that comes after it in
    /// canonical unit order, so a prerequisite would follow its dependent.
    CauseNotBefore(Unit),
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
    /// The unit a test makes block.
    #[cfg(test)]
    blocked: Option<Unit>,
    /// Whether a test fills the evaluation with results no check reports.
    #[cfg(test)]
    fabricated: bool,
}

impl Evaluation {
    /// An evaluation in which `unit`, once its prerequisite is admitted,
    /// blocks whatever it checks, so a test sees what waits on it: a unit
    /// that needs its result is not evaluated, and a unit whose subject only
    /// its result names is not recorded.
    #[cfg(test)]
    pub(crate) fn blocking(unit: Unit) -> Self {
        Self {
            blocked: Some(unit),
            ..Self::default()
        }
    }

    /// An evaluation a test fills with results no check of the resolver
    /// reports, such as a code standing for an earlier phase's. Every other
    /// evaluation a test runs reports only the codes each unit's Check
    /// Registry row lists.
    #[cfg(test)]
    pub(crate) fn fabricated() -> Self {
        Self {
            fabricated: true,
            ..Self::default()
        }
    }

    /// This evaluation, which a test goes on to fill with results no check
    /// of the resolver reports, such as a notice added to a real resolution.
    #[cfg(test)]
    pub(crate) fn fabricating(self) -> Self {
        Self {
            fabricated: true,
            ..self
        }
    }

    /// Asserts that `unit`'s Check Registry row lists `code`, that the code
    /// blocks exactly when the result does, as its Finding Registry row says,
    /// and that the result carries what the row's payload strategy gives the
    /// code: a
    /// reason exactly when its details have one, and then one the row lists
    /// for the code, a bound's or a shortfall's
    /// details exactly when they have its members, and past a bound an exact
    /// total only under a key that names one value, and otherwise the
    /// smallest witness `limit + 1`. Unless a test made the result up or made
    /// the unit block.
    #[cfg(test)]
    fn registered(
        &self,
        unit: &Unit,
        code: Code,
        reason: Option<Reason>,
        details: Option<Details>,
        blocking: bool,
    ) {
        use crate::construction::registry::check::{testing, Derivation, DetailsMember};
        if self.fabricated || self.blocked.as_ref() == Some(unit) {
            return;
        }
        let kind = unit.subject.kind();
        let listed = testing::possible_codes(kind, unit.check);
        assert!(
            listed.contains(&code),
            "{unit:?} reports {}, which its Check Registry row does not list",
            code.as_str()
        );
        assert_eq!(
            crate::construction::registry::finding::testing::blocking(code),
            blocking,
            "{unit:?} reports {} {}, against its Finding Registry row",
            code.as_str(),
            if blocking { "blocking" } else { "as a notice" }
        );
        let members = testing::details_members(code);
        assert_eq!(
            reason.is_some(),
            members.contains(&DetailsMember::Reason),
            "{unit:?} reports {} with {reason:?}, against its payload strategy",
            code.as_str()
        );
        if let Some(reason) = reason {
            assert!(
                testing::reasons(kind, unit.check, code).contains(&reason),
                "{unit:?} reports {} with {}, which its Check Registry row does not list",
                code.as_str(),
                reason.as_str()
            );
        }
        let carried = match details {
            Some(Details::Limit(exceeded)) => {
                let limit = exceeded.limit.get();
                let measured = match exceeded.actual {
                    // An exact total only under a key that names one value.
                    Actual::Exact(total) => {
                        total > limit && testing::derivation(kind, unit.check) == Derivation::Exact
                    }
                    Actual::AtLeast(witness) => witness == limit + 1,
                };
                measured && members.contains(&DetailsMember::Actual)
            }
            Some(Details::Shortfall(_)) => members.contains(&DetailsMember::Available),
            Some(Details::BootstrapLimit(exceeded)) => {
                let limit = exceeded.limit.get();
                let measured = match exceeded.actual {
                    Actual::Exact(total) => total > limit,
                    Actual::AtLeast(witness) => witness == limit + 1,
                };
                measured && members.contains(&DetailsMember::Actual)
            }
            None => {
                !members.contains(&DetailsMember::Actual)
                    && !members.contains(&DetailsMember::Available)
            }
        };
        assert!(
            carried,
            "{unit:?} reports {} with {details:?}, against its payload strategy",
            code.as_str()
        );
    }

    /// Runs `unit` when its prerequisite is admitted and records its status.
    /// A unit whose subject is of another phase's kind, that runs a check its
    /// kind does not run, or that waits on a result not before it, is a
    /// resolver bug and is not recorded.
    /// `evaluate` returns the value the unit establishes or the blocking result
    /// it found. An unavailable prerequisite becomes the unit's causes: the unit
    /// reports nothing, and whatever depends on it inherits the same causes.
    pub(crate) fn check<P, T>(
        &mut self,
        unit: Unit,
        prerequisite: Admitted<P>,
        evaluate: impl FnOnce(P) -> Result<T, Blocking>,
    ) -> Result<Admitted<T>, EvaluationInvariant> {
        let kind = unit.subject.kind();
        if kind.phase() != unit.phase {
            return Err(EvaluationInvariant::KindOutsidePhase(unit));
        }
        if !kind.checks().contains(&unit.check) {
            return Err(EvaluationInvariant::CheckOutsideKind(unit));
        }
        if let Err(causes) = &prerequisite {
            if causes.keys().any(|cause| cause.unit >= unit) {
                return Err(EvaluationInvariant::CauseNotBefore(unit));
            }
        }
        if self.statuses.contains_key(&unit) {
            return Err(EvaluationInvariant::UnitRecordedTwice(unit));
        }
        #[cfg(test)]
        let evaluate = |value| {
            if self.blocked.as_ref() == Some(&unit) {
                // Any code serves: the test reads only which unit blocked.
                Err(Blocking {
                    code: Code::JsonInvalid,
                    reason: None,
                    details: None,
                })
            } else {
                evaluate(value)
            }
        };
        let (status, result) = match prerequisite {
            Err(causes) => (Status::NotEvaluated(causes.clone()), Err(causes)),
            Ok(value) => match evaluate(value) {
                Ok(value) => (Status::Evaluated, Ok(value)),
                Err(Blocking {
                    code,
                    reason,
                    details,
                }) => {
                    #[cfg(test)]
                    self.registered(&unit, code, reason, details, true);
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
        #[cfg(test)]
        self.registered(&unit, code, None, None, false);
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
        let phases = Phase::ALL;
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
            Code::ALL.map(Code::as_str),
            [
                "project-profile-json-invalid",
                "project-profile-json-member-duplicate",
                "project-profile-input-not-json-compatible",
                "project-profile-schema-version-unsupported",
                "project-profile-required-field-missing",
                "project-profile-field-type-invalid",
                "project-profile-field-value-invalid",
                "project-profile-unknown-field",
                "project-profile-selector-required",
                "project-profile-selector-invalid",
                "project-profile-selector-unknown",
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
                "project-profile-target-locale-direction-missing",
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
                "project-profile-bootstrap-limit-exceeded",
                "project-profile-resource-limit-exceeded",
                "project-profile-reporting-limit-exceeded",
            ]
        );
        assert_eq!(
            Reason::ALL.map(Reason::as_str),
            [
                "unsupported-type",
                "non-finite-number",
                "non-portable-number",
                "invalid-unicode",
                "non-string-key",
                "dynamic-container",
                "dynamic-property",
                "sparse-array",
                "cycle",
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
                "explicit-override",
                "inherited-project-default",
                "same-destination",
                "conflicting-destination",
                "literal-self-reference",
                "literal-duplicate",
                "intent-source-duplicate",
                "locale-not-requested",
                "surface-class-unknown",
                "surface-class-duplicate",
                "not-found",
                "kind-mismatch",
                "specification-revision-mismatch",
                "specification-revision-unsupported",
                "role-kind-mismatch",
                "envelope-invalid",
                "kind-unsupported",
                "duplicate-envelope",
                "conflicting-content",
                "equal-endpoint",
                "unknown-endpoint",
                "cross-group",
                "duplicate-relation",
                "server-capability-missing",
                "client-capability-missing",
                "dual-role",
                "requested-locales",
                "effective-default",
                "body-schema-invalid",
                "project-profile-resolution-missing",
                "project-profile-resolution-invalid",
                "entry",
                "shared",
            ]
        );
    }

    #[test]
    fn a_reason_reads_and_writes_as_015_spells_it() {
        for reason in Reason::ALL {
            let value = serde_json::to_value(reason).unwrap();
            assert_eq!(value, reason.as_str());
            assert_eq!(serde_json::from_value::<Reason>(value).unwrap(), reason);
        }
        assert!(serde_json::from_value::<Reason>(serde_json::json!("missing")).is_err());
    }

    /// 015's tables list each code's stable reasons; every reason is some
    /// code's, and the reasons are ordered where each first appears.
    #[test]
    fn each_codes_reasons_are_the_ones_015_lists() {
        assert_eq!(
            Code::InputNotJsonCompatible
                .reasons()
                .iter()
                .map(|reason| reason.as_str())
                .collect::<Vec<_>>(),
            [
                "unsupported-type",
                "non-finite-number",
                "non-portable-number",
                "invalid-unicode",
                "non-string-key",
                "dynamic-container",
                "dynamic-property",
                "sparse-array",
                "cycle"
            ]
        );
        assert_eq!(
            Code::ArtifactReferenceNotAdmitted
                .reasons()
                .iter()
                .map(|reason| reason.as_str())
                .collect::<Vec<_>>(),
            [
                "not-found",
                "kind-mismatch",
                "identity-mismatch",
                "revision-mismatch",
                "specification-revision-mismatch",
                "specification-revision-unsupported",
                "role-kind-mismatch",
                "digest-mismatch"
            ]
        );
        assert_eq!(
            Code::ReportingLimitExceeded.reasons(),
            [Reason::Entry, Reason::Shared]
        );
        assert!(Code::LocaleDataNotAdmitted.reasons().is_empty());
        let mut first: Vec<Reason> = Vec::new();
        for code in Code::ALL {
            for &reason in code.reasons() {
                if !first.contains(&reason) {
                    first.push(reason);
                }
            }
        }
        assert_eq!(first, Reason::ALL);
        let reasoned = Code::ALL
            .into_iter()
            .filter(|code| !code.reasons().is_empty())
            .count();
        assert_eq!(reasoned, 19);
    }

    #[test]
    fn a_code_reads_and_writes_as_015_spells_it() {
        for code in Code::ALL {
            let value = serde_json::to_value(code).unwrap();
            assert_eq!(value, code.as_str());
            assert_eq!(serde_json::from_value::<Code>(value).unwrap(), code);
        }
        assert!(serde_json::from_value::<Code>(serde_json::json!("json-invalid")).is_err());
        let schema = serde_json::to_value(schemars::schema_for!(Code)).unwrap();
        assert_eq!(
            schema["enum"],
            serde_json::json!(Code::ALL.map(Code::as_str).to_vec())
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

    /// A result `data()`'s check never reports.
    const MADE_UP: Blocking = Blocking {
        code: Code::LocaleInvalid,
        reason: None,
        details: None,
    };

    #[test]
    #[should_panic(expected = "which its Check Registry row does not list")]
    fn a_test_evaluation_records_only_codes_the_units_row_lists() {
        let mut evaluation = Evaluation::default();
        let _ = evaluation.check(data(), Ok(()), |()| Err::<(), _>(MADE_UP));
    }

    #[test]
    #[should_panic(expected = "which its Check Registry row does not list")]
    fn a_test_evaluation_records_only_notices_the_units_row_lists() {
        let mut evaluation = Evaluation::default();
        let _ = evaluation.check(data(), Ok(()), |()| Ok::<(), Blocking>(()));
        let _ = evaluation.notice(data(), Code::LocaleNonCanonical);
    }

    #[test]
    fn a_made_up_result_or_a_blocked_unit_is_not_compared_with_the_registry() {
        for mut evaluation in [Evaluation::fabricated(), Evaluation::blocking(data())] {
            let found = evaluation
                .check(data(), Ok(()), |()| Err::<(), _>(MADE_UP))
                .unwrap();
            assert!(found.is_err());
        }
        let mut evaluation = Evaluation::fabricated();
        evaluation
            .check(data(), Ok(()), |()| Ok::<(), Blocking>(()))
            .unwrap()
            .unwrap();
        evaluation.notice(data(), Code::LocaleNonCanonical).unwrap();
        // A real evaluation a test goes on to fill.
        let mut evaluation = Evaluation::default();
        evaluation
            .check(data(), Ok(()), |()| Ok::<(), Blocking>(()))
            .unwrap()
            .unwrap();
        let mut evaluation = evaluation.fabricating();
        evaluation.notice(data(), Code::LocaleNonCanonical).unwrap();
        assert_eq!(evaluation.notices().count(), 1);
    }

    #[test]
    #[should_panic(expected = "which its Check Registry row does not list")]
    fn a_test_evaluation_records_only_reasons_the_units_row_lists() {
        let mut evaluation = Evaluation::default();
        let _ = evaluation.check(slot(Slot::Coverage), Ok(()), |()| {
            Err::<(), _>(Blocking {
                code: Code::CoverageSpecificationNotAdmitted,
                ..MISSING
            })
        });
    }

    #[test]
    #[should_panic(expected = "against its Finding Registry row")]
    fn a_test_evaluation_records_a_blocking_code_only_as_a_blocking_result() {
        let mut evaluation = Evaluation::default();
        let _ = evaluation.check(data(), Ok(()), |()| Ok::<(), Blocking>(()));
        let _ = evaluation.notice(data(), Code::LocaleDataNotAdmitted);
    }

    #[test]
    #[should_panic(expected = "against its Finding Registry row")]
    fn a_test_evaluation_records_a_non_blocking_code_only_as_a_notice() {
        let mut evaluation = Evaluation::default();
        let _ = evaluation.check(spelled(), Ok(()), |()| {
            Err::<(), _>(Blocking {
                code: Code::LocaleNonCanonical,
                reason: None,
                details: None,
            })
        });
    }

    /// The data's check reports a code without details.
    const UNAVAILABLE: Code = Code::LocaleDataNotAdmitted;

    #[test]
    #[should_panic(expected = "against its payload strategy")]
    fn a_test_evaluation_records_no_reason_where_the_codes_details_have_none() {
        let mut evaluation = Evaluation::default();
        let _ = evaluation.check(data(), Ok(()), |()| {
            Err::<(), _>(Blocking {
                code: UNAVAILABLE,
                reason: Some(Reason::Incompatible),
                details: None,
            })
        });
    }

    #[test]
    #[should_panic(expected = "against its payload strategy")]
    fn a_test_evaluation_records_a_reason_where_the_codes_details_have_one() {
        let mut evaluation = Evaluation::default();
        let _ = evaluation.check(slot(Slot::Coverage), Ok(()), |()| {
            Err::<(), _>(Blocking {
                reason: None,
                ..MISSING
            })
        });
    }

    #[test]
    #[should_panic(expected = "against its payload strategy")]
    fn a_test_evaluation_records_a_bounds_details_only_where_the_code_has_them() {
        let mut evaluation = Evaluation::default();
        let _ = evaluation.check(data(), Ok(()), |()| {
            Err::<(), _>(Blocking {
                code: UNAVAILABLE,
                reason: None,
                details: Some(Details::Limit(exceeded(Actual::Exact(4)))),
            })
        });
    }

    #[test]
    #[should_panic(expected = "against its payload strategy")]
    fn a_test_evaluation_records_a_bounds_details_wherever_the_code_has_them() {
        let mut evaluation = Evaluation::default();
        let _ = evaluation.check(occurrences(), Ok(()), |()| {
            Err::<(), _>(Blocking {
                details: None,
                ..Blocking::limit(exceeded(Actual::Exact(4)))
            })
        });
    }

    /// The occurrence bound's unit, whose key names one total.
    fn occurrences() -> Unit {
        Unit {
            phase: Phase::LocaleResolution,
            subject: Subject::Locale(LocaleSubject::Occurrences),
            check: Check::WithinBound,
        }
    }

    /// A requested locale's unit, whose key can stand for several spellings.
    fn spelled() -> Unit {
        use crate::resolver::locales::{Content, LocaleKey};
        Unit {
            phase: Phase::LocaleResolution,
            subject: Subject::Locale(LocaleSubject::RequestedLocale(LocaleKey::Content(
                Content::new("x-long"),
            ))),
            check: Check::CanonicalLocale,
        }
    }

    fn exceeded(actual: Actual) -> LimitExceeded {
        LimitExceeded {
            bound: BoundId {
                group: crate::resolver::limits::Group::LocaleResolution,
                name: "maxLocaleOccurrences",
            },
            limit: Bound::new(3).unwrap(),
            actual,
        }
    }

    #[test]
    fn past_a_bound_a_unit_records_its_exact_total_or_the_smallest_witness() {
        let mut evaluation = Evaluation::default();
        for (unit, actual) in [
            (occurrences(), Actual::Exact(4)),
            (spelled(), Actual::AtLeast(4)),
        ] {
            let found = evaluation
                .check(unit, Ok(()), |()| {
                    Err::<(), _>(Blocking::limit(exceeded(actual)))
                })
                .unwrap();
            assert!(found.is_err());
        }
        let mut evaluation = Evaluation::default();
        let found = evaluation
            .check(occurrences(), Ok(()), |()| {
                Err::<(), _>(Blocking::limit(exceeded(Actual::AtLeast(4))))
            })
            .unwrap();
        assert!(found.is_err());
    }

    #[test]
    #[should_panic(expected = "against its payload strategy")]
    fn past_a_bound_a_key_that_can_stand_for_several_values_records_only_the_witness() {
        let mut evaluation = Evaluation::default();
        let _ = evaluation.check(spelled(), Ok(()), |()| {
            Err::<(), _>(Blocking::limit(exceeded(Actual::Exact(4))))
        });
    }

    #[test]
    #[should_panic(expected = "against its payload strategy")]
    fn past_a_bound_the_witness_is_the_smallest() {
        let mut evaluation = Evaluation::default();
        let _ = evaluation.check(occurrences(), Ok(()), |()| {
            Err::<(), _>(Blocking::limit(exceeded(Actual::AtLeast(5))))
        });
    }

    #[test]
    #[should_panic(expected = "against its payload strategy")]
    fn an_exact_total_is_past_its_bound() {
        let mut evaluation = Evaluation::default();
        let _ = evaluation.check(occurrences(), Ok(()), |()| {
            Err::<(), _>(Blocking::limit(exceeded(Actual::Exact(3))))
        });
    }

    #[test]
    fn the_first_over_witness_replaces_a_bounds_total_and_nothing_else() {
        assert_eq!(
            Blocking::limit(exceeded(Actual::Exact(9))).first_over(),
            Blocking::limit(exceeded(Actual::AtLeast(4)))
        );
        assert_eq!(
            Blocking::limit(exceeded(Actual::AtLeast(4))).first_over(),
            Blocking::limit(exceeded(Actual::AtLeast(4)))
        );
        for unchanged in [
            MISSING,
            Blocking::shortfall(Shortfall {
                bound: exceeded(Actual::Exact(4)).bound,
                required: Bound::new(4).unwrap(),
                available: Bound::new(3).unwrap(),
            }),
        ] {
            assert_eq!(unchanged.first_over(), unchanged);
        }
    }

    #[test]
    fn a_test_can_make_a_unit_block_to_see_what_waits_on_it() {
        let blocked = slot(Slot::LocaleCanonicalization);
        let mut evaluation = Evaluation::blocking(blocked.clone());
        let mut ran = false;
        let causes = evaluation
            .check(blocked.clone(), Ok(()), |()| {
                ran = true;
                Ok::<(), Blocking>(())
            })
            .unwrap()
            .unwrap_err();
        assert!(!ran);
        let found: Vec<_> = evaluation.issues().cloned().collect();
        assert_eq!(causes.keys().collect::<Vec<_>>(), [&found[0]]);
        assert_eq!(found[0].unit(), &blocked);
        assert_eq!(evaluation.status(&blocked), Some(&Status::Evaluated));
        // What waits on it is not evaluated, and every other unit runs.
        let waiting = evaluation
            .check(data(), Err::<(), _>(causes.clone()), |()| {
                Ok::<(), Blocking>(())
            })
            .unwrap();
        assert_eq!(waiting, Err(causes.clone()));
        assert_eq!(
            evaluation.status(&data()),
            Some(&Status::NotEvaluated(causes))
        );
        let doubled = evaluation
            .check(slot(Slot::Coverage), Ok(2), |value| {
                Ok::<_, Blocking>(value * 2)
            })
            .unwrap();
        assert_eq!(doubled, Ok(4));
        // Without its prerequisite, the unit is not evaluated and finds nothing.
        let mut evaluation = Evaluation::blocking(data());
        let causes = missing(&mut evaluation, blocked);
        let waiting = evaluation
            .check(data(), Err::<(), _>(causes.clone()), |()| {
                Ok::<(), Blocking>(())
            })
            .unwrap();
        assert_eq!(waiting, Err(causes.clone()));
        assert_eq!(
            evaluation.status(&data()),
            Some(&Status::NotEvaluated(causes))
        );
        assert_eq!(evaluation.issues().count(), 1);
    }

    #[test]
    fn a_unit_is_of_a_kind_its_phase_checks_and_runs_one_of_its_checks() {
        let mut evaluation = Evaluation::default();
        let mut ran = false;
        let outside = Unit {
            phase: Phase::ResourcePolicyAdmission,
            ..data()
        };
        assert_eq!(
            evaluation
                .check(outside.clone(), Ok(()), |()| {
                    ran = true;
                    Ok::<(), Blocking>(())
                })
                .err(),
            Some(EvaluationInvariant::KindOutsidePhase(outside))
        );
        // An unavailable prerequisite does not hide a check the kind lacks.
        let foreign = Unit {
            check: Check::WithinBound,
            ..data()
        };
        let causes = missing(&mut evaluation, slot(Slot::Coverage));
        assert_eq!(
            evaluation
                .check(foreign.clone(), Err::<(), _>(causes), |()| {
                    ran = true;
                    Ok::<(), Blocking>(())
                })
                .err(),
            Some(EvaluationInvariant::CheckOutsideKind(foreign))
        );
        assert!(!ran);
        // Neither was recorded, so the right unit still can be.
        assert_eq!(evaluation.statuses().count(), 1);
        evaluation
            .check(data(), Ok(()), |()| Ok::<(), Blocking>(()))
            .unwrap()
            .unwrap();
        assert_eq!(evaluation.status(&data()), Some(&Status::Evaluated));
    }

    #[test]
    fn a_unit_waits_only_on_results_before_it() {
        // The data comes after the slots in its phase, and the declaration's
        // fields in the next phase.
        let mut earlier = Evaluation::fabricated();
        let slot_causes = missing(&mut earlier, slot(Slot::Coverage));
        let data_causes = missing(&mut earlier, data());
        let field_causes = missing(&mut earlier, field(DeclarationField::ProjectId));
        let mut ran = false;
        let mut evaluation = Evaluation::fabricated();
        for (unit, causes) in [
            // Not on a result of a later subject kind in the same phase,
            (slot(Slot::Coverage), data_causes.clone()),
            // of a later phase,
            (data(), field_causes),
            // or of the unit itself.
            (data(), data_causes),
            // One cause after it is enough.
            (
                data(),
                slot_causes
                    .clone()
                    .union(missing(&mut Evaluation::fabricated(), data())),
            ),
        ] {
            assert_eq!(
                evaluation
                    .check(unit.clone(), Err::<(), _>(causes), |()| {
                        ran = true;
                        Ok::<(), Blocking>(())
                    })
                    .err(),
                Some(EvaluationInvariant::CauseNotBefore(unit))
            );
        }
        assert!(!ran);
        assert_eq!(evaluation.statuses().count(), 0);
        // On an earlier result, the unit is not evaluated.
        evaluation
            .check(
                data(),
                Err::<(), _>(slot_causes.clone()),
                Ok::<(), Blocking>,
            )
            .unwrap()
            .unwrap_err();
        assert_eq!(
            evaluation.status(&data()),
            Some(&Status::NotEvaluated(slot_causes))
        );
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
        let mut evaluation = Evaluation::fabricated();
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
        let mut evaluation = Evaluation::fabricated();
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
        let mut evaluation = Evaluation::fabricated();
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
