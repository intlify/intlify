// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The subject kinds of the resolver's units: each kind is one shape of
//! Logical subject key in one phase, the unit of design 015's Check Registry
//! rows. Kinds are declared phase by phase in rank order, so a kind's place
//! among its phase's kinds is its rank, and subjects order by kind before key.
//! A kind names the checks it runs, in check order. Not the registry itself:
//! a kind's registry ID is its name in kebab case, and the Check Registry
//! gives it its key strategy, owning scope, parent, and projection. A later
//! change may split or merge kinds as 015's subject-key rules require.
//! The kinds of entry admission, structural admission, profile selection, and
//! reporting finalization are declared for the registry before the resolver
//! records their units ([`SubjectKind::declared_only`]).

use std::borrow::Cow;
use std::cmp::Ordering;

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::artifact_resolution::CollectionSubject;
use super::coverage::CoverageSubject;
use super::dependency::SemanticGroup;
use super::evaluation::{Check, Phase, Subject};
use super::fallback::FallbackSubject;
use super::groups::GroupSubject;
use super::hydration::HydrationSubject;
use super::locales::LocaleSubject;
use super::negotiation::NegotiationSubject;
use super::resource_policy::{ArtifactSubject, PolicySubject, ReferenceField};
use super::specification::Slot;
use super::targets::TargetSubject;

/// The kind of a unit's subject, declared phase by phase in rank order. Its
/// registry ID is its name in kebab case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SubjectKind {
    // resolver-bootstrap-admission
    /// The `project-profile` slot of the Profile Resolution Specification
    /// Input.
    ProjectProfileSlot,

    // entry-admission
    /// The owner of the entry dependencies: the entry sources, the selector,
    /// and the inputs' origins.
    EntryDependencies,
    /// The complete input of the entry, and the root it materializes.
    CompleteInput,

    // structural-admission
    /// The owner of the materialized dependencies, one for each slot whose
    /// identity was established.
    MaterializedDependencies,
    /// The Configuration Schema Selection the root's `schemaVersion` makes.
    ConfigurationSchemaSelection,
    /// The schema-guided structural analysis, under the capability.
    CapabilityStructuralAnalysis,
    /// A field of the root, by its path.
    RootField,
    /// A key of `profiles`.
    ProfileKey,
    /// A field of one profile, by its path in the profile.
    ProfileField,
    /// A key of one profile's `targetProfiles`.
    TargetProfileKey,
    /// A field of one Target ID entry.
    TargetField,
    /// A key of one profile's `deploymentGroups`.
    DeploymentGroupKey,
    /// A field of one Group ID entry.
    GroupField,
    /// The `profileSpecification` group, which Phase 0 establishes: its unit
    /// owns the group's semantic dependency.
    ProfileSpecificationDependency,

    // profile-selection
    /// The root `profiles` object's declarations, under the capability.
    CapabilityProfiles,
    /// One declared Profile ID, under the capability.
    CapabilityProfileId,
    /// The normalized External Profile Selector Input.
    ProfileSelector,

    // resource-policy-admission
    /// Every submitted artifact occurrence, under the capability.
    CapabilityArtifacts,
    /// The occurrences declaring a Policy kind, under the capability.
    CapabilityPolicyArtifacts,
    /// The occurrences declaring the Target Profile kind, under the
    /// capability.
    CapabilityTargetProfileArtifacts,
    /// The selected declaration's references, under the capability.
    CapabilityReferences,
    /// The submitted bodies of one member key, under the capability.
    CapabilityArtifact,
    /// Every occurrence's canonical bytes, under the capability.
    CapabilityArtifactBytes,
    /// One `policies` reference of the selected declaration, under the
    /// capability.
    CapabilityPolicyReference,
    /// One Target ID's Target Profile reference, under the capability.
    CapabilityTargetProfileReference,
    /// Every reference's canonical bytes, under the capability.
    CapabilityReferenceBytes,
    /// The selected `resourceLimits` reference and the policy it names.
    ResourceLimits,
    /// The materialized root's logical values.
    RootNodes,
    /// The materialized root's deepest value level.
    RootDepth,
    /// The materialized root's object members and array elements.
    RootCollectionEntries,
    /// The bytes of the materialized root's strings and object keys.
    RootStringBytes,
    /// The materialized root's longest string or object key.
    RootLongestString,
    /// The root `profiles` object's declarations.
    Profiles,
    /// One declared Profile ID.
    ProfileId,
    /// A string External Profile Selector Input.
    Selector,
    /// The completed structural analysis.
    StructuralAnalysis,
    /// Every submitted artifact occurrence, under the admitted policy.
    Artifacts,
    /// The occurrences declaring a Policy kind, under the admitted policy.
    PolicyArtifacts,
    /// The occurrences declaring the Target Profile kind, under the admitted
    /// policy.
    TargetProfileArtifacts,
    /// The selected declaration's references, under the admitted policy.
    References,
    /// The submitted bodies of one member key, under the admitted policy.
    Artifact,
    /// Every occurrence's canonical bytes, under the admitted policy.
    ArtifactBytes,
    /// One `policies` reference of the selected declaration, under the
    /// admitted policy.
    PolicyReference,
    /// One Target ID's Target Profile reference, under the admitted policy.
    TargetProfileReference,
    /// Every reference's canonical bytes, under the admitted policy.
    ReferenceBytes,

    // specification-admission
    /// The `locale-canonicalization` slot.
    LocaleCanonicalizationSlot,
    /// The `locale-negotiation` slot.
    LocaleNegotiationSlot,
    /// The `message-locale-fallback` slot.
    MessageLocaleFallbackSlot,
    /// The `coverage` slot.
    CoverageSlot,
    /// The `intent-surface-class-vocabulary` slot.
    IntentSurfaceClassVocabularySlot,
    /// The `delivery-graph` slot.
    DeliveryGraphSlot,
    /// The `delivery-placement` slot.
    DeliveryPlacementSlot,
    /// The Intent Surface-Class Vocabulary Artifact.
    Vocabulary,
    /// The Locale Canonicalization Data Artifact.
    CanonicalizationData,
    /// The `localeCanonicalization` group: its unit owns the group's semantic
    /// dependency.
    LocaleCanonicalizationDependency,

    // project-identity-resolution
    /// `projectId` or `selectionScope`.
    DeclarationField,
    /// The `projectIdentity` group: its unit owns the group's semantic
    /// dependency.
    ProjectIdentityDependency,

    // locale-resolution
    /// Every locale spelling of the declaration.
    LocaleOccurrences,
    /// A member of the project's `requestedLocales`.
    RequestedLocale,
    /// The project's requested-locale set.
    RequestedLocales,
    /// The project's `defaultSourceLocale`.
    DefaultSourceLocale,
    /// The project's `defaultRequestedLocale`.
    DefaultRequestedLocale,
    /// A key of `localeNegotiation.aliases`.
    NegotiationAliasKey,
    /// An alias destination.
    NegotiationAliasValue,
    /// A key of `messageFallback`.
    MessageFallbackKey,
    /// The submitted Target ID map.
    TargetProfiles,
    /// One Target ID key.
    TargetId,
    /// A member of one target's `requestedLocales`.
    TargetRequestedLocale,
    /// One target's `defaultRequestedLocale`.
    TargetDefaultRequestedLocale,
    /// The `projectLocales` group: its unit owns the group's semantic
    /// dependency.
    ProjectLocalesDependency,

    // locale-policy-resolution
    /// The submitted alias map.
    NegotiationAliases,
    /// An alias key, for collisions.
    NegotiationKey,
    /// An alias destination, for membership.
    NegotiationDestination,
    /// The submitted fallback mapping.
    FallbackSources,
    /// A fallback mapping key.
    FallbackSource,
    /// The sequence of one fallback mapping key.
    FallbackSequence,
    /// A candidate at its position in its key's sequence.
    FallbackCandidate,
    /// A literal candidate's locale in its source's sequence.
    FallbackLiteral,
    /// The Intent source-locale candidate in its source's sequence.
    FallbackIntentSource,
    /// The submitted coverage rules.
    CoverageRules,
    /// Every coverage locale selector item.
    CoverageLocaleSelectors,
    /// Every coverage surface selector item.
    CoverageSurfaceSelectors,
    /// A locale one rule's selector names.
    CoverageLocale,
    /// A surface class one rule's selector names.
    CoverageSurfaceClass,
    /// A rule, normalized from its selector items.
    CoverageRule,
    /// A rule's normalized domain.
    CoverageDomain,
    /// The decision table's cells.
    CoverageCells,
    /// Every submitted rule against every cell.
    CoverageComparisons,
    /// One cell of the decision table.
    CoverageCell,
    /// The complete decision table.
    CoverageTable,
    /// The `localeNegotiation` group: its unit owns the group's semantic
    /// dependency.
    LocaleNegotiationDependency,
    /// The `messageFallback` group: its unit owns the group's semantic
    /// dependency.
    MessageFallbackDependency,
    /// The `coverage` group: its unit owns the group's semantic dependency.
    CoverageDependency,

    // artifact-reference-resolution
    /// The submitted bodies of one member key of the collection.
    CollectionMember,
    /// One conflict group: the members sharing a kind, identity, revision,
    /// and specification revision.
    CollectionGroup,
    /// One `policies` reference, resolved against the Set.
    ResolvedPolicyReference,
    /// One Target ID's Target Profile reference, resolved against the Set.
    ResolvedTargetProfileReference,
    /// The `requiredPolicies` group: its unit owns the group's semantic
    /// dependency.
    RequiredPoliciesDependency,
    /// The `optionalPolicies` group: its unit owns the group's semantic
    /// dependency.
    OptionalPoliciesDependency,

    // delivery-resolution
    /// The declaration's delivery placement.
    DeliveryPlacement,
    /// The `delivery` group: its unit owns the group's semantic dependency.
    DeliveryDependency,

    // target-resolution
    /// One Target ID.
    Target,
    /// The `targets` group: its unit owns the group's semantic dependency.
    TargetsDependency,

    // group-resolution
    /// The submitted Group ID map.
    Groups,
    /// Every member occurrence of every group.
    Memberships,
    /// One Group ID.
    Group,
    /// One group's member occurrences.
    GroupMembers,
    /// A member of one group, under `maxTargetIdBytes`.
    GroupMember,
    /// A declared Target ID's place among the groups.
    Assignment,

    // hydration-validation
    /// Every relation occurrence of every group.
    Relations,
    /// One endpoint of one relation: under `maxTargetIdBytes`, declared, in
    /// the relation's group, and taking its field's role.
    RelationEndpoint,
    /// One relation of one group.
    Relation,
    /// A Target ID's roles among one group's relations.
    Roles,
    /// The `deploymentGroups` group: its unit owns the group's semantic
    /// dependency.
    DeploymentGroupsDependency,

    // reporting-finalization
    /// The diagnostic reporting domains, a singleton: a terminal Finding's
    /// reason names the domain it exhausted.
    DiagnosticDomain,
}

