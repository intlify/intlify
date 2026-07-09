import { spawnSync } from 'node:child_process'
import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, relative, resolve } from 'node:path'
import { parseArgs } from 'node:util'

import { FORMAT_BENCHMARK_PHASES } from '../benchmark-phases.mjs'
import { assertValidFormatBenchmarkResult } from '../result-schema.mjs'

const packageRoot = resolve(import.meta.dirname, '..')
const repoRoot = resolve(packageRoot, '../..')
const defaultResultPath = resolve(packageRoot, 'results/latest.json')

const cli = parseCliArgs(process.argv.slice(2))
const packageVersion = JSON.parse(await readFile(resolve(repoRoot, 'package.json'), 'utf8')).version
const fixtures = await readSelectedFixtures(cli.fixtureNames)

if (!cli.skipBuild) {
  buildBenchmarkArtifacts()
}

const results = []
await runApiBenchmarks(results)
runCliBenchmarks(results)

const benchmarkResult = {
  schemaVersion: '0',
  tool: 'intlify-format-bench',
  version: packageVersion,
  generatedAt: new Date().toISOString(),
  iterations: cli.iterations,
  phases: FORMAT_BENCHMARK_PHASES,
  fixtures: fixtures.map(fixture => ({
    name: fixture.name,
    path: slash(relative(repoRoot, fixture.absolutePath)),
    inputBytes: Buffer.byteLength(fixture.source)
  })),
  results
}

assertValidFormatBenchmarkResult(benchmarkResult)
await mkdir(dirname(cli.out), { recursive: true })
await writeFile(cli.out, `${JSON.stringify(benchmarkResult, null, 2)}\n`)
console.log(cli.out)

function parseCliArgs(args) {
  const { values } = parseArgs({
    args,
    options: {
      fixture: { type: 'string', multiple: true },
      iterations: { type: 'string' },
      out: { type: 'string' },
      'skip-build': { type: 'boolean' },
      'allow-skips': { type: 'boolean' }
    },
    allowPositionals: false
  })

  const iterations = Number(values.iterations ?? '10')
  if (!Number.isInteger(iterations) || iterations < 1) {
    throw new Error(`--iterations must be a positive integer: ${String(values.iterations)}`)
  }

  return {
    fixtureNames: values.fixture ?? [],
    iterations,
    out: resolve(values.out ?? defaultResultPath),
    skipBuild: values['skip-build'] === true,
    allowSkips: values['allow-skips'] === true
  }
}

async function readSelectedFixtures(names) {
  const selection = JSON.parse(
    await readFile(resolve(packageRoot, 'fixture-selection.json'), 'utf8')
  )
  if (!Array.isArray(selection.fixtures) || selection.fixtures.length === 0) {
    throw new Error('fixture-selection.json must contain fixtures')
  }

  const selectedNames = new Set(names)
  const fixtures = []
  for (const fixture of selection.fixtures) {
    if (selectedNames.size > 0 && !selectedNames.has(fixture.name)) {
      continue
    }
    const absolutePath = resolve(packageRoot, fixture.path)
    fixtures.push({
      name: fixture.name,
      absolutePath,
      source: await readFile(absolutePath, 'utf8')
    })
  }

  const missing = [...selectedNames].filter(
    name => !fixtures.some(fixture => fixture.name === name)
  )
  if (missing.length > 0) {
    throw new Error(`unknown formatter benchmark fixture(s): ${missing.join(', ')}`)
  }
  if (fixtures.length === 0) {
    throw new Error('no formatter benchmark fixtures selected')
  }
  return fixtures
}

function buildBenchmarkArtifacts() {
  run('vp', ['run', '@intlify/ox-mf2-shared#build'], { cwd: repoRoot })
  run('vp', ['run', '@intlify/ox-mf2-napi#build'], { cwd: repoRoot })
  run('vp', ['run', '@intlify/format-napi#build'], { cwd: repoRoot })
  run('vp', ['run', '@intlify/format-wasm#build'], { cwd: repoRoot })
  run('cargo', ['build', '--release', '-p', 'intlify_cli', '--bin', 'intlify'], { cwd: repoRoot })
}

