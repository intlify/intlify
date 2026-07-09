import {
  FORMAT_BENCHMARK_COST_NAMES,
  FORMAT_BENCHMARK_PHASE_NAMES,
  FORMAT_BENCHMARK_PHASES
} from './benchmark-phases.mjs'

export const FORMAT_BENCH_RESULT_SCHEMA_VERSION = '0'

/**
 * Validate the local formatter benchmark result shape.
 *
 * The benchmark numbers are observational, so this validator checks only
 * structural integrity and the required phase/cost vocabulary.
 *
 * @param value - Parsed benchmark result.
 */
export function assertValidFormatBenchmarkResult(value) {
  assertObject(value, 'result')
  assertEqual(value.schemaVersion, FORMAT_BENCH_RESULT_SCHEMA_VERSION, '/schemaVersion')
  assertEqual(value.tool, 'intlify-format-bench', '/tool')
  assertString(value.version, '/version')
  assertString(value.generatedAt, '/generatedAt')
  assertPositiveInteger(value.iterations, '/iterations')
  assertPhaseTable(value.phases)
  assertFixtures(value.fixtures)
  assertResults(value.results)
}

function assertPhaseTable(phases) {
  assertArray(phases, '/phases')
  assertEqual(JSON.stringify(phases), JSON.stringify(FORMAT_BENCHMARK_PHASES), '/phases')
}

function assertFixtures(fixtures) {
  assertArray(fixtures, '/fixtures')
  if (fixtures.length === 0) {
    throw new Error('/fixtures must contain at least one fixture')
  }
  for (const [index, fixture] of fixtures.entries()) {
    const pointer = `/fixtures/${index}`
    assertObject(fixture, pointer)
    assertString(fixture.name, `${pointer}/name`)
    assertString(fixture.path, `${pointer}/path`)
    assertNonNegativeInteger(fixture.inputBytes, `${pointer}/inputBytes`)
  }
}

function assertResults(results) {
  assertArray(results, '/results')
  if (results.length === 0) {
    throw new Error('/results must contain at least one measurement or skip record')
  }

  const phaseNames = new Set(FORMAT_BENCHMARK_PHASE_NAMES)
  const costNames = new Set(FORMAT_BENCHMARK_COST_NAMES)
  const phaseCostMap = new Map(
    FORMAT_BENCHMARK_PHASES.map(phase => [phase.name, new Set(phase.costs)])
  )
  for (const [index, result] of results.entries()) {
    const pointer = `/results/${index}`
    assertObject(result, pointer)
    assertString(result.phase, `${pointer}/phase`)
    assertString(result.cost, `${pointer}/cost`)
    assertString(result.fixture, `${pointer}/fixture`)
    assertString(result.runtime, `${pointer}/runtime`)
    assertString(result.operation, `${pointer}/operation`)
    assertEqual(
      phaseNames.has(result.phase),
      true,
      `${pointer}/phase must be a formatter benchmark phase`
    )
    assertEqual(
      costNames.has(result.cost),
      true,
      `${pointer}/cost must be a formatter benchmark cost`
    )
    assertEqual(
      phaseCostMap.get(result.phase)?.has(result.cost) ?? false,
      true,
      `${pointer}/cost must be a valid cost for phase "${result.phase}"`
    )

    if (result.status === 'skipped') {
      assertString(result.reason, `${pointer}/reason`)
      continue
    }

    assertEqual(result.status, 'measured', `${pointer}/status`)
    assertPositiveInteger(result.iterations, `${pointer}/iterations`)
    assertNonNegativeNumber(result.elapsedMs, `${pointer}/elapsedMs`)
    assertNonNegativeInteger(result.checksum, `${pointer}/checksum`)
    assertNonNegativeInteger(result.inputBytes, `${pointer}/inputBytes`)
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

function assertPositiveInteger(value, pointer) {
  if (!Number.isInteger(value) || value < 1) {
    throw new Error(`${pointer} must be a positive integer`)
  }
}

function assertNonNegativeInteger(value, pointer) {
  if (!Number.isInteger(value) || value < 0) {
    throw new Error(`${pointer} must be a non-negative integer`)
  }
}

function assertNonNegativeNumber(value, pointer) {
  if (typeof value !== 'number' || !Number.isFinite(value) || value < 0) {
    throw new Error(`${pointer} must be a non-negative number`)
  }
}

function assertEqual(actual, expected, pointer) {
  if (actual !== expected) {
    throw new Error(`${pointer} mismatch: expected ${expected}, got ${actual}`)
  }
}
