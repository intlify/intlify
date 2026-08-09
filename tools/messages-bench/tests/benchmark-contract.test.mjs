import { readFileSync } from 'node:fs'

import { describe, expect, test } from 'vite-plus/test'

import {
  BoundaryRecorder,
  MESSAGE_BENCHMARK_BOUNDARIES,
  MESSAGE_BENCHMARK_NON_INTERVALS,
  MESSAGE_BENCHMARK_OVERLAP_TOPOLOGY,
  MESSAGE_BENCHMARK_PHASES,
  assertValidBoundaryRegistry,
  resource_host_parse_and_entry_extraction
} from '../benchmark-phases.mjs'
import {
  MESSAGE_BENCHMARK_E2E_EXTRACTION_COMPANIONS,
  MESSAGE_BENCHMARK_PROFILE_REVISION,
  MESSAGE_BENCHMARK_REQUIRED_CASES,
  assertValidCompanionMappings
} from '../benchmark-profile.mjs'
import {
  MESSAGE_DURATION_OBSERVATIONS,
  aggregateRepeatedChecksum,
  checksumOccurrenceSequence
} from '../checksum-observations.mjs'
import {
  MESSAGE_BENCH_RESULT_SCHEMA_VERSION,
  assertValidMessageBenchmarkResult
} from '../result-schema.mjs'
import { resource_host_parse_and_entry_extraction as ownedResourceDescriptor } from '../../resource-bench/benchmark-phases.mjs'

const fixtures = JSON.parse(
  readFileSync(new URL('../fixture-selection.json', import.meta.url), 'utf8')
)
const checksumVectors = JSON.parse(
  readFileSync(new URL('../checksum-vectors.json', import.meta.url), 'utf8')
)

test('active project-link boundaries cover every phase/cost and import the 013 descriptor by identity', () => {
  expect(() => assertValidBoundaryRegistry()).not.toThrow()
  expect(resource_host_parse_and_entry_extraction).toBe(ownedResourceDescriptor)
  expect(MESSAGE_BENCHMARK_BOUNDARIES.length + MESSAGE_BENCHMARK_NON_INTERVALS.length).toBe(
    MESSAGE_BENCHMARK_PHASES.reduce((count, phase) => count + phase.costs.length, 0)
  )
  expect(
    MESSAGE_BENCHMARK_BOUNDARIES.every(entry => !/_v\d+$/.test(entry.descriptor.boundaryId))
  ).toBe(true)
})

test('generated fixture profiles declare their exact checked Rust generator', () => {
  for (const profile of fixtures.profiles) {
    expect(profile.generator).toEqual({
      kind: 'rust',
      id: profile.shape
    })
  }
})

test('typed-key model costs activate the exact generated scales and sibling boundaries', () => {
  expect(MESSAGE_BENCHMARK_PHASES.find(phase => phase.name === 'message_typed_key_model')).toEqual({
    name: 'message_typed_key_model',
    costs: ['coverage_baseline_selection', 'typed_key_model_construction']
  })
  const fixture = fixtures.profiles.find(profile => profile.shape === 'typed_key_model')
  expect(fixture.scales).toEqual([16, 64, 256])
  const required = MESSAGE_BENCHMARK_REQUIRED_CASES.filter(
    record => record.phase === 'message_typed_key_model'
  )
  expect(required).toHaveLength(6)
  expect(new Set(required.map(record => record.scale))).toEqual(new Set(fixture.scales))
  expect(new Set(required.map(record => record.cost))).toEqual(
    new Set(['coverage_baseline_selection', 'typed_key_model_construction'])
  )
  expect(
    MESSAGE_BENCHMARK_OVERLAP_TOPOLOGY.filter(
      edge =>
        edge.parentBoundaryId === 'project_link_e2e' &&
        ['coverage_baseline_selection', 'typed_key_model_construction'].includes(
          edge.childBoundaryId
        )
    )
  ).toHaveLength(2)
})

