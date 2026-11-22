// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Versioned workload facts for input/structural/selection, single-locale
//! canonicalization, 015 Phases 7 and 8 locale and locale-policy resolution,
//! 015 Phase 4 resource policy admission, and 015 Phase 9 collection admission
//! and exact resolution, 015 Phases 11 to 13 target, group, and hydration
//! resolution, 015's resolver construction and its identity, its Evidence
//! stages, its complete invocation, and a Snapshot's admission. These are not
//! physical metrics or formal Resource Policy observations.

use serde::{Deserialize, Serialize};

use crate::input_limits::ValueCounts;
use crate::locale::{CanonicalizationFailure, Canonicalized, ProviderFailure, Spelling};
use crate::materialize::{InputCounts, MaterializationError};
use crate::structural::selection::SelectorByteObservation;

use super::operation::{Analysis, Operation, Output, Prepared};
use super::quantity::Quantity;

mod binding;
mod collection;
mod construction;
mod evidence;
mod hydration;
mod policy;
mod resolution;
mod snapshot;
mod target_group;
mod workflow;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(super) enum WorkKind {
    RawFileBytes,
    ParserTokensVisited,
    LogicalValueNodes,
    MaximumValueDepth,
    CollectionEntries,
    TotalDecodedStringBytes,
    MaximumDecodedStringBytes,
    ProfileDeclarations,
    MaximumDeclaredProfileIdBytes,
    StructuralAnalysisUnits,
    RetainedAdmissionIssues,
    RetainedSchemaFragments,
    RetainedSchemaIssuesIncludingAlternatives,
    SelectorIdBytes,
    LocaleOccurrences,
    RawLocaleIdentifierBytes,
    CanonicalLocaleIdentifierBytes,
    RetainedCanonicalLocaleValues,
    LocaleCorrectionSuggestions,
    LocalePolicyResolutionOccurrences,
    LocalePolicyResolutionRawIdentifierBytes,
    LocalePolicyResolutionCanonicalizedOccurrences,
    LocalePolicyResolutionCanonicalRequestedLocales,
    LocalePolicyResolutionEvaluationUnits,
    LocalePolicyResolutionNotEvaluatedUnits,
    LocalePolicyResolutionBlockingIssues,
    LocalePolicyResolutionCorrections,
    LocalePolicyResolutionNegotiationAliases,
    LocalePolicyResolutionFallbackSources,
    LocalePolicyResolutionMaximumFallbackCandidates,
    LocalePolicyResolutionCoverageRules,
    LocalePolicyResolutionCoverageLocaleSelectorItems,
    LocalePolicyResolutionCoverageSurfaceSelectorItems,
    LocalePolicyResolutionCoverageDecisionCells,
    LocalePolicyResolutionCoverageRuleCellComparisons,
    ResourcePolicyArtifactOccurrences,
    ResourcePolicyPolicyArtifacts,
    ResourcePolicyTargetProfileArtifacts,
    ResourcePolicyDistinctArtifactBodies,
    ResourcePolicyArtifactCanonicalBytes,
    ResourcePolicyReferences,
    ResourcePolicyReferenceCanonicalBytes,
    ResourcePolicyEvaluationUnits,
    ResourcePolicyNotEvaluatedUnits,
    ResourcePolicyBlockingIssues,
    ArtifactCollectionArtifactOccurrences,
    ArtifactCollectionPolicyArtifacts,
    ArtifactCollectionTargetProfileArtifacts,
    ArtifactCollectionDistinctArtifactBodies,
    ArtifactCollectionArtifactCanonicalBytes,
    ArtifactCollectionEvaluationUnits,
    ArtifactCollectionBlockingIssues,
    ArtifactCollectionSetArtifacts,
    ResolvedBindingReferences,
    ResolvedBindingDistinctReferences,
    ResolvedBindingReferenceCanonicalBytes,
    ResolvedBindingSetArtifacts,
    ResolvedBindingEvaluationUnits,
    ResolvedBindingNotEvaluatedUnits,
    ResolvedBindingBlockingIssues,
    ResolvedBindingBindings,
    TargetAndGroupTargets,
    TargetAndGroupMaximumTargetIdBytes,
    TargetAndGroupGroups,
    TargetAndGroupMaximumGroupIdBytes,
    TargetAndGroupMaximumMembersPerGroup,
    TargetAndGroupMembershipOccurrences,
    TargetAndGroupEvaluationUnits,
    TargetAndGroupNotEvaluatedUnits,
    TargetAndGroupBlockingIssues,
    TargetAndGroupResolvedTargets,
    TargetAndGroupResolvedGroups,
    HydrationRelationOccurrences,
    HydrationDistinctRelations,
    HydrationMaximumEndpointIdBytes,
    HydrationRoleTargets,
    HydrationEvaluationUnits,
    HydrationNotEvaluatedUnits,
    HydrationBlockingIssues,
    HydrationCheckedRelations,
    HydrationCheckedGroups,
    ConstructionTotalArtifactBytes,
    ConstructionMaximumArtifactBytes,
    ConstructionMaximumDepth,
    ConstructionNodes,
    ConstructionCollectionEntries,
    ConstructionTotalStringBytes,
    ConstructionMaximumStringBytes,
    ConstructionConfigurationSchemas,
    ConstructionMaximumSchemaArtifactBytes,
    ConstructionTotalSchemaArtifactBytes,
    ConstructionSchemaNodes,
    ConstructionSchemaReferences,
    ConstructionSchemaValidationWorkUnits,
    ConstructionRegistryRows,
    ConstructionMaximumRegistryMemberBytes,
    ConstructionTotalRegistryArtifactBytes,
    ConstructionRegistryValidationWorkUnits,
    ConstructionCrossAuthorityChecks,
    ConstructionIdentityPreimageBytes,
    SharedEvidenceFindingCandidates,
    SharedEvidenceFindings,
    SharedEvidenceEvaluationStatuses,
    SharedEvidenceMaximumCausesPerStatus,
    SharedEvidenceCauseOccurrences,
    SharedEvidenceRecords,
    SharedEvidenceCanonicalBytes,
    SharedEvidenceSubjectOccurrences,
    SharedEvidenceTokenInputs,
    SharedEvidenceCollisionComparisons,
    SharedEvidenceMetadataWorkUnits,
    EntryEvidenceFindingCandidates,
    EntryEvidenceFindings,
    EntryEvidenceMaximumRelatedEvidencePerOccurrenceGroup,
    EntryEvidenceRelatedEvidenceOccurrences,
    EntryEvidenceMaximumSingleEvidenceBytes,
    EntryEvidenceTotalEvidenceBytes,
    EntryEvidenceEvaluationStatuses,
    EntryEvidenceMaximumCausesPerStatus,
    EntryEvidenceCauseOccurrences,
    EntryEvidenceRecords,
    EntryEvidenceCanonicalBytes,
    EntryEvidenceMaximumLogicalPathSegments,
    EntryEvidenceMaximumLogicalPathCanonicalBytes,
    EntryEvidenceSubjectOccurrences,
    EntryEvidenceTokenInputs,
    EntryEvidenceCollisionComparisons,
    EntryEvidenceMetadataWorkUnits,
    OutcomeFinalizationEntryRecords,
    OutcomeFinalizationSharedRecords,
    OutcomeFinalizationReserveCanonicalBytes,
    WorkflowSubmittedMembers,
    WorkflowSubmittedCanonicalBytes,
    WorkflowSharedRecords,
    WorkflowSharedCanonicalBytes,
    WorkflowEntryRecords,
    WorkflowEntryCanonicalBytes,
    WorkflowReserveCanonicalBytes,
    SnapshotNodes,
    SnapshotMaximumDepth,
    SnapshotCollectionEntries,
    SnapshotTotalStringBytes,
    SnapshotMaximumStringBytes,
    SnapshotRejectedMarkers,
    SnapshotEnvelopeCanonicalBytes,
    SnapshotMaximumMarkerCanonicalBytes,
    SnapshotTraversalUnits,
    ReplayRetainedMembers,
    ReplayRetainedCanonicalBytes,
    ReplayBindings,
}