impl SubjectKind {
    /// Every kind, phase by phase in rank order.
    pub(crate) const ALL: [Self; 115] = [
        Self::ProjectProfileSlot,
        Self::EntryDependencies,
        Self::CompleteInput,
        Self::MaterializedDependencies,
        Self::ConfigurationSchemaSelection,
        Self::CapabilityStructuralAnalysis,
        Self::RootField,
        Self::ProfileKey,
        Self::ProfileField,
        Self::TargetProfileKey,
        Self::TargetField,
        Self::DeploymentGroupKey,
        Self::GroupField,
        Self::ProfileSpecificationDependency,
        Self::CapabilityProfiles,
        Self::CapabilityProfileId,
        Self::ProfileSelector,
        Self::CapabilityArtifacts,
        Self::CapabilityPolicyArtifacts,
        Self::CapabilityTargetProfileArtifacts,
        Self::CapabilityReferences,
        Self::CapabilityArtifact,
        Self::CapabilityArtifactBytes,
        Self::CapabilityPolicyReference,
        Self::CapabilityTargetProfileReference,
        Self::CapabilityReferenceBytes,
        Self::ResourceLimits,
        Self::RootNodes,
        Self::RootDepth,
        Self::RootCollectionEntries,
        Self::RootStringBytes,
        Self::RootLongestString,
        Self::Profiles,
        Self::ProfileId,
        Self::Selector,
        Self::StructuralAnalysis,
        Self::Artifacts,
        Self::PolicyArtifacts,
        Self::TargetProfileArtifacts,
        Self::References,
        Self::Artifact,
        Self::ArtifactBytes,
        Self::PolicyReference,
        Self::TargetProfileReference,
        Self::ReferenceBytes,
        Self::LocaleCanonicalizationSlot,
        Self::LocaleNegotiationSlot,
        Self::MessageLocaleFallbackSlot,
        Self::CoverageSlot,
        Self::IntentSurfaceClassVocabularySlot,
        Self::DeliveryGraphSlot,
        Self::DeliveryPlacementSlot,
        Self::Vocabulary,
        Self::CanonicalizationData,
        Self::LocaleCanonicalizationDependency,
        Self::DeclarationField,
        Self::ProjectIdentityDependency,
        Self::LocaleOccurrences,
        Self::RequestedLocale,
        Self::RequestedLocales,
        Self::DefaultSourceLocale,
        Self::DefaultRequestedLocale,
        Self::NegotiationAliasKey,
        Self::NegotiationAliasValue,
        Self::MessageFallbackKey,
        Self::TargetProfiles,
        Self::TargetId,
        Self::TargetRequestedLocale,
        Self::TargetDefaultRequestedLocale,
        Self::ProjectLocalesDependency,
        Self::NegotiationAliases,
        Self::NegotiationKey,
        Self::NegotiationDestination,
        Self::FallbackSources,
        Self::FallbackSource,
        Self::FallbackSequence,
        Self::FallbackCandidate,
        Self::FallbackLiteral,
        Self::FallbackIntentSource,
        Self::CoverageRules,
        Self::CoverageLocaleSelectors,
        Self::CoverageSurfaceSelectors,
        Self::CoverageLocale,
        Self::CoverageSurfaceClass,
        Self::CoverageRule,
        Self::CoverageDomain,
        Self::CoverageCells,
        Self::CoverageComparisons,
        Self::CoverageCell,
        Self::CoverageTable,
        Self::LocaleNegotiationDependency,
        Self::MessageFallbackDependency,
        Self::CoverageDependency,
        Self::CollectionMember,
        Self::CollectionGroup,
        Self::ResolvedPolicyReference,
        Self::ResolvedTargetProfileReference,
        Self::RequiredPoliciesDependency,
        Self::OptionalPoliciesDependency,
        Self::DeliveryPlacement,
        Self::DeliveryDependency,
        Self::Target,
        Self::TargetsDependency,
        Self::Groups,
        Self::Memberships,
        Self::Group,
        Self::GroupMembers,
        Self::GroupMember,
        Self::Assignment,
        Self::Relations,
        Self::RelationEndpoint,
        Self::Relation,
        Self::Roles,
        Self::DeploymentGroupsDependency,
        Self::DiagnosticDomain,
    ];

