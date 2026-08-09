import { readFileSync } from 'node:fs'

import {
  MESSAGE_BENCHMARK_BOUNDARIES,
  MESSAGE_BENCHMARK_NON_INTERVALS,
  resource_host_parse_and_entry_extraction
} from './benchmark-phases.mjs'

export const MESSAGE_BENCHMARK_PROFILE_REVISION = 4

const selection = JSON.parse(
  readFileSync(new URL('./fixture-selection.json', import.meta.url), 'utf8')
)
const descriptorByPair = new Map(
  [...MESSAGE_BENCHMARK_BOUNDARIES, ...MESSAGE_BENCHMARK_NON_INTERVALS].map(entry => [
    `${entry.descriptor.phase}\0${entry.descriptor.cost}`,
    entry.descriptor
  ])
)
const project = selection.projectFixture
const dense = selection.profiles.find(profile => profile.shape === 'exact_reference_dense')
const typedKeyModel = selection.profiles.find(profile => profile.shape === 'typed_key_model')
const localeFallbackExpansion = selection.profiles.find(
  profile => profile.shape === 'locale_fallback_expansion'
)
const messageExportEsm = selection.profiles.find(profile => profile.shape === 'message_export_esm')
const messageOutputRegistration = selection.profiles.find(
  profile => profile.shape === 'message_output_registration'
)
const messagesEmit = selection.profiles.find(profile => profile.shape === 'messages_emit')

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
  ...selection.profiles
    .filter(
      profile =>
        ![
          'typed_key_model',
          'locale_fallback_expansion',
          'message_export_esm',
          'message_output_registration',
          'messages_emit'
        ].includes(profile.shape)
    )
    .flatMap(profile =>
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
    ),
  ...typedKeyModel.scales.flatMap(scale =>
    ['coverage_baseline_selection', 'typed_key_model_construction'].map(cost => {
      const descriptor = descriptorFor('message_typed_key_model', cost)
      return requiredCase({
        phase: 'message_typed_key_model',
        cost,
        fixture: typedKeyModel.name,
        variant: 'typed_key_model',
        scale,
        operation: descriptor.boundaryId,
        metric: descriptor.metric
      })
    })
  ),
  ...localeFallbackExpansion.scales.flatMap(scale =>
    [
      'fallback_chain_construction',
      'locale_aware_resolution',
      'locale_finding_materialization'
    ].map(cost => {
      const descriptor = descriptorFor('message_link_fallback', cost)
      return requiredCase({
        phase: 'message_link_fallback',
        cost,
        fixture: localeFallbackExpansion.name,
        variant: 'locale_fallback_expansion',
        scale,
        operation: descriptor.boundaryId,
        metric: descriptor.metric
      })
    })
  ),
  ...messageExportEsm.scales.flatMap(scale =>
    ['all_definitions_baseline', 'linked_output'].flatMap(variant => [
      ...[
        'selected_message_parse',
        'message_semantic_validation',
        'portable_diagnostic_mapping',
        'argument_signature_derivation',
        'validated_export_batch_construction'
      ].map(cost =>
        requiredBoundaryCase('message_export_prepare', cost, messageExportEsm, variant, scale)
      ),
      ...[
        'locale_asset_rendering',
        'loader_map_rendering',
        'typed_key_accessor_rendering',
        'export_artifact_set_construction'
      ].map(cost =>
        requiredBoundaryCase('message_export_esm', cost, messageExportEsm, variant, scale)
      )
    ])
  ),
  ...messageExportEsm.scales.map(scale =>
    requiredCase({
      phase: 'message_export_artifact_size',
      cost: 'payload_size_comparison',
      fixture: messageExportEsm.name,
      variant: 'all_definitions_baseline_vs_linked_output',
      scale,
      operation: 'payload_size_comparison',
      metric: 'artifact_payload_size'
    })
  ),
  ...messageOutputRegistration.scales.flatMap(scale => [
    ...messageOutputRegistration.variants.flatMap(variant =>
      ['capability_preflight', 'path_mapping_and_ownership_inspection'].map(cost =>
        requiredBoundaryCase(
          'message_output_register',
          cost,
          messageOutputRegistration,
          variant,
          scale
        )
      )
    ),
    ...['staging', 'commit'].map(cost =>
      requiredBoundaryCase(
        'message_output_register',
        cost,
        messageOutputRegistration,
        'write_absent',
        scale
      )
    ),
    ...['check_matched', 'check_different'].map(variant =>
      requiredBoundaryCase(
        'message_output_register',
        'check_comparison',
        messageOutputRegistration,
        variant,
        scale
      )
    )
  ]),
  ...messagesEmit.scales.flatMap(scale =>
    messagesEmit.variants.flatMap(variant => {
      const phase = variant.startsWith('check_') ? 'messages_emit_check_e2e' : 'messages_emit_e2e'
      return [
        requiredBoundaryCase(phase, 'complete_workflow', messagesEmit, variant, scale),
        requiredBoundaryCase('messages_emit_json', 'json_reporter', messagesEmit, variant, scale),
        requiredCase({
          phase: resource_host_parse_and_entry_extraction.phase,
          cost: resource_host_parse_and_entry_extraction.cost,
          fixture: messagesEmit.name,
          variant,
          scale,
          operation: resource_host_parse_and_entry_extraction.boundaryId,
          metric: resource_host_parse_and_entry_extraction.metric
        })
      ]
    })
  )
])

function requiredBoundaryCase(phase, cost, fixture, variant, scale) {
  const descriptor = descriptorFor(phase, cost)
  return requiredCase({
    phase,
    cost,
    fixture: fixture.name,
    variant,
    scale,
    operation: descriptor.boundaryId,
    metric: descriptor.metric
  })
}

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
  }),
  ...messagesEmit.scales.flatMap(scale =>
    messagesEmit.variants.map(variant => {
      const phase = variant.startsWith('check_') ? 'messages_emit_check_e2e' : 'messages_emit_e2e'
      return Object.freeze({
        e2e: requiredBoundaryCase(phase, 'complete_workflow', messagesEmit, variant, scale),
        extraction: requiredCase({
          phase: resource_host_parse_and_entry_extraction.phase,
          cost: resource_host_parse_and_entry_extraction.cost,
          fixture: messagesEmit.name,
          variant,
          scale,
          operation: resource_host_parse_and_entry_extraction.boundaryId,
          metric: resource_host_parse_and_entry_extraction.metric
        })
      })
    })
  )
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
