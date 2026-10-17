// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's `projectProfileResolution` section: the bounds a Resource Limit Policy
//! sets on profile resolution, in five closed groups with no defaults. Not a
//! decoder (the section arrives with the Resource Limit Policy's content), not
//! the Admitted Implementation Capability (whose `policyComparable` group
//! mirrors these bounds), and not an accounting of any input: each phase applies
//! the bounds it owns.

use crate::input_limits::{Bound, ValueLimits};
use crate::structural::StructuralLimits;

/// Every bound is required; 015 defines none by default.
#[derive(Debug, Clone)]
pub(crate) struct ResolutionLimits {
    pub(crate) configuration_input: ConfigurationInputLimits,
    pub(crate) locale_resolution: LocaleResolutionLimits,
    pub(crate) artifact_admission: ArtifactAdmissionLimits,
    pub(crate) target_grouping: TargetGroupingLimits,
    pub(crate) diagnostics: DiagnosticsLimits,
}

/// The materialized root and structural analysis. Its bounds are the ones the
/// entry and structural stages already take, so the Policy recheck hands them
/// the same values.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ConfigurationInputLimits {
    pub(crate) value: ValueLimits,
    pub(crate) structural: StructuralLimits,
}

#[derive(Debug, Clone, Copy)]
#[expect(clippy::struct_field_names, reason = "mirror 015's explicit bound IDs")]
pub(crate) struct LocaleResolutionLimits {
    pub(crate) max_locale_identifier_bytes: Bound,
    pub(crate) max_locale_occurrences: Bound,
    pub(crate) max_requested_locales: Bound,
    pub(crate) max_negotiation_aliases: Bound,
    pub(crate) max_fallback_sources: Bound,
    pub(crate) max_fallback_candidates_per_source: Bound,
    pub(crate) max_coverage_rules: Bound,
    pub(crate) max_coverage_locale_selector_occurrences: Bound,
    pub(crate) max_coverage_surface_selector_occurrences: Bound,
    pub(crate) max_coverage_decision_cells: Bound,
    pub(crate) max_coverage_rule_cell_comparisons: Bound,
}

#[derive(Debug, Clone, Copy)]
#[expect(clippy::struct_field_names, reason = "mirror 015's explicit bound IDs")]
pub(crate) struct ArtifactAdmissionLimits {
    pub(crate) max_artifacts: Bound,
    pub(crate) max_policy_artifacts: Bound,
    pub(crate) max_target_profile_artifacts: Bound,
    pub(crate) max_artifact_references: Bound,
    pub(crate) max_single_artifact_canonical_bytes: Bound,
    pub(crate) max_total_artifact_canonical_bytes: Bound,
    pub(crate) max_single_reference_canonical_bytes: Bound,
    pub(crate) max_total_reference_canonical_bytes: Bound,
}

#[derive(Debug, Clone, Copy)]
#[expect(clippy::struct_field_names, reason = "mirror 015's explicit bound IDs")]
pub(crate) struct TargetGroupingLimits {
    pub(crate) max_target_profiles: Bound,
    pub(crate) max_target_id_bytes: Bound,
    pub(crate) max_deployment_groups: Bound,
    pub(crate) max_group_id_bytes: Bound,
    pub(crate) max_members_per_group: Bound,
    pub(crate) max_membership_occurrences: Bound,
    pub(crate) max_hydration_relations: Bound,
}

/// Two independent reporting domains. The same bound name in both is two
/// values and two counters; nothing bounds the domains together.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DiagnosticsLimits {
    pub(crate) entry: EntryDiagnosticsLimits,
    pub(crate) shared: SharedDiagnosticsLimits,
}

#[derive(Debug, Clone, Copy)]
#[expect(clippy::struct_field_names, reason = "mirror 015's explicit bound IDs")]
pub(crate) struct EntryDiagnosticsLimits {
    pub(crate) max_finding_occurrences: Bound,
    pub(crate) max_findings: Bound,
    pub(crate) max_related_evidence_per_occurrence_group: Bound,
    pub(crate) max_related_evidence_occurrences: Bound,
    pub(crate) max_single_evidence_bytes: Bound,
    pub(crate) max_total_evidence_bytes: Bound,
    pub(crate) max_evaluation_status_entries: Bound,
    pub(crate) max_suppression_causes_per_status: Bound,
    pub(crate) max_suppression_cause_occurrences: Bound,
    pub(crate) max_records: Bound,
    pub(crate) max_total_canonical_bytes: Bound,
}

