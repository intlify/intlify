import { readFileSync } from 'node:fs'

import {
  MESSAGE_BENCHMARK_BOUNDARIES,
  resource_host_parse_and_entry_extraction
} from './benchmark-phases.mjs'

export const MESSAGE_BENCHMARK_PROFILE_REVISION = 1

const selection = JSON.parse(
  readFileSync(new URL('./fixture-selection.json', import.meta.url), 'utf8')
)
const descriptorByPair = new Map(
  MESSAGE_BENCHMARK_BOUNDARIES.map(entry => [
    `${entry.descriptor.phase}\0${entry.descriptor.cost}`,
    entry.descriptor
  ])
)
const project = selection.projectFixture
const dense = selection.profiles.find(profile => profile.shape === 'exact_reference_dense')

const projectPairs = [
  ['message_project_input_io', 'inventory_metadata_io'],
  ['message_project_input_io', 'definition_snapshot_read'],
  ['message_project_input_io', 'reference_snapshot_read'],
  ['message_project_input_io', 'external_artifact_read'],
  ['message_reference_produce_js', 'source_parse'],
  ['message_reference_produce_js', 'recognizer_match_and_static_evaluation'],
  ['message_reference_produce_js', 'key_and_provenance_construction'],
  ['message_reference_produce_js', 'reference_artifact_construction'],
  ['message_reference_produce_js_cache', 'cache_miss_production'],
  ['message_definition_project', 'pre_extraction_admission'],
  ['message_definition_project', 'definition_projection'],
  ['resource_extract', 'host_parse_and_entry_extraction'],
  ['message_link_core', 'request_validation_and_scope_mapping'],
  ['message_link_core', 'semantic_index_construction'],
  ['message_link_core', 'selector_expansion_and_reference_resolution'],
  ['message_link_core', 'reachability_and_placement'],
  ['message_link_core', 'finding_and_plan_materialization'],
  ['message_project_link_e2e', 'complete_workflow']
]

export const MESSAGE_BENCHMARK_REQUIRED_CASES = Object.freeze([
  ...projectPairs.map(([phase, cost]) => {
    const descriptor = descriptorFor(phase, cost)
    return requiredCase({
      phase,
      cost,
      fixture: project.name,
      variant:
        phase === 'message_reference_produce_js' ||
        (phase === 'message_reference_produce_js_cache' && cost === 'cache_miss_production')
          ? 'cache_miss'
          : 'representative_project',
      scale: 1,
      operation: descriptor.boundaryId,
      metric: descriptor.metric
    })
  }),
  ...[
    'reference_artifact_encode',
    'reference_artifact_decode',
    'definition_artifact_encode',
    'definition_artifact_decode'
  ].map(cost => {
    const descriptor = descriptorFor('message_artifact_codec', cost)
    return requiredCase({
      phase: 'message_artifact_codec',
      cost,
      fixture: project.name,
      variant: 'canonical_json',
      scale: 1,
      operation: descriptor.boundaryId,
      metric: descriptor.metric
    })
  }),
  requiredCase({
    phase: 'message_reference_produce_js_cache',
    cost: 'cache_hit_validation_and_access',
    fixture: dense.name,
    variant: 'cache_hit',
    scale: dense.scales[0],
    operation: 'js_cache_hit_access',
    metric: 'duration'
  }),
  ...selection.profiles.flatMap(profile =>
    profile.scales.map(scale =>
      requiredCase({
        phase: 'message_link_peak_memory',
        cost: 'link_core_peak_live_memory',
        fixture: profile.name,
        variant: profile.shape,
        scale,
        operation: 'link_core_peak_live_memory',
        metric: 'peak_live_memory'
      })
    )
  )
])

function descriptorFor(phase, cost) {
  const descriptor = descriptorByPair.get(`${phase}\0${cost}`)
  if (!descriptor) {
    throw new Error(`missing boundary descriptor for ${phase}/${cost}`)
  }
  return descriptor
}

function requiredCase(value) {
  return Object.freeze({
    ...value,
    runtime: 'intlify-messages-bench-rs',
    executionModel: 'in_process'
  })
}

export const MESSAGE_BENCHMARK_E2E_EXTRACTION_COMPANIONS = Object.freeze([
  Object.freeze({
    e2e: requiredCase({
      phase: 'message_project_link_e2e',
      cost: 'complete_workflow',
      fixture: project.name,
      variant: 'representative_project',
      scale: 1,
      operation: 'project_link_e2e',
      metric: 'duration'
    }),
    extraction: requiredCase({
      phase: resource_host_parse_and_entry_extraction.phase,
      cost: resource_host_parse_and_entry_extraction.cost,
      fixture: project.name,
      variant: 'representative_project',
      scale: 1,
      operation: resource_host_parse_and_entry_extraction.boundaryId,
      metric: resource_host_parse_and_entry_extraction.metric
    })
  })
])

/**
 * Reject ambiguous or shared E2E-to-extraction relations before result lookup.
 *
 * @param mappings - Checked one-to-one companion mappings.
 */
export function assertValidCompanionMappings(
  mappings = MESSAGE_BENCHMARK_E2E_EXTRACTION_COMPANIONS
) {
  const e2eCases = new Set()
  const extractionCases = new Set()
  for (const [index, mapping] of mappings.entries()) {
    if (!mapping?.e2e || !mapping?.extraction) {
      throw new Error(`/companions/${index} must contain e2e and extraction tuples`)
    }
    const e2e = benchmarkCaseKey(mapping.e2e)
    const extraction = benchmarkCaseKey(mapping.extraction)
    if (e2eCases.has(e2e)) {
      throw new Error(`/companions/${index} duplicates an E2E tuple`)
    }
    if (extractionCases.has(extraction)) {
      throw new Error(`/companions/${index} shares an extraction tuple`)
    }
    e2eCases.add(e2e)
    extractionCases.add(extraction)
  }
}

/**
 * Construct the exact required-case identity tuple.
 *
 * @param value - Result record or required-case descriptor.
 * @returns NUL-delimited exact benchmark-case key.
 */
export function benchmarkCaseKey(value) {
  return [
    value.phase,
    value.cost,
    value.fixture,
    value.variant,
    value.scale,
    value.runtime,
    value.operation,
    value.executionModel,
    value.metric
  ].join('\0')
}

assertValidCompanionMappings()