    /// The phase whose units have subjects of this kind.
    pub(crate) const fn phase(self) -> Phase {
        match self {
            Self::ProjectProfileSlot => Phase::ResolverBootstrapAdmission,
            Self::EntryDependencies | Self::CompleteInput => Phase::EntryAdmission,
            Self::MaterializedDependencies
            | Self::ConfigurationSchemaSelection
            | Self::CapabilityStructuralAnalysis
            | Self::RootField
            | Self::ProfileKey
            | Self::ProfileField
            | Self::TargetProfileKey
            | Self::TargetField
            | Self::DeploymentGroupKey
            | Self::GroupField
            | Self::ProfileSpecificationDependency => Phase::StructuralAdmission,
            Self::CapabilityProfiles | Self::CapabilityProfileId | Self::ProfileSelector => {
                Phase::ProfileSelection
            }
            Self::CapabilityArtifacts
            | Self::CapabilityPolicyArtifacts
            | Self::CapabilityTargetProfileArtifacts
            | Self::CapabilityReferences
            | Self::CapabilityArtifact
            | Self::CapabilityArtifactBytes
            | Self::CapabilityPolicyReference
            | Self::CapabilityTargetProfileReference
            | Self::CapabilityReferenceBytes
            | Self::ResourceLimits
            | Self::RootNodes
            | Self::RootDepth
            | Self::RootCollectionEntries
            | Self::RootStringBytes
            | Self::RootLongestString
            | Self::Profiles
            | Self::ProfileId
            | Self::Selector
            | Self::StructuralAnalysis
            | Self::Artifacts
            | Self::PolicyArtifacts
            | Self::TargetProfileArtifacts
            | Self::References
            | Self::Artifact
            | Self::ArtifactBytes
            | Self::PolicyReference
            | Self::TargetProfileReference
            | Self::ReferenceBytes => Phase::ResourcePolicyAdmission,
            Self::LocaleCanonicalizationSlot
            | Self::LocaleNegotiationSlot
            | Self::MessageLocaleFallbackSlot
            | Self::CoverageSlot
            | Self::IntentSurfaceClassVocabularySlot
            | Self::DeliveryGraphSlot
            | Self::DeliveryPlacementSlot
            | Self::Vocabulary
            | Self::CanonicalizationData
            | Self::LocaleCanonicalizationDependency => Phase::SpecificationAdmission,
            Self::DeclarationField | Self::ProjectIdentityDependency => {
                Phase::ProjectIdentityResolution
            }
            Self::LocaleOccurrences
            | Self::RequestedLocale
            | Self::RequestedLocales
            | Self::DefaultSourceLocale
            | Self::DefaultRequestedLocale
            | Self::NegotiationAliasKey
            | Self::NegotiationAliasValue
            | Self::MessageFallbackKey
            | Self::TargetProfiles
            | Self::TargetId
            | Self::TargetRequestedLocale
            | Self::TargetDefaultRequestedLocale
            | Self::ProjectLocalesDependency => Phase::LocaleResolution,
            Self::NegotiationAliases
            | Self::NegotiationKey
            | Self::NegotiationDestination
            | Self::FallbackSources
            | Self::FallbackSource
            | Self::FallbackSequence
            | Self::FallbackCandidate
            | Self::FallbackLiteral
            | Self::FallbackIntentSource
            | Self::CoverageRules
            | Self::CoverageLocaleSelectors
            | Self::CoverageSurfaceSelectors
            | Self::CoverageLocale
            | Self::CoverageSurfaceClass
            | Self::CoverageRule
            | Self::CoverageDomain
            | Self::CoverageCells
            | Self::CoverageComparisons
            | Self::CoverageCell
            | Self::CoverageTable
            | Self::LocaleNegotiationDependency
            | Self::MessageFallbackDependency
            | Self::CoverageDependency => Phase::LocalePolicyResolution,
            Self::CollectionMember
            | Self::CollectionGroup
            | Self::ResolvedPolicyReference
            | Self::ResolvedTargetProfileReference
            | Self::RequiredPoliciesDependency
            | Self::OptionalPoliciesDependency => Phase::ArtifactReferenceResolution,
            Self::DeliveryPlacement | Self::DeliveryDependency => Phase::DeliveryResolution,
            Self::Target | Self::TargetsDependency => Phase::TargetResolution,
            Self::Groups
            | Self::Memberships
            | Self::Group
            | Self::GroupMembers
            | Self::GroupMember
            | Self::Assignment => Phase::GroupResolution,
            Self::Relations
            | Self::RelationEndpoint
            | Self::Relation
            | Self::Roles
            | Self::DeploymentGroupsDependency => Phase::HydrationValidation,
            Self::DiagnosticDomain => Phase::ReportingFinalization,
        }
    }

