import { spawnSync } from 'node:child_process'
import { existsSync, linkSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, relative, resolve } from 'node:path'
import { parseArgs } from 'node:util'

import {
  RESOURCE_BENCHMARK_CLI_PHASE_NAMES,
  RESOURCE_BENCHMARK_CORE_PHASE_NAMES,
  RESOURCE_BENCHMARK_PHASES
} from '../benchmark-phases.mjs'
import { assertValidResourceBenchmarkResult } from '../result-schema.mjs'

const packageRoot = resolve(import.meta.dirname, '..')
const repoRoot = resolve(packageRoot, '../..')
const fixtureSelectionPath = resolve(packageRoot, 'fixture-selection.json')
const defaultResultPath = resolve(packageRoot, 'results/latest.json')
const coreManifestPath = resolve(packageRoot, 'rs/Cargo.toml')
const coreBinary = resolve(
  packageRoot,
  'rs/target/release',
  process.platform === 'win32' ? 'intlify-resource-bench.exe' : 'intlify-resource-bench'
)
const cliBinary = resolve(
  repoRoot,
  'target/release',
  process.platform === 'win32' ? 'intlify.exe' : 'intlify'
)

const cli = parseCliArgs(process.argv.slice(2))
const selection = JSON.parse(await readFile(fixtureSelectionPath, 'utf8'))
const packageVersion = JSON.parse(await readFile(resolve(repoRoot, 'package.json'), 'utf8')).version
const catalogFixture = await readCatalogFixture(selection)

if (!cli.skipBuild) {
  buildBenchmarkArtifacts()
}

const core = runCoreBenchmarks(selection)
const results = [...core.results]
runCliBenchmarks(results, catalogFixture)

const benchmarkResult = {
  schemaVersion: '0',
  tool: 'intlify-resource-bench',
  version: packageVersion,
  generatedAt: new Date().toISOString(),
  iterations: cli.iterations,
  phases: RESOURCE_BENCHMARK_PHASES,
  fixtures: fixtureTable(selection, catalogFixture),
  results,
  memoryGrowthChecks: core.memoryGrowthChecks
}

assertValidResourceBenchmarkResult(benchmarkResult)
if (!cli.allowSkips && results.some(result => result.status === 'skipped')) {
  throw new Error('resource benchmark produced skipped phases without --allow-skips')
}
await mkdir(dirname(cli.out), { recursive: true })
await writeFile(cli.out, `${JSON.stringify(benchmarkResult, null, 2)}\n`)
console.log(cli.out)

function parseCliArgs(args) {
  const { values } = parseArgs({
    args,
    options: {
      iterations: { type: 'string' },
      out: { type: 'string' },
      'skip-build': { type: 'boolean' },
      'allow-skips': { type: 'boolean' }
    },
    allowPositionals: false
  })
  const iterations = Number(values.iterations ?? '5')
  if (!Number.isSafeInteger(iterations) || iterations < 1) {
    throw new Error(`--iterations must be a positive integer: ${String(values.iterations)}`)
  }
  return {
    iterations,
    out: resolve(values.out ?? defaultResultPath),
    skipBuild: values['skip-build'] === true,
    allowSkips: values['allow-skips'] === true
  }
}

async function readCatalogFixture(value) {
  if (!Array.isArray(value.profiles) || value.profiles.length === 0) {
    throw new Error('fixture-selection.json must contain profiles')
  }
  const configured = value.catalogFixture
  if (!configured || typeof configured.name !== 'string' || typeof configured.path !== 'string') {
    throw new Error('fixture-selection.json must contain catalogFixture')
  }
  const absolutePath = resolve(packageRoot, configured.path)
  return {
    name: configured.name,
    path: slash(relative(repoRoot, absolutePath)),
    absolutePath,
    source: await readFile(absolutePath, 'utf8')
  }
}

function fixtureTable(value, catalog) {
  const profiles = value.profiles.map(profile => ({
    name: profile.name,
    kind: 'generated_profile',
    shape: profile.shape,
    scales: profile.scales
  }))
  return [
    ...profiles,
    {
      name: catalog.name,
      kind: 'catalog_file',
      path: catalog.path,
      inputBytes: Buffer.byteLength(catalog.source)
    }
  ]
}

function buildBenchmarkArtifacts() {
  run('cargo', [
    'build',
    '--release',
    '--manifest-path',
    coreManifestPath,
    '--bin',
    'intlify-resource-bench'
  ])
  run('cargo', ['build', '--release', '-p', 'intlify_cli', '--bin', 'intlify'], {
    cwd: repoRoot
  })
}

