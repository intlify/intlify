import { RESOURCE_BENCHMARK_PHASES } from './benchmark-phases.mjs'

export const RESOURCE_BENCH_RESULT_SCHEMA_VERSION = '0'

/**
 * Validate the local resource benchmark result shape.
 *
 * Timing values are observational. This validator gates phase coverage,
 * structural integrity, and the resource memory-growth checks only.
 *
 * @param value - Parsed benchmark result.
 */
export function assertValidResourceBenchmarkResult(value) {
  assertObject(value, 'result')
  assertEqual(value.schemaVersion, RESOURCE_BENCH_RESULT_SCHEMA_VERSION, '/schemaVersion')
  assertEqual(value.tool, 'intlify-resource-bench', '/tool')
  assertString(value.version, '/version')
  assertString(value.generatedAt, '/generatedAt')
  assertPositiveInteger(value.iterations, '/iterations')
  assertEqual(JSON.stringify(value.phases), JSON.stringify(RESOURCE_BENCHMARK_PHASES), '/phases')
  const fixtureMetadata = assertFixtures(value.fixtures)
  const measuredCoreMemory = assertResults(value.results, fixtureMetadata)
  assertMemoryGrowthChecks(value.memoryGrowthChecks, measuredCoreMemory)
}

function assertFixtures(fixtures) {
  assertArray(fixtures, '/fixtures')
  if (fixtures.length < 3) {
    throw new Error('/fixtures must contain generated profiles and a catalog fixture')
  }
  const names = new Set()
  const shapes = new Set()
  const byName = new Map()
  const expectedCoreMemory = new Map()
  let catalogFixtureCount = 0
  for (const [index, fixture] of fixtures.entries()) {
    const pointer = `/fixtures/${index}`
    assertObject(fixture, pointer)
    assertString(fixture.name, `${pointer}/name`)
    if (names.has(fixture.name)) {
      throw new Error(`${pointer}/name must be unique`)
    }
    names.add(fixture.name)
    if (fixture.kind === 'generated_profile') {
      assertString(fixture.shape, `${pointer}/shape`)
      shapes.add(fixture.shape)
      assertArray(fixture.scales, `${pointer}/scales`)
      if (fixture.scales.length < 3) {
        throw new Error(`${pointer}/scales must contain at least three samples`)
      }
      for (const [scaleIndex, scale] of fixture.scales.entries()) {
        assertPositiveInteger(scale, `${pointer}/scales/${scaleIndex}`)
        if (scaleIndex > 0 && fixture.scales[scaleIndex - 1] >= scale) {
          throw new Error(`${pointer}/scales must be strictly increasing`)
        }
      }
      for (const variant of ['original', 'candidate']) {
        expectedCoreMemory.set(`${fixture.name}\0${variant}`, new Set(fixture.scales))
      }
      byName.set(fixture.name, {
        kind: fixture.kind,
        scales: new Set(fixture.scales)
      })
      continue
    }
    assertEqual(fixture.kind, 'catalog_file', `${pointer}/kind`)
    catalogFixtureCount += 1
    assertString(fixture.path, `${pointer}/path`)
    assertPositiveInteger(fixture.inputBytes, `${pointer}/inputBytes`)
    byName.set(fixture.name, { kind: fixture.kind })
  }
  for (const shape of ['message_dense', 'structurally_dense_few_message']) {
    if (!shapes.has(shape)) {
      throw new Error(`/fixtures is missing generated profile shape ${shape}`)
    }
  }
  if (catalogFixtureCount === 0) {
    throw new Error('/fixtures is missing a catalog fixture')
  }
  return { byName, expectedCoreMemory }
}