#[derive(Debug, Clone, Copy)]
#[expect(clippy::struct_field_names, reason = "mirror 015's explicit bound IDs")]
pub(crate) struct SharedDiagnosticsLimits {
    pub(crate) max_finding_occurrences: Bound,
    pub(crate) max_findings: Bound,
    pub(crate) max_evaluation_status_entries: Bound,
    pub(crate) max_suppression_causes_per_status: Bound,
    pub(crate) max_suppression_cause_occurrences: Bound,
    pub(crate) max_records: Bound,
    pub(crate) max_total_canonical_bytes: Bound,
}

/// The section's groups, `diagnostics` split into its two domains, in 015's
/// canonical order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Group {
    ConfigurationInput,
    LocaleResolution,
    ArtifactAdmission,
    TargetGrouping,
    DiagnosticsEntry,
    DiagnosticsShared,
}

impl Group {
    /// The group's path inside `projectProfileResolution`.
    pub(crate) const fn path(self) -> &'static str {
        match self {
            Self::ConfigurationInput => "configurationInput",
            Self::LocaleResolution => "localeResolution",
            Self::ArtifactAdmission => "artifactAdmission",
            Self::TargetGrouping => "targetGrouping",
            Self::DiagnosticsEntry => "diagnostics.entry",
            Self::DiagnosticsShared => "diagnostics.shared",
        }
    }
}

/// Where one bound sits in the section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BoundId {
    pub(crate) group: Group,
    pub(crate) name: &'static str,
}

/// The number of bounds in the section.
pub(crate) const BOUND_COUNT: usize = 52;