function runCoreBenchmarks(selectionValue) {
  if (!existsSync(coreBinary)) {
    return unavailableCore(selectionValue, `missing resource benchmark binary at ${coreBinary}`)
  }
  const result = spawnSync(
    coreBinary,
    ['--fixture-selection', fixtureSelectionPath, '--iterations', String(cli.iterations)],
    { cwd: repoRoot, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 }
  )
  if (result.status !== 0) {
    const reason = result.stderr.trim() || `${coreBinary} failed with status ${result.status}`
    return unavailableCore(selectionValue, reason)
  }
  try {
    const output = JSON.parse(result.stdout)
    if (!Array.isArray(output.results) || !Array.isArray(output.memoryGrowthChecks)) {
      throw new Error('core output is missing result arrays')
    }
    return {
      results: output.results.map(record => ({
        ...record,
        runtime: 'intlify-resource-bench-rs',
        operation: record.cost
      })),
      memoryGrowthChecks: output.memoryGrowthChecks
    }
  } catch (error) {
    return unavailableCore(
      selectionValue,
      `resource benchmark core emitted malformed JSON: ${error.message}`
    )
  }
}

function unavailableCore(selectionValue, reason) {
  if (!cli.allowSkips) {
    throw new Error(reason)
  }
  const fixture = selectionValue.profiles[0]?.name ?? 'generated-resource-profile'
  return {
    results: skipPhases(RESOURCE_BENCHMARK_CORE_PHASE_NAMES, fixture, reason),
    memoryGrowthChecks: []
  }
}

function runCliBenchmarks(output, fixture) {
  if (!existsSync(cliBinary)) {
    const reason = `missing CLI binary at ${cliBinary}`
    if (!cli.allowSkips) {
      throw new Error(reason)
    }
    output.push(...skipPhases(RESOURCE_BENCHMARK_CLI_PHASE_NAMES, fixture.name, reason))
    return
  }
  output.push(measureCatalogCheck(fixture))
  output.push(measureCatalogWrite(fixture))
  output.push(measureSequentialAggregation(fixture))
}

function measureCatalogCheck(fixture) {
  let elapsedNs = 0n
  let checksum = 0
  let entryCount = 0
  for (let index = 0; index < cli.iterations; index++) {
    const cwd = mkdtempSync(resolve(tmpdir(), 'intlify-resource-check-'))
    try {
      writeFileSync(resolve(cwd, 'catalog.json'), fixture.source)
      const started = process.hrtime.bigint()
      const result = runCli(['fmt', '--check', '--reporter=json', 'catalog.json'], cwd, [1])
      elapsedNs += process.hrtime.bigint() - started
      const envelope = parseCatalogEnvelope(result.stdout, 'catalog.json')
      if (readFileSync(resolve(cwd, 'catalog.json'), 'utf8') !== fixture.source) {
        throw new Error('fmt --check modified the catalog fixture')
      }
      entryCount = envelope.results[0].entries.length
      checksum = (checksum + checksumString(result.stdout) + checksumString(result.stderr)) >>> 0
    } finally {
      rmSync(cwd, { recursive: true, force: true })
    }
  }
  return cliDurationRecord({
    phase: 'fmt_catalog_check_e2e',
    cost: 'catalog_file_read_and_cli_pipeline',
    fixture,
    variant: 'check',
    operation: 'fmt --check --reporter=json catalog.json',
    elapsedNs,
    checksum,
    entryCount,
    scale: 1,
    inputBytes: Buffer.byteLength(fixture.source),
    iterations: cli.iterations
  })
}

function measureCatalogWrite(fixture) {
  let elapsedNs = 0n
  let checksum = 0
  let entryCount = 0
  for (let index = 0; index < cli.iterations; index++) {
    const cwd = mkdtempSync(resolve(tmpdir(), 'intlify-resource-write-'))
    try {
      const path = resolve(cwd, 'catalog.json')
      writeFileSync(path, fixture.source)
      const started = process.hrtime.bigint()
      const result = runCli(['fmt', '--reporter=json', 'catalog.json'], cwd, [0])
      const formatted = readFileSync(path, 'utf8')
      elapsedNs += process.hrtime.bigint() - started
      const envelope = parseCatalogEnvelope(result.stdout, 'catalog.json')
      if (formatted === fixture.source) {
        throw new Error('fmt write did not change the unformatted catalog fixture')
      }
      entryCount = envelope.results[0].entries.length
      checksum =
        (checksum +
          checksumString(result.stdout) +
          checksumString(result.stderr) +
          checksumString(formatted)) >>>
        0
    } finally {
      rmSync(cwd, { recursive: true, force: true })
    }
  }
  return cliDurationRecord({
    phase: 'fmt_catalog_write_e2e',
    cost: 'catalog_file_read_write_and_cli_pipeline',
    fixture,
    variant: 'write',
    operation: 'fmt --reporter=json catalog.json',
    elapsedNs,
    checksum,
    entryCount,
    scale: 1,
    inputBytes: Buffer.byteLength(fixture.source),
    iterations: cli.iterations
  })
}

