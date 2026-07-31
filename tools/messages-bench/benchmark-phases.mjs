import { resource_host_parse_and_entry_extraction } from '../resource-bench/benchmark-phases.mjs'

export { resource_host_parse_and_entry_extraction }

export const MESSAGE_BENCHMARK_PHASES = Object.freeze([
  phase('message_project_input_io', [
    'inventory_metadata_io',
    'definition_snapshot_read',
    'reference_snapshot_read',
    'external_artifact_read'
  ]),
  phase('message_reference_produce_js', [
    'source_parse',
    'recognizer_match_and_static_evaluation',
    'key_and_provenance_construction',
    'reference_artifact_construction'
  ]),
  phase('message_reference_produce_js_cache', [
    'cache_miss_production',
    'cache_hit_validation_and_access'
  ]),
  phase('message_artifact_codec', [
    'reference_artifact_encode',
    'reference_artifact_decode',
    'definition_artifact_encode',
    'definition_artifact_decode'
  ]),
  phase('message_definition_project', ['pre_extraction_admission', 'definition_projection']),
  phase('resource_extract', ['host_parse_and_entry_extraction']),
  phase('message_link_core', [
    'request_validation_and_scope_mapping',
    'semantic_index_construction',
    'selector_expansion_and_reference_resolution',
    'reachability_and_placement',
    'finding_and_plan_materialization'
  ]),
  phase('message_link_peak_memory', ['link_core_peak_live_memory']),
  phase('message_project_link_e2e', ['complete_workflow'])
])

function phase(name, costs) {
  return Object.freeze({ name, costs: Object.freeze(costs) })
}

const PER_WORKFLOW = 'one_per_workflow_iteration'
const PER_INVENTORY_CLASS = 'per_planned_project_inventory_class'
const PER_DEFINITION_GROUP = 'per_planned_definition_physical_source_group'
const PER_JS_GROUP = 'per_planned_js_reference_physical_source_group'
const PER_REFERENCE_ARTIFACT = 'per_planned_reference_artifact'
const PER_DEFINITION_ARTIFACT = 'per_planned_definition_artifact'

const messagesOwnedDescriptors = [
  boundary(
    'message_project_input_io',
    'inventory_metadata_io',
    'project_inventory_metadata_io',
    'duration',
    PER_INVENTORY_CLASS
  ),
  boundary(
    'message_project_input_io',
    'definition_snapshot_read',
    'project_definition_snapshot_read',
    'duration',
    PER_WORKFLOW
  ),
  boundary(
    'message_project_input_io',
    'reference_snapshot_read',
    'project_reference_snapshot_read',
    'duration',
    PER_WORKFLOW
  ),
  boundary(
    'message_project_input_io',
    'external_artifact_read',
    'project_external_artifact_read',
    'duration',
    PER_WORKFLOW
  ),
  boundary(
    'message_reference_produce_js',
    'source_parse',
    'js_source_parse',
    'duration',
    PER_JS_GROUP
  ),
  boundary(
    'message_reference_produce_js',
    'recognizer_match_and_static_evaluation',
    'js_recognizer_static_evaluation',
    'duration',
    PER_JS_GROUP
  ),
  boundary(
    'message_reference_produce_js',
    'key_and_provenance_construction',
    'js_key_provenance_construction',
    'duration',
    PER_JS_GROUP
  ),
  boundary(
    'message_reference_produce_js',
    'reference_artifact_construction',
    'js_reference_artifact_construction',
    'duration',
    PER_JS_GROUP
  ),
  boundary(
    'message_reference_produce_js_cache',
    'cache_miss_production',
    'js_cache_miss_production',
    'duration',
    PER_JS_GROUP
  ),
  boundary(
    'message_reference_produce_js_cache',
    'cache_hit_validation_and_access',
    'js_cache_hit_access',
    'duration',
    PER_JS_GROUP
  ),
  boundary(
    'message_artifact_codec',
    'reference_artifact_encode',
    'reference_artifact_encode',
    'duration',
    PER_REFERENCE_ARTIFACT
  ),
  boundary(
    'message_artifact_codec',
    'reference_artifact_decode',
    'reference_artifact_decode',
    'duration',
    PER_REFERENCE_ARTIFACT
  ),
  boundary(
    'message_artifact_codec',
    'definition_artifact_encode',
    'definition_artifact_encode',
    'duration',
    PER_DEFINITION_ARTIFACT
  ),
  boundary(
    'message_artifact_codec',
    'definition_artifact_decode',
    'definition_artifact_decode',
    'duration',
    PER_DEFINITION_ARTIFACT
  ),
  boundary(
    'message_definition_project',
    'pre_extraction_admission',
    'definition_pre_extraction_admission',
    'duration',
    PER_DEFINITION_GROUP
  ),
  boundary(
    'message_definition_project',
    'definition_projection',
    'definition_projection',
    'duration',
    PER_WORKFLOW
  ),
  boundary(
    'message_link_core',
    'request_validation_and_scope_mapping',
    'link_request_validation_scope_mapping',
    'duration',
    PER_WORKFLOW
  ),
  boundary(
    'message_link_core',
    'semantic_index_construction',
    'link_semantic_index_construction',
    'duration',
    PER_WORKFLOW
  ),
  boundary(
    'message_link_core',
    'selector_expansion_and_reference_resolution',
    'link_selector_resolution',
    'duration',
    PER_WORKFLOW
  ),
  boundary(
    'message_link_core',
    'reachability_and_placement',
    'link_reachability_placement',
    'duration',
    PER_WORKFLOW
  ),
  boundary(
    'message_link_core',
    'finding_and_plan_materialization',
    'link_finding_plan_materialization',
    'duration',
    PER_WORKFLOW
  ),
  boundary(
    'message_link_peak_memory',
    'link_core_peak_live_memory',
    'link_core_peak_live_memory',
    'peak_live_memory',
    PER_WORKFLOW
  ),
  boundary(
    'message_project_link_e2e',
    'complete_workflow',
    'project_link_e2e',
    'duration',
    PER_WORKFLOW
  )
]