impl WorkKind {
    const INPUT: [Self; 14] = [
        Self::RawFileBytes,
        Self::ParserTokensVisited,
        Self::LogicalValueNodes,
        Self::MaximumValueDepth,
        Self::CollectionEntries,
        Self::TotalDecodedStringBytes,
        Self::MaximumDecodedStringBytes,
        Self::ProfileDeclarations,
        Self::MaximumDeclaredProfileIdBytes,
        Self::StructuralAnalysisUnits,
        Self::RetainedAdmissionIssues,
        Self::RetainedSchemaFragments,
        Self::RetainedSchemaIssuesIncludingAlternatives,
        Self::SelectorIdBytes,
    ];

    const LOCALE: [Self; 5] = [
        Self::LocaleOccurrences,
        Self::RawLocaleIdentifierBytes,
        Self::CanonicalLocaleIdentifierBytes,
        Self::RetainedCanonicalLocaleValues,
        Self::LocaleCorrectionSuggestions,
    ];

    const LOCALE_POLICY_RESOLUTION: [Self; 16] = [
        Self::LocalePolicyResolutionOccurrences,
        Self::LocalePolicyResolutionRawIdentifierBytes,
        Self::LocalePolicyResolutionCanonicalizedOccurrences,
        Self::LocalePolicyResolutionCanonicalRequestedLocales,
        Self::LocalePolicyResolutionEvaluationUnits,
        Self::LocalePolicyResolutionNotEvaluatedUnits,
        Self::LocalePolicyResolutionBlockingIssues,
        Self::LocalePolicyResolutionCorrections,
        Self::LocalePolicyResolutionNegotiationAliases,
        Self::LocalePolicyResolutionFallbackSources,
        Self::LocalePolicyResolutionMaximumFallbackCandidates,
        Self::LocalePolicyResolutionCoverageRules,
        Self::LocalePolicyResolutionCoverageLocaleSelectorItems,
        Self::LocalePolicyResolutionCoverageSurfaceSelectorItems,
        Self::LocalePolicyResolutionCoverageDecisionCells,
        Self::LocalePolicyResolutionCoverageRuleCellComparisons,
    ];

    const RESOURCE_POLICY: [Self; 10] = [
        Self::ResourcePolicyArtifactOccurrences,
        Self::ResourcePolicyPolicyArtifacts,
        Self::ResourcePolicyTargetProfileArtifacts,
        Self::ResourcePolicyDistinctArtifactBodies,
        Self::ResourcePolicyArtifactCanonicalBytes,
        Self::ResourcePolicyReferences,
        Self::ResourcePolicyReferenceCanonicalBytes,
        Self::ResourcePolicyEvaluationUnits,
        Self::ResourcePolicyNotEvaluatedUnits,
        Self::ResourcePolicyBlockingIssues,
    ];

    const ARTIFACT_COLLECTION: [Self; 8] = [
        Self::ArtifactCollectionArtifactOccurrences,
        Self::ArtifactCollectionPolicyArtifacts,
        Self::ArtifactCollectionTargetProfileArtifacts,
        Self::ArtifactCollectionDistinctArtifactBodies,
        Self::ArtifactCollectionArtifactCanonicalBytes,
        Self::ArtifactCollectionEvaluationUnits,
        Self::ArtifactCollectionBlockingIssues,
        Self::ArtifactCollectionSetArtifacts,
    ];

    const RESOLVED_BINDING: [Self; 8] = [
        Self::ResolvedBindingReferences,
        Self::ResolvedBindingDistinctReferences,
        Self::ResolvedBindingReferenceCanonicalBytes,
        Self::ResolvedBindingSetArtifacts,
        Self::ResolvedBindingEvaluationUnits,
        Self::ResolvedBindingNotEvaluatedUnits,
        Self::ResolvedBindingBlockingIssues,
        Self::ResolvedBindingBindings,
    ];

