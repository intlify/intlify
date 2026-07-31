import { MESSAGE_BENCHMARK_BOUNDARIES } from './benchmark-phases.mjs'

const textEncoder = new TextEncoder()

const semanticFieldsByBoundaryId = Object.freeze({
  project_inventory_metadata_io: Object.freeze([
    'canonicalLogicalSources',
    'canonicalPhysicalGroups',
    'sourceKind'
  ]),
  project_definition_snapshot_read: Object.freeze([
    'canonicalGroupPaths',
    'sourceByteLength',
    'sourceBytes'
  ]),
  project_reference_snapshot_read: Object.freeze([
    'canonicalGroupPaths',
    'selectedSourceProfile',
    'sourceByteLength',
    'sourceBytes'
  ]),
  project_external_artifact_read: Object.freeze([
    'canonicalGroupPaths',
    'artifactByteLength',
    'artifactBytes'
  ]),
  js_source_parse: Object.freeze([
    'sourceProfile',
    'sourceBytes',
    'orderedSyntaxEventsOrParserFailure'
  ]),
  js_recognizer_static_evaluation: Object.freeze([
    'orderedRecognizerMatches',
    'orderedProducerFailures'
  ]),
  js_key_provenance_construction: Object.freeze([
    'orderedReferenceRecords',
    'orderedProducerFailures'
  ]),
  js_reference_artifact_construction: Object.freeze(['canonicalReferenceArtifactBytes']),
  js_cache_miss_production: Object.freeze([
    'cacheOutcome',
    'canonicalReferenceArtifactBytes',
    'cacheIdentityComponents'
  ]),
  js_cache_hit_access: Object.freeze([
    'cacheOutcome',
    'canonicalReferenceArtifactBytes',
    'cacheIdentityComponents'
  ]),
  reference_artifact_encode: Object.freeze(['encodedArtifactBytes']),
  reference_artifact_decode: Object.freeze(['canonicalDecodedArtifactBytes']),
  definition_artifact_encode: Object.freeze(['encodedArtifactBytes']),
  definition_artifact_decode: Object.freeze(['canonicalDecodedArtifactBytes']),
  definition_pre_extraction_admission: Object.freeze([
    'canonicalGroupPaths',
    'resolvedHostFormat',
    'scope',
    'locale',
    'admissionOutcome'
  ]),
  definition_projection: Object.freeze(['canonicalDefinitionArtifactSequence']),
  link_request_validation_scope_mapping: Object.freeze([
    'canonicalReferenceArtifacts',
    'canonicalDefinitionArtifacts',
    'policy',
    'scopeMappings',
    'scopeCompleteness',
    'deliveryGraph',
    'validationOutcome'
  ]),
  link_semantic_index_construction: Object.freeze([
    'canonicalReferenceIndex',
    'canonicalDefinitionIndex',
    'ambiguityFacts'
  ]),
  link_selector_resolution: Object.freeze([
    'canonicalSelectorExpansions',
    'canonicalResolutionFacts'
  ]),
  link_reachability_placement: Object.freeze(['canonicalReachability', 'canonicalPlacements']),
  link_finding_plan_materialization: Object.freeze([
    'canonicalFindings',
    'canonicalBundlePlans',
    'generationDisposition'
  ]),
  project_link_e2e: Object.freeze([
    'canonicalDefinitionArtifacts',
    'canonicalReferenceArtifacts',
    'checkedRequestObservation',
    'canonicalLinkOutcome'
  ])
})

export const MESSAGE_DURATION_OBSERVATIONS = Object.freeze(
  MESSAGE_BENCHMARK_BOUNDARIES.filter(
    entry => entry.owner === '014' && entry.descriptor.metric === 'duration'
  ).map(entry =>
    Object.freeze({
      phase: entry.descriptor.phase,
      cost: entry.descriptor.cost,
      codecId: `${entry.descriptor.boundaryId}_observation`,
      semanticFields: semanticFieldsByBoundaryId[entry.descriptor.boundaryId]
    })
  )
)

/**
 * Apply the benchmark checksum's exact FNV-1a numeric routine.
 *
 * @param bytes - Exact bytes in semantic framing order.
 * @returns Unsigned 32-bit FNV-1a checksum.
 */
export function fnv1a32(bytes) {
  let state = 2_166_136_261
  for (const byte of bytes) {
    state ^= byte
    state = Math.imul(state, 16_777_619) >>> 0
  }
  return state
}