impl ResolutionLimits {
    /// Every bound in 015's canonical order: the groups, `entry` before
    /// `shared`, then each table's declaration order. Policy/capability
    /// comparison and resource Findings follow this order.
    pub(crate) fn bounds(&self) -> [(BoundId, Bound); BOUND_COUNT] {
        use Group::{
            ArtifactAdmission, ConfigurationInput, DiagnosticsEntry, DiagnosticsShared,
            LocaleResolution, TargetGrouping,
        };
        let id = |group, name| BoundId { group, name };
        let value = &self.configuration_input.value;
        let structural = &self.configuration_input.structural;
        let locale = &self.locale_resolution;
        let artifact = &self.artifact_admission;
        let target = &self.target_grouping;
        let entry = &self.diagnostics.entry;
        let shared = &self.diagnostics.shared;
        [
            (id(ConfigurationInput, "maxNodes"), value.max_nodes),
            (id(ConfigurationInput, "maxDepth"), value.max_depth),
            (
                id(ConfigurationInput, "maxCollectionEntries"),
                value.max_collection_entries,
            ),
            (
                id(ConfigurationInput, "maxTotalStringBytes"),
                value.max_total_string_bytes,
            ),
            (
                id(ConfigurationInput, "maxSingleStringBytes"),
                value.max_single_string_bytes,
            ),
            (
                id(ConfigurationInput, "maxProfiles"),
                structural.max_profiles,
            ),
            (
                id(ConfigurationInput, "maxProfileIdBytes"),
                structural.max_profile_id_bytes,
            ),
            (
                id(ConfigurationInput, "maxStructuralAnalysisUnits"),
                structural.max_structural_analysis_units,
            ),
            (
                id(LocaleResolution, "maxLocaleIdentifierBytes"),
                locale.max_locale_identifier_bytes,
            ),
            (
                id(LocaleResolution, "maxLocaleOccurrences"),
                locale.max_locale_occurrences,
            ),
            (
                id(LocaleResolution, "maxRequestedLocales"),
                locale.max_requested_locales,
            ),
            (
                id(LocaleResolution, "maxNegotiationAliases"),
                locale.max_negotiation_aliases,
            ),
            (
                id(LocaleResolution, "maxFallbackSources"),
                locale.max_fallback_sources,
            ),
            (
                id(LocaleResolution, "maxFallbackCandidatesPerSource"),
                locale.max_fallback_candidates_per_source,
            ),
            (
                id(LocaleResolution, "maxCoverageRules"),
                locale.max_coverage_rules,
            ),
            (
                id(LocaleResolution, "maxCoverageLocaleSelectorOccurrences"),
                locale.max_coverage_locale_selector_occurrences,
            ),
            (
                id(LocaleResolution, "maxCoverageSurfaceSelectorOccurrences"),
                locale.max_coverage_surface_selector_occurrences,
            ),
            (
                id(LocaleResolution, "maxCoverageDecisionCells"),
                locale.max_coverage_decision_cells,
            ),
            (
                id(LocaleResolution, "maxCoverageRuleCellComparisons"),
                locale.max_coverage_rule_cell_comparisons,
            ),
            (
                id(ArtifactAdmission, "maxArtifacts"),
                artifact.max_artifacts,
            ),
            (
                id(ArtifactAdmission, "maxPolicyArtifacts"),
                artifact.max_policy_artifacts,
            ),
            (
                id(ArtifactAdmission, "maxTargetProfileArtifacts"),
                artifact.max_target_profile_artifacts,
            ),
            (
                id(ArtifactAdmission, "maxArtifactReferences"),
                artifact.max_artifact_references,
            ),
            (
                id(ArtifactAdmission, "maxSingleArtifactCanonicalBytes"),
                artifact.max_single_artifact_canonical_bytes,
            ),
            (
                id(ArtifactAdmission, "maxTotalArtifactCanonicalBytes"),
                artifact.max_total_artifact_canonical_bytes,
            ),
            (
                id(ArtifactAdmission, "maxSingleReferenceCanonicalBytes"),
                artifact.max_single_reference_canonical_bytes,
            ),
            (
                id(ArtifactAdmission, "maxTotalReferenceCanonicalBytes"),
                artifact.max_total_reference_canonical_bytes,
            ),
            (
                id(TargetGrouping, "maxTargetProfiles"),
                target.max_target_profiles,
            ),
            (
                id(TargetGrouping, "maxTargetIdBytes"),
                target.max_target_id_bytes,
            ),
            (
                id(TargetGrouping, "maxDeploymentGroups"),
                target.max_deployment_groups,
            ),
            (
                id(TargetGrouping, "maxGroupIdBytes"),
                target.max_group_id_bytes,
            ),
            (
                id(TargetGrouping, "maxMembersPerGroup"),
                target.max_members_per_group,
            ),
            (
                id(TargetGrouping, "maxMembershipOccurrences"),
                target.max_membership_occurrences,
            ),
            (
                id(TargetGrouping, "maxHydrationRelations"),
                target.max_hydration_relations,
            ),
            (
                id(DiagnosticsEntry, "maxFindingOccurrences"),
                entry.max_finding_occurrences,
            ),
            (id(DiagnosticsEntry, "maxFindings"), entry.max_findings),
            (
                id(DiagnosticsEntry, "maxRelatedEvidencePerOccurrenceGroup"),
                entry.max_related_evidence_per_occurrence_group,
            ),
            (
                id(DiagnosticsEntry, "maxRelatedEvidenceOccurrences"),
                entry.max_related_evidence_occurrences,
            ),
            (
                id(DiagnosticsEntry, "maxSingleEvidenceBytes"),
                entry.max_single_evidence_bytes,
            ),
            (
                id(DiagnosticsEntry, "maxTotalEvidenceBytes"),
                entry.max_total_evidence_bytes,
            ),
            (
                id(DiagnosticsEntry, "maxEvaluationStatusEntries"),
                entry.max_evaluation_status_entries,
            ),
            (
                id(DiagnosticsEntry, "maxSuppressionCausesPerStatus"),
                entry.max_suppression_causes_per_status,
            ),
            (
                id(DiagnosticsEntry, "maxSuppressionCauseOccurrences"),
                entry.max_suppression_cause_occurrences,
            ),
            (id(DiagnosticsEntry, "maxRecords"), entry.max_records),
            (
                id(DiagnosticsEntry, "maxTotalCanonicalBytes"),
                entry.max_total_canonical_bytes,
            ),
            (
                id(DiagnosticsShared, "maxFindingOccurrences"),
                shared.max_finding_occurrences,
            ),
            (id(DiagnosticsShared, "maxFindings"), shared.max_findings),
            (
                id(DiagnosticsShared, "maxEvaluationStatusEntries"),
                shared.max_evaluation_status_entries,
            ),
            (
                id(DiagnosticsShared, "maxSuppressionCausesPerStatus"),
                shared.max_suppression_causes_per_status,
            ),
            (
                id(DiagnosticsShared, "maxSuppressionCauseOccurrences"),
                shared.max_suppression_cause_occurrences,
            ),
            (id(DiagnosticsShared, "maxRecords"), shared.max_records),
            (
                id(DiagnosticsShared, "maxTotalCanonicalBytes"),
                shared.max_total_canonical_bytes,
            ),
        ]
    }
}