    /// The checks a unit of this kind may run, in check order.
    pub(crate) const fn checks(self) -> &'static [Check] {
        match self {
            Self::ProjectProfileSlot
            | Self::LocaleCanonicalizationSlot
            | Self::LocaleNegotiationSlot
            | Self::MessageLocaleFallbackSlot
            | Self::CoverageSlot
            | Self::IntentSurfaceClassVocabularySlot
            | Self::DeliveryGraphSlot
            | Self::DeliveryPlacementSlot => &[Check::SupportedAssertion],
            Self::CapabilityArtifacts
            | Self::CapabilityPolicyArtifacts
            | Self::CapabilityTargetProfileArtifacts
            | Self::CapabilityReferences
            | Self::CapabilityArtifact
            | Self::CapabilityArtifactBytes
            | Self::CapabilityPolicyReference
            | Self::CapabilityTargetProfileReference
            | Self::CapabilityReferenceBytes
            | Self::CapabilityStructuralAnalysis
            | Self::CapabilityProfiles
            | Self::CapabilityProfileId => &[Check::WithinCapability],
            // Each artifactBootstrap bound comes before the work it protects
            // (Decision 015-164).
            Self::ResourceLimits => &[
                Check::WithinMetadataScan,
                Check::BootstrapCandidate,
                Check::WithinEnvelopeDepth,
                Check::WithinDecodedAllocation,
                Check::WithinValidationWork,
                Check::CandidateBody,
                Check::CompatiblePolicy,
            ],
            Self::RootNodes
            | Self::RootDepth
            | Self::RootCollectionEntries
            | Self::RootStringBytes
            | Self::RootLongestString
            | Self::Profiles
            | Self::ProfileId
            | Self::Selector
            | Self::StructuralAnalysis
            | Self::Artifacts
            | Self::PolicyArtifacts
            | Self::TargetProfileArtifacts
            | Self::References
            | Self::Artifact
            | Self::ArtifactBytes
            | Self::PolicyReference
            | Self::TargetProfileReference
            | Self::ReferenceBytes
            | Self::LocaleOccurrences
            | Self::RequestedLocales
            | Self::NegotiationAliases
            | Self::FallbackSources
            | Self::FallbackSequence
            | Self::CoverageRules
            | Self::CoverageLocaleSelectors
            | Self::CoverageSurfaceSelectors
            | Self::CoverageCells
            | Self::CoverageComparisons
            | Self::TargetProfiles
            | Self::TargetId
            | Self::Groups
            | Self::Memberships
            | Self::Group
            | Self::GroupMembers
            | Self::Relations => &[Check::WithinBound],
            Self::Vocabulary => &[Check::IntactArtifact, Check::AssertedArtifact],
            Self::CanonicalizationData => &[Check::MatchingData],
            Self::DeclarationField => &[Check::OpaqueIdentity],
            Self::RequestedLocale | Self::TargetRequestedLocale => {
                &[Check::CanonicalLocale, Check::UniqueLocale]
            }
            Self::DefaultSourceLocale
            | Self::NegotiationAliasKey
            | Self::NegotiationAliasValue
            | Self::MessageFallbackKey
            | Self::TargetDefaultRequestedLocale => &[Check::CanonicalLocale],
            Self::DefaultRequestedLocale => &[Check::CanonicalLocale, Check::RequestedMember],
            Self::NegotiationKey => &[Check::UniqueLocale],
            Self::NegotiationDestination => &[Check::RequestedMember],
            Self::FallbackSource => &[Check::UniqueLocale, Check::RequestedMember],
            Self::FallbackCandidate => &[Check::CanonicalLocale, Check::DistinctFromSource],
            Self::FallbackLiteral | Self::FallbackIntentSource => &[Check::UniqueCandidate],
            Self::CoverageLocale => &[
                Check::CanonicalLocale,
                Check::UniqueLocale,
                Check::RequestedMember,
            ],
            Self::CoverageSurfaceClass => &[Check::VocabularyMember, Check::UniqueSurfaceClass],
            Self::CoverageRule => &[Check::NormalizedDomain],
            Self::CoverageDomain => &[Check::UniqueDomain],
            Self::CoverageCell | Self::DeliveryPlacement => &[Check::EffectiveMode],
            Self::CoverageTable => &[Check::CompleteTable],
            Self::CollectionMember => &[Check::AdmissibleMember, Check::SingleOccurrence],
            Self::CollectionGroup => &[Check::ConsistentContent],
            Self::ResolvedPolicyReference | Self::ResolvedTargetProfileReference => {
                &[Check::ResolvedReference]
            }
            Self::Target => &[
                Check::ProjectSubset,
                Check::SupportedDefault,
                Check::DirectedLocales,
            ],
            Self::GroupMember => &[Check::WithinBound, Check::KnownMember, Check::UniqueMember],
            Self::Assignment => &[Check::SingleGroup],
            Self::RelationEndpoint => &[
                Check::WithinBound,
                Check::KnownEndpoint,
                Check::SameGroupEndpoint,
                Check::EndpointRole,
            ],
            Self::Relation => &[
                Check::DistinctEndpoints,
                Check::UniqueRelation,
                Check::SameRequestedLocales,
                Check::SameEffectiveDefault,
                Check::StaticCompatibility,
            ],
            Self::Roles => &[Check::SingleRole],
            Self::ProfileSpecificationDependency
            | Self::LocaleCanonicalizationDependency
            | Self::ProjectIdentityDependency
            | Self::ProjectLocalesDependency
            | Self::LocaleNegotiationDependency
            | Self::MessageFallbackDependency
            | Self::CoverageDependency
            | Self::RequiredPoliciesDependency
            | Self::OptionalPoliciesDependency
            | Self::DeliveryDependency
            | Self::TargetsDependency
            | Self::DeploymentGroupsDependency => &[Check::EstablishedGroup],
            Self::EntryDependencies | Self::MaterializedDependencies => &[Check::EstablishedSlots],
            Self::CompleteInput => &[
                Check::WithinFileBytes,
                Check::WithinParserTokens,
                Check::WellFormedJson,
                Check::UniqueMembers,
                Check::JsonCompatible,
                Check::WithinNodes,
                Check::WithinDepth,
                Check::WithinCollectionEntries,
                Check::WithinTotalStringBytes,
                Check::WithinSingleStringBytes,
            ],
            Self::ConfigurationSchemaSelection => &[
                Check::ObjectRoot,
                Check::PresentSchemaVersion,
                Check::StringSchemaVersion,
                Check::SupportedSchemaVersion,
            ],
            Self::RootField | Self::ProfileField | Self::TargetField | Self::GroupField => &[
                Check::RequiredField,
                Check::FieldType,
                Check::FieldValue,
                Check::KnownField,
            ],
            Self::ProfileKey | Self::TargetProfileKey | Self::DeploymentGroupKey => {
                &[Check::FieldValue]
            }
            Self::ProfileSelector => &[
                Check::WithinCapability,
                Check::RequiredSelector,
                Check::ValidSelector,
                Check::DeclaredSelector,
            ],
            Self::DiagnosticDomain => &[Check::CompleteReporting],
        }
    }

    /// Whether the kind is declared for the Check Registry while the resolver
    /// records no unit of it yet: entry admission, structural admission, and
    /// profile selection build their units with their Findings, and reporting
    /// finalization with the outcome. Of these phases, only the owner of the
    /// `profileSpecification` group, which waits on Phase 0 alone, is recorded.
    pub(crate) const fn declared_only(self) -> bool {
        matches!(
            self,
            Self::EntryDependencies
                | Self::CompleteInput
                | Self::MaterializedDependencies
                | Self::ConfigurationSchemaSelection
                | Self::CapabilityStructuralAnalysis
                | Self::RootField
                | Self::ProfileKey
                | Self::ProfileField
                | Self::TargetProfileKey
                | Self::TargetField
                | Self::DeploymentGroupKey
                | Self::GroupField
                | Self::CapabilityProfiles
                | Self::CapabilityProfileId
                | Self::ProfileSelector
                | Self::DiagnosticDomain
        )
    }

    /// The kind's place among its phase's kinds.
    pub(crate) fn rank(self) -> usize {
        Self::ALL
            .iter()
            .filter(|kind| kind.phase() == self.phase())
            .position(|&kind| kind == self)
            .expect("every kind is listed")
    }
}

/// A kind's schema is the closed list of its registry IDs, in rank order,
/// without the variants' documentation.
impl JsonSchema for SubjectKind {
    fn schema_name() -> Cow<'static, str> {
        "SubjectKind".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        let ids: Vec<Value> = Self::ALL
            .iter()
            .filter_map(|kind| serde_json::to_value(kind).ok())
            .collect();
        schemars::json_schema!({ "type": "string", "enum": ids })
    }
}