test('fallback-aware costs activate the exact generated scales and nested sibling boundaries', () => {
  expect(MESSAGE_BENCH_RESULT_SCHEMA_VERSION).toBe('3')
  expect(MESSAGE_BENCHMARK_PROFILE_REVISION).toBe(4)
  expect(fixtures.revision).toBe(4)
  expect(MESSAGE_BENCHMARK_PHASES.find(phase => phase.name === 'message_link_fallback')).toEqual({
    name: 'message_link_fallback',
    costs: [
      'fallback_chain_construction',
      'locale_aware_resolution',
      'locale_finding_materialization'
    ]
  })
  const fixture = fixtures.profiles.find(profile => profile.shape === 'locale_fallback_expansion')
  expect(fixture).toEqual({
    name: 'locale-fallback-expansion',
    revision: 1,
    shape: 'locale_fallback_expansion',
    generator: { kind: 'rust', id: 'locale_fallback_expansion' },
    variants: ['locale_fallback_expansion'],
    scales: [16, 64, 256]
  })
  const required = MESSAGE_BENCHMARK_REQUIRED_CASES.filter(
    record => record.phase === 'message_link_fallback'
  )
  expect(required).toHaveLength(9)
  expect(new Set(required.map(record => record.scale))).toEqual(new Set(fixture.scales))
  expect(new Set(required.map(record => record.cost))).toEqual(
    new Set([
      'fallback_chain_construction',
      'locale_aware_resolution',
      'locale_finding_materialization'
    ])
  )
  for (const cost of [
    'fallback_chain_construction',
    'locale_aware_resolution',
    'locale_finding_materialization'
  ]) {
    const descriptor = MESSAGE_BENCHMARK_BOUNDARIES.find(
      entry => entry.descriptor.phase === 'message_link_fallback' && entry.descriptor.cost === cost
    ).descriptor
    expect(descriptor).toMatchObject({
      boundaryId: cost,
      metric: 'duration',
      occurrencePolicy: 'one_per_workflow_iteration',
      firstMarker: `before_${cost}`,
      finalMarker: `complete_${cost}_output_retained`,
      includedMarkers: [cost],
      excludedMarkers: ['fixture_setup', 'warmup', 'checksum_observation', 'result_aggregation']
    })
  }
  expect(
    MESSAGE_BENCHMARK_OVERLAP_TOPOLOGY.filter(
      edge => edge.parentBoundaryId === 'link_selector_resolution'
    ).map(edge => edge.childBoundaryId)
  ).toEqual(['fallback_chain_construction', 'locale_aware_resolution'])
  expect(MESSAGE_BENCHMARK_OVERLAP_TOPOLOGY).toContainEqual({
    parentBoundaryId: 'link_finding_plan_materialization',
    childBoundaryId: 'locale_finding_materialization'
  })
  expect(
    MESSAGE_BENCHMARK_OVERLAP_TOPOLOGY.some(edge =>
      [
        'fallback_chain_construction',
        'locale_aware_resolution',
        'locale_finding_materialization'
      ].includes(edge.parentBoundaryId)
    )
  ).toBe(false)
  expect(
    MESSAGE_DURATION_OBSERVATIONS.find(
      observation => observation.cost === 'typed_key_model_construction'
    ).semanticFields
  ).toContain('canonicalCoverageAnalysisStates')
})

test('JavaScript occurrence checksum matches the shared Rust vectors', () => {
  expect(checksumVectors.revision).toBe(1)
  for (const vector of checksumVectors.vectors) {
    expect(checksumOccurrenceSequence(vector.domain, vector.occurrences), vector.name).toBe(
      vector.expected
    )
  }
})

test('every messages-owned duration boundary declares a semantic observation DTO', () => {
  const expected = MESSAGE_BENCHMARK_BOUNDARIES.filter(
    entry => entry.owner === '014' && entry.descriptor.metric === 'duration'
  )
  expect(MESSAGE_DURATION_OBSERVATIONS).toHaveLength(expected.length)
  expect(
    MESSAGE_DURATION_OBSERVATIONS.every(observation => observation.semanticFields.length > 0)
  ).toBe(true)
})

test('duration repetition aggregation accepts zero, wraps, and rejects instability', () => {
  expect(aggregateRepeatedChecksum([0, 0])).toBe(0)
  expect(aggregateRepeatedChecksum([0xffff_ffff, 0xffff_ffff])).toBe(0xffff_fffe)
  expect(() => aggregateRepeatedChecksum([])).toThrow(/requires measured repetitions/)
  expect(() => aggregateRepeatedChecksum([1, 2])).toThrow(/changed across repetitions/)
  expect(() => aggregateRepeatedChecksum([-1])).toThrow(/must be a u32/)
})

