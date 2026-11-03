/**
 * Design 015's capability bounds, as the checkers transcribe them.
 *
 * The bound vectors' checker lists the capability bounds from them, and the
 * authorities' checker the Profile Specification's `resourceBounds` and the
 * Capability Specification's body. Neither reads the design document.
 */

/**
 * Design 015's `projectProfileResolution` tables, in the canonical order:
 * each group's path and its bounds in table order, each with its counting
 * unit as the table writes it. `policyComparable` mirrors them, each with the
 * minimum one.
 *
 * @type {[string, [string, string][]][]}
 */
export const POLICY_GROUPS = [
  [
    'configurationInput',
    [
      ['maxNodes', 'One logical JSON value; the root counts as one and object keys are not nodes'],
      ['maxDepth', 'Maximum value level with the root at depth one'],
      ['maxCollectionEntries', 'One object-member or array-element occurrence'],
      ['maxTotalStringBytes', 'One UTF-8 byte in every string value and object key'],
      ['maxSingleStringBytes', 'UTF-8 byte length of one string value or object key'],
      ['maxProfiles', 'One profile declaration'],
      [
        'maxProfileIdBytes',
        'UTF-8 byte length of one complete raw ID, or the smallest first-over witness for an over-limit selector'
      ],
      [
        'maxStructuralAnalysisUnits',
        'One applicable `(schema keyword, logical subject)` evaluation'
      ]
    ]
  ],
  [
    'localeResolution',
    [
      ['maxLocaleIdentifierBytes', 'UTF-8 bytes in one spelling'],
      ['maxLocaleOccurrences', 'One raw locale spelling'],
      ['maxRequestedLocales', 'One unique canonical locale identity'],
      ['maxNegotiationAliases', 'One raw map entry'],
      ['maxFallbackSources', 'One raw mapping-key occurrence'],
      ['maxFallbackCandidatesPerSource', 'One candidate occurrence of either admitted kind'],
      ['maxCoverageRules', 'One rule occurrence'],
      ['maxCoverageLocaleSelectorOccurrences', 'One raw selector item'],
      ['maxCoverageSurfaceSelectorOccurrences', 'One raw selector item'],
      ['maxCoverageDecisionCells', 'One canonical Cartesian-product cell'],
      ['maxCoverageRuleCellComparisons', 'One logical rule-to-cell predicate evaluation']
    ]
  ],
  [
    'artifactAdmission',
    [
      ['maxArtifacts', 'One submitted artifact envelope occurrence'],
      ['maxPolicyArtifacts', 'One artifact occurrence declaring an admitted Policy kind'],
      ['maxTargetProfileArtifacts', 'One artifact occurrence declaring Target Profile kind'],
      ['maxArtifactReferences', 'One Policy or Target Profile reference occurrence'],
      ['maxSingleArtifactCanonicalBytes', '017-defined canonical bytes of one envelope and body'],
      [
        'maxTotalArtifactCanonicalBytes',
        'One canonical byte across submitted artifact occurrences'
      ],
      ['maxSingleReferenceCanonicalBytes', '017-defined canonical bytes of one reference'],
      ['maxTotalReferenceCanonicalBytes', 'One canonical byte across references']
    ]
  ],
  [
    'targetGrouping',
    [
      ['maxTargetProfiles', 'One Target ID entry'],
      ['maxTargetIdBytes', 'UTF-8 bytes in one raw ID'],
      ['maxDeploymentGroups', 'One Group entry'],
      ['maxGroupIdBytes', 'UTF-8 bytes in one raw ID'],
      ['maxMembersPerGroup', 'One Target ID occurrence'],
      ['maxMembershipOccurrences', 'One Target ID occurrence'],
      ['maxHydrationRelations', 'One relation occurrence']
    ]
  ],
  [
    'diagnostics.entry',
    [
      ['maxFindingOccurrences', 'One candidate occurrence'],
      ['maxFindings', 'One distinct Entry Finding Key'],
      ['maxRelatedEvidencePerOccurrenceGroup', 'One distinct related-Evidence record'],
      ['maxRelatedEvidenceOccurrences', 'One related-evidence occurrence'],
      ['maxSingleEvidenceBytes', "Length of `C` of one evidence record's canonical view"],
      ['maxTotalEvidenceBytes', 'One canonical Evidence byte'],
      ['maxEvaluationStatusEntries', 'One evaluated or not-evaluated Entry status'],
      ['maxSuppressionCausesPerStatus', 'One distinct causal Finding Key'],
      ['maxSuppressionCauseOccurrences', 'One causal Finding Key occurrence'],
      [
        'maxRecords',
        'One Entry Finding, status, Finding Occurrence, Coverage Decision Evidence record, source-evidence-index record, entry dependency, or shared-unavailability cause reference'
      ],
      ['maxTotalCanonicalBytes', "One byte of `C` of one record's accounting view"]
    ]
  ],
  [
    'diagnostics.shared',
    [
      ['maxFindingOccurrences', 'One candidate occurrence'],
      ['maxFindings', 'One distinct Shared Finding Key'],
      ['maxEvaluationStatusEntries', 'One evaluated or not-evaluated Shared status'],
      ['maxSuppressionCausesPerStatus', 'One distinct causal Finding Key'],
      ['maxSuppressionCauseOccurrences', 'One causal Finding Key occurrence'],
      [
        'maxRecords',
        'One Shared Finding, status, Coverage Decision Basis record, semantic dependency, or materialized re-resolution dependency'
      ],
      ['maxTotalCanonicalBytes', "One byte of `C` of one record's accounting view"]
    ]
  ]
]