function assertResults(results, fixtureMetadata) {
  assertArray(results, '/results')
  if (results.length === 0) {
    throw new Error('/results must contain measurements or skip records')
  }

  const allowed = new Map(
    RESOURCE_BENCHMARK_PHASES.map(phase => [phase.name, new Set(phase.costs)])
  )
  const observed = new Set()
  const measuredCoreMemory = new Map()

  for (const [index, result] of results.entries()) {
    const pointer = `/results/${index}`
    assertObject(result, pointer)
    assertString(result.phase, `${pointer}/phase`)
    assertString(result.cost, `${pointer}/cost`)
    assertString(result.fixture, `${pointer}/fixture`)
    assertString(result.variant, `${pointer}/variant`)
    assertString(result.runtime, `${pointer}/runtime`)
    assertString(result.operation, `${pointer}/operation`)
    const costs = allowed.get(result.phase)
    if (!costs) {
      throw new Error(`${pointer}/phase must be a resource benchmark phase`)
    }
    if (!costs.has(result.cost)) {
      throw new Error(`${pointer}/cost must be valid for phase "${result.phase}"`)
    }
    const fixture = fixtureMetadata.byName.get(result.fixture)
    if (!fixture) {
      throw new Error(`${pointer}/fixture must reference a declared fixture`)
    }
    observed.add(`${result.phase}\0${result.cost}`)

    if (result.status === 'skipped') {
      assertString(result.reason, `${pointer}/reason`)
      continue
    }

    assertEqual(result.status, 'measured', `${pointer}/status`)
    assertPositiveInteger(result.scale, `${pointer}/scale`)
    if (fixture.kind === 'generated_profile' && !fixture.scales.has(result.scale)) {
      throw new Error(`${pointer}/scale is not declared by generated fixture "${result.fixture}"`)
    }
    assertPositiveInteger(result.inputBytes, `${pointer}/inputBytes`)
    assertNonNegativeInteger(result.entryCount, `${pointer}/entryCount`)
    if (result.metric === 'duration') {
      assertPositiveInteger(result.iterations, `${pointer}/iterations`)
      assertNonNegativeNumber(result.elapsedMs, `${pointer}/elapsedMs`)
      assertNonNegativeSafeInteger(result.checksum, `${pointer}/checksum`)
      continue
    }
    assertEqual(result.metric, 'peak_live_memory', `${pointer}/metric`)
    assertPositiveInteger(result.peakLiveBytes, `${pointer}/peakLiveBytes`)
    assertNonNegativeInteger(result.retainedLiveBytes, `${pointer}/retainedLiveBytes`)
    assertPositiveInteger(result.allocationCount, `${pointer}/allocationCount`)
    if (result.phase === 'resource_extract_peak_memory') {
      const key = `${result.fixture}\0${result.variant}`
      const expectedScales = fixtureMetadata.expectedCoreMemory.get(key)
      if (!expectedScales) {
        throw new Error(`${pointer} does not match a declared extraction-memory fixture/variant`)
      }
      if (!expectedScales.has(result.scale)) {
        throw new Error(`${pointer}/scale is not declared by its fixture`)
      }
      const measuredScales = measuredCoreMemory.get(key) ?? new Set()
      if (measuredScales.has(result.scale)) {
        throw new Error(`${pointer} duplicates an extraction-memory scale`)
      }
      measuredScales.add(result.scale)
      measuredCoreMemory.set(key, measuredScales)
    }
  }

  for (const phase of RESOURCE_BENCHMARK_PHASES) {
    for (const cost of phase.costs) {
      if (!observed.has(`${phase.name}\0${cost}`)) {
        throw new Error(`/results is missing ${phase.name}/${cost}`)
      }
    }
  }
  if (measuredCoreMemory.size > 0) {
    for (const [key, expectedScales] of fixtureMetadata.expectedCoreMemory) {
      const measuredScales = measuredCoreMemory.get(key)
      if (!measuredScales || measuredScales.size !== expectedScales.size) {
        throw new Error(
          `/results is missing extraction-memory samples for ${key.replace('\0', '/')}`
        )
      }
    }
  }
  return new Map([...measuredCoreMemory].map(([key, scales]) => [key, scales.size]))
}

function assertMemoryGrowthChecks(checks, measuredSamples) {
  assertArray(checks, '/memoryGrowthChecks')
  if (measuredSamples.size > 0 && checks.length === 0) {
    throw new Error('/memoryGrowthChecks must accompany measured extraction memory')
  }
  const observed = new Set()
  for (const [index, check] of checks.entries()) {
    const pointer = `/memoryGrowthChecks/${index}`
    assertObject(check, pointer)
    assertString(check.fixture, `${pointer}/fixture`)
    assertString(check.variant, `${pointer}/variant`)
    assertPositiveInteger(check.sampleCount, `${pointer}/sampleCount`)
    if (check.sampleCount < 3) {
      throw new Error(`${pointer}/sampleCount must be at least three`)
    }
    assertNonNegativeNumber(check.maxNormalizedStepGrowth, `${pointer}/maxNormalizedStepGrowth`)
    assertPositiveNumber(check.limit, `${pointer}/limit`)
    assertEqual(check.status, 'passed', `${pointer}/status`)
    if (check.maxNormalizedStepGrowth > check.limit) {
      throw new Error(`${pointer} exceeds its near-linear memory-growth limit`)
    }
    const key = `${check.fixture}\0${check.variant}`
    if (observed.has(key)) {
      throw new Error(`${pointer} duplicates a fixture/variant check`)
    }
    const expectedSamples = measuredSamples.get(key)
    if (expectedSamples == null) {
      throw new Error(`${pointer} has no measured extraction-memory samples`)
    }
    if (check.sampleCount !== expectedSamples) {
      throw new Error(
        `${pointer}/sampleCount mismatch: expected ${expectedSamples}, got ${check.sampleCount}`
      )
    }
    observed.add(key)
  }
  for (const key of measuredSamples.keys()) {
    if (!observed.has(key)) {
      throw new Error(`/memoryGrowthChecks is missing ${key.replace('\0', '/')}`)
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

function assertNonNegativeSafeInteger(value, pointer) {
  assertNonNegativeInteger(value, pointer)
}

function assertPositiveNumber(value, pointer) {
  if (typeof value !== 'number' || !Number.isFinite(value) || value <= 0) {
    throw new Error(`${pointer} must be a positive number`)
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
