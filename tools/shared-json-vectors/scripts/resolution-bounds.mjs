/**
 * Design 015's `projectProfileResolution` bounds, as the checkers transcribe
 * them.
 *
 * The bound vectors' checker lists the capability bounds from them, and the
 * authorities' checker the Profile Specification's `resourceBounds`. Neither
 * reads the design document.
 */

/**
 * Design 015's `projectProfileResolution` tables, in the canonical order:
 * each group's path and its bounds in table order. `policyComparable` mirrors
 * them, each with the minimum one.
 *
 * @type {[string, string[]][]}
 */
export const POLICY_GROUPS = [
  [
    'configurationInput',
    [
      'maxNodes',
      'maxDepth',
      'maxCollectionEntries',
      'maxTotalStringBytes',
      'maxSingleStringBytes',
      'maxProfiles',
      'maxProfileIdBytes',
      'maxStructuralAnalysisUnits'
    ]
  ],
  [
    'localeResolution',
    [
      'maxLocaleIdentifierBytes',
      'maxLocaleOccurrences',
      'maxRequestedLocales',
      'maxNegotiationAliases',
      'maxFallbackSources',
      'maxFallbackCandidatesPerSource',
      'maxCoverageRules',
      'maxCoverageLocaleSelectorOccurrences',
      'maxCoverageSurfaceSelectorOccurrences',
      'maxCoverageDecisionCells',
      'maxCoverageRuleCellComparisons'
    ]
  ],
  [
    'artifactAdmission',
    [
      'maxArtifacts',
      'maxPolicyArtifacts',
      'maxTargetProfileArtifacts',
      'maxArtifactReferences',
      'maxSingleArtifactCanonicalBytes',
      'maxTotalArtifactCanonicalBytes',
      'maxSingleReferenceCanonicalBytes',
      'maxTotalReferenceCanonicalBytes'
    ]
  ],
  [
    'targetGrouping',
    [
      'maxTargetProfiles',
      'maxTargetIdBytes',
      'maxDeploymentGroups',
      'maxGroupIdBytes',
      'maxMembersPerGroup',
      'maxMembershipOccurrences',
      'maxHydrationRelations'
    ]
  ],
  [
    'diagnostics.entry',
    [
      'maxFindingOccurrences',
      'maxFindings',
      'maxRelatedEvidencePerOccurrenceGroup',
      'maxRelatedEvidenceOccurrences',
      'maxSingleEvidenceBytes',
      'maxTotalEvidenceBytes',
      'maxEvaluationStatusEntries',
      'maxSuppressionCausesPerStatus',
      'maxSuppressionCauseOccurrences',
      'maxRecords',
      'maxTotalCanonicalBytes'
    ]
  ],
  [
    'diagnostics.shared',
    [
      'maxFindingOccurrences',
      'maxFindings',
      'maxEvaluationStatusEntries',
      'maxSuppressionCausesPerStatus',
      'maxSuppressionCauseOccurrences',
      'maxRecords',
      'maxTotalCanonicalBytes'
    ]
  ]
]