impl Subject {
    /// The kind of this subject: one shape of key in one phase.
    pub(crate) const fn kind(&self) -> SubjectKind {
        match self {
            Self::Slot(slot) => match slot {
                Slot::ProjectProfile => SubjectKind::ProjectProfileSlot,
                Slot::LocaleCanonicalization => SubjectKind::LocaleCanonicalizationSlot,
                Slot::LocaleNegotiation => SubjectKind::LocaleNegotiationSlot,
                Slot::MessageLocaleFallback => SubjectKind::MessageLocaleFallbackSlot,
                Slot::Coverage => SubjectKind::CoverageSlot,
                Slot::IntentSurfaceClassVocabulary => SubjectKind::IntentSurfaceClassVocabularySlot,
                Slot::DeliveryGraph => SubjectKind::DeliveryGraphSlot,
                Slot::DeliveryPlacement => SubjectKind::DeliveryPlacementSlot,
            },
            Self::Vocabulary => SubjectKind::Vocabulary,
            Self::CanonicalizationData => SubjectKind::CanonicalizationData,
            Self::ResourcePolicy(subject) => match subject {
                PolicySubject::Capability(subject) => match subject {
                    ArtifactSubject::Artifacts => SubjectKind::CapabilityArtifacts,
                    ArtifactSubject::PolicyArtifacts => SubjectKind::CapabilityPolicyArtifacts,
                    ArtifactSubject::TargetProfileArtifacts => {
                        SubjectKind::CapabilityTargetProfileArtifacts
                    }
                    ArtifactSubject::References => SubjectKind::CapabilityReferences,
                    ArtifactSubject::Artifact(_) => SubjectKind::CapabilityArtifact,
                    ArtifactSubject::ArtifactBytes => SubjectKind::CapabilityArtifactBytes,
                    ArtifactSubject::Reference(ReferenceField::Policy(_)) => {
                        SubjectKind::CapabilityPolicyReference
                    }
                    ArtifactSubject::Reference(ReferenceField::TargetProfile(_)) => {
                        SubjectKind::CapabilityTargetProfileReference
                    }
                    ArtifactSubject::ReferenceBytes => SubjectKind::CapabilityReferenceBytes,
                },
                PolicySubject::ResourceLimits => SubjectKind::ResourceLimits,
                PolicySubject::Nodes => SubjectKind::RootNodes,
                PolicySubject::Depth => SubjectKind::RootDepth,
                PolicySubject::CollectionEntries => SubjectKind::RootCollectionEntries,
                PolicySubject::StringBytes => SubjectKind::RootStringBytes,
                PolicySubject::LongestString => SubjectKind::RootLongestString,
                PolicySubject::Profiles => SubjectKind::Profiles,
                PolicySubject::ProfileId(_) => SubjectKind::ProfileId,
                PolicySubject::Selector => SubjectKind::Selector,
                PolicySubject::StructuralAnalysis => SubjectKind::StructuralAnalysis,
                PolicySubject::Recheck(subject) => match subject {
                    ArtifactSubject::Artifacts => SubjectKind::Artifacts,
                    ArtifactSubject::PolicyArtifacts => SubjectKind::PolicyArtifacts,
                    ArtifactSubject::TargetProfileArtifacts => SubjectKind::TargetProfileArtifacts,
                    ArtifactSubject::References => SubjectKind::References,
                    ArtifactSubject::Artifact(_) => SubjectKind::Artifact,
                    ArtifactSubject::ArtifactBytes => SubjectKind::ArtifactBytes,
                    ArtifactSubject::Reference(ReferenceField::Policy(_)) => {
                        SubjectKind::PolicyReference
                    }
                    ArtifactSubject::Reference(ReferenceField::TargetProfile(_)) => {
                        SubjectKind::TargetProfileReference
                    }
                    ArtifactSubject::ReferenceBytes => SubjectKind::ReferenceBytes,
                },
            },
            Self::DeclarationField(_) => SubjectKind::DeclarationField,
            Self::Locale(subject) => match subject {
                LocaleSubject::Occurrences => SubjectKind::LocaleOccurrences,
                LocaleSubject::RequestedLocale(_) => SubjectKind::RequestedLocale,
                LocaleSubject::RequestedLocales => SubjectKind::RequestedLocales,
                LocaleSubject::DefaultSourceLocale => SubjectKind::DefaultSourceLocale,
                LocaleSubject::DefaultRequestedLocale => SubjectKind::DefaultRequestedLocale,
                LocaleSubject::NegotiationAliasKey(_) => SubjectKind::NegotiationAliasKey,
                LocaleSubject::NegotiationAliasValue(..) => SubjectKind::NegotiationAliasValue,
                LocaleSubject::MessageFallbackKey(_) => SubjectKind::MessageFallbackKey,
                LocaleSubject::TargetProfiles => SubjectKind::TargetProfiles,
                LocaleSubject::TargetId(_) => SubjectKind::TargetId,
                LocaleSubject::TargetRequestedLocale(..) => SubjectKind::TargetRequestedLocale,
                LocaleSubject::TargetDefaultRequestedLocale(..) => {
                    SubjectKind::TargetDefaultRequestedLocale
                }
            },
            Self::Negotiation(subject) => match subject {
                NegotiationSubject::Aliases => SubjectKind::NegotiationAliases,
                NegotiationSubject::Key(_) => SubjectKind::NegotiationKey,
                NegotiationSubject::Destination(..) => SubjectKind::NegotiationDestination,
            },
            Self::Fallback(subject) => match subject {
                FallbackSubject::Sources => SubjectKind::FallbackSources,
                FallbackSubject::Source(_) => SubjectKind::FallbackSource,
                FallbackSubject::Sequence(_) => SubjectKind::FallbackSequence,
                FallbackSubject::Candidate(..) => SubjectKind::FallbackCandidate,
                FallbackSubject::Literal(..) => SubjectKind::FallbackLiteral,
                FallbackSubject::IntentSource(_) => SubjectKind::FallbackIntentSource,
            },
            Self::Coverage(subject) => match subject {
                CoverageSubject::Rules => SubjectKind::CoverageRules,
                CoverageSubject::LocaleSelectors => SubjectKind::CoverageLocaleSelectors,
                CoverageSubject::SurfaceSelectors => SubjectKind::CoverageSurfaceSelectors,
                CoverageSubject::Locale(..) => SubjectKind::CoverageLocale,
                CoverageSubject::SurfaceClass(..) => SubjectKind::CoverageSurfaceClass,
                CoverageSubject::Rule(_) => SubjectKind::CoverageRule,
                CoverageSubject::Domain(_) => SubjectKind::CoverageDomain,
                CoverageSubject::Cells => SubjectKind::CoverageCells,
                CoverageSubject::Comparisons => SubjectKind::CoverageComparisons,
                CoverageSubject::Cell(..) => SubjectKind::CoverageCell,
                CoverageSubject::Table => SubjectKind::CoverageTable,
            },
            Self::ArtifactCollection(subject) => match subject {
                CollectionSubject::Member(_) => SubjectKind::CollectionMember,
                CollectionSubject::Group(_) => SubjectKind::CollectionGroup,
            },
            Self::ArtifactReference(field) => match field {
                ReferenceField::Policy(_) => SubjectKind::ResolvedPolicyReference,
                ReferenceField::TargetProfile(_) => SubjectKind::ResolvedTargetProfileReference,
            },
            Self::DeliveryPlacement => SubjectKind::DeliveryPlacement,
            Self::Target(subject) => match subject {
                TargetSubject::Target(_) => SubjectKind::Target,
            },
            Self::Group(subject) => match subject {
                GroupSubject::Groups => SubjectKind::Groups,
                GroupSubject::Memberships => SubjectKind::Memberships,
                GroupSubject::Group(_) => SubjectKind::Group,
                GroupSubject::Members(_) => SubjectKind::GroupMembers,
                GroupSubject::Member(..) => SubjectKind::GroupMember,
                GroupSubject::Assignment(_) => SubjectKind::Assignment,
            },
            Self::Hydration(subject) => match subject {
                HydrationSubject::Relations => SubjectKind::Relations,
                HydrationSubject::Endpoint(..) => SubjectKind::RelationEndpoint,
                HydrationSubject::Relation(..) => SubjectKind::Relation,
                HydrationSubject::Roles(..) => SubjectKind::Roles,
            },
            Self::SemanticDependency(group) => match group {
                SemanticGroup::ProfileSpecification => SubjectKind::ProfileSpecificationDependency,
                SemanticGroup::ProjectIdentity => SubjectKind::ProjectIdentityDependency,
                SemanticGroup::LocaleCanonicalization => {
                    SubjectKind::LocaleCanonicalizationDependency
                }
                SemanticGroup::ProjectLocales => SubjectKind::ProjectLocalesDependency,
                SemanticGroup::LocaleNegotiation => SubjectKind::LocaleNegotiationDependency,
                SemanticGroup::MessageFallback => SubjectKind::MessageFallbackDependency,
                SemanticGroup::Coverage => SubjectKind::CoverageDependency,
                SemanticGroup::RequiredPolicies => SubjectKind::RequiredPoliciesDependency,
                SemanticGroup::OptionalPolicies => SubjectKind::OptionalPoliciesDependency,
                SemanticGroup::Targets => SubjectKind::TargetsDependency,
                SemanticGroup::DeploymentGroups => SubjectKind::DeploymentGroupsDependency,
                SemanticGroup::Delivery => SubjectKind::DeliveryDependency,
            },
        }
    }
}