    const TARGET_AND_GROUP: [Self; 11] = [
        Self::TargetAndGroupTargets,
        Self::TargetAndGroupMaximumTargetIdBytes,
        Self::TargetAndGroupGroups,
        Self::TargetAndGroupMaximumGroupIdBytes,
        Self::TargetAndGroupMaximumMembersPerGroup,
        Self::TargetAndGroupMembershipOccurrences,
        Self::TargetAndGroupEvaluationUnits,
        Self::TargetAndGroupNotEvaluatedUnits,
        Self::TargetAndGroupBlockingIssues,
        Self::TargetAndGroupResolvedTargets,
        Self::TargetAndGroupResolvedGroups,
    ];

    const HYDRATION: [Self; 9] = [
        Self::HydrationRelationOccurrences,
        Self::HydrationDistinctRelations,
        Self::HydrationMaximumEndpointIdBytes,
        Self::HydrationRoleTargets,
        Self::HydrationEvaluationUnits,
        Self::HydrationNotEvaluatedUnits,
        Self::HydrationBlockingIssues,
        Self::HydrationCheckedRelations,
        Self::HydrationCheckedGroups,
    ];

    /// The construction-root bounds' totals, in the bounds' canonical order.
    const CONSTRUCTION: [Self; 18] = [
        Self::ConstructionTotalArtifactBytes,
        Self::ConstructionMaximumArtifactBytes,
        Self::ConstructionMaximumDepth,
        Self::ConstructionNodes,
        Self::ConstructionCollectionEntries,
        Self::ConstructionTotalStringBytes,
        Self::ConstructionMaximumStringBytes,
        Self::ConstructionConfigurationSchemas,
        Self::ConstructionMaximumSchemaArtifactBytes,
        Self::ConstructionTotalSchemaArtifactBytes,
        Self::ConstructionSchemaNodes,
        Self::ConstructionSchemaReferences,
        Self::ConstructionSchemaValidationWorkUnits,
        Self::ConstructionRegistryRows,
        Self::ConstructionMaximumRegistryMemberBytes,
        Self::ConstructionTotalRegistryArtifactBytes,
        Self::ConstructionRegistryValidationWorkUnits,
        Self::ConstructionCrossAuthorityChecks,
    ];

    const CONSTRUCTION_IDENTITY: [Self; 1] = [Self::ConstructionIdentityPreimageBytes];

    /// The Shared domain's seven `diagnostics.shared` counts over what it
    /// committed, and the Shared scope's four `evidenceMetadata` counts.
    const SHARED_EVIDENCE: [Self; 11] = [
        Self::SharedEvidenceFindingCandidates,
        Self::SharedEvidenceFindings,
        Self::SharedEvidenceEvaluationStatuses,
        Self::SharedEvidenceMaximumCausesPerStatus,
        Self::SharedEvidenceCauseOccurrences,
        Self::SharedEvidenceRecords,
        Self::SharedEvidenceCanonicalBytes,
        Self::SharedEvidenceSubjectOccurrences,
        Self::SharedEvidenceTokenInputs,
        Self::SharedEvidenceCollisionComparisons,
        Self::SharedEvidenceMetadataWorkUnits,
    ];

    const ENTRY_EVIDENCE: [Self; 17] = [
        Self::EntryEvidenceFindingCandidates,
        Self::EntryEvidenceFindings,
        Self::EntryEvidenceMaximumRelatedEvidencePerOccurrenceGroup,
        Self::EntryEvidenceRelatedEvidenceOccurrences,
        Self::EntryEvidenceMaximumSingleEvidenceBytes,
        Self::EntryEvidenceTotalEvidenceBytes,
        Self::EntryEvidenceEvaluationStatuses,
        Self::EntryEvidenceMaximumCausesPerStatus,
        Self::EntryEvidenceCauseOccurrences,
        Self::EntryEvidenceRecords,
        Self::EntryEvidenceCanonicalBytes,
        Self::EntryEvidenceMaximumLogicalPathSegments,
        Self::EntryEvidenceMaximumLogicalPathCanonicalBytes,
        Self::EntryEvidenceSubjectOccurrences,
        Self::EntryEvidenceTokenInputs,
        Self::EntryEvidenceCollisionComparisons,
        Self::EntryEvidenceMetadataWorkUnits,
    ];

    const OUTCOME_FINALIZATION: [Self; 3] = [
        Self::OutcomeFinalizationEntryRecords,
        Self::OutcomeFinalizationSharedRecords,
        Self::OutcomeFinalizationReserveCanonicalBytes,
    ];

    /// What strict file entry counts of the file and its value, the
    /// submitted collection's members and their bytes, and what outcome
    /// finalization counts, with each projection's bytes.
    const WORKFLOW: [Self; 14] = [
        Self::RawFileBytes,
        Self::ParserTokensVisited,
        Self::LogicalValueNodes,
        Self::MaximumValueDepth,
        Self::CollectionEntries,
        Self::TotalDecodedStringBytes,
        Self::MaximumDecodedStringBytes,
        Self::WorkflowSubmittedMembers,
        Self::WorkflowSubmittedCanonicalBytes,
        Self::WorkflowSharedRecords,
        Self::WorkflowSharedCanonicalBytes,
        Self::WorkflowEntryRecords,
        Self::WorkflowEntryCanonicalBytes,
        Self::WorkflowReserveCanonicalBytes,
    ];