async function runApiBenchmarks(output) {
  const parserApi = await optionalImport('@intlify/ox-mf2-napi')
  const formatNapi = await optionalImport('@intlify/format-napi')
  const formatWasm = await optionalImport('@intlify/format-wasm')

  if (formatWasm.ok) {
    try {
      await formatWasm.module.init()
    } catch (error) {
      formatWasm.ok = false
      formatWasm.error = error
    }
  }

  for (const fixture of fixtures) {
    if (parserApi.ok) {
      measure(output, {
        phase: 'format_standard',
        cost: 'parse',
        fixture,
        runtime: '@intlify/ox-mf2-napi',
        operation: 'parseMessage',
        fn: () => parserApi.module.parseMessage(fixture.source).snapshot.rootCount()
      })
      measure(output, {
        phase: 'format_standard',
        cost: 'snapshot_encode',
        fixture,
        runtime: '@intlify/ox-mf2-napi',
        operation: 'parseMessage.snapshot.toBytes',
        fn: () => parserApi.module.parseMessage(fixture.source).snapshot.toBytes().length
      })

      const snapshotBytes = parserApi.module.parseMessage(fixture.source).snapshot.toBytes()
      measure(output, {
        phase: 'format_standard',
        cost: 'snapshot_decode_access',
        fixture,
        runtime: '@intlify/ox-mf2-napi',
        operation: 'decodeSnapshot.rootCount',
        fn: () => parserApi.module.decodeSnapshot(snapshotBytes).snapshot.rootCount()
      })
    } else {
      skipApi(output, fixture, '@intlify/ox-mf2-napi', parserApi.error, [
        ['format_standard', 'parse', 'parseMessage'],
        ['format_standard', 'snapshot_encode', 'parseMessage.snapshot.toBytes'],
        ['format_standard', 'snapshot_decode_access', 'decodeSnapshot.rootCount']
      ])
    }

    if (formatNapi.ok) {
      measureFormatterApi(output, {
        phase: 'format_standard',
        cost: 'napi_binding_call',
        fixture,
        runtime: '@intlify/format-napi',
        operation: 'formatMessage',
        mode: 'standard',
        api: formatNapi.module
      })
      measureFormatterApi(output, {
        phase: 'format_preserve',
        cost: 'napi_binding_call',
        fixture,
        runtime: '@intlify/format-napi',
        operation: 'formatMessage',
        mode: 'preserve',
        api: formatNapi.module
      })
    } else {
      skipApi(output, fixture, '@intlify/format-napi', formatNapi.error, [
        ['format_standard', 'napi_binding_call', 'formatMessage'],
        ['format_preserve', 'napi_binding_call', 'formatMessage']
      ])
    }

    if (formatWasm.ok) {
      measureFormatterApi(output, {
        phase: 'format_standard',
        cost: 'wasm_binding_call',
        fixture,
        runtime: '@intlify/format-wasm',
        operation: 'formatMessage',
        mode: 'standard',
        api: formatWasm.module
      })
      measureFormatterApi(output, {
        phase: 'format_preserve',
        cost: 'wasm_binding_call',
        fixture,
        runtime: '@intlify/format-wasm',
        operation: 'formatMessage',
        mode: 'preserve',
        api: formatWasm.module
      })
    } else {
      skipApi(output, fixture, '@intlify/format-wasm', formatWasm.error, [
        ['format_standard', 'wasm_binding_call', 'formatMessage'],
        ['format_preserve', 'wasm_binding_call', 'formatMessage']
      ])
    }
  }
}

function measureFormatterApi(output, options) {
  measure(output, {
    ...options,
    fn: () => {
      const result = options.api.formatMessage(options.fixture.source, { mode: options.mode })
      if (!result.ok) {
        throw new Error(`${options.runtime} ${options.operation} returned failure`)
      }
      return checksumString(result.code)
    }
  })
}

