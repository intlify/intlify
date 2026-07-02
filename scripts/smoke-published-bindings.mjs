import { spawnSync } from 'node:child_process'
import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const tag = firstNonEmpty(process.argv[2], process.env.TAG, process.env.GITHUB_REF_NAME)
if (!tag?.startsWith('v')) {
  throw new Error(`release tag must start with "v": ${String(tag)}`)
}

const version = tag.slice(1)
const packages = ['@intlify/ox-mf2-napi', '@intlify/ox-mf2-wasm']
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
} finally {
  await rm(tempDir, { force: true, recursive: true })
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
