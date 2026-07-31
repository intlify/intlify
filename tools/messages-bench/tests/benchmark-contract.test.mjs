import { readFileSync } from 'node:fs'

import { describe, expect, test } from 'vite-plus/test'

import {
  BoundaryRecorder,
  MESSAGE_BENCHMARK_BOUNDARIES,
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
import { assertValidMessageBenchmarkResult } from '../result-schema.mjs'
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
  expect(MESSAGE_BENCHMARK_BOUNDARIES).toHaveLength(
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

function validResult() {
  const fixtureByName = new Map([
    ...fixtures.profiles.map(profile => [profile.name, profile]),
    [fixtures.projectFixture.name, fixtures.projectFixture]
  ])
  return {
    schemaVersion: '0',
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
      const descriptor = MESSAGE_BENCHMARK_BOUNDARIES.find(
        entry =>
          entry.descriptor.phase === required.phase && entry.descriptor.cost === required.cost
      ).descriptor
      const record = {
        status: 'measured',
        ...required,
        boundaryId: descriptor.boundaryId,
        fixtureRevision: fixture.revision,
        inputCount: 0,
        outputCount: 0
      }
      if (required.metric === 'duration') {
        Object.assign(record, { iterations: 1, elapsedMs: 0, checksum: 0 })
      } else {
        Object.assign(record, {
          peakLiveBytes: 0,
          retainedLiveBytes: 0,
          allocationCount: 0
        })
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
