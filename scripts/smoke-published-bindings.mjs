import { spawnSync } from 'node:child_process'
import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { parseArgs } from 'node:util'

const { packageNames: selectedPackageNames, tag: explicitTag } = parseCliArgs(process.argv.slice(2))
const tag = firstNonEmpty(explicitTag, process.env.TAG, process.env.GITHUB_REF_NAME)
if (!tag?.startsWith('v')) {
  throw new Error(`release tag must start with "v": ${String(tag)}`)
}

const version = tag.slice(1)
const smokeCases = [
  {
    packageName: '@intlify/ox-mf2-napi',
    smoke: tempDir =>
      run(
        'node',
        [
          '--input-type=module',
          '-e',
          `
            import { parseMessage } from '@intlify/ox-mf2-napi'
            const result = parseMessage('hello')
            if (result.snapshot.rootCount() < 1) throw new Error('N-API smoke failed')
            console.log('@intlify/ox-mf2-napi smoke ok')
          `
        ],
        { cwd: tempDir }
      )
  },
  {
    packageName: '@intlify/ox-mf2-wasm',
    smoke: tempDir =>
      run(
        'node',
        [
          '--input-type=module',
          '-e',
          `
            import { init, parseMessage } from '@intlify/ox-mf2-wasm'
            await init()
            const result = parseMessage('hello')
            if (result.snapshot.rootCount() < 1) throw new Error('WASM smoke failed')
            console.log('@intlify/ox-mf2-wasm smoke ok')
          `
        ],
        { cwd: tempDir }
      )
  },
  {
    packageName: '@intlify/format-napi',
    smoke: tempDir =>
      run(
        'node',
        [
          '--input-type=module',
          '-e',
          `
            import { checkFormat, formatMessage } from '@intlify/format-napi'
            const source = '.input   {$count   :number}\\n{{Value {$count   :number}}}'
            const formatted = formatMessage(source)
            if (!formatted.ok || formatted.code !== '.input {$count :number}\\n{{Value {$count :number}}}') {
              throw new Error('formatter N-API formatMessage smoke failed')
            }
            const checked = checkFormat(source)
            if (!checked.ok || checked.changed !== true) throw new Error('formatter N-API checkFormat smoke failed')
            console.log('@intlify/format-napi smoke ok')
          `
        ],
        { cwd: tempDir }
      )
  },
  {
    packageName: '@intlify/format-wasm',
    smoke: tempDir =>
      run(
        'node',
        [
          '--input-type=module',
          '-e',
          `
            import { init, formatMessage } from '@intlify/format-wasm'
            await init()
            const result = formatMessage('.input   {$count   :number}\\n{{Value {$count   :number}}}')
            if (!result.ok || result.code !== '.input {$count :number}\\n{{Value {$count :number}}}') {
              throw new Error('formatter WASM smoke failed')
            }
            console.log('@intlify/format-wasm smoke ok')
          `
        ],
        { cwd: tempDir }
      )
  }
]
const selectedSmokeCases =
  selectedPackageNames.size === 0
    ? smokeCases
    : smokeCases.filter(smokeCase => selectedPackageNames.has(smokeCase.packageName))
const missingPackageNames = [...selectedPackageNames].filter(
  packageName => !smokeCases.some(smokeCase => smokeCase.packageName === packageName)
)

if (missingPackageNames.length > 0) {
  throw new Error(`unknown package(s): ${missingPackageNames.join(', ')}`)
}

const packages = selectedSmokeCases.map(smokeCase => smokeCase.packageName)
const npmAvailabilityMaxAttempts = readPositiveIntegerEnv('NPM_SMOKE_MAX_ATTEMPTS', 60)
const npmAvailabilityDelayMs = readPositiveIntegerEnv('NPM_SMOKE_DELAY_MS', 10_000)

for (const packageName of packages) {
  await waitForPackage(packageName, version)
}

const tempDir = await mkdtemp(join(tmpdir(), 'ox-mf2-published-smoke-'))

try {
  run('npm', ['init', '-y'], { cwd: tempDir })
  run(
    'npm',
    ['install', '--ignore-scripts', ...packages.map(packageName => `${packageName}@${version}`)],
    { cwd: tempDir }
  )

  for (const smokeCase of selectedSmokeCases) {
    smokeCase.smoke(tempDir)
  }
} finally {
  await rm(tempDir, { force: true, recursive: true })
}

function parseCliArgs(args) {
  const { values, positionals } = parseArgs({
    args,
    options: {
      package: { type: 'string', multiple: true }
    },
    allowPositionals: true
  })

  if (positionals.length > 1) {
    throw new Error(`expected at most one tag, received: ${positionals.join(' ')}`)
  }

  return {
    packageNames: new Set(values.package ?? []),
    tag: positionals[0]
  }
}

async function waitForPackage(packageName, packageVersion) {
  for (let attempt = 1; attempt <= npmAvailabilityMaxAttempts; attempt++) {
    const result = spawnSync('npm', ['view', `${packageName}@${packageVersion}`, 'version'], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore']
    })
    if (result.status === 0 && result.stdout.trim() === packageVersion) {
      console.log(`${packageName}@${packageVersion} is available on npm`)
      return
    }
    console.log(
      `Waiting for ${packageName}@${packageVersion} on npm (${attempt}/${npmAvailabilityMaxAttempts})`
    )
    await new Promise(resolve => setTimeout(resolve, npmAvailabilityDelayMs))
  }
  throw new Error(`${packageName}@${packageVersion} is not available on npm`)
}

function run(commandName, commandArgs, options) {
  const result = spawnSync(commandName, commandArgs, {
    ...options,
    stdio: 'inherit',
    shell: process.platform === 'win32'
  })
  if (result.status !== 0) {
    throw new Error(`${commandName} ${commandArgs.join(' ')} failed`)
  }
}

function readPositiveIntegerEnv(name, fallback) {
  const rawValue = process.env[name]
  if (rawValue == null || rawValue === '') {
    return fallback
  }

  const value = Number(rawValue)
  if (!Number.isInteger(value) || value <= 0) {
    throw new Error(`${name} must be a positive integer`)
  }
  return value
}

function firstNonEmpty(...values) {
  return values.find(value => value != null && value !== '')
}