describe('boundary recorder', () => {
  test('accepts the three canonical project inventory class occurrences', () => {
    const recorder = new BoundaryRecorder({
      project_inventory_metadata_io: [
        'definition-inventory',
        'reference-js-inventory',
        'reference-external-inventory'
      ]
    })
    for (const identity of [
      'definition-inventory',
      'reference-js-inventory',
      'reference-external-inventory'
    ]) {
      recorder.start('project_inventory_metadata_io', identity)
      recorder.stop('project_inventory_metadata_io', identity)
    }
    expect(() => recorder.finish()).not.toThrow()
  })

  test('accepts the declared E2E and JS-cache nesting in occurrence order', () => {
    const recorder = new BoundaryRecorder({
      project_link_e2e: ['project'],
      js_cache_miss_production: ['src/app.mts'],
      js_source_parse: ['src/app.mts']
    })
    recorder.start('project_link_e2e', 'project')
    recorder.start('js_cache_miss_production', 'src/app.mts')
    recorder.start('js_source_parse', 'src/app.mts')
    recorder.stop('js_source_parse', 'src/app.mts')
    recorder.stop('js_cache_miss_production', 'src/app.mts')
    recorder.stop('project_link_e2e', 'project')
    expect(() => recorder.finish()).not.toThrow()
  })

  test('accepts typed-key model siblings and rejects nesting one inside the other', () => {
    const recorder = new BoundaryRecorder({
      project_link_e2e: ['project'],
      coverage_baseline_selection: ['link-request'],
      typed_key_model_construction: ['link-request']
    })
    recorder.start('project_link_e2e', 'project')
    recorder.start('coverage_baseline_selection', 'link-request')
    recorder.stop('coverage_baseline_selection', 'link-request')
    recorder.start('typed_key_model_construction', 'link-request')
    recorder.stop('typed_key_model_construction', 'link-request')
    recorder.stop('project_link_e2e', 'project')
    expect(() => recorder.finish()).not.toThrow()

    const nested = new BoundaryRecorder({
      coverage_baseline_selection: ['link-request'],
      typed_key_model_construction: ['link-request']
    })
    nested.start('coverage_baseline_selection', 'link-request')
    expect(() => nested.start('typed_key_model_construction', 'link-request')).toThrow(
      /undeclared nesting/
    )
  })

  test('accepts fallback children only under their declared enclosing boundaries', () => {
    const recorder = new BoundaryRecorder({
      link_selector_resolution: ['link-request'],
      fallback_chain_construction: ['link-request'],
      locale_aware_resolution: ['link-request'],
      link_finding_plan_materialization: ['link-request'],
      locale_finding_materialization: ['link-request']
    })
    recorder.start('link_selector_resolution', 'link-request')
    recorder.start('fallback_chain_construction', 'link-request')
    recorder.stop('fallback_chain_construction', 'link-request')
    recorder.start('locale_aware_resolution', 'link-request')
    recorder.stop('locale_aware_resolution', 'link-request')
    recorder.stop('link_selector_resolution', 'link-request')
    recorder.start('link_finding_plan_materialization', 'link-request')
    recorder.start('locale_finding_materialization', 'link-request')
    recorder.stop('locale_finding_materialization', 'link-request')
    recorder.stop('link_finding_plan_materialization', 'link-request')
    expect(() => recorder.finish()).not.toThrow()

    const nestedChildren = new BoundaryRecorder({
      fallback_chain_construction: ['link-request'],
      locale_aware_resolution: ['link-request']
    })
    nestedChildren.start('fallback_chain_construction', 'link-request')
    expect(() => nestedChildren.start('locale_aware_resolution', 'link-request')).toThrow(
      /undeclared nesting/
    )

    const wrongParent = new BoundaryRecorder({
      link_selector_resolution: ['link-request'],
      locale_finding_materialization: ['link-request']
    })
    wrongParent.start('link_selector_resolution', 'link-request')
    expect(() => wrongParent.start('locale_finding_materialization', 'link-request')).toThrow(
      /undeclared nesting/
    )
  })

  test('rejects undeclared nesting, crossing, duplicate start and stop', () => {
    const nested = new BoundaryRecorder({
      project_link_e2e: ['project'],
      reference_artifact_encode: ['reference']
    })
    nested.start('project_link_e2e', 'project')
    expect(() => nested.start('reference_artifact_encode', 'reference')).toThrow(
      /undeclared nesting/
    )

    const crossing = new BoundaryRecorder({
      project_link_e2e: ['project'],
      project_inventory_metadata_io: ['inventory']
    })
    crossing.start('project_link_e2e', 'project')
    crossing.start('project_inventory_metadata_io', 'inventory')
    expect(() => crossing.stop('project_link_e2e', 'project')).toThrow(/crossing/)

    const duplicateStart = new BoundaryRecorder({
      project_link_e2e: ['project']
    })
    duplicateStart.start('project_link_e2e', 'project')
    expect(() => duplicateStart.start('project_link_e2e', 'project')).toThrow(/duplicate/)

    const duplicateStop = new BoundaryRecorder({
      project_link_e2e: ['project']
    })
    duplicateStop.start('project_link_e2e', 'project')
    duplicateStop.stop('project_link_e2e', 'project')
    expect(() => duplicateStop.stop('project_link_e2e', 'project')).toThrow(
      /duplicate or unmatched/
    )
  })

  test('rejects wrong occurrence order and missing occurrences', () => {
    const wrong = new BoundaryRecorder({
      definition_pre_extraction_admission: ['a.json', 'b.json']
    })
    expect(() => wrong.start('definition_pre_extraction_admission', 'b.json')).toThrow(
      /wrong occurrence order/
    )

    const missing = new BoundaryRecorder({
      definition_pre_extraction_admission: ['a.json']
    })
    expect(() => missing.finish()).toThrow(/missing/)
  })
})