/**
 * Design 015's `bootstrapOnly` table, in the canonical order: each group and
 * its bounds in table order, each with its counting unit as the table writes
 * it and its revision-`"0"` minimum.
 *
 * @type {[string, [string, string, string][]][]}
 */
export const BOOTSTRAP_GROUPS = [
  [
    'rawInput',
    [
      ['maxFileBytes', 'One submitted raw file byte', '1'],
      ['maxParserTokens', 'One strict-JSON lexical value, member-name, or punctuation token', '1']
    ]
  ],
  [
    'snapshot',
    [
      ['maxNodes', 'One canonical Snapshot node, including a rejected marker', '1'],
      ['maxDepth', 'One Snapshot tree level, with the root at one', '1'],
      ['maxCollectionEntries', 'One Snapshot object-member or array-element occurrence', '1'],
      ['maxTotalStringBytes', 'One UTF-8 byte in a Snapshot string or object key', '1'],
      ['maxSingleStringBytes', 'UTF-8 byte length of one Snapshot string or object key', '1'],
      ['maxRejectedMarkers', 'One canonical rejected-node marker', '1'],
      [
        'maxEnvelopeCanonicalBytes',
        'One canonical byte in the complete formal Snapshot envelope',
        '4096'
      ],
      ['maxSingleMarkerCanonicalBytes', 'One canonical byte in one rejected-node marker', '1024'],
      ['maxTraversalUnits', 'One visited Snapshot node or edge', '1']
    ]
  ],
  [
    'inputBoundary',
    [
      [
        'maxDepth',
        'One JSON nesting level of one raw boundary input or collection member, with its root at one',
        '1'
      ],
      [
        'maxSpecificationInputBytes',
        'One byte of the raw Profile Resolution Specification Input',
        '1'
      ],
      ['maxVocabularyBytes', 'One byte of the raw Intent Surface-Class Vocabulary Artifact', '1'],
      ['maxVocabularyMembers', 'One vocabulary member', '1'],
      ['maxVocabularyMemberBytes', 'UTF-8 byte length of one vocabulary member', '1'],
      ['maxVocabularyTotalMemberBytes', 'One UTF-8 byte across all vocabulary members', '1'],
      [
        'maxLocaleDataManifestBytes',
        'One byte of the raw Locale Canonicalization Data Artifact manifest',
        '1'
      ],
      ['maxCollectionMemberBytes', 'Raw byte length of one submitted collection member', '1'],
      ['maxCollectionTotalBytes', 'One raw byte across all submitted collection members', '1'],
      [
        'maxVerificationInputBytes',
        'One byte of the raw Resource Limit Policy Verification Input',
        '1'
      ],
      ['maxVerificationPins', 'One submitted pin', '1']
    ]
  ],
  [
    'artifactBootstrap',
    [
      [
        'maxEnvelopeDepth',
        'One nesting level of the selected candidate member, with the member root at one',
        '1'
      ],
      [
        'maxDecodedAllocationBytes',
        "One UTF-8 byte of a string value or object key decoded from the selected candidate's `content`",
        '1'
      ],
      [
        'maxMetadataScanUnits',
        'One envelope field read from one member to establish its exact-reference tuple: `kind`, `specification.identity`, `specification.revision`, `body.identity`, or `body.revision`',
        '1'
      ],
      [
        'maxValidationWorkUnits',
        "One evaluation of the selected candidate's integrity, its pin lookup, its common body, or one `projectProfileResolution` rule: the section's presence, one group's closed shape, or one bound's value",
        '1'
      ]
    ]
  ],
  [
    'localeData',
    [
      ['maxPayloads', 'One payload entry of the admitted manifest', '1'],
      ['maxPayloadBytes', 'Declared byte length of one payload', '1'],
      ['maxMarkerBytes', 'One byte of the four marker payloads as compact JSON text', '1'],
      ['maxValidityGroups', 'One conformance validity group', '1'],
      ['maxCodes', 'One conformance code', '1'],
      ['maxExtensionKeys', 'One conformance extension key', '1'],
      ['maxTypes', 'One conformance type', '1'],
      ['maxCorrections', 'One conformance correction', '1'],
      [
        'maxValidationWorkUnits',
        'One conformance group, code, key, type, or correction, each counted once',
        '1'
      ]
    ]
  ],
  [
    'evidenceMetadata',
    [
      ['maxSourceIdentityBytes', 'One byte in an exact source-identity input', '1'],
      [
        'maxSourceRevisionBytes',
        'One byte in an exact Source-content, Programmatic Entry, or Call-Site Source revision input',
        '1'
      ],
      ['maxLocatorBytes', 'One byte in a locator candidate', '1'],
      [
        'maxCallSiteSourceBytes',
        'One byte in the optional exact UTF-8 call-site source snapshot',
        '1'
      ],
      [
        'maxCallSiteMappings',
        'One submitted Snapshot Structural Path and location-role mapping',
        '1'
      ],
      ['maxLogicalPathSegments', 'One internal or projected path segment', '1'],
      ['maxLogicalPathCanonicalBytes', 'One canonical byte in one projected logical path', '1'],
      ['maxSubjectOccurrences', 'One pre-deduplication logical subject occurrence', '1'],
      ['maxTokenInputs', 'One safe projected token-input occurrence', '1'],
      [
        'maxCollisionComparisons',
        'One full safe-projection comparison inside a digest-collision bucket',
        '1'
      ],
      [
        'maxMetadataWorkUnits',
        'One source, locator, path, projection, or token rule evaluation',
        '1'
      ]
    ]
  ],
  [
    'mandatoryReserve',
    [
      ['maxOutcomeEnvelopeSlots', 'One complete checked-or-blocked outcome envelope', '1'],
      ['maxConstructionIdentitySlots', 'One outcome-level Resolver Construction Identity', '1'],
      ['maxEntryTerminalFindingSlots', 'One Entry-domain terminal Finding', '1'],
      ['maxEntryTerminalStatusSlots', 'One Entry-domain terminal evaluation-status record', '1'],
      ['maxSharedTerminalFindingSlots', 'One Shared-domain terminal Finding', '1'],
      ['maxSharedTerminalStatusSlots', 'One Shared-domain terminal evaluation-status record', '1'],
      [
        'maxSharedUnavailableCauseSlots',
        'One Entry-owned Shared-unavailability cause reference',
        '1'
      ],
      [
        'maxCanonicalBytes',
        'One byte in the complete revision-`"0"` mandatory logical reserve representation',
        '65536'
      ]
    ]
  ]
]