function measureSequentialAggregation(fixture) {
  const cwd = mkdtempSync(resolve(tmpdir(), 'intlify-resource-groups-'))
  const iterations = Math.max(cli.iterations, 2)
  let elapsedNs = 0n
  let checksum = 0
  let entryCount = 0
  let baseline = null
  try {
    const first = resolve(cwd, 'a-catalog.json')
    writeFileSync(first, fixture.source)
    linkSync(first, resolve(cwd, 'b-alias.json'))
    writeFileSync(resolve(cwd, 'c-catalog.json'), fixture.source)

    for (let index = 0; index < iterations; index++) {
      const started = process.hrtime.bigint()
      const result = runCli(
        ['fmt', '--check', '--reporter=json', 'c-catalog.json', 'b-alias.json', 'a-catalog.json'],
        cwd,
        [1]
      )
      elapsedNs += process.hrtime.bigint() - started
      const envelope = JSON.parse(result.stdout)
      const paths = envelope.results.map(item => item.path)
      if (
        paths.length !== 3 ||
        new Set(paths).size !== 3 ||
        !['a-catalog.json', 'b-alias.json', 'c-catalog.json'].every(path => paths.includes(path))
      ) {
        throw new Error('sequential aggregation omitted or duplicated a physical-group alias')
      }
      const observable = `${result.status}\0${result.stdout}\0${result.stderr}`
      if (baseline !== null && observable !== baseline) {
        throw new Error('sequential physical-group aggregation is not deterministic')
      }
      baseline = observable
      entryCount = envelope.results.reduce(
        (count, item) => count + (Array.isArray(item.entries) ? item.entries.length : 0),
        0
      )
      checksum = (checksum + checksumString(observable)) >>> 0
    }
  } finally {
    rmSync(cwd, { recursive: true, force: true })
  }
  return cliDurationRecord({
    phase: 'sequential_physical_group_aggregation',
    cost: 'ordered_cli_aggregation',
    fixture,
    variant: 'hardlink_alias_group',
    operation: 'fmt --check --reporter=json three catalog paths',
    elapsedNs,
    checksum,
    entryCount,
    scale: 3,
    inputBytes: Buffer.byteLength(fixture.source) * 3,
    iterations
  })
}

function runCli(args, cwd, allowedStatuses) {
  const result = spawnSync(cliBinary, args, {
    cwd,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
    maxBuffer: 16 * 1024 * 1024
  })
  if (!allowedStatuses.includes(result.status)) {
    throw new Error(result.stderr || `${cliBinary} ${args.join(' ')} failed`)
  }
  return result
}

function parseCatalogEnvelope(stdout, expectedPath) {
  const envelope = JSON.parse(stdout)
  if (
    envelope.command !== 'fmt' ||
    !Array.isArray(envelope.results) ||
    envelope.results.length !== 1 ||
    envelope.results[0].path !== expectedPath ||
    !Array.isArray(envelope.results[0].entries)
  ) {
    throw new Error('catalog CLI benchmark received an unexpected JSON envelope')
  }
  return envelope
}

function cliDurationRecord(options) {
  return {
    status: 'measured',
    phase: options.phase,
    cost: options.cost,
    fixture: options.fixture.name,
    variant: options.variant,
    runtime: 'intlify',
    operation: options.operation,
    scale: options.scale,
    inputBytes: options.inputBytes,
    entryCount: options.entryCount,
    metric: 'duration',
    iterations: options.iterations,
    elapsedMs: Number(options.elapsedNs) / 1_000_000,
    checksum: options.checksum
  }
}

function skipPhases(names, fixture, reason) {
  const selected = new Set(names)
  return RESOURCE_BENCHMARK_PHASES.filter(phase => selected.has(phase.name)).flatMap(phase =>
    phase.costs.map(cost => ({
      status: 'skipped',
      phase: phase.name,
      cost,
      fixture,
      variant: 'unavailable',
      runtime:
        phase.name.startsWith('resource_') || phase.name.startsWith('fmt_catalog_output')
          ? 'intlify-resource-bench-rs'
          : 'intlify',
      operation: cost,
      reason
    }))
  )
}

function checksumString(value) {
  let checksum = 0
  for (let index = 0; index < value.length; index++) {
    checksum = Math.imul(checksum, 16_777_619) + value.charCodeAt(index)
    checksum >>>= 0
  }
  return checksum
}

function run(commandName, args, options = {}) {
  const result = spawnSync(commandName, args, {
    cwd: options.cwd ?? repoRoot,
    stdio: 'inherit',
    shell: process.platform === 'win32'
  })
  if (result.status !== 0) {
    throw new Error(`${commandName} ${args.join(' ')} failed`)
  }
}

function slash(path) {
  return path.replace(/\\/g, '/')
}