test('result schema accepts the exact required matrix without performance thresholds', () => {
  const result = validResult()
  expect(() => assertValidMessageBenchmarkResult(result)).not.toThrow()
  expect(
    result.results.some(
      record => record.metric === 'duration' && record.elapsedMs === 0 && record.checksum === 0
    )
  ).toBe(true)
  expect(
    result.results.some(
      record => record.metric === 'peak_live_memory' && record.peakLiveBytes === 0
    )
  ).toBe(true)
})

test('result schema permits one declared observational case without replacing a requirement', () => {
  const result = validResult()
  const observational = {
    ...result.results.find(record => record.phase === 'message_project_input_io')
  }
  const fixture = fixtures.profiles.find(profile => profile.shape === 'exact_reference_dense')
  Object.assign(observational, {
    fixture: fixture.name,
    fixtureRevision: fixture.revision,
    variant: 'exact_reference_dense',
    scale: fixture.scales[0]
  })
  result.results.push(observational)
  expect(() => assertValidMessageBenchmarkResult(result)).not.toThrow()

  result.results.push({ ...observational })
  expect(() => assertValidMessageBenchmarkResult(result)).toThrow(/duplicates case/)
})

test('result schema rejects missing, duplicate, mismatched and shared companions', () => {
  const missing = validResult()
  missing.results = missing.results.filter(record => record.phase !== 'resource_extract')
  expect(() => assertValidMessageBenchmarkResult(missing)).toThrow()

  const duplicate = validResult()
  duplicate.results.push({
    ...duplicate.results.find(record => record.phase === 'resource_extract')
  })
  expect(() => assertValidMessageBenchmarkResult(duplicate)).toThrow()

  const mismatched = validResult()
  mismatched.results.find(record => record.phase === 'resource_extract').variant = 'cache_miss'
  expect(() => assertValidMessageBenchmarkResult(mismatched)).toThrow()

  const sharedCompanion = [
    ...MESSAGE_BENCHMARK_E2E_EXTRACTION_COMPANIONS,
    {
      e2e: {
        ...MESSAGE_BENCHMARK_E2E_EXTRACTION_COMPANIONS[0].e2e,
        variant: 'second-e2e'
      },
      extraction: MESSAGE_BENCHMARK_E2E_EXTRACTION_COMPANIONS[0].extraction
    }
  ]
  expect(() => assertValidCompanionMappings(sharedCompanion)).toThrow(/shares/)
})

test('checked registries are not serialized into benchmark results', () => {
  const result = validResult()
  result.boundaries = MESSAGE_BENCHMARK_BOUNDARIES
  expect(() => assertValidMessageBenchmarkResult(result)).toThrow(/boundaries is unknown/)
})

test('result schema rejects unknown top-level and metric-record fields', () => {
  const unknownTopLevel = validResult()
  unknownTopLevel.unversionedExtension = true
  expect(() => assertValidMessageBenchmarkResult(unknownTopLevel)).toThrow(
    /unversionedExtension is unknown/
  )

  const unknownDurationField = validResult()
  unknownDurationField.results.find(record => record.metric === 'duration').sampleCount = 1
  expect(() => assertValidMessageBenchmarkResult(unknownDurationField)).toThrow(
    /sampleCount is unknown/
  )

  const unknownMemoryField = validResult()
  unknownMemoryField.results.find(record => record.metric === 'peak_live_memory').checksum = 0
  expect(() => assertValidMessageBenchmarkResult(unknownMemoryField)).toThrow(/checksum is unknown/)

  const unknownOperation = validResult()
  unknownOperation.results.find(record => record.metric === 'duration').operation = 'custom'
  expect(() => assertValidMessageBenchmarkResult(unknownOperation)).toThrow(/operation mismatch/)
})

