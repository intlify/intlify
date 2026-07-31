import { spawnSync } from 'node:child_process'
import { existsSync } from 'node:fs'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { parseArgs } from 'node:util'

import { MESSAGE_BENCHMARK_PHASES } from '../benchmark-phases.mjs'
import {
  MESSAGE_BENCHMARK_PROFILE_REVISION,
  MESSAGE_BENCHMARK_REQUIRED_CASES,
  benchmarkCaseKey
} from '../benchmark-profile.mjs'
import {
  MESSAGE_BENCH_RESULT_SCHEMA_VERSION,
  assertValidMessageBenchmarkResult
} from '../result-schema.mjs'

const packageRoot = resolve(import.meta.dirname, '..')
const repoRoot = resolve(packageRoot, '../..')
const fixtureSelectionPath = resolve(packageRoot, 'fixture-selection.json')
const manifestPath = resolve(packageRoot, 'rs/Cargo.toml')
const binaryPath = resolve(
  process.env.INTLIFY_MESSAGES_BENCH_BINARY ??
    resolve(
      packageRoot,
      'rs/target/release',
      process.platform === 'win32' ? 'intlify-messages-bench.exe' : 'intlify-messages-bench'
    )
)
const defaultResultPath = resolve(packageRoot, 'results/latest.json')
const cli = parseCliArgs(process.argv.slice(2))

if (!cli.skipBuild) {
  buildRunner()
}
if (!existsSync(binaryPath)) {
  throw new Error(`missing messages benchmark binary at ${binaryPath}`)
}

const processResult = spawnSync(
  binaryPath,
  [
    '--fixture-selection',
    fixtureSelectionPath,
    '--iterations',
    String(cli.iterations),
    '--warmup-iterations',
    String(cli.warmupIterations)
  ],
  {
    cwd: repoRoot,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024
  }
)
if (processResult.error) {
  throw new Error(`failed to launch messages benchmark runner: ${processResult.error.message}`)
}
if (processResult.status !== 0) {
  throw new Error(
    processResult.stderr || `${binaryPath} exited with status ${String(processResult.status)}`
  )
}
let core
try {
  core = JSON.parse(processResult.stdout)
} catch (error) {
  throw new Error(`messages benchmark emitted malformed JSON: ${error.message}`)
}
if (!core || !Array.isArray(core.results)) {
  throw new Error('messages benchmark output is missing results')
}
const requiredOrder = new Map(
  MESSAGE_BENCHMARK_REQUIRED_CASES.map((required, index) => [benchmarkCaseKey(required), index])
)
core.results.sort(
  (left, right) =>
    (requiredOrder.get(benchmarkCaseKey(left)) ?? Number.MAX_SAFE_INTEGER) -
    (requiredOrder.get(benchmarkCaseKey(right)) ?? Number.MAX_SAFE_INTEGER)
)

const packageVersion = JSON.parse(await readFile(resolve(repoRoot, 'package.json'), 'utf8')).version
const fixtures = JSON.parse(await readFile(fixtureSelectionPath, 'utf8'))
const result = {
  schemaVersion: MESSAGE_BENCH_RESULT_SCHEMA_VERSION,
  benchmarkProfileRevision: MESSAGE_BENCHMARK_PROFILE_REVISION,
  tool: 'intlify-messages-bench',
  version: packageVersion,
  generatedAt: new Date().toISOString(),
  iterations: cli.iterations,
  warmupIterations: cli.warmupIterations,
  phases: MESSAGE_BENCHMARK_PHASES,
  fixtures,
  results: core.results
}

assertValidMessageBenchmarkResult(result)
await mkdir(dirname(cli.out), { recursive: true })
await writeFile(cli.out, `${JSON.stringify(result, null, 2)}\n`)
console.log(cli.out)

function parseCliArgs(args) {
  const { values } = parseArgs({
    args,
    options: {
      iterations: { type: 'string' },
      'warmup-iterations': { type: 'string' },
      out: { type: 'string' },
      'skip-build': { type: 'boolean' }
    },
    allowPositionals: false
  })
  const iterations = positiveInteger(values.iterations ?? '5', '--iterations')
  const warmupIterations = nonNegativeInteger(
    values['warmup-iterations'] ?? '1',
    '--warmup-iterations'
  )
  return {
    iterations,
    warmupIterations,
    out: resolve(values.out ?? defaultResultPath),
    skipBuild: values['skip-build'] === true
  }
}

function positiveInteger(value, option) {
  const parsed = Number(value)
  if (!Number.isSafeInteger(parsed) || parsed < 1) {
    throw new Error(`${option} must be a positive integer`)
  }
  return parsed
}

function nonNegativeInteger(value, option) {
  const parsed = Number(value)
  if (!Number.isSafeInteger(parsed) || parsed < 0) {
    throw new Error(`${option} must be a non-negative integer`)
  }
  return parsed
}

function buildRunner() {
  const result = spawnSync(
    'cargo',
    ['build', '--release', '--manifest-path', manifestPath, '--bin', 'intlify-messages-bench'],
    { cwd: repoRoot, stdio: 'inherit', shell: process.platform === 'win32' }
  )
  if (result.status !== 0) {
    throw new Error('failed to build the release messages benchmark runner')
  }
}
