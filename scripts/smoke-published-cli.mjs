import { spawnSync } from 'node:child_process'
import { existsSync } from 'node:fs'
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const semverPattern =
  /^(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-(?:0|[1-9]\d*|[a-z-][0-9a-z-]*)(?:\.(?:0|[1-9]\d*|[a-z-][0-9a-z-]*))*)?(?:\+[0-9a-z-]+(?:\.[0-9a-z-]+)*)?$/i
const tag = firstNonEmpty(process.argv[2], process.env.TAG, process.env.GITHUB_REF_NAME)
if (!tag?.startsWith('v')) {
  throw new Error(`release tag must start with "v": ${String(tag)}`)
}

const version = tag.slice(1)
if (!semverPattern.test(version)) {
  throw new Error(`release tag must contain a strict semver version: ${tag}`)
}

const packages = ['@intlify/cli', '@intlify/cli-native']
const npmAvailabilityMaxAttempts = readPositiveIntegerEnv('NPM_SMOKE_MAX_ATTEMPTS', 60)
const npmAvailabilityDelayMs = readPositiveIntegerEnv('NPM_SMOKE_DELAY_MS', 10_000)
const defaultRunTimeoutMs = readPositiveIntegerEnv('CLI_SMOKE_RUN_TIMEOUT_MS', 120_000)
const formatterSmokeInput = '.input   {$count   :number}\n{{Value {$count   :number}}}'
const formatterSmokeOutput = '.input {$count :number}\n{{Value {$count :number}}}\n'
const nativeTargets = [
  {
    platform: 'darwin',
    arch: 'x64',
    rustTarget: 'x86_64-apple-darwin',
    binaryName: 'intlify'
  },
  {
    platform: 'darwin',
    arch: 'arm64',
    rustTarget: 'aarch64-apple-darwin',
    binaryName: 'intlify'
  },
  {
    platform: 'linux',
    arch: 'x64',
    libc: 'glibc',
    rustTarget: 'x86_64-unknown-linux-gnu',
    binaryName: 'intlify'
  },
  {
    platform: 'linux',
    arch: 'arm64',
    libc: 'glibc',
    rustTarget: 'aarch64-unknown-linux-gnu',
    binaryName: 'intlify'
  },
  {
    platform: 'linux',
    arch: 'x64',
    libc: 'musl',
    rustTarget: 'x86_64-unknown-linux-musl',
    binaryName: 'intlify'
  },
  {
    platform: 'win32',
    arch: 'x64',
    rustTarget: 'x86_64-pc-windows-msvc',
    binaryName: 'intlify.exe'
  }
]

for (const packageName of packages) {
  await waitForPackage(packageName, version)
}

const tempDir = await mkdtemp(join(tmpdir(), 'intlify-cli-published-smoke-'))

try {
  run('npm', ['init', '-y'], { cwd: tempDir })
  run(
    'npm',
    ['install', '--ignore-scripts', '--no-audit', '--no-fund', `@intlify/cli@${version}`],
    {
      cwd: tempDir
    }
  )

  const wrapperBinPath =
    process.platform === 'win32'
      ? join(tempDir, 'node_modules', '.bin', 'intlify.cmd')
      : join(tempDir, 'node_modules', '.bin', 'intlify')
  const wrapperVersion = run(wrapperBinPath, ['--version'], { cwd: tempDir, capture: true })
  assertStdoutEquals(wrapperVersion, version, 'published wrapper version')

  const formatterFixturePath = join(tempDir, 'count.mf2')
  await writeFile(formatterFixturePath, formatterSmokeInput)
  const formatted = run(wrapperBinPath, ['fmt', '--reporter=json', 'count.mf2'], {
    cwd: tempDir,
    capture: true
  })
  const envelope = JSON.parse(formatted.stdout)
  assertEqual(formatted.stderr, '', 'published formatter smoke stderr')
  assertEqual(envelope.command, 'fmt', 'published formatter envelope command')
  assertEqual(envelope.summary?.status, 'success', 'published formatter summary status')
  assertEqual(envelope.summary?.operation, 'write', 'published formatter operation')
  assertEqual(envelope.summary?.formattedFiles, 1, 'published formatter formatted file count')
  assertEqual(envelope.results?.[0]?.path, 'count.mf2', 'published formatter result path')
  assertEqual(envelope.results?.[0]?.status, 'formatted', 'published formatter result status')
  assertEqual(
    await readFile(formatterFixturePath, 'utf8'),
    formatterSmokeOutput,
    'published formatter output'
  )

  const schemaPath = join(
    tempDir,
    'node_modules',
    '@intlify',
    'cli',
    'schema',
    'config.schema.json'
  )
  const schema = JSON.parse(await readFile(schemaPath, 'utf8'))
  assertEqual(schema.$schema, 'http://json-schema.org/draft-07/schema#', 'config schema draft')

  const target = nativeTarget()
  const nativeBinaryPath = join(
    tempDir,
    'node_modules',
    '@intlify',
    'cli-native',
    'bin',
    target.rustTarget,
    target.binaryName
  )
  if (!existsSync(nativeBinaryPath)) {
    throw new Error(`published native binary is missing at ${nativeBinaryPath}`)
  }
  const nativeVersion = run(nativeBinaryPath, ['--version'], { cwd: tempDir, capture: true })
  assertStdoutEquals(nativeVersion, version, 'published native binary version')

  console.log('@intlify/cli published smoke ok')
} finally {
  await rm(tempDir, { force: true, recursive: true })
}

async function waitForPackage(packageName, packageVersion) {
  for (let attempt = 1; attempt <= npmAvailabilityMaxAttempts; attempt++) {
    const result = spawnSync('npm', ['view', `${packageName}@${packageVersion}`, 'version'], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
      timeout: defaultRunTimeoutMs
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

function nativeTarget() {
  const linuxLibc = process.platform === 'linux' ? detectLinuxLibc() : undefined
  const target = nativeTargets.find(
    candidate =>
      candidate.platform === process.platform &&
      candidate.arch === process.arch &&
      (candidate.libc ?? undefined) === linuxLibc
  )
  if (!target) {
    throw new Error(`unsupported CLI smoke platform: ${process.platform}/${process.arch}`)
  }
  return target
}

function detectLinuxLibc() {
  return process.report?.getReport?.().header?.glibcVersionRuntime ? 'glibc' : 'musl'
}

function run(commandName, commandArgs, options = {}) {
  const result = spawnSync(commandName, commandArgs, {
    cwd: options.cwd,
    encoding: 'utf8',
    stdio: options.capture ? ['ignore', 'pipe', 'pipe'] : 'inherit',
    shell: process.platform === 'win32',
    timeout: options.timeoutMs ?? defaultRunTimeoutMs
  })
  const allowExitCodes = options.allowExitCodes ?? [0]
  if (!allowExitCodes.includes(result.status)) {
    throw new Error(
      `${commandName} ${commandArgs.join(' ')} failed with exit code ${result.status}` +
        (result.error ? `\n${result.error.message}` : '') +
        (result.stderr ? `\n${result.stderr.trimEnd()}` : '')
    )
  }
  return result
}

function assertStdoutEquals(result, expectedVersion, label) {
  assertEqual(result.stdout, `${expectedVersion}\n`, label)
  if (result.stderr) {
    console.warn(`${label} stderr:\n${result.stderr.trimEnd()}`)
  }
}

function assertEqual(actual, expected, label) {
  if (actual !== expected) {
    throw new Error(
      `${label} mismatch: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`
    )
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
