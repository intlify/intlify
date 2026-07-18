import { mkdtemp, readFile, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'

import { expect, test } from 'vite-plus/test'

import { RESOURCE_BENCHMARK_PHASE_NAMES, RESOURCE_BENCHMARK_PHASES } from '../benchmark-phases.mjs'
import { assertValidResourceBenchmarkResult } from '../result-schema.mjs'

test('resource benchmark phases retain the resource and catalog acceptance vocabulary', () => {
  expect(RESOURCE_BENCHMARK_PHASE_NAMES).toEqual([
    'resource_extract',
    'resource_extract_peak_memory',
    'resource_write_back',
    'fmt_catalog_output_admission_peak_memory',
    'fmt_catalog_check_e2e',
    'fmt_catalog_write_e2e',
    'sequential_physical_group_aggregation'
  ])
})

test('result schema accepts complete phase coverage and memory checks', () => {
  expect(() => assertValidResourceBenchmarkResult(validResult())).not.toThrow()
})

test('result schema rejects a missing write-back cost', () => {
  const result = validResult()
  result.results = result.results.filter(
    measurement => measurement.cost !== 'candidate_reparse_and_validation'
  )

  expect(() => assertValidResourceBenchmarkResult(result)).toThrow(
    '/results is missing resource_write_back/candidate_reparse_and_validation'
  )
})

test('result schema rejects a failed near-linear memory check', () => {
  const result = validResult()
  result.memoryGrowthChecks[0].maxNormalizedStepGrowth = 3

  expect(() => assertValidResourceBenchmarkResult(result)).toThrow(
    '/memoryGrowthChecks/0 exceeds its near-linear memory-growth limit'
  )
})

test('result schema rejects missing extraction memory for a declared generated profile', () => {
  const result = validResult()
  result.results = result.results.filter(
    measurement =>
      measurement.phase !== 'resource_extract_peak_memory' ||
      measurement.fixture !== 'structurally-dense-few-message'
  )

  expect(() => assertValidResourceBenchmarkResult(result)).toThrow(
    '/results is missing extraction-memory samples for structurally-dense-few-message/original'
  )
})

test('result schema rejects a result that references an undeclared fixture', () => {
  const result = validResult()
  result.results.find(measurement => measurement.phase === 'fmt_catalog_check_e2e').fixture =
    'missing-catalog'

  expect(() => assertValidResourceBenchmarkResult(result)).toThrow(
    '/fixture must reference a declared fixture'
  )
})

test('result schema rejects a generated-profile timing at an undeclared scale', () => {
  const result = validResult()
  result.results.find(measurement => measurement.phase === 'resource_extract').scale = 4

  expect(() => assertValidResourceBenchmarkResult(result)).toThrow(
    '/scale is not declared by generated fixture "message-dense"'
  )
})

test('timing values remain observational rather than threshold gates', () => {
  const result = validResult()
  const timing = result.results.find(measurement => measurement.metric === 'duration')
  timing.elapsedMs = Number.MAX_SAFE_INTEGER

  expect(() => assertValidResourceBenchmarkResult(result)).not.toThrow()
})

test('benchmark command reads fixtures and emits a schema-valid result with unavailable binaries', async () => {
  const tempDir = await mkdtemp(join(tmpdir(), 'intlify-resource-bench-test-'))
  try {
    const out = join(tempDir, 'result.json')
    const missingCoreBinary = join(tempDir, 'missing-resource-bench')
    const missingCliBinary = join(tempDir, 'missing-intlify')
    const result = spawnSync(
      process.execPath,
      ['scripts/run.mjs', '--skip-build', '--allow-skips', '--iterations', '1', '--out', out],
      {
        cwd: fileURLToPath(new URL('..', import.meta.url)),
        encoding: 'utf8',
        env: {
          ...process.env,
          INTLIFY_RESOURCE_BENCH_CORE_BINARY: missingCoreBinary,
          INTLIFY_RESOURCE_BENCH_CLI_BINARY: missingCliBinary
        }
      }
    )

    expect(result.status).toBe(0)
    const benchmark = JSON.parse(await readFile(out, 'utf8'))
    assertValidResourceBenchmarkResult(benchmark)
    expect(benchmark.results.every(measurement => measurement.status === 'skipped')).toBe(true)
    expect(new Set(benchmark.results.map(measurement => measurement.reason))).toEqual(
      new Set([
        `missing resource benchmark binary at ${missingCoreBinary}`,
        `missing CLI binary at ${missingCliBinary}`
      ])
    )
  } finally {
    await rm(tempDir, { recursive: true, force: true })
  }
})

function validResult() {
  const results = RESOURCE_BENCHMARK_PHASES.flatMap(phase =>
    phase.costs.map(cost => {
      const memory =
        phase.name === 'resource_extract_peak_memory' ||
        cost === 'combined_formatter_output_and_admission'
      const variant =
        phase.name === 'resource_extract_peak_memory'
          ? cost === 'original_extraction'
            ? 'original'
            : 'candidate'
          : 'fixture'
      return {
        status: 'measured',
        phase: phase.name,
        cost,
        fixture: phase.name.startsWith('resource_') ? 'message-dense' : 'catalog-format',
        variant,
        runtime: phase.name.startsWith('resource_') ? 'intlify-resource-bench-rs' : 'intlify',
        operation: cost,
        scale: 3,
        inputBytes: 1024,
        entryCount: 3,
        metric: memory ? 'peak_live_memory' : 'duration',
        ...(memory
          ? {
              peakLiveBytes: 2048,
              retainedLiveBytes: 1024,
              allocationCount: 5
            }
          : {
              iterations: 1,
              elapsedMs: 0.1,
              checksum: 1
            })
      }
    })
  )
  const messageDenseMemory = results.filter(
    measurement => measurement.phase === 'resource_extract_peak_memory'
  )
  results.push(
    ...messageDenseMemory.flatMap(measurement => [
      { ...measurement, scale: 6, inputBytes: 2048, peakLiveBytes: 4096 },
      { ...measurement, scale: 12, inputBytes: 4096, peakLiveBytes: 8192 }
    ]),
    ...messageDenseMemory.flatMap(measurement => [
      {
        ...measurement,
        fixture: 'structurally-dense-few-message',
        scale: 3,
        entryCount: 1
      },
      {
        ...measurement,
        fixture: 'structurally-dense-few-message',
        scale: 6,
        inputBytes: 2048,
        peakLiveBytes: 4096,
        entryCount: 1
      },
      {
        ...measurement,
        fixture: 'structurally-dense-few-message',
        scale: 12,
        inputBytes: 4096,
        peakLiveBytes: 8192,
        entryCount: 1
      }
    ])
  )
  return {
    schemaVersion: '0',
    tool: 'intlify-resource-bench',
    version: '0.14.0',
    generatedAt: '2026-01-01T00:00:00.000Z',
    iterations: 1,
    phases: RESOURCE_BENCHMARK_PHASES,
    fixtures: [
      {
        name: 'message-dense',
        kind: 'generated_profile',
        shape: 'message_dense',
        scales: [3, 6, 12]
      },
      {
        name: 'structurally-dense-few-message',
        kind: 'generated_profile',
        shape: 'structurally_dense_few_message',
        scales: [3, 6, 12]
      },
      {
        name: 'catalog-format',
        kind: 'catalog_file',
        path: 'tools/resource-bench/fixtures/catalog-format.json',
        inputBytes: 100
      }
    ],
    results,
    memoryGrowthChecks: ['message-dense', 'structurally-dense-few-message'].flatMap(fixture =>
      ['original', 'candidate'].map(variant => ({
        fixture,
        variant,
        sampleCount: 3,
        maxNormalizedStepGrowth: 1.1,
        limit: 2.5,
        status: 'passed'
      }))
    )
  }
}