/**
 * Checksum one canonically ordered stage-occurrence sequence.
 *
 * This shared implementation fixes the outer framing consumed by the Rust
 * runner. Each occurrence carries its canonical identity and the checksum of
 * the stage-specific semantic observation.
 *
 * @param domain - Stable phase cost used as the observation domain.
 * @param occurrences - Canonically ordered identity/checksum pairs.
 * @returns Unsigned checksum for the complete occurrence sequence.
 */
export function checksumOccurrenceSequence(domain, occurrences) {
  if (typeof domain !== 'string' || domain.length === 0) {
    throw new Error('duration checksum domain must be a non-empty string')
  }
  if (!Array.isArray(occurrences)) {
    throw new Error('duration checksum occurrences must be an array')
  }

  let state = fnv1a32(textEncoder.encode(`${domain}\0`))
  state = updateField(state, 0x01, encodeU64(occurrences.length))
  for (const [index, occurrence] of occurrences.entries()) {
    if (
      occurrence === null ||
      typeof occurrence !== 'object' ||
      typeof occurrence.identity !== 'string'
    ) {
      throw new Error(`/occurrences/${index}/identity must be a string`)
    }
    if (
      !Number.isSafeInteger(occurrence.checksum) ||
      occurrence.checksum < 0 ||
      occurrence.checksum > 0xffff_ffff
    ) {
      throw new Error(`/occurrences/${index}/checksum must be a u32`)
    }
    state = updateField(state, 0x02, textEncoder.encode(occurrence.identity))
    state = updateField(state, 0x03, encodeU32(occurrence.checksum))
  }
  return state
}

/**
 * Aggregate equal per-iteration checksums after all warmups have completed.
 *
 * @param perIterationChecksums - Checksums from measured repetitions only.
 * @returns Wrapping sum of the equal measured checksums.
 */
export function aggregateRepeatedChecksum(perIterationChecksums) {
  if (!Array.isArray(perIterationChecksums) || perIterationChecksums.length === 0) {
    throw new Error('duration checksum aggregation requires measured repetitions')
  }
  const expected = perIterationChecksums[0]
  if (!Number.isSafeInteger(expected) || expected < 0 || expected > 0xffff_ffff) {
    throw new Error('duration checksum must be a u32')
  }
  let aggregate = 0
  for (const checksum of perIterationChecksums) {
    if (checksum !== expected) {
      throw new Error('duration semantic checksum changed across repetitions')
    }
    aggregate = (aggregate + checksum) >>> 0
  }
  return aggregate
}

/**
 * Validate the closed duration-observation registry.
 */
export function assertValidChecksumObservationRegistry() {
  const pairs = new Set()
  const codecIds = new Set()
  for (const [index, observation] of MESSAGE_DURATION_OBSERVATIONS.entries()) {
    const key = `${observation.phase}\0${observation.cost}`
    if (pairs.has(key)) {
      throw new Error(`/checksumObservations/${index} duplicates a phase/cost`)
    }
    if (
      typeof observation.codecId !== 'string' ||
      codecIds.has(observation.codecId) ||
      !Array.isArray(observation.semanticFields) ||
      observation.semanticFields.length === 0
    ) {
      throw new Error(`/checksumObservations/${index} is malformed`)
    }
    pairs.add(key)
    codecIds.add(observation.codecId)
  }
}

function updateField(state, tag, payload) {
  const framed = new Uint8Array(1 + 8 + payload.length)
  framed[0] = tag
  framed.set(encodeU64(payload.length), 1)
  framed.set(payload, 9)
  return updateFnv1a32(state, framed)
}

function encodeU32(value) {
  const bytes = new Uint8Array(4)
  new DataView(bytes.buffer).setUint32(0, value, false)
  return bytes
}

function encodeU64(value) {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new Error('checksum length must be a non-negative safe integer')
  }
  const bytes = new Uint8Array(8)
  new DataView(bytes.buffer).setBigUint64(0, BigInt(value), false)
  return bytes
}

function updateFnv1a32(initialState, bytes) {
  let state = initialState
  for (const byte of bytes) {
    state ^= byte
    state = Math.imul(state, 16_777_619) >>> 0
  }
  return state
}

assertValidChecksumObservationRegistry()