    /// A programmatic entry's complete invocation counts its Snapshot's
    /// envelope bytes and traversal units in place of the file's bytes and
    /// parser tokens, and the rest as a file entry's does.
    const PROGRAMMATIC_WORKFLOW: [Self; 14] = [
        Self::SnapshotEnvelopeCanonicalBytes,
        Self::SnapshotTraversalUnits,
        Self::LogicalValueNodes,
        Self::MaximumValueDepth,
        Self::CollectionEntries,
        Self::TotalDecodedStringBytes,
        Self::MaximumDecodedStringBytes,
        Self::WorkflowSubmittedMembers,
        Self::WorkflowSubmittedCanonicalBytes,
        Self::WorkflowSharedRecords,
        Self::WorkflowSharedCanonicalBytes,
        Self::WorkflowEntryRecords,
        Self::WorkflowEntryCanonicalBytes,
        Self::WorkflowReserveCanonicalBytes,
    ];

    /// A replay counts its set's value and members, which were admitted when
    /// the set was created, and its result's Shared records with their bytes
    /// and the bindings beside its profile.
    const REPLAY: [Self; 10] = [
        Self::LogicalValueNodes,
        Self::MaximumValueDepth,
        Self::CollectionEntries,
        Self::TotalDecodedStringBytes,
        Self::MaximumDecodedStringBytes,
        Self::ReplayRetainedMembers,
        Self::ReplayRetainedCanonicalBytes,
        Self::WorkflowSharedRecords,
        Self::WorkflowSharedCanonicalBytes,
        Self::ReplayBindings,
    ];

    /// What the Snapshot boundary counts under each of the nine
    /// `bootstrapOnly.snapshot` bounds, in the bounds' order, then the
    /// root-value counts of the document.
    const SNAPSHOT: [Self; 14] = [
        Self::SnapshotNodes,
        Self::SnapshotMaximumDepth,
        Self::SnapshotCollectionEntries,
        Self::SnapshotTotalStringBytes,
        Self::SnapshotMaximumStringBytes,
        Self::SnapshotRejectedMarkers,
        Self::SnapshotEnvelopeCanonicalBytes,
        Self::SnapshotMaximumMarkerCanonicalBytes,
        Self::SnapshotTraversalUnits,
        Self::LogicalValueNodes,
        Self::MaximumValueDepth,
        Self::CollectionEntries,
        Self::TotalDecodedStringBytes,
        Self::MaximumDecodedStringBytes,
    ];