impl Ord for Subject {
    /// By kind, then by key. One kind is one inner variant, so two subjects of
    /// one kind compare by that variant's key, and the singletons are equal.
    fn cmp(&self, other: &Self) -> Ordering {
        self.kind()
            .cmp(&other.kind())
            .then_with(|| match (self, other) {
                (Self::Slot(own), Self::Slot(theirs)) => own.cmp(theirs),
                (Self::ResourcePolicy(own), Self::ResourcePolicy(theirs)) => own.cmp(theirs),
                (Self::DeclarationField(own), Self::DeclarationField(theirs)) => own.cmp(theirs),
                (Self::Locale(own), Self::Locale(theirs)) => own.cmp(theirs),
                (Self::Negotiation(own), Self::Negotiation(theirs)) => own.cmp(theirs),
                (Self::Fallback(own), Self::Fallback(theirs)) => own.cmp(theirs),
                (Self::Coverage(own), Self::Coverage(theirs)) => own.cmp(theirs),
                (Self::ArtifactCollection(own), Self::ArtifactCollection(theirs)) => {
                    own.cmp(theirs)
                }
                (Self::ArtifactReference(own), Self::ArtifactReference(theirs)) => own.cmp(theirs),
                (Self::Target(own), Self::Target(theirs)) => own.cmp(theirs),
                (Self::Group(own), Self::Group(theirs)) => own.cmp(theirs),
                (Self::Hydration(own), Self::Hydration(theirs)) => own.cmp(theirs),
                (Self::Vocabulary, Self::Vocabulary)
                | (Self::CanonicalizationData, Self::CanonicalizationData)
                | (Self::DeliveryPlacement, Self::DeliveryPlacement)
                | (Self::SemanticDependency(_), Self::SemanticDependency(_)) => Ordering::Equal,
                _ => unreachable!("subjects of one kind share one variant"),
            })
    }
}

impl PartialOrd for Subject {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use intlify_shared_json::token::Token;
    use serde_json::json;

    use super::*;
    use crate::artifacts::envelope::{ArtifactKind, FormalMember};
    use crate::model::{CoverageRule, DeploymentGroupId, ProfileId};
    use crate::resolver::artifact_resolution::ConflictKey;
    use crate::resolver::coverage::{RuleContent, SurfaceKey};
    use crate::resolver::hydration::EndpointField;
    use crate::resolver::identity::DeclarationField;
    use crate::resolver::locales::fixtures::{canonical, content, key, target};
    use crate::resolver::resource_policy::PolicySlot;