export const MESSAGE_BENCHMARK_BOUNDARIES = Object.freeze([
  ...messagesOwnedDescriptors.map(descriptor => Object.freeze({ owner: '014', descriptor })),
  Object.freeze({
    owner: '013',
    descriptor: resource_host_parse_and_entry_extraction
  })
])

export const MESSAGE_BENCHMARK_OVERLAP_TOPOLOGY = Object.freeze([
  ...[
    'project_inventory_metadata_io',
    'project_definition_snapshot_read',
    'project_reference_snapshot_read',
    'project_external_artifact_read',
    'definition_pre_extraction_admission',
    'resource_host_parse_and_entry_extraction',
    'definition_projection',
    'js_cache_miss_production',
    'reference_artifact_decode',
    'link_request_validation_scope_mapping',
    'link_semantic_index_construction',
    'link_selector_resolution',
    'link_reachability_placement',
    'link_finding_plan_materialization'
  ].map(childBoundaryId =>
    Object.freeze({ parentBoundaryId: 'project_link_e2e', childBoundaryId })
  ),
  ...[
    'js_source_parse',
    'js_recognizer_static_evaluation',
    'js_key_provenance_construction',
    'js_reference_artifact_construction'
  ].map(childBoundaryId =>
    Object.freeze({ parentBoundaryId: 'js_cache_miss_production', childBoundaryId })
  )
])

function boundary(phaseName, cost, boundaryId, metric, occurrencePolicy) {
  return Object.freeze({
    phase: phaseName,
    cost,
    boundaryId,
    metric,
    occurrencePolicy,
    firstMarker: `before_${boundaryId}`,
    finalMarker: `complete_${boundaryId}_output_retained`,
    includedMarkers: Object.freeze([boundaryId]),
    excludedMarkers: Object.freeze([
      'fixture_setup',
      'warmup',
      'checksum_observation',
      'result_aggregation'
    ])
  })
}

/**
 * Validate exact active boundary coverage and the allowed overlap topology.
 *
 * @param registry - Boundary descriptors under validation.
 * @param topology - Explicitly allowed parent/child interval relations.
 */
export function assertValidBoundaryRegistry(
  registry = MESSAGE_BENCHMARK_BOUNDARIES,
  topology = MESSAGE_BENCHMARK_OVERLAP_TOPOLOGY
) {
  const activePairs = new Set(
    MESSAGE_BENCHMARK_PHASES.flatMap(active => active.costs.map(cost => `${active.name}\0${cost}`))
  )
  const pairs = new Set()
  const ids = new Set()
  for (const [index, entry] of registry.entries()) {
    const pointer = `/boundaries/${index}`
    if (!entry || !['013', '014'].includes(entry.owner) || !entry.descriptor) {
      throw new Error(`${pointer} must declare owner and descriptor`)
    }
    const descriptor = entry.descriptor
    const pair = `${descriptor.phase}\0${descriptor.cost}`
    if (!activePairs.has(pair) || pairs.has(pair)) {
      throw new Error(`${pointer} must identify one unique active phase/cost`)
    }
    if (
      typeof descriptor.boundaryId !== 'string' ||
      /_v\d+$/.test(descriptor.boundaryId) ||
      ids.has(descriptor.boundaryId)
    ) {
      throw new Error(`${pointer}/boundaryId must be unique and version-suffix-free`)
    }
    if (!['duration', 'peak_live_memory'].includes(descriptor.metric)) {
      throw new Error(`${pointer}/metric is inactive`)
    }
    for (const field of [
      'occurrencePolicy',
      'firstMarker',
      'finalMarker',
      'includedMarkers',
      'excludedMarkers'
    ]) {
      if (descriptor[field] == null) {
        throw new Error(`${pointer}/${field} is required`)
      }
    }
    if (entry.owner === '013' && descriptor !== resource_host_parse_and_entry_extraction) {
      throw new Error(`${pointer} must import the exact 013 descriptor object`)
    }
    pairs.add(pair)
    ids.add(descriptor.boundaryId)
  }
  if (pairs.size !== activePairs.size) {
    throw new Error('/boundaries must cover every active phase/cost exactly once')
  }

  const edges = new Set()
  for (const [index, edge] of topology.entries()) {
    const pointer = `/topology/${index}`
    if (
      !ids.has(edge.parentBoundaryId) ||
      !ids.has(edge.childBoundaryId) ||
      edge.parentBoundaryId === edge.childBoundaryId
    ) {
      throw new Error(`${pointer} must reference two distinct active boundaries`)
    }
    const key = `${edge.parentBoundaryId}\0${edge.childBoundaryId}`
    if (edges.has(key)) {
      throw new Error(`${pointer} duplicates an overlap edge`)
    }
    edges.add(key)
  }
  assertAcyclic(ids, topology)
}

