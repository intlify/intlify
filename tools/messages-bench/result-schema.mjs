import { readFileSync } from 'node:fs'

import {
  MESSAGE_BENCHMARK_BOUNDARIES,
  MESSAGE_BENCHMARK_PHASES,
  MESSAGE_BENCHMARK_OVERLAP_TOPOLOGY,
  assertValidBoundaryRegistry,
  resource_host_parse_and_entry_extraction
} from './benchmark-phases.mjs'
import {
  MESSAGE_BENCHMARK_E2E_EXTRACTION_COMPANIONS,
  MESSAGE_BENCHMARK_PROFILE_REVISION,
  MESSAGE_BENCHMARK_REQUIRED_CASES,
  assertValidCompanionMappings,
  benchmarkCaseKey
} from './benchmark-profile.mjs'
import { assertValidChecksumObservationRegistry } from './checksum-observations.mjs'

export const MESSAGE_BENCH_RESULT_SCHEMA_VERSION = '0'

const fixtureSelection = JSON.parse(
  readFileSync(new URL('./fixture-selection.json', import.meta.url), 'utf8')
)
const descriptorByPair = new Map(
  MESSAGE_BENCHMARK_BOUNDARIES.map(entry => [
    `${entry.descriptor.phase}\0${entry.descriptor.cost}`,
    entry.descriptor
  ])
)

/**
 * Validate one aggregate messages benchmark result.
 *
 * Performance magnitudes remain observational. This gate checks only the
 * closed profile, record structure, required-case coverage, and companion
 * integrity.
 *
 * @param value - Parsed benchmark result.
 */
export function assertValidMessageBenchmarkResult(value) {
  assertValidBoundaryRegistry()
  assertValidChecksumObservationRegistry()
  assertValidCompanionMappings()
  assertObject(value, 'result')
  assertExactFields(value, '/', [
    'schemaVersion',
    'benchmarkProfileRevision',
    'tool',
    'version',
    'generatedAt',
    'iterations',
    'warmupIterations',
    'phases',
    'fixtures',
    'results'
  ])
  assertEqual(value.schemaVersion, MESSAGE_BENCH_RESULT_SCHEMA_VERSION, '/schemaVersion')
  assertEqual(
    value.benchmarkProfileRevision,
    MESSAGE_BENCHMARK_PROFILE_REVISION,
    '/benchmarkProfileRevision'
  )
  assertEqual(value.tool, 'intlify-messages-bench', '/tool')
  assertString(value.version, '/version')
  assertString(value.generatedAt, '/generatedAt')
  assertPositiveInteger(value.iterations, '/iterations')
  assertNonNegativeInteger(value.warmupIterations, '/warmupIterations')
  assertEqual(JSON.stringify(value.phases), JSON.stringify(MESSAGE_BENCHMARK_PHASES), '/phases')
  const fixtures = assertFixtures(value.fixtures)
  const results = assertResults(value.results, fixtures, value.iterations)
  assertRequiredCases(results)
  assertExtractionCompanions(results)
}

function assertFixtures(value) {
  assertObject(value, '/fixtures')
  assertEqual(value.revision, fixtureSelection.revision, '/fixtures/revision')
  assertEqual(JSON.stringify(value), JSON.stringify(fixtureSelection), '/fixtures')
  const fixtures = new Map()
  for (const profile of fixtureSelection.profiles) {
    if (fixtures.has(profile.name)) {
      throw new Error('/fixtures contains duplicate names')
    }
    fixtures.set(profile.name, profile)
  }
  if (fixtures.has(fixtureSelection.projectFixture.name)) {
    throw new Error('/fixtures contains duplicate names')
  }
  fixtures.set(fixtureSelection.projectFixture.name, fixtureSelection.projectFixture)
  return fixtures
}