test('artifact payload size is active without an interval boundary', () => {
  expect(MESSAGE_BENCHMARK_NON_INTERVALS).toEqual([
    {
      owner: '014',
      descriptor: {
        phase: 'message_export_artifact_size',
        cost: 'payload_size_comparison',
        metric: 'artifact_payload_size'
      }
    }
  ])
  expect(
    MESSAGE_BENCHMARK_BOUNDARIES.some(
      entry => entry.descriptor.phase === 'message_export_artifact_size'
    )
  ).toBe(false)
})

test('export, registration, and emit fixtures retain three increasing scales and closed variants', () => {
  const byShape = new Map(fixtures.profiles.map(profile => [profile.shape, profile]))
  expect(byShape.get('message_export_esm')).toMatchObject({
    revision: 1,
    scales: [4, 16, 64],
    variants: [
      'all_definitions_baseline',
      'linked_output',
      'all_definitions_baseline_vs_linked_output'
    ],
    benchmarkContractRevision: 1,
    groupingMode: 'exporter_associations'
  })
  for (const shape of ['message_output_registration', 'messages_emit']) {
    expect(byShape.get(shape)).toMatchObject({
      revision: 1,
      scales: [4, 16, 64],
      variants: ['write_absent', 'write_unchanged', 'check_matched', 'check_different']
    })
  }
})

test('artifact comparison reconciles exporter observations, fingerprints, roots, and buckets', () => {
  const malformedFingerprint = validResult()
  artifactComparison(malformedFingerprint).baselineArtifacts[0].payloadFingerprint.digest = 'ABC'
  expect(() => assertValidMessageBenchmarkResult(malformedFingerprint)).toThrow(/BLAKE3-256/)

  const mismatchedExporter = validResult()
  artifactComparison(mismatchedExporter).baselineArtifacts[0].payloadBytes += 1
  expect(() => assertValidMessageBenchmarkResult(mismatchedExporter)).toThrow(
    /baselineArtifacts mismatch/
  )

  const unresolvedRelationship = validResult()
  exportArtifactRecord(
    unresolvedRelationship,
    'all_definitions_baseline'
  ).artifacts[1].relationships[0].target = ['missing.mjs']
  expect(() => assertValidMessageBenchmarkResult(unresolvedRelationship)).toThrow(/unresolved/)

  const wrongBucket = validResult()
  artifactComparison(wrongBucket).buckets[0].linkedBytes += 1
  expect(() => assertValidMessageBenchmarkResult(wrongBucket)).toThrow(/buckets mismatch/)

  const boundaryOnSize = validResult()
  artifactComparison(boundaryOnSize).boundaryId = 'payload_size_comparison'
  expect(() => assertValidMessageBenchmarkResult(boundaryOnSize)).toThrow(/boundaryId is forbidden/)
})

test('each emit E2E case has one distinct imported extraction companion', () => {
  const emitMappings = MESSAGE_BENCHMARK_E2E_EXTRACTION_COMPANIONS.filter(
    mapping => mapping.e2e.fixture === 'messages-emit'
  )
  expect(emitMappings).toHaveLength(12)
  expect(new Set(emitMappings.map(mapping => JSON.stringify(mapping.extraction))).size).toBe(12)
})

function artifactComparison(result) {
  return result.results.find(record => record.metric === 'artifact_payload_size')
}

function exportArtifactRecord(result, variant) {
  return result.results.find(
    record =>
      record.phase === 'message_export_esm' &&
      record.cost === 'export_artifact_set_construction' &&
      record.variant === variant
  )
}