function runCliBenchmarks(output) {
  const binary = resolve(
    repoRoot,
    'target',
    'release',
    process.platform === 'win32' ? 'intlify.exe' : 'intlify'
  )
  if (!existsSync(binary)) {
    for (const fixture of fixtures) {
      skipApi(output, fixture, 'intlify', new Error(`missing CLI binary at ${binary}`), [
        ['format_check_cli_e2e', 'cli_e2e', 'fmt --check'],
        ['format_check_json', 'cli_json_reporter', 'fmt --check --reporter=json'],
        ['e2e_format', 'cli_e2e', 'fmt']
      ])
    }
    return
  }

  for (const fixture of fixtures) {
    measureCli(output, {
      phase: 'format_check_cli_e2e',
      cost: 'cli_e2e',
      fixture,
      binary,
      args: ['fmt', '--check', 'input.mf2'],
      allowedStatuses: [0, 1]
    })
    measureCli(output, {
      phase: 'format_check_json',
      cost: 'cli_json_reporter',
      fixture,
      binary,
      args: ['fmt', '--check', '--reporter=json', 'input.mf2'],
      allowedStatuses: [0, 1]
    })
    measureCli(output, {
      phase: 'e2e_format',
      cost: 'cli_e2e',
      fixture,
      binary,
      args: ['fmt', 'input.mf2'],
      allowedStatuses: [0]
    })
  }
}

function measureCli(output, { phase, cost, fixture, binary, args, allowedStatuses }) {
  measure(output, {
    phase,
    cost,
    fixture,
    runtime: 'intlify',
    operation: args.join(' '),
    fn: () => {
      const cwd = mkdtempSync(resolve(tmpdir(), 'intlify-format-bench-'))
      try {
        writeFileSync(resolve(cwd, 'input.mf2'), fixture.source)
        const result = spawnSync(binary, args, {
          cwd,
          encoding: 'utf8',
          stdio: ['ignore', 'pipe', 'pipe']
        })
        if (!allowedStatuses.includes(result.status)) {
          throw new Error(result.stderr || `${binary} ${args.join(' ')} failed`)
        }
        return checksumString(result.stdout) + checksumString(result.stderr)
      } finally {
        rmSync(cwd, { recursive: true, force: true })
      }
    }
  })
}

function measure(output, { phase, cost, fixture, runtime, operation, fn }) {
  const started = process.hrtime.bigint()
  let checksum = 0
  try {
    for (let index = 0; index < cli.iterations; index++) {
      checksum = (checksum + checksumValue(fn())) >>> 0
    }
  } catch (error) {
    if (!cli.allowSkips) {
      throw error
    }
    output.push({
      status: 'skipped',
      phase,
      cost,
      fixture: fixture.name,
      runtime,
      operation,
      reason: error instanceof Error ? error.message : String(error)
    })
    return
  }
  const elapsedMs = Number(process.hrtime.bigint() - started) / 1_000_000
  output.push({
    status: 'measured',
    phase,
    cost,
    fixture: fixture.name,
    runtime,
    operation,
    iterations: cli.iterations,
    elapsedMs,
    checksum,
    inputBytes: Buffer.byteLength(fixture.source)
  })
}

function skipApi(output, fixture, runtime, error, entries) {
  if (!cli.allowSkips) {
    const message = error instanceof Error ? error.message : String(error)
    throw new Error(`${runtime} is unavailable for formatter benchmark: ${message}`)
  }
  for (const [phase, cost, operation] of entries) {
    output.push({
      status: 'skipped',
      phase,
      cost,
      fixture: fixture.name,
      runtime,
      operation,
      reason: error instanceof Error ? error.message : String(error)
    })
  }
}

async function optionalImport(specifier) {
  try {
    return {
      ok: true,
      module: await import(specifier),
      error: null
    }
  } catch (error) {
    return {
      ok: false,
      module: null,
      error
    }
  }
}

function checksumValue(value) {
  if (typeof value === 'number') {
    return value >>> 0
  }
  return checksumString(String(value))
}

function checksumString(value) {
  let checksum = 0
  for (let index = 0; index < value.length; index++) {
    checksum = (checksum + value.charCodeAt(index)) >>> 0
  }
  return checksum
}

function run(commandName, args, options) {
  const result = spawnSync(commandName, args, {
    cwd: options.cwd,
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