/// The Resource Limit Policy that 015 Phase 4 admitted and found compatible
/// with the implementation capability, which also confirms the provisional
/// profile selection. Post-policy phases run only under it. Until Phase 4
/// exists, only tests and the owners of the finite locale fixtures, the
/// measurements and the developer example, construct one.
#[derive(Debug, Clone)]
pub(crate) struct ConfirmedPolicy {
    limits: ResolutionLimits,
}

impl ConfirmedPolicy {
    /// A policy a fixture owner states outright, as Phase 4 would confirm it.
    #[cfg(any(test, feature = "benchmark", feature = "dev-example"))]
    pub(crate) const fn fixture_owned(limits: ResolutionLimits) -> Self {
        Self { limits }
    }

    pub(crate) const fn limits(&self) -> &ResolutionLimits {
        &self.limits
    }
}

/// Finite fixture bounds, each a distinct value counting up from `first` in
/// 015's canonical order. Never a product default.
#[cfg(any(test, feature = "benchmark", feature = "dev-example"))]
pub(crate) fn fixture_limits(first: u64) -> ResolutionLimits {
    let mut next = first;
    let mut bound = || {
        let value = Bound::new(next).expect("finite positive fixture bound");
        next += 1;
        value
    };
    ResolutionLimits {
        configuration_input: ConfigurationInputLimits {
            value: ValueLimits {
                max_nodes: bound(),
                max_depth: bound(),
                max_collection_entries: bound(),
                max_total_string_bytes: bound(),
                max_single_string_bytes: bound(),
            },
            structural: StructuralLimits {
                max_profiles: bound(),
                max_profile_id_bytes: bound(),
                max_structural_analysis_units: bound(),
            },
        },
        locale_resolution: LocaleResolutionLimits {
            max_locale_identifier_bytes: bound(),
            max_locale_occurrences: bound(),
            max_requested_locales: bound(),
            max_negotiation_aliases: bound(),
            max_fallback_sources: bound(),
            max_fallback_candidates_per_source: bound(),
            max_coverage_rules: bound(),
            max_coverage_locale_selector_occurrences: bound(),
            max_coverage_surface_selector_occurrences: bound(),
            max_coverage_decision_cells: bound(),
            max_coverage_rule_cell_comparisons: bound(),
        },
        artifact_admission: ArtifactAdmissionLimits {
            max_artifacts: bound(),
            max_policy_artifacts: bound(),
            max_target_profile_artifacts: bound(),
            max_artifact_references: bound(),
            max_single_artifact_canonical_bytes: bound(),
            max_total_artifact_canonical_bytes: bound(),
            max_single_reference_canonical_bytes: bound(),
            max_total_reference_canonical_bytes: bound(),
        },
        target_grouping: TargetGroupingLimits {
            max_target_profiles: bound(),
            max_target_id_bytes: bound(),
            max_deployment_groups: bound(),
            max_group_id_bytes: bound(),
            max_members_per_group: bound(),
            max_membership_occurrences: bound(),
            max_hydration_relations: bound(),
        },
        diagnostics: DiagnosticsLimits {
            entry: EntryDiagnosticsLimits {
                max_finding_occurrences: bound(),
                max_findings: bound(),
                max_related_evidence_per_occurrence_group: bound(),
                max_related_evidence_occurrences: bound(),
                max_single_evidence_bytes: bound(),
                max_total_evidence_bytes: bound(),
                max_evaluation_status_entries: bound(),
                max_suppression_causes_per_status: bound(),
                max_suppression_cause_occurrences: bound(),
                max_records: bound(),
                max_total_canonical_bytes: bound(),
            },
            shared: SharedDiagnosticsLimits {
                max_finding_occurrences: bound(),
                max_findings: bound(),
                max_evaluation_status_entries: bound(),
                max_suppression_causes_per_status: bound(),
                max_suppression_cause_occurrences: bound(),
                max_records: bound(),
                max_total_canonical_bytes: bound(),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_section_has_015s_groups_and_bounds_in_canonical_order() {
        let paths = fixture_limits(1)
            .bounds()
            .map(|(id, _)| format!("{}.{}", id.group.path(), id.name));
        assert_eq!(
            paths,
            [
                "configurationInput.maxNodes",
                "configurationInput.maxDepth",
                "configurationInput.maxCollectionEntries",
                "configurationInput.maxTotalStringBytes",
                "configurationInput.maxSingleStringBytes",
                "configurationInput.maxProfiles",
                "configurationInput.maxProfileIdBytes",
                "configurationInput.maxStructuralAnalysisUnits",
                "localeResolution.maxLocaleIdentifierBytes",
                "localeResolution.maxLocaleOccurrences",
                "localeResolution.maxRequestedLocales",
                "localeResolution.maxNegotiationAliases",
                "localeResolution.maxFallbackSources",
                "localeResolution.maxFallbackCandidatesPerSource",
                "localeResolution.maxCoverageRules",
                "localeResolution.maxCoverageLocaleSelectorOccurrences",
                "localeResolution.maxCoverageSurfaceSelectorOccurrences",
                "localeResolution.maxCoverageDecisionCells",
                "localeResolution.maxCoverageRuleCellComparisons",
                "artifactAdmission.maxArtifacts",
                "artifactAdmission.maxPolicyArtifacts",
                "artifactAdmission.maxTargetProfileArtifacts",
                "artifactAdmission.maxArtifactReferences",
                "artifactAdmission.maxSingleArtifactCanonicalBytes",
                "artifactAdmission.maxTotalArtifactCanonicalBytes",
                "artifactAdmission.maxSingleReferenceCanonicalBytes",
                "artifactAdmission.maxTotalReferenceCanonicalBytes",
                "targetGrouping.maxTargetProfiles",
                "targetGrouping.maxTargetIdBytes",
                "targetGrouping.maxDeploymentGroups",
                "targetGrouping.maxGroupIdBytes",
                "targetGrouping.maxMembersPerGroup",
                "targetGrouping.maxMembershipOccurrences",
                "targetGrouping.maxHydrationRelations",
                "diagnostics.entry.maxFindingOccurrences",
                "diagnostics.entry.maxFindings",
                "diagnostics.entry.maxRelatedEvidencePerOccurrenceGroup",
                "diagnostics.entry.maxRelatedEvidenceOccurrences",
                "diagnostics.entry.maxSingleEvidenceBytes",
                "diagnostics.entry.maxTotalEvidenceBytes",
                "diagnostics.entry.maxEvaluationStatusEntries",
                "diagnostics.entry.maxSuppressionCausesPerStatus",
                "diagnostics.entry.maxSuppressionCauseOccurrences",
                "diagnostics.entry.maxRecords",
                "diagnostics.entry.maxTotalCanonicalBytes",
                "diagnostics.shared.maxFindingOccurrences",
                "diagnostics.shared.maxFindings",
                "diagnostics.shared.maxEvaluationStatusEntries",
                "diagnostics.shared.maxSuppressionCausesPerStatus",
                "diagnostics.shared.maxSuppressionCauseOccurrences",
                "diagnostics.shared.maxRecords",
                "diagnostics.shared.maxTotalCanonicalBytes",
            ]
        );
    }

    #[test]
    fn every_bound_reads_its_own_field() {
        // The test bounds count up in field order, so a bound read from another
        // field, or a field read twice, breaks the sequence.
        let values = fixture_limits(7).bounds().map(|(_, bound)| bound.get());
        assert_eq!(values, std::array::from_fn(|index| index as u64 + 7));
    }

    #[test]
    fn a_confirmed_policy_keeps_the_limits_it_was_confirmed_with() {
        let policy = ConfirmedPolicy::fixture_owned(fixture_limits(3));
        assert_eq!(
            policy
                .limits()
                .locale_resolution
                .max_locale_identifier_bytes
                .get(),
            11
        );
        assert_eq!(
            policy.limits().bounds().map(|(_, bound)| bound.get()),
            fixture_limits(3).bounds().map(|(_, bound)| bound.get())
        );
    }
}