    /// One subject of every kind but the normalized coverage domain, which
    /// only coverage can build, and the kinds declared only, which have no
    /// subject yet, in the order of [`SubjectKind::ALL`].
    fn samples() -> Vec<Subject> {
        let group: DeploymentGroupId = serde_json::from_value(json!("web")).unwrap();
        let rule: CoverageRule =
            serde_json::from_value(json!({"requestedLocales": ["ja"], "mode": "fallback-allowed"}))
                .unwrap();
        let rule = RuleContent::of(&rule);
        let surface = Token::new("checkout").unwrap();
        let member = FormalMember::new(json!({"schemaRevision": "0"}))
            .unwrap()
            .key();
        let profile: ProfileId = serde_json::from_value(json!("storefront")).unwrap();
        let policy = |subject| Subject::ResourcePolicy(subject);
        let capability = |subject| Subject::ResourcePolicy(PolicySubject::Capability(subject));
        let recheck = |subject| Subject::ResourcePolicy(PolicySubject::Recheck(subject));
        let policy_reference =
            || ArtifactSubject::Reference(ReferenceField::Policy(PolicySlot::Trust));
        let target_reference =
            || ArtifactSubject::Reference(ReferenceField::TargetProfile(target("browser")));
        let locale = |subject| Subject::Locale(subject);
        let coverage = |subject| Subject::Coverage(subject);
        let groups = |subject| Subject::Group(subject);
        let hydration = |subject| Subject::Hydration(subject);
        let owner = |group| Subject::SemanticDependency(group);
        vec![
            Subject::Slot(Slot::ProjectProfile),
            owner(SemanticGroup::ProfileSpecification),
            capability(ArtifactSubject::Artifacts),
            capability(ArtifactSubject::PolicyArtifacts),
            capability(ArtifactSubject::TargetProfileArtifacts),
            capability(ArtifactSubject::References),
            capability(ArtifactSubject::Artifact(member.clone())),
            capability(ArtifactSubject::ArtifactBytes),
            capability(policy_reference()),
            capability(target_reference()),
            capability(ArtifactSubject::ReferenceBytes),
            policy(PolicySubject::ResourceLimits),
            policy(PolicySubject::Nodes),
            policy(PolicySubject::Depth),
            policy(PolicySubject::CollectionEntries),
            policy(PolicySubject::StringBytes),
            policy(PolicySubject::LongestString),
            policy(PolicySubject::Profiles),
            policy(PolicySubject::ProfileId(profile)),
            policy(PolicySubject::Selector),
            policy(PolicySubject::StructuralAnalysis),
            recheck(ArtifactSubject::Artifacts),
            recheck(ArtifactSubject::PolicyArtifacts),
            recheck(ArtifactSubject::TargetProfileArtifacts),
            recheck(ArtifactSubject::References),
            recheck(ArtifactSubject::Artifact(member.clone())),
            recheck(ArtifactSubject::ArtifactBytes),
            recheck(policy_reference()),
            recheck(target_reference()),
            recheck(ArtifactSubject::ReferenceBytes),
            Subject::Slot(Slot::LocaleCanonicalization),
            Subject::Slot(Slot::LocaleNegotiation),
            Subject::Slot(Slot::MessageLocaleFallback),
            Subject::Slot(Slot::Coverage),
            Subject::Slot(Slot::IntentSurfaceClassVocabulary),
            Subject::Slot(Slot::DeliveryGraph),
            Subject::Slot(Slot::DeliveryPlacement),
            Subject::Vocabulary,
            Subject::CanonicalizationData,
            owner(SemanticGroup::LocaleCanonicalization),
            Subject::DeclarationField(DeclarationField::ProjectId),
            owner(SemanticGroup::ProjectIdentity),
            locale(LocaleSubject::Occurrences),
            locale(LocaleSubject::RequestedLocale(key("en"))),
            locale(LocaleSubject::RequestedLocales),
            locale(LocaleSubject::DefaultSourceLocale),
            locale(LocaleSubject::DefaultRequestedLocale),
            locale(LocaleSubject::NegotiationAliasKey(key("fr"))),
            locale(LocaleSubject::NegotiationAliasValue(
                key("fr"),
                key("fr-FR"),
            )),
            locale(LocaleSubject::MessageFallbackKey(key("ja"))),
            locale(LocaleSubject::TargetProfiles),
            locale(LocaleSubject::TargetId(target("browser"))),
            locale(LocaleSubject::TargetRequestedLocale(
                target("browser"),
                key("en"),
            )),
            locale(LocaleSubject::TargetDefaultRequestedLocale(target(
                "browser",
            ))),
            owner(SemanticGroup::ProjectLocales),
            Subject::Negotiation(NegotiationSubject::Aliases),
            Subject::Negotiation(NegotiationSubject::Key(key("fr"))),
            Subject::Negotiation(NegotiationSubject::Destination(key("fr"), key("fr-FR"))),
            Subject::Fallback(FallbackSubject::Sources),
            Subject::Fallback(FallbackSubject::Source(key("ja"))),
            Subject::Fallback(FallbackSubject::Sequence(key("ja"))),
            Subject::Fallback(FallbackSubject::Candidate(key("ja"), 0)),
            Subject::Fallback(FallbackSubject::Literal(key("ja"), key("en"))),
            Subject::Fallback(FallbackSubject::IntentSource(key("ja"))),
            coverage(CoverageSubject::Rules),
            coverage(CoverageSubject::LocaleSelectors),
            coverage(CoverageSubject::SurfaceSelectors),
            coverage(CoverageSubject::Locale(rule.clone(), content("JA!"))),
            coverage(CoverageSubject::SurfaceClass(
                rule.clone(),
                SurfaceKey::Member(surface.clone()),
            )),
            coverage(CoverageSubject::Rule(rule)),
            coverage(CoverageSubject::Cells),
            coverage(CoverageSubject::Comparisons),
            coverage(CoverageSubject::Cell(canonical("ja"), surface)),
            coverage(CoverageSubject::Table),
            owner(SemanticGroup::LocaleNegotiation),
            owner(SemanticGroup::MessageFallback),
            owner(SemanticGroup::Coverage),
            Subject::ArtifactCollection(CollectionSubject::Member(member)),
            Subject::ArtifactCollection(CollectionSubject::Group(ConflictKey {
                kind: ArtifactKind::TrustPolicy,
                identity: Token::new("storefront-trust").unwrap(),
                revision: Token::new("1").unwrap(),
                specification_revision: Token::new("0").unwrap(),
            })),
            Subject::ArtifactReference(ReferenceField::Policy(PolicySlot::Trust)),
            Subject::ArtifactReference(ReferenceField::TargetProfile(target("browser"))),
            owner(SemanticGroup::RequiredPolicies),
            owner(SemanticGroup::OptionalPolicies),
            Subject::DeliveryPlacement,
            owner(SemanticGroup::Delivery),
            Subject::Target(TargetSubject::Target(target("browser"))),
            owner(SemanticGroup::Targets),
            groups(GroupSubject::Groups),
            groups(GroupSubject::Memberships),
            groups(GroupSubject::Group(group.clone())),
            groups(GroupSubject::Members(group.clone())),
            groups(GroupSubject::Member(group.clone(), target("browser"))),
            groups(GroupSubject::Assignment(target("browser"))),
            hydration(HydrationSubject::Relations),
            hydration(HydrationSubject::Endpoint(
                group.clone(),
                target("server"),
                target("browser"),
                EndpointField::Client,
            )),
            hydration(HydrationSubject::Relation(
                group.clone(),
                target("server"),
                target("browser"),
            )),
            hydration(HydrationSubject::Roles(group, target("browser"))),
            owner(SemanticGroup::DeploymentGroups),
        ]
    }

    #[test]
    fn each_kind_is_the_kind_of_one_variant_and_orders_its_subjects() {
        let samples = samples();
        let kinds: Vec<SubjectKind> = samples.iter().map(Subject::kind).collect();
        let listed: Vec<SubjectKind> = SubjectKind::ALL
            .into_iter()
            .filter(|&kind| kind != SubjectKind::CoverageDomain && !kind.declared_only())
            .collect();
        assert_eq!(kinds, listed);
        // Every pair of distinct kinds orders as the kinds do, whatever the
        // keys, and the comparison agrees with equality.
        for (index, own) in samples.iter().enumerate() {
            for (other, theirs) in samples.iter().enumerate() {
                assert_eq!(own.cmp(theirs), index.cmp(&other), "{own:?} {theirs:?}");
                assert_eq!(own.cmp(theirs) == Ordering::Equal, own == theirs);
            }
        }
    }