function validResult() {
  const fixtureByName = new Map([
    ...fixtures.profiles.map(profile => [profile.name, profile]),
    [fixtures.projectFixture.name, fixtures.projectFixture]
  ])
  return {
    schemaVersion: MESSAGE_BENCH_RESULT_SCHEMA_VERSION,
    benchmarkProfileRevision: MESSAGE_BENCHMARK_PROFILE_REVISION,
    tool: 'intlify-messages-bench',
    version: '0.0.0',
    generatedAt: '2026-01-01T00:00:00.000Z',
    iterations: 1,
    warmupIterations: 0,
    phases: MESSAGE_BENCHMARK_PHASES,
    fixtures,
    results: MESSAGE_BENCHMARK_REQUIRED_CASES.map(required => {
      const fixture = fixtureByName.get(required.fixture)
      const descriptor = [...MESSAGE_BENCHMARK_BOUNDARIES, ...MESSAGE_BENCHMARK_NON_INTERVALS].find(
        entry =>
          entry.descriptor.phase === required.phase && entry.descriptor.cost === required.cost
      ).descriptor
      const record = {
        status: 'measured',
        ...required,
        fixtureRevision: fixture.revision,
        inputCount: 0,
        outputCount: 0
      }
      if (descriptor.boundaryId) {
        record.boundaryId = descriptor.boundaryId
      }
      if (required.metric === 'duration') {
        Object.assign(record, { iterations: 1, elapsedMs: 0, checksum: 0 })
      } else if (required.metric === 'peak_live_memory') {
        Object.assign(record, {
          peakLiveBytes: 0,
          retainedLiveBytes: 0,
          allocationCount: 0
        })
      } else {
        const observation = fixtureArtifactObservation(fixture)
        Object.assign(record, {
          baselineArtifacts: observation.artifacts,
          linkedArtifacts: observation.artifacts,
          baselineAssociations: observation.associations,
          linkedAssociations: observation.associations,
          entryRoots: fixture.entryRoots,
          buckets: observation.buckets
        })
      }
      if (
        required.phase === 'message_export_esm' &&
        required.cost === 'export_artifact_set_construction'
      ) {
        record.artifacts = fixtureArtifactObservation(fixture).artifacts
      }
      if (required.phase === 'resource_extract') {
        Object.assign(record, {
          physicalGroupCount: 1,
          hostByteCount: 1,
          entryCount: 0
        })
      }
      return record
    })
  }
}

function fixtureArtifactObservation(fixture) {
  const accessorPath = fixture.entryRoots[1]
  const enPath = ['locales', 'en.mjs']
  const jaPath = ['locales', 'ja.mjs']
  const loaderPath = fixture.entryRoots[0]
  const fingerprint = { algorithm: 'blake3-256', digest: '0'.repeat(64) }
  const artifact = (path, kind, relationships = []) => ({
    path,
    kind,
    formatVersion: { major: 0, minor: 1 },
    payloadBytes: 1,
    payloadFingerprint: fingerprint,
    relationships
  })
  const artifacts = [
    artifact(accessorPath, 'dev.intlify/typescript-accessor'),
    artifact(loaderPath, 'dev.intlify/loader-map', [
      { kind: 'eager-load', target: enPath },
      { kind: 'lazy-load', target: jaPath }
    ]),
    artifact(enPath, 'dev.intlify/esm-module'),
    artifact(jaPath, 'dev.intlify/esm-module')
  ]
  const shared = { kind: 'shared' }
  const main = { kind: 'unit', segments: ['main'] }
  const associations = [
    { path: accessorPath, locale: shared, deliveryUnit: shared },
    { path: loaderPath, locale: shared, deliveryUnit: main },
    { path: enPath, locale: { kind: 'locale', value: 'en' }, deliveryUnit: main },
    { path: jaPath, locale: { kind: 'locale', value: 'ja' }, deliveryUnit: main }
  ]
  const bucket = (axis, identity, bytes) => ({
    axis,
    identity,
    baselineBytes: bytes,
    linkedBytes: bytes,
    difference: { direction: 'equal', bytes: 0 },
    ratio: { state: 'defined', numerator: bytes, denominator: bytes }
  })
  const buckets = [
    bucket('complete-set', { kind: 'all' }, 4),
    bucket('initial-eager-load', { kind: 'entry-root-closure' }, 3),
    bucket('locale', { kind: 'locale', value: 'en' }, 1),
    bucket('locale', { kind: 'locale', value: 'ja' }, 1),
    bucket('locale', shared, 2),
    bucket('delivery-unit', main, 3),
    bucket('delivery-unit', shared, 1),
    bucket('kind', { kind: 'artifact-kind', value: 'dev.intlify/esm-module' }, 2),
    bucket('kind', { kind: 'artifact-kind', value: 'dev.intlify/loader-map' }, 1),
    bucket('kind', { kind: 'artifact-kind', value: 'dev.intlify/typescript-accessor' }, 1)
  ]
  return { artifacts, associations, buckets }
}