    pub(super) fn for_operation(operation: Operation) -> &'static [Self] {
        match operation {
            Operation::LocaleCanonicalization => &Self::LOCALE,
            Operation::LocalePolicyResolution => &Self::LOCALE_POLICY_RESOLUTION,
            Operation::ResourcePolicyBootstrap => &Self::RESOURCE_POLICY,
            Operation::ArtifactCollectionAdmission => &Self::ARTIFACT_COLLECTION,
            Operation::ResolvedBindingConstruction => &Self::RESOLVED_BINDING,
            Operation::TargetAndGroupResolution => &Self::TARGET_AND_GROUP,
            Operation::HydrationRelationValidation => &Self::HYDRATION,
            Operation::ConstructionAuthorityAdmission => &Self::CONSTRUCTION,
            Operation::ConstructionIdentity => &Self::CONSTRUCTION_IDENTITY,
            Operation::SharedEvidence => &Self::SHARED_EVIDENCE,
            Operation::EntryEvidence => &Self::ENTRY_EVIDENCE,
            Operation::OutcomeFinalization => &Self::OUTCOME_FINALIZATION,
            Operation::FileEntryCompleteWorkflow => &Self::WORKFLOW,
            Operation::ProgrammaticEntryCompleteWorkflow => &Self::PROGRAMMATIC_WORKFLOW,
            Operation::ProgrammaticSnapshotAdmission => &Self::SNAPSHOT,
            Operation::MaterializedReresolution => &Self::REPLAY,
            _ => &Self::INPUT,
        }
    }

    const fn unit(self) -> WorkUnit {
        match self {
            Self::RawFileBytes
            | Self::ConstructionTotalArtifactBytes
            | Self::ConstructionMaximumArtifactBytes
            | Self::ConstructionMaximumSchemaArtifactBytes
            | Self::ConstructionTotalSchemaArtifactBytes
            | Self::ConstructionMaximumRegistryMemberBytes
            | Self::ConstructionTotalRegistryArtifactBytes
            | Self::ConstructionIdentityPreimageBytes => WorkUnit::Octet,
            Self::SharedEvidenceFindingCandidates | Self::EntryEvidenceFindingCandidates => {
                WorkUnit::FindingCandidate
            }
            Self::SharedEvidenceFindings | Self::EntryEvidenceFindings => WorkUnit::FindingKey,
            // A status's distinct causes, after cause-set deduplication, and
            // every cause occurrence, before it (D15:1953, 1954, 1965, 1966).
            Self::SharedEvidenceMaximumCausesPerStatus
            | Self::EntryEvidenceMaximumCausesPerStatus => WorkUnit::CausalFindingKey,
            Self::SharedEvidenceCauseOccurrences | Self::EntryEvidenceCauseOccurrences => {
                WorkUnit::CausalFindingKeyOccurrence
            }
            Self::EntryEvidenceMaximumRelatedEvidencePerOccurrenceGroup => {
                WorkUnit::RelatedEvidence
            }
            Self::EntryEvidenceRelatedEvidenceOccurrences => WorkUnit::RelatedEvidenceOccurrence,
            Self::EntryEvidenceMaximumLogicalPathSegments => WorkUnit::PathSegment,
            Self::SharedEvidenceSubjectOccurrences | Self::EntryEvidenceSubjectOccurrences => {
                WorkUnit::SubjectOccurrence
            }
            Self::SharedEvidenceTokenInputs | Self::EntryEvidenceTokenInputs => {
                WorkUnit::TokenInput
            }
            Self::SharedEvidenceCollisionComparisons | Self::EntryEvidenceCollisionComparisons => {
                WorkUnit::CollisionComparison
            }
            Self::SharedEvidenceMetadataWorkUnits | Self::EntryEvidenceMetadataWorkUnits => {
                WorkUnit::EvidenceMetadataWork
            }
            Self::ConstructionConfigurationSchemas => WorkUnit::ConfigurationSchema,
            Self::ConstructionSchemaNodes => WorkUnit::SchemaNode,
            Self::ConstructionSchemaReferences => WorkUnit::SchemaReference,
            Self::ConstructionSchemaValidationWorkUnits
            | Self::ConstructionRegistryValidationWorkUnits => WorkUnit::ValidationPair,
            Self::ConstructionRegistryRows => WorkUnit::RegistryRow,
            Self::ConstructionCrossAuthorityChecks => WorkUnit::CrossAuthorityCheck,
            Self::TotalDecodedStringBytes
            | Self::MaximumDecodedStringBytes
            | Self::SnapshotTotalStringBytes
            | Self::SnapshotMaximumStringBytes
            | Self::MaximumDeclaredProfileIdBytes
            | Self::SelectorIdBytes
            | Self::RawLocaleIdentifierBytes
            | Self::LocalePolicyResolutionRawIdentifierBytes
            | Self::CanonicalLocaleIdentifierBytes
            | Self::TargetAndGroupMaximumTargetIdBytes
            | Self::TargetAndGroupMaximumGroupIdBytes
            | Self::HydrationMaximumEndpointIdBytes
            | Self::ConstructionTotalStringBytes
            | Self::ConstructionMaximumStringBytes => WorkUnit::Utf8Octet,
            Self::ParserTokensVisited => WorkUnit::ParserToken,
            Self::LogicalValueNodes | Self::ConstructionNodes => WorkUnit::LogicalValue,
            Self::MaximumValueDepth
            | Self::ConstructionMaximumDepth
            | Self::SnapshotMaximumDepth => WorkUnit::ValueLevel,
            Self::CollectionEntries
            | Self::ConstructionCollectionEntries
            | Self::SnapshotCollectionEntries => WorkUnit::CollectionEntry,
            Self::SnapshotNodes => WorkUnit::SnapshotNode,
            Self::SnapshotRejectedMarkers => WorkUnit::RejectedMarker,
            Self::SnapshotTraversalUnits => WorkUnit::TraversalUnit,
            Self::ProfileDeclarations => WorkUnit::ProfileDeclaration,
            Self::StructuralAnalysisUnits => WorkUnit::ApplicableKeywordSubject,
            Self::RetainedAdmissionIssues
            | Self::RetainedSchemaFragments
            | Self::RetainedSchemaIssuesIncludingAlternatives
            | Self::LocalePolicyResolutionBlockingIssues
            | Self::ResourcePolicyBlockingIssues
            | Self::ArtifactCollectionBlockingIssues
            | Self::ResolvedBindingBlockingIssues
            | Self::TargetAndGroupBlockingIssues
            | Self::HydrationBlockingIssues
            | Self::SharedEvidenceRecords
            | Self::EntryEvidenceRecords
            | Self::OutcomeFinalizationEntryRecords
            | Self::OutcomeFinalizationSharedRecords
            | Self::WorkflowSharedRecords
            | Self::WorkflowEntryRecords => WorkUnit::RetainedRecord,
            Self::LocaleOccurrences
            | Self::LocalePolicyResolutionOccurrences
            | Self::LocalePolicyResolutionCanonicalizedOccurrences
            | Self::LocalePolicyResolutionCoverageLocaleSelectorItems => WorkUnit::LocaleOccurrence,
            Self::LocalePolicyResolutionNegotiationAliases => WorkUnit::NegotiationAlias,
            Self::LocalePolicyResolutionFallbackSources => WorkUnit::FallbackSource,
            Self::LocalePolicyResolutionMaximumFallbackCandidates => WorkUnit::FallbackCandidate,
            Self::LocalePolicyResolutionCoverageRules => WorkUnit::CoverageRule,
            Self::LocalePolicyResolutionCoverageSurfaceSelectorItems => {
                WorkUnit::SurfaceClassOccurrence
            }
            Self::LocalePolicyResolutionCoverageDecisionCells => WorkUnit::DecisionCell,
            Self::LocalePolicyResolutionCoverageRuleCellComparisons => WorkUnit::RuleCellComparison,
            Self::RetainedCanonicalLocaleValues
            | Self::LocalePolicyResolutionCanonicalRequestedLocales => {
                WorkUnit::CanonicalLocaleValue
            }
            Self::LocalePolicyResolutionEvaluationUnits
            | Self::LocalePolicyResolutionNotEvaluatedUnits
            | Self::ResourcePolicyEvaluationUnits
            | Self::ResourcePolicyNotEvaluatedUnits
            | Self::ArtifactCollectionEvaluationUnits
            | Self::ResolvedBindingEvaluationUnits
            | Self::ResolvedBindingNotEvaluatedUnits
            | Self::TargetAndGroupEvaluationUnits
            | Self::TargetAndGroupNotEvaluatedUnits
            | Self::HydrationEvaluationUnits
            | Self::HydrationNotEvaluatedUnits
            | Self::SharedEvidenceEvaluationStatuses
            | Self::EntryEvidenceEvaluationStatuses => WorkUnit::EvaluationUnit,
            Self::ResourcePolicyArtifactOccurrences
            | Self::ResourcePolicyPolicyArtifacts
            | Self::ResourcePolicyTargetProfileArtifacts
            | Self::ArtifactCollectionArtifactOccurrences
            | Self::ArtifactCollectionPolicyArtifacts
            | Self::ArtifactCollectionTargetProfileArtifacts
            | Self::WorkflowSubmittedMembers
            | Self::ReplayRetainedMembers => WorkUnit::ArtifactOccurrence,
            Self::ResourcePolicyDistinctArtifactBodies
            | Self::ArtifactCollectionDistinctArtifactBodies
            | Self::ArtifactCollectionSetArtifacts
            | Self::ResolvedBindingSetArtifacts => WorkUnit::CanonicalArtifactBody,
            Self::ResourcePolicyArtifactCanonicalBytes
            | Self::ResourcePolicyReferenceCanonicalBytes
            | Self::ArtifactCollectionArtifactCanonicalBytes
            | Self::ResolvedBindingReferenceCanonicalBytes
            | Self::SharedEvidenceCanonicalBytes
            | Self::EntryEvidenceMaximumSingleEvidenceBytes
            | Self::EntryEvidenceTotalEvidenceBytes
            | Self::EntryEvidenceCanonicalBytes
            | Self::EntryEvidenceMaximumLogicalPathCanonicalBytes
            | Self::OutcomeFinalizationReserveCanonicalBytes
            | Self::WorkflowSubmittedCanonicalBytes
            | Self::ReplayRetainedCanonicalBytes
            | Self::WorkflowSharedCanonicalBytes
            | Self::WorkflowEntryCanonicalBytes
            | Self::WorkflowReserveCanonicalBytes
            | Self::SnapshotEnvelopeCanonicalBytes
            | Self::SnapshotMaximumMarkerCanonicalBytes => WorkUnit::CanonicalOctet,
            Self::ResourcePolicyReferences
            | Self::ResolvedBindingReferences
            | Self::ResolvedBindingDistinctReferences
            | Self::ResolvedBindingBindings
            | Self::ReplayBindings => WorkUnit::ArtifactReference,
            Self::LocaleCorrectionSuggestions | Self::LocalePolicyResolutionCorrections => {
                WorkUnit::CorrectionSuggestion
            }
            Self::TargetAndGroupTargets
            | Self::TargetAndGroupResolvedTargets
            | Self::HydrationRoleTargets => WorkUnit::TargetDeclaration,
            Self::TargetAndGroupGroups
            | Self::TargetAndGroupResolvedGroups
            | Self::HydrationCheckedGroups => WorkUnit::DeploymentGroup,
            Self::HydrationRelationOccurrences => WorkUnit::RelationOccurrence,
            Self::HydrationDistinctRelations | Self::HydrationCheckedRelations => {
                WorkUnit::HydrationRelation
            }
            Self::TargetAndGroupMaximumMembersPerGroup
            | Self::TargetAndGroupMembershipOccurrences => WorkUnit::MembershipOccurrence,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
enum WorkUnit {
    Octet,
    Utf8Octet,
    ParserToken,
    LogicalValue,
    ValueLevel,
    CollectionEntry,
    ProfileDeclaration,
    ApplicableKeywordSubject,
    RetainedRecord,
    LocaleOccurrence,
    CanonicalLocaleValue,
    EvaluationUnit,
    CorrectionSuggestion,
    ArtifactOccurrence,
    CanonicalArtifactBody,
    CanonicalOctet,
    ArtifactReference,
    NegotiationAlias,
    FallbackSource,
    FallbackCandidate,
    CoverageRule,
    SurfaceClassOccurrence,
    DecisionCell,
    RuleCellComparison,
    TargetDeclaration,
    DeploymentGroup,
    MembershipOccurrence,
    RelationOccurrence,
    HydrationRelation,
    ConfigurationSchema,
    SchemaNode,
    SchemaReference,
    ValidationPair,
    RegistryRow,
    CrossAuthorityCheck,
    FindingCandidate,
    FindingKey,
    CausalFindingKey,
    CausalFindingKeyOccurrence,
    SubjectOccurrence,
    TokenInput,
    CollisionComparison,
    EvidenceMetadataWork,
    RelatedEvidence,
    RelatedEvidenceOccurrence,
    PathSegment,
    SnapshotNode,
    RejectedMarker,
    TraversalUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
enum WorkStage {
    OperationResult,
    PreparedInput,
    NotApplicable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
enum UnavailableWork {
    RawInputNotComplete,
    ProfileContainerNotAdmitted,
    SchemaPrerequisiteUnavailable,
    LocaleNotCanonicalized,
    LocalePolicyResolutionRequestedNotResolved,
    ArtifactCollectionNotAdmitted,
    ArtifactReferencesNotResolved,
    EvidencePathsNotAdmitted,
    SnapshotHoldsMarkers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "state", rename_all = "kebab-case", deny_unknown_fields)]
enum WorkValue {
    Exact { value: Quantity },
    AtLeast { value: Quantity },
    Unavailable { reason: UnavailableWork },
    NotApplicable {},
}

impl WorkValue {
    const fn exact(value: u64) -> Self {
        Self::Exact {
            value: Quantity::new(value),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkFact {
    kind: WorkKind,
    unit: WorkUnit,
    stage: WorkStage,
    observation: WorkValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct LogicalWork {
    profile_identity: String,
    profile_revision: String,
    operation: Operation,
    facts: Vec<WorkFact>,
}

impl LogicalWork {
    /// The exact value of `kind`, when the work holds one.
    pub(super) fn exact(&self, kind: WorkKind) -> Option<u64> {
        self.facts
            .iter()
            .find(|fact| fact.kind == kind)
            .and_then(|fact| match fact.observation {
                WorkValue::Exact { value } => Some(value.get()),
                _ => None,
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(super) enum WorkFailure {
    OperationMismatch,
    InvalidOrdinaryResult,
    UnrepresentableCounter,
}

impl LogicalWork {
    pub(super) fn observe(prepared: &Prepared, output: &Output) -> Result<Self, WorkFailure> {
        let mut work = Self {
            profile_identity: match prepared.operation() {
                Operation::LocaleCanonicalization => "intlify-config-minimum-single-locale-work",
                Operation::LocalePolicyResolution => "intlify-config-locale-policy-resolution-work",
                Operation::ResourcePolicyBootstrap => {
                    "intlify-config-resource-policy-bootstrap-work"
                }
                Operation::ArtifactCollectionAdmission => {
                    "intlify-config-artifact-collection-admission-work"
                }
                Operation::ResolvedBindingConstruction => {
                    "intlify-config-resolved-binding-construction-work"
                }
                Operation::TargetAndGroupResolution => {
                    "intlify-config-target-and-group-resolution-work"
                }
                Operation::HydrationRelationValidation => {
                    "intlify-config-hydration-relation-validation-work"
                }
                Operation::ConstructionAuthorityAdmission => {
                    "intlify-config-resolver-construction-work"
                }
                Operation::ConstructionIdentity => {
                    "intlify-config-resolver-construction-identity-work"
                }
                Operation::SharedEvidence => "intlify-config-shared-evidence-work",
                Operation::EntryEvidence => "intlify-config-entry-evidence-work",
                Operation::OutcomeFinalization => "intlify-config-outcome-finalization-work",
                Operation::FileEntryCompleteWorkflow => {
                    "intlify-config-file-entry-complete-workflow-work"
                }
                Operation::ProgrammaticEntryCompleteWorkflow => {
                    "intlify-config-programmatic-entry-complete-workflow-work"
                }
                Operation::ProgrammaticSnapshotAdmission => {
                    "intlify-config-programmatic-snapshot-admission-work"
                }
                Operation::MaterializedReresolution => {
                    "intlify-config-materialized-reresolution-work"
                }
                _ => "intlify-config-minimum-input-work",
            }
            .into(),
            profile_revision: "0".into(),
            operation: prepared.operation(),
            facts: WorkKind::for_operation(prepared.operation())
                .iter()
                .map(|&kind| WorkFact {
                    kind,
                    unit: kind.unit(),
                    stage: WorkStage::NotApplicable,
                    observation: WorkValue::NotApplicable {},
                })
                .collect(),
        };
        match (prepared, output) {
            (Prepared::Entry { .. }, Output::Entry(Ok(doc))) => {
                let counts = doc.counts().expect("the benchmark materializes files");
                work.input(counts, WorkStage::OperationResult);
            }
            (Prepared::Entry { .. }, Output::Entry(Err(error))) => work.failed_input(error),
            (Prepared::Structural { .. }, Output::Structural(Ok(analysis))) => {
                work.input(analysis.benchmark_input_counts(), WorkStage::PreparedInput);
                work.structural(analysis, WorkStage::OperationResult);
            }
            (Prepared::Authoring(analysis), Output::Authoring(Ok(Some(_)))) => {
                if !analysis.is_complete() {
                    return Err(WorkFailure::InvalidOrdinaryResult);
                }
                work.input(analysis.benchmark_input_counts(), WorkStage::PreparedInput);
                work.structural(analysis, WorkStage::PreparedInput);
            }
            (Prepared::Select { analysis, selector }, Output::Select(Ok(_))) => {
                work.input(analysis.benchmark_input_counts(), WorkStage::PreparedInput);
                work.structural(analysis, WorkStage::PreparedInput);
                let observation = match selector.benchmark_id_bytes() {
                    SelectorByteObservation::NotApplicable => WorkValue::NotApplicable {},
                    SelectorByteObservation::Exact(value) => WorkValue::exact(value),
                    SelectorByteObservation::AtLeast(value) => WorkValue::AtLeast {
                        value: Quantity::new(value),
                    },
                };
                let stage = if observation == (WorkValue::NotApplicable {}) {
                    WorkStage::NotApplicable
                } else {
                    WorkStage::PreparedInput
                };
                work.set(WorkKind::SelectorIdBytes, stage, observation);
            }
            (Prepared::Structural { .. }, Output::Structural(Err(_)))
            | (Prepared::Authoring(_), Output::Authoring(_))
            | (Prepared::Select { .. }, Output::Select(Err(_))) => {
                return Err(WorkFailure::InvalidOrdinaryResult);
            }
            (
                Prepared::Locale { input, .. } | Prepared::Icu4xLocale { input, .. },
                Output::Locale(result),
            ) => work.single_locale(input, result)?,
            (
                Prepared::LocalePolicyResolution(prepared),
                Output::LocalePolicyResolution(resolved),
            ) => {
                work.locales(prepared, resolved)?;
            }
            (Prepared::ResourcePolicy(prepared), Output::ResourcePolicy(resolved)) => {
                work.policy(prepared, resolved)?;
            }
            (Prepared::ArtifactCollection(prepared), Output::ArtifactCollection(resolved)) => {
                work.collection(prepared, resolved)?;
            }
            (Prepared::ResolvedBinding(prepared), Output::ResolvedBinding(resolved)) => {
                work.bindings(prepared, resolved)?;
            }
            (Prepared::TargetAndGroup(prepared), Output::TargetAndGroup(resolved)) => {
                work.targets(prepared, resolved)?;
            }
            (Prepared::Hydration(prepared), Output::Hydration(resolved)) => {
                work.hydration(prepared, resolved)?;
            }
            (Prepared::Construction(prepared), Output::Construction(_)) => {
                work.construction(prepared);
            }
            (Prepared::ConstructionIdentity(prepared), Output::ConstructionIdentity(_)) => {
                work.construction_identity(prepared)?;
            }
            (Prepared::SharedEvidence(_), Output::SharedEvidence(resolved)) => {
                work.shared_evidence(resolved)?;
            }
            (Prepared::EntryEvidence(_), Output::EntryEvidence(resolved)) => {
                work.entry_evidence(resolved)?;
            }
            (Prepared::OutcomeFinalization(_), Output::OutcomeFinalization(resolved)) => {
                work.outcome_finalization(resolved)?;
            }
            (Prepared::Workflow(_), Output::Workflow(resolved)) => work.workflow(resolved)?,
            (Prepared::Snapshot(_), Output::Snapshot(resolved)) => work.snapshot(resolved)?,
            (Prepared::Replay(prepared), Output::Replay(resolved)) => {
                work.replay(prepared, resolved)?;
            }
            _ => return Err(WorkFailure::OperationMismatch),
        }
        Ok(work)
    }

    /// One identifier, whichever provider canonicalized it.
    fn single_locale(
        &mut self,
        input: &str,
        result: &Result<Canonicalized, CanonicalizationFailure>,
    ) -> Result<(), WorkFailure> {
        self.set(
            WorkKind::LocaleOccurrences,
            WorkStage::PreparedInput,
            WorkValue::exact(1),
        );
        self.set(
            WorkKind::RawLocaleIdentifierBytes,
            WorkStage::PreparedInput,
            WorkValue::exact(
                u64::try_from(input.len()).map_err(|_| WorkFailure::UnrepresentableCounter)?,
            ),
        );
        let bytes = match result {
            Ok(value) => WorkValue::exact(
                u64::try_from(value.locale().as_str().len())
                    .map_err(|_| WorkFailure::UnrepresentableCounter)?,
            ),
            Err(CanonicalizationFailure::ByteLimit {
                spelling: Spelling::Canonical,
                actual,
                ..
            }) => WorkValue::exact(*actual),
            Err(
                CanonicalizationFailure::ByteLimit {
                    spelling: Spelling::Raw,
                    ..
                }
                | CanonicalizationFailure::Provider(ProviderFailure::InvalidIdentifier),
            ) => WorkValue::Unavailable {
                reason: UnavailableWork::LocaleNotCanonicalized,
            },
            _ => return Err(WorkFailure::InvalidOrdinaryResult),
        };
        self.set(
            WorkKind::CanonicalLocaleIdentifierBytes,
            WorkStage::OperationResult,
            bytes,
        );
        self.set(
            WorkKind::RetainedCanonicalLocaleValues,
            WorkStage::OperationResult,
            WorkValue::exact(u64::from(result.is_ok())),
        );
        self.set(
            WorkKind::LocaleCorrectionSuggestions,
            WorkStage::OperationResult,
            WorkValue::exact(u64::from(
                result
                    .as_ref()
                    .is_ok_and(|value| value.suggested_replacement().is_some()),
            )),
        );
        Ok(())
    }

    fn set(&mut self, kind: WorkKind, stage: WorkStage, observation: WorkValue) {
        let fact = self
            .facts
            .iter_mut()
            .find(|fact| fact.kind == kind)
            .expect("complete finite owner work vocabulary");
        fact.stage = stage;
        fact.observation = observation;
    }

    fn input(&mut self, counts: InputCounts, stage: WorkStage) {
        self.set(
            WorkKind::RawFileBytes,
            stage,
            WorkValue::exact(counts.file_bytes),
        );
        self.set(
            WorkKind::ParserTokensVisited,
            stage,
            WorkValue::exact(counts.parser_tokens),
        );
        self.values(Some(counts.value), stage);
    }

    fn values(&mut self, value: Option<ValueCounts>, stage: WorkStage) {
        self.values_or(value, stage, UnavailableWork::RawInputNotComplete);
    }

    /// The five value counts, each unavailable for `reason` without a value.
    fn values_or(&mut self, value: Option<ValueCounts>, stage: WorkStage, reason: UnavailableWork) {
        let kinds = [
            WorkKind::LogicalValueNodes,
            WorkKind::MaximumValueDepth,
            WorkKind::CollectionEntries,
            WorkKind::TotalDecodedStringBytes,
            WorkKind::MaximumDecodedStringBytes,
        ];
        let observations = value.map(|value| {
            [
                value.nodes,
                value.depth,
                value.collection_entries,
                value.total_string_bytes,
                value.single_string_bytes,
            ]
        });
        for (index, kind) in kinds.into_iter().enumerate() {
            self.set(
                kind,
                stage,
                observations.map_or(WorkValue::Unavailable { reason }, |values| {
                    WorkValue::exact(values[index])
                }),
            );
        }
    }

    fn failed_input(&mut self, error: &MaterializationError) {
        self.set(
            WorkKind::RawFileBytes,
            WorkStage::OperationResult,
            WorkValue::exact(error.progress.file_bytes),
        );
        self.set(
            WorkKind::ParserTokensVisited,
            WorkStage::OperationResult,
            WorkValue::exact(error.progress.parser_tokens_visited),
        );
        self.values(
            error.progress.complete_value.as_deref().copied(),
            WorkStage::OperationResult,
        );
    }

    fn structural(&mut self, analysis: &Analysis, stage: WorkStage) {
        let profiles = analysis.benchmark_profile_counts();
        for (kind, observation) in [
            (WorkKind::ProfileDeclarations, profiles.map(|value| value.0)),
            (
                WorkKind::MaximumDeclaredProfileIdBytes,
                profiles.map(|value| value.1),
            ),
        ] {
            self.set(
                kind,
                stage,
                observation.map_or(
                    WorkValue::Unavailable {
                        reason: UnavailableWork::ProfileContainerNotAdmitted,
                    },
                    WorkValue::exact,
                ),
            );
        }
        self.set(
            WorkKind::StructuralAnalysisUnits,
            stage,
            analysis.benchmark_structural_units().map_or(
                WorkValue::Unavailable {
                    reason: UnavailableWork::SchemaPrerequisiteUnavailable,
                },
                WorkValue::exact,
            ),
        );
        let (admission, fragments, issues) = analysis.benchmark_retained_record_counts();
        for (kind, value) in [
            (WorkKind::RetainedAdmissionIssues, admission),
            (WorkKind::RetainedSchemaFragments, fragments),
            (WorkKind::RetainedSchemaIssuesIncludingAlternatives, issues),
        ] {
            self.set(kind, stage, WorkValue::exact(value));
        }
    }

    /// Full equality includes vocabulary order, units, stages, unavailable state,
    /// and exact values. `expected` must come from independently checked fixture
    /// preparation, never from the decoded record being admitted.
    pub(super) fn matches_expected(&self, expected: &Self) -> bool {
        self == expected
    }
}

#[cfg(test)]
mod tests;
