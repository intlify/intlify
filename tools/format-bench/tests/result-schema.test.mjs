import { mkdtemp, readFile, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'

import { expect, test } from 'vite-plus/test'

import { FORMAT_BENCHMARK_PHASE_NAMES, FORMAT_BENCHMARK_PHASES } from '../benchmark-phases.mjs'
import { assertValidFormatBenchmarkResult } from '../result-schema.mjs'

test('formatter benchmark phases match the Phase 3B design names', () => {
  expect(FORMAT_BENCHMARK_PHASE_NAMES).toEqual([
    'format_standard',
    'format_preserve',
    'format_check_cli_e2e',
    'format_check_json',
    'e2e_format'
  ])
})

test('result schema accepts the benchmark command output shape', () => {
  expect(() => assertValidFormatBenchmarkResult(validResult())).not.toThrow()
})

test('result schema rejects malformed phase names', () => {
  const result = validResult()
  result.results[0].phase = 'format_unknown'

  expect(() => assertValidFormatBenchmarkResult(result)).toThrow(
    '/results/0/phase must be a formatter benchmark phase'
  )
})

test('result schema rejects malformed cost names', () => {
  const result = validResult()
  result.results[0].cost = 'made_up_cost'

  expect(() => assertValidFormatBenchmarkResult(result)).toThrow(
    '/results/0/cost must be a formatter benchmark cost'
  )
})

test('result schema rejects phase and cost mismatches', () => {
  const result = validResult()
  result.results[0].phase = 'format_check_cli_e2e'
  result.results[0].cost = 'napi_binding_call'

  expect(() => assertValidFormatBenchmarkResult(result)).toThrow(
    '/results/0/cost must be a valid cost for phase "format_check_cli_e2e"'
  )
})

test('result schema rejects missing warmup iterations', () => {
  const result = validResult()
  delete result.warmupIterations

  expect(() => assertValidFormatBenchmarkResult(result)).toThrow(
    '/warmupIterations must be a positive integer'
  )
})

test('result schema rejects malformed cold-start stages', () => {
  const result = validResult()
  result.coldStartResults[0].stage = 'unknown'

  expect(() => assertValidFormatBenchmarkResult(result)).toThrow(
    '/coldStartResults/0/stage must be a cold-start stage'
  )
})

test('benchmark command reads fixtures and emits a schema-valid result', async () => {
  const tempDir = await mkdtemp(join(tmpdir(), 'intlify-format-bench-test-'))
  try {
    const out = join(tempDir, 'result.json')
    const result = spawnSync(
      process.execPath,
      [
        'scripts/run.mjs',
        '--skip-build',
        '--allow-skips',
        '--iterations',
        '1',
        '--warmup-iterations',
        '1',
        '--fixture',
        'basic-message',
        '--out',
        out
      ],
      {
        cwd: fileURLToPath(new URL('..', import.meta.url)),
        encoding: 'utf8'
      }
    )

    expect(result.status).toBe(0)
    assertValidFormatBenchmarkResult(JSON.parse(await readFile(out, 'utf8')))
  } finally {
    await rm(tempDir, { recursive: true, force: true })
  }
})

function validResult() {
  return {
    schemaVersion: '1',
    tool: 'intlify-format-bench',
    version: '0.14.0',
    generatedAt: '2026-01-01T00:00:00.000Z',
    iterations: 1,
    warmupIterations: 1,
    phases: FORMAT_BENCHMARK_PHASES,
    fixtures: [
      {
        name: 'basic-message',
        path: 'crates/intlify_format/fixtures/basic-message/input.mf2',
        inputBytes: 12
      }
    ],
    coldStartResults: [
      {
        status: 'measured',
        measurement: 'cold_start',
        stage: 'module_load',
        executionModel: 'in_process',
        runtime: '@intlify/format-napi',
        operation: 'import(@intlify/format-napi)',
        elapsedMs: 1.2,
        checksum: 0
      },
      {
        status: 'measured',
        measurement: 'cold_start',
        stage: 'first_call',
        executionModel: 'in_process',
        fixture: 'basic-message',
        runtime: '@intlify/format-napi',
        operation: 'formatMessage',
        elapsedMs: 0.4,
        checksum: 1,
        inputBytes: 12
      }
    ],
    results: [
      {
        status: 'measured',
        measurement: 'warm',
        executionModel: 'in_process',
        phase: 'format_standard',
        cost: 'napi_binding_call',
        fixture: 'basic-message',
        runtime: '@intlify/format-napi',
        operation: 'formatMessage',
        iterations: 1,
        elapsedMs: 0.1,
        checksum: 1,
        inputBytes: 12
      }
    ]
  }
}