function assertResults(value, fixtures, defaultIterations) {
  assertArray(value, '/results')
  if (value.length === 0) {
    throw new Error('/results must not be empty')
  }
  const byCase = new Map()
  for (const [index, result] of value.entries()) {
    const pointer = `/results/${index}`
    assertObject(result, pointer)
    const commonFields = [
      'status',
      'phase',
      'cost',
      'boundaryId',
      'fixture',
      'fixtureRevision',
      'variant',
      'scale',
      'runtime',
      'executionModel',
      'operation',
      'metric',
      'inputCount',
      'outputCount'
    ]
    assertEqual(result.status, 'measured', `${pointer}/status`)
    for (const field of [
      'phase',
      'cost',
      'boundaryId',
      'fixture',
      'variant',
      'runtime',
      'executionModel',
      'operation',
      'metric'
    ]) {
      assertString(result[field], `${pointer}/${field}`)
    }
    assertEqual(result.executionModel, 'in_process', `${pointer}/executionModel`)
    assertPositiveInteger(result.fixtureRevision, `${pointer}/fixtureRevision`)
    assertPositiveInteger(result.scale, `${pointer}/scale`)
    assertNonNegativeInteger(result.inputCount, `${pointer}/inputCount`)
    assertNonNegativeInteger(result.outputCount, `${pointer}/outputCount`)
    const fixture = fixtures.get(result.fixture)
    if (!fixture) {
      throw new Error(`${pointer}/fixture is undeclared`)
    }
    assertEqual(result.fixtureRevision, fixture.revision, `${pointer}/fixtureRevision`)
    if (!fixture.scales.includes(result.scale)) {
      throw new Error(`${pointer}/scale is undeclared by its fixture`)
    }
    if (!fixture.variants.includes(result.variant)) {
      throw new Error(`${pointer}/variant is undeclared by its fixture`)
    }

    const descriptor = descriptorByPair.get(`${result.phase}\0${result.cost}`)
    if (!descriptor) {
      throw new Error(`${pointer} uses an inactive phase/cost`)
    }
    assertEqual(result.boundaryId, descriptor.boundaryId, `${pointer}/boundaryId`)
    assertEqual(result.operation, descriptor.boundaryId, `${pointer}/operation`)
    assertEqual(result.metric, descriptor.metric, `${pointer}/metric`)
    if (/_v\d+$/.test(result.boundaryId)) {
      throw new Error(`${pointer}/boundaryId must not carry a version suffix`)
    }

    if (result.metric === 'duration') {
      const durationFields = ['iterations', 'elapsedMs', 'checksum']
      const resourceFields =
        descriptor === resource_host_parse_and_entry_extraction
          ? ['physicalGroupCount', 'hostByteCount', 'entryCount']
          : []
      assertExactFields(result, pointer, [...commonFields, ...durationFields, ...resourceFields])
      assertPositiveInteger(result.iterations, `${pointer}/iterations`)
      assertEqual(result.iterations, defaultIterations, `${pointer}/iterations`)
      assertNonNegativeNumber(result.elapsedMs, `${pointer}/elapsedMs`)
      assertU32(result.checksum, `${pointer}/checksum`)
    } else {
      assertEqual(result.metric, 'peak_live_memory', `${pointer}/metric`)
      assertExactFields(result, pointer, [
        ...commonFields,
        'peakLiveBytes',
        'retainedLiveBytes',
        'allocationCount'
      ])
      assertNonNegativeInteger(result.peakLiveBytes, `${pointer}/peakLiveBytes`)
      assertNonNegativeInteger(result.retainedLiveBytes, `${pointer}/retainedLiveBytes`)
      assertNonNegativeInteger(result.allocationCount, `${pointer}/allocationCount`)
    }

    if (descriptor === resource_host_parse_and_entry_extraction) {
      assertPositiveInteger(result.physicalGroupCount, `${pointer}/physicalGroupCount`)
      assertPositiveInteger(result.hostByteCount, `${pointer}/hostByteCount`)
      assertNonNegativeInteger(result.entryCount, `${pointer}/entryCount`)
    }

    const key = benchmarkCaseKey(result)
    const records = byCase.get(key) ?? []
    records.push(result)
    byCase.set(key, records)
  }
  return byCase
}

function assertRequiredCases(byCase) {
  const requiredKeys = new Set()
  for (const required of MESSAGE_BENCHMARK_REQUIRED_CASES) {
    const key = benchmarkCaseKey(required)
    if (requiredKeys.has(key)) {
      throw new Error(`required-case matrix duplicates ${key}`)
    }
    requiredKeys.add(key)
    const records = byCase.get(key) ?? []
    if (records.length !== 1) {
      throw new Error(`/results must contain required case exactly once: ${key}`)
    }
  }
  for (const [key, records] of byCase) {
    if (records.length !== 1) {
      throw new Error(`/results duplicates case: ${key}`)
    }
  }
}

function assertExtractionCompanions(byCase) {
  const usedExtraction = new Set()
  for (const [index, mapping] of MESSAGE_BENCHMARK_E2E_EXTRACTION_COMPANIONS.entries()) {
    const e2eKey = benchmarkCaseKey(mapping.e2e)
    const extractionKey = benchmarkCaseKey(mapping.extraction)
    const e2e = byCase.get(e2eKey) ?? []
    const extraction = byCase.get(extractionKey) ?? []
    if (e2e.length !== 1 || extraction.length !== 1) {
      throw new Error(`/companions/${index} is missing or duplicated`)
    }
    if (usedExtraction.has(extractionKey)) {
      throw new Error(`/companions/${index} shares an extraction record`)
    }
    usedExtraction.add(extractionKey)
    for (const field of ['fixture', 'variant', 'scale', 'runtime', 'executionModel']) {
      if (e2e[0][field] !== extraction[0][field]) {
        throw new Error(`/companions/${index} mismatches ${field}`)
      }
    }
  }
}

function assertExactFields(value, pointer, allowedFields) {
  const allowed = new Set(allowedFields)
  for (const field of Object.keys(value)) {
    if (!allowed.has(field)) {
      const separator = pointer === '/' ? '' : '/'
      throw new Error(`${pointer}${separator}${field} is unknown`)
    }
  }
}

function assertObject(value, pointer) {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${pointer} must be an object`)
  }
}

function assertArray(value, pointer) {
  if (!Array.isArray(value)) {
    throw new Error(`${pointer} must be an array`)
  }
}

function assertString(value, pointer) {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${pointer} must be a non-empty string`)
  }
}

function assertEqual(actual, expected, pointer) {
  if (actual !== expected) {
    throw new Error(`${pointer} mismatch: expected ${JSON.stringify(expected)}`)
  }
}

function assertPositiveInteger(value, pointer) {
  if (!Number.isSafeInteger(value) || value < 1) {
    throw new Error(`${pointer} must be a positive safe integer`)
  }
}

function assertNonNegativeInteger(value, pointer) {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new Error(`${pointer} must be a non-negative safe integer`)
  }
}

function assertU32(value, pointer) {
  if (!Number.isSafeInteger(value) || value < 0 || value > 0xffff_ffff) {
    throw new Error(`${pointer} must be a u32`)
  }
}

function assertNonNegativeNumber(value, pointer) {
  if (typeof value !== 'number' || !Number.isFinite(value) || value < 0) {
    throw new Error(`${pointer} must be a non-negative finite number`)
  }
}

void MESSAGE_BENCHMARK_OVERLAP_TOPOLOGY