    #[test]
    fn kinds_are_grouped_by_phase_in_phase_order() {
        assert!(SubjectKind::ALL.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(SubjectKind::ALL
            .windows(2)
            .all(|pair| pair[0].phase() <= pair[1].phase()));
        let phases: Vec<Phase> = SubjectKind::ALL.iter().map(|kind| kind.phase()).fold(
            Vec::new(),
            |mut phases, phase| {
                if phases.last() != Some(&phase) {
                    phases.push(phase);
                }
                phases
            },
        );
        assert_eq!(
            phases,
            [
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
                Phase::ReportingFinalization,
            ]
        );
    }

    #[test]
    fn a_kinds_rank_is_its_place_among_its_phases_kinds() {
        for phase in Phase::ALL {
            let ranks: Vec<usize> = SubjectKind::ALL
                .iter()
                .filter(|kind| kind.phase() == phase)
                .map(|kind| kind.rank())
                .collect();
            assert_eq!(ranks, (0..ranks.len()).collect::<Vec<_>>(), "{phase:?}");
        }
        assert_eq!(SubjectKind::ProjectProfileSlot.rank(), 0);
        assert_eq!(SubjectKind::ResourceLimits.rank(), 9);
        assert_eq!(SubjectKind::Roles.rank(), 3);
        // The owners of the entry and materialized dependencies rank first,
        // and Configuration Schema Selection before every structural kind.
        assert_eq!(SubjectKind::EntryDependencies.rank(), 0);
        assert_eq!(SubjectKind::MaterializedDependencies.rank(), 0);
        assert_eq!(SubjectKind::ConfigurationSchemaSelection.rank(), 1);
    }

    #[test]
    fn the_kinds_declared_only_are_those_of_the_phases_without_units() {
        for kind in SubjectKind::ALL {
            let unrecorded = matches!(
                kind.phase(),
                Phase::EntryAdmission
                    | Phase::StructuralAdmission
                    | Phase::ProfileSelection
                    | Phase::ReportingFinalization
            ) && kind != SubjectKind::ProfileSpecificationDependency;
            assert_eq!(kind.declared_only(), unrecorded, "{kind:?}");
        }
    }

    /// The declared kinds in rank order, each with the checks 015 gives it.
    #[test]
    fn the_declared_kinds_run_the_checks_015_gives_them() {
        let declared: Vec<(SubjectKind, &[Check])> = SubjectKind::ALL
            .into_iter()
            .filter(|kind| kind.declared_only())
            .map(|kind| (kind, kind.checks()))
            .collect();
        // Each scope level runs the four keyword classes (D15:2167).
        let fields: &[Check] = &[
            Check::RequiredField,
            Check::FieldType,
            Check::FieldValue,
            Check::KnownField,
        ];
        let key: &[Check] = &[Check::FieldValue];
        let capability: &[Check] = &[Check::WithinCapability];
        assert_eq!(
            declared,
            [
                // The owners rank first in their phases (D15:2510).
                (
                    SubjectKind::EntryDependencies,
                    &[Check::EstablishedSlots][..]
                ),
                // The bootstrap bounds before the checks they protect, then
                // the five root-value bounds in table order (D15:1870,
                // 1993, 2000, 2350).
                (
                    SubjectKind::CompleteInput,
                    &[
                        Check::WithinFileBytes,
                        Check::WithinParserTokens,
                        Check::WellFormedJson,
                        Check::UniqueMembers,
                        Check::JsonCompatible,
                        Check::WithinNodes,
                        Check::WithinDepth,
                        Check::WithinCollectionEntries,
                        Check::WithinTotalStringBytes,
                        Check::WithinSingleStringBytes,
                    ][..],
                ),
                (
                    SubjectKind::MaterializedDependencies,
                    &[Check::EstablishedSlots][..]
                ),
                // Before every structural kind (D15:2124, 2167).
                (
                    SubjectKind::ConfigurationSchemaSelection,
                    &[
                        Check::ObjectRoot,
                        Check::PresentSchemaVersion,
                        Check::StringSchemaVersion,
                        Check::SupportedSchemaVersion,
                    ][..],
                ),
                (SubjectKind::CapabilityStructuralAnalysis, capability),
                // The scope levels and the identity maps' keys, parents first
                // (D15:2167, 3340).
                (SubjectKind::RootField, fields),
                (SubjectKind::ProfileKey, key),
                (SubjectKind::ProfileField, fields),
                (SubjectKind::TargetProfileKey, key),
                (SubjectKind::TargetField, fields),
                (SubjectKind::DeploymentGroupKey, key),
                (SubjectKind::GroupField, fields),
                // The declarations and each declared ID, then the selector,
                // checked apart from the IDs (D15:1879, 1880, 1984).
                (SubjectKind::CapabilityProfiles, capability),
                (SubjectKind::CapabilityProfileId, capability),
                (
                    SubjectKind::ProfileSelector,
                    &[
                        Check::WithinCapability,
                        Check::RequiredSelector,
                        Check::ValidSelector,
                        Check::DeclaredSelector,
                    ][..],
                ),
                // One singleton whose Finding's reason names the domain
                // (D15:2353, 2402).
                (
                    SubjectKind::DiagnosticDomain,
                    &[Check::CompleteReporting][..]
                ),
            ]
        );
    }

    #[test]
    fn every_kind_runs_its_checks_in_check_order_once() {
        for kind in SubjectKind::ALL {
            let checks = kind.checks();
            assert!(!checks.is_empty(), "{kind:?}");
            assert!(checks.windows(2).all(|pair| pair[0] < pair[1]), "{kind:?}");
        }
    }

    #[test]
    fn subjects_of_one_kind_order_by_key_and_kinds_by_rank() {
        let field = |field| Subject::DeclarationField(field);
        assert!(field(DeclarationField::ProjectId) < field(DeclarationField::SelectionScope));
        assert_eq!(
            field(DeclarationField::ProjectId).cmp(&field(DeclarationField::ProjectId)),
            Ordering::Equal
        );
        // The kinds of the two reference fields order before their keys: every
        // policy reference precedes every Target Profile reference, whatever
        // the slot and the Target ID.
        let reference = |field| Subject::ArtifactReference(field);
        let last_slot = reference(ReferenceField::Policy(PolicySlot::Trust));
        let first_target = reference(ReferenceField::TargetProfile(target("a")));
        assert_eq!(last_slot.kind(), SubjectKind::ResolvedPolicyReference);
        assert_eq!(
            first_target.kind(),
            SubjectKind::ResolvedTargetProfileReference
        );
        assert!(last_slot < first_target);
        // Singletons of one kind are equal, and kinds of one phase keep their
        // rank.
        assert_eq!(
            Subject::Vocabulary.cmp(&Subject::Vocabulary),
            Ordering::Equal
        );
        assert!(Subject::Slot(Slot::Coverage) < Subject::Vocabulary);
        assert!(Subject::Vocabulary < Subject::CanonicalizationData);
        // Each bound's check under the capability and its Policy recheck are
        // of different kinds, on either side of the selected policy's.
        let policy = |subject| Subject::ResourcePolicy(subject);
        let bound = || ArtifactSubject::Reference(ReferenceField::TargetProfile(target("z")));
        let capability = policy(PolicySubject::Capability(bound()));
        let recheck = policy(PolicySubject::Recheck(bound()));
        assert_eq!(
            capability.kind(),
            SubjectKind::CapabilityTargetProfileReference
        );
        assert_eq!(recheck.kind(), SubjectKind::TargetProfileReference);
        assert!(capability < policy(PolicySubject::ResourceLimits));
        assert!(policy(PolicySubject::ResourceLimits) < policy(PolicySubject::Nodes));
        assert!(policy(PolicySubject::StructuralAnalysis) < recheck);
        assert_eq!(
            SubjectKind::CapabilityTargetProfileReference.checks(),
            [Check::WithinCapability]
        );
        assert_eq!(
            SubjectKind::TargetProfileReference.checks(),
            [Check::WithinBound]
        );
    }
}