function assertAcyclic(ids, topology) {
  const children = new Map([...ids].map(id => [id, []]))
  for (const edge of topology) {
    children.get(edge.parentBoundaryId).push(edge.childBoundaryId)
  }
  const visiting = new Set()
  const complete = new Set()
  const visit = id => {
    if (visiting.has(id)) {
      throw new Error('/topology must be acyclic')
    }
    if (complete.has(id)) {
      return
    }
    visiting.add(id)
    for (const child of children.get(id)) {
      visit(child)
    }
    visiting.delete(id)
    complete.add(id)
  }
  for (const id of ids) {
    visit(id)
  }
}

/**
 * Enforce canonical occurrence order and properly nested interval markers.
 */
export class BoundaryRecorder {
  #descriptors
  #edges
  #expected
  #next
  #active = []
  #stopped = new Set()

  constructor(expectedOccurrences, options = {}) {
    const registry = options.registry ?? MESSAGE_BENCHMARK_BOUNDARIES
    const topology = options.topology ?? MESSAGE_BENCHMARK_OVERLAP_TOPOLOGY
    assertValidBoundaryRegistry(registry, topology)
    this.#descriptors = new Map(
      registry.map(entry => [entry.descriptor.boundaryId, entry.descriptor])
    )
    this.#edges = new Set(topology.map(edge => `${edge.parentBoundaryId}\0${edge.childBoundaryId}`))
    this.#expected = new Map(
      Object.entries(expectedOccurrences).map(([id, identities]) => [id, [...identities]])
    )
    this.#next = new Map([...this.#expected].map(([id]) => [id, 0]))
  }

  start(boundaryId, identity) {
    if (!this.#descriptors.has(boundaryId)) {
      throw new Error(`undeclared boundary start: ${boundaryId}`)
    }
    if (this.#active.some(active => active.boundaryId === boundaryId)) {
      throw new Error(`duplicate boundary start: ${boundaryId}`)
    }
    const parent = this.#active.at(-1)
    if (parent && !this.#edges.has(`${parent.boundaryId}\0${boundaryId}`)) {
      throw new Error(`undeclared nesting: ${parent.boundaryId} -> ${boundaryId}`)
    }
    const expected = this.#expected.get(boundaryId)
    const ordinal = this.#next.get(boundaryId) ?? 0
    if (!expected || expected[ordinal] !== identity) {
      throw new Error(`wrong occurrence order for ${boundaryId}: ${identity}`)
    }
    this.#active.push({ boundaryId, identity })
  }

  stop(boundaryId, identity) {
    const active = this.#active.at(-1)
    if (!active) {
      throw new Error(`duplicate or unmatched boundary stop: ${boundaryId}`)
    }
    if (active.boundaryId !== boundaryId || active.identity !== identity) {
      throw new Error(`crossing boundary stop: ${boundaryId}`)
    }
    this.#active.pop()
    const key = `${boundaryId}\0${identity}`
    if (this.#stopped.has(key)) {
      throw new Error(`duplicate boundary stop: ${boundaryId}`)
    }
    this.#stopped.add(key)
    this.#next.set(boundaryId, (this.#next.get(boundaryId) ?? 0) + 1)
  }

  finish() {
    if (this.#active.length !== 0) {
      throw new Error('boundary recorder has unterminated intervals')
    }
    for (const [boundaryId, identities] of this.#expected) {
      if (this.#next.get(boundaryId) !== identities.length) {
        throw new Error(`boundary recorder is missing ${boundaryId} occurrences`)
      }
    }
  }
}

assertValidBoundaryRegistry()
