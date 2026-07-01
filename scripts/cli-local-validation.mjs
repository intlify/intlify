#!/usr/bin/env node
// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

import { spawn } from 'node:child_process'
import { existsSync } from 'node:fs'
import { chmod, copyFile, mkdir, mkdtemp, readFile, rm, stat, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { performance } from 'node:perf_hooks'
import { fileURLToPath } from 'node:url'

import {
  detectLinuxLibc,
  NATIVE_TARGETS,
  resolveNativeTarget
} from '../packages/cli/bin/intlify.mjs'

const workspaceRoot = fileURLToPath(new URL('..', import.meta.url))
const cliPackageRoot = join(workspaceRoot, 'packages', 'cli')
const nativePackageRoot = join(workspaceRoot, 'packages', 'cli-native')
const cliPackageJsonPath = join(cliPackageRoot, 'package.json')
const nativePackageJsonPath = join(nativePackageRoot, 'package.json')
const cargoManifestPath = join(workspaceRoot, 'crates', 'intlify_cli', 'Cargo.toml')
const wrapperBinPath = join(cliPackageRoot, 'bin', 'intlify.mjs')
const schemaPath = join(cliPackageRoot, 'schema', 'config.schema.json')
const unixExecutableMask = 0o111
const executableMode = 0o755
const supportedNativeBinaryPaths = new Map(
  NATIVE_TARGETS.map(target => [`bin/${target.rustTarget}/${target.binaryName}`, target])
)

const command = process.argv[2]

try {
  switch (command) {
    case 'build':
      await buildCli()
      break
    case 'pack-check':
      await checkCliPackages()
      break
    case 'smoke':
      await smokeCli()
      break
    case 'bench-startup':
      await benchStartup()
      break
    default:
      throw new Error(
        `Unknown command '${command ?? ''}'. Expected build, pack-check, smoke, or bench-startup.`
      )
  }
} catch (error) {
  console.error(`error: ${error.message}`)
  process.exitCode = 1
}

async function buildCli() {
  const target = hostNativeTarget()
  await run('cargo', ['build', '--release', '-p', 'intlify_cli', '--bin', 'intlify'])

  const sourceBinaryPath = join(workspaceRoot, 'target', 'release', target.binaryName)
  const destinationDirectory = nativeBinaryDirectory(target)
  const destinationBinaryPath = nativeBinaryPath(target)

  if (!existsSync(sourceBinaryPath)) {
    throw new Error(`release binary was not produced at ${sourceBinaryPath}`)
  }

  await mkdir(destinationDirectory, { recursive: true })
  await copyFile(sourceBinaryPath, destinationBinaryPath)

  if (process.platform !== 'win32') {
    await chmod(destinationBinaryPath, executableMode)
    await chmod(wrapperBinPath, executableMode)
  }

  console.log(`Built intlify CLI for ${target.rustTarget}`)
  console.log(`Copied ${sourceBinaryPath} -> ${destinationBinaryPath}`)
}

async function checkCliPackages() {
  const target = hostNativeTarget()
  await assertVersionConsistency()
  await assertWrapperExecutable()
  await assertNativeBinary(target)
  await assertSchemaPresence()

  const cliDryRun = await npmPackDryRun(cliPackageRoot)
  assertPackFiles(cliDryRun, {
    packageName: '@intlify/cli',
    exactFiles: ['README.md', 'bin/intlify.mjs', 'package.json', 'schema/config.schema.json']
  })
  assertPackMode(cliDryRun, 'bin/intlify.mjs', unixExecutableMask)

  const nativeDryRun = await npmPackDryRun(nativePackageRoot)
  assertNativePackFiles(nativeDryRun, target)

  const packDirectory = await mkdtemp(join(tmpdir(), 'intlify-cli-pack-check-'))
  const installDirectory = await mkdtemp(join(tmpdir(), 'intlify-cli-pack-install-'))
  try {
    const nativeTarball = await pnpmPack(nativePackageRoot, packDirectory)
    const cliTarball = await pnpmPack(cliPackageRoot, packDirectory)
    await installPackedCli({ installDirectory, nativeTarball, cliTarball })
    await assertInstalledPackagePermissions({ installDirectory, target })
    console.log(`Created ${nativeTarball}`)
    console.log(`Created ${cliTarball}`)
  } finally {
    await rm(packDirectory, { recursive: true, force: true })
    await rm(installDirectory, { recursive: true, force: true })
  }
}

async function smokeCli() {
  const target = hostNativeTarget()
  await assertNativeBinary(target)
  await assertSchemaPresence()

  const native = await run(nativeBinaryPath(target), ['--version'], { capture: true })
  assertStdoutEquals(native, await cliVersion())

  const wrapper = await run(process.execPath, [wrapperBinPath, '--version'], { capture: true })
  assertStdoutEquals(wrapper, await cliVersion())

  const reserved = await run(process.execPath, [wrapperBinPath, 'fmt', '--reporter=json'], {
    capture: true,
    allowExitCodes: [2]
  })
  const envelope = JSON.parse(reserved.stdout)
  assertEqual(envelope.command, 'fmt', 'reserved command envelope command')
  assertEqual(envelope.errors?.[0]?.code, 'command_not_ready', 'reserved command error code')
  assertEqual(envelope.errors?.[0]?.details?.phase, '3A', 'reserved command phase')

  console.log('CLI smoke validation passed')
}

async function benchStartup() {
  const target = hostNativeTarget()
  const sampleCount = Number.parseInt(process.env.CLI_STARTUP_BENCH_SAMPLES ?? '7', 10)
  if (!Number.isInteger(sampleCount) || sampleCount < 1) {
    throw new Error('CLI_STARTUP_BENCH_SAMPLES must be a positive integer')
  }

  const packDirectory = await mkdtemp(join(tmpdir(), 'intlify-cli-bench-pack-'))
  const installDirectory = await mkdtemp(join(tmpdir(), 'intlify-cli-bench-install-'))
  try {
    const nativeTarball = await pnpmPack(nativePackageRoot, packDirectory)
    const cliTarball = await pnpmPack(cliPackageRoot, packDirectory)
    const installedBinPath = await installPackedCli({
      installDirectory,
      nativeTarball,
      cliTarball
    })

    const version = await cliVersion()
    const phases = [
      {
        name: 'cli_startup_native',
        command: [nativeBinaryPath(target), '--version'],
        nativeBinaryPath: nativeBinaryPath(target)
      },
      {
        name: 'cli_startup_wrapper',
        command: [process.execPath, wrapperBinPath, '--version'],
        nativeBinaryPath: nativeBinaryPath(target),
        nodeVersion: process.version,
        npmVersion: await toolVersion('npm')
      },
      {
        name: 'cli_startup_installed',
        command: [installedBinPath, '--version'],
        nativeBinaryPath: nativeBinaryPath(target),
        nodeVersion: process.version,
        npmVersion: await toolVersion('npm')
      }
    ]

    const reports = []
    for (const phase of phases) {
      reports.push(await measurePhase(phase, { sampleCount, expectedVersion: version }))
    }

    const nativeMean = reports[0].timing.meanMs
    const wrapperMean = reports[1].timing.meanMs
    const installedMean = reports[2].timing.meanMs
    const report = {
      packageVersion: version,
      gitCommit: await gitCommit(),
      platform: {
        os: process.platform,
        arch: process.arch,
        ...(target.libc ? { libc: target.libc } : {})
      },
      sampleCount,
      phases: reports,
      overhead: {
        wrapperMeanMs: round(wrapperMean - nativeMean),
        installedMeanMs: round(installedMean - nativeMean)
      }
    }

    console.log(JSON.stringify(report, null, 2))
  } finally {
    await rm(packDirectory, { recursive: true, force: true })
    await rm(installDirectory, { recursive: true, force: true })
  }
}

function hostNativeTarget() {
  const platform = process.platform
  const arch = process.arch
  const libc = platform === 'linux' ? detectLinuxLibc() : undefined
  const target = resolveNativeTarget({ platform, arch, libc })

  if (!target) {
    throw new Error(`unsupported host platform for CLI native build: ${platform}/${arch}/${libc}`)
  }

  return target
}

function nativeBinaryDirectory(target) {
  return join(nativePackageRoot, 'bin', target.rustTarget)
}

function nativeBinaryPath(target) {
  return join(nativeBinaryDirectory(target), target.binaryName)
}

async function assertVersionConsistency() {
  const cliPackage = await readJson(cliPackageJsonPath)
  const nativePackage = await readJson(nativePackageJsonPath)
  const cargoVersion = await readCargoPackageVersion()

  assertEqual(cliPackage.version, nativePackage.version, 'CLI and native package versions')
  assertEqual(cliPackage.version, cargoVersion, 'CLI package and Rust crate versions')
}

async function assertWrapperExecutable() {
  const source = await readFile(wrapperBinPath, 'utf8')
  if (!source.startsWith('#!/usr/bin/env node\n')) {
    throw new Error('packages/cli/bin/intlify.mjs must keep its node shebang')
  }
  if (process.platform !== 'win32') {
    await assertExecutable(wrapperBinPath)
  }
}

async function assertNativeBinary(target) {
  const path = nativeBinaryPath(target)
  if (!existsSync(path)) {
    throw new Error(`host native binary is missing at ${path}`)
  }
  if (process.platform !== 'win32') {
    await assertExecutable(path)
  }
}

async function assertInstalledPackagePermissions({ installDirectory, target }) {
  if (process.platform === 'win32') {
    return
  }

  await assertExecutable(
    join(
      installDirectory,
      'node_modules',
      '@intlify',
      'cli-native',
      'bin',
      target.rustTarget,
      target.binaryName
    )
  )
  await assertExecutable(join(installDirectory, 'node_modules', '.bin', 'intlify'))
}

async function assertSchemaPresence() {
  const schema = await readJson(schemaPath)
  assertEqual(schema.$schema, 'http://json-schema.org/draft-07/schema#', 'config schema draft')
  assertEqual(schema.type, 'object', 'config schema root type')
}

async function assertExecutable(path) {
  const mode = (await stat(path)).mode
  if ((mode & unixExecutableMask) === 0) {
    throw new Error(`${path} must be executable`)
  }
}

async function npmPackDryRun(cwd) {
  const result = await run('npm', ['pack', '--dry-run', '--json'], { cwd, capture: true })
  const jsonStart = result.stdout.indexOf('[')
  if (jsonStart === -1) {
    throw new Error(`npm pack did not return JSON for ${cwd}`)
  }
  const packs = JSON.parse(result.stdout.slice(jsonStart))
  if (packs.length !== 1) {
    throw new Error(`expected one npm pack result for ${cwd}, got ${packs.length}`)
  }
  return packs[0]
}

function assertPackFiles(pack, { packageName, exactFiles }) {
  assertEqual(pack.name, packageName, `${packageName} pack name`)
  const actualFiles = pack.files.map(file => file.path).sort(compareStrings)
  const expectedFiles = [...exactFiles].sort(compareStrings)
  assertJsonEqual(actualFiles, expectedFiles, `${packageName} packed files`)
}

function assertPackMode(pack, path, executableMask) {
  const file = pack.files.find(entry => entry.path === path)
  if (!file) {
    throw new Error(`packed file ${path} was not found`)
  }
  if ((file.mode & executableMask) === 0) {
    throw new Error(`packed file ${path} must be executable`)
  }
}

function assertNativePackFiles(pack, hostTarget) {
  assertEqual(pack.name, '@intlify/cli-native', '@intlify/cli-native pack name')

  const paths = pack.files.map(file => file.path).sort(compareStrings)
  for (const requiredPath of [
    'README.md',
    'package.json',
    `bin/${hostTarget.rustTarget}/${hostTarget.binaryName}`
  ]) {
    if (!paths.includes(requiredPath)) {
      throw new Error(`@intlify/cli-native pack is missing ${requiredPath}`)
    }
  }

  for (const file of pack.files) {
    if (file.path === 'README.md' || file.path === 'package.json') {
      continue
    }
    const target = supportedNativeBinaryPaths.get(file.path)
    if (!target) {
      throw new Error(`unexpected @intlify/cli-native packed file ${file.path}`)
    }
    if (target.binaryName === 'intlify' && (file.mode & unixExecutableMask) === 0) {
      throw new Error(`packed native binary ${file.path} must be executable`)
    }
  }
}

async function pnpmPack(packageRoot, destination) {
  const packageMetadata = await readJson(join(packageRoot, 'package.json'))
  const expectedTarball = join(
    destination,
    `${packageMetadata.name.replace(/^@/, '').replace('/', '-')}-${packageMetadata.version}.tgz`
  )
  await run('pnpm', ['--dir', packageRoot, 'pack', '--pack-destination', destination], {
    capture: true
  })
  if (!existsSync(expectedTarball)) {
    throw new Error(`pnpm pack did not create a tarball for ${packageRoot}`)
  }
  return expectedTarball
}

async function installPackedCli({ installDirectory, nativeTarball, cliTarball }) {
  await writeFile(
    join(installDirectory, 'package.json'),
    `${JSON.stringify({ private: true, type: 'module' }, null, 2)}\n`
  )
  // Install both local tarballs together so npm satisfies @intlify/cli's native
  // dependency from the tarball produced by the same source tree.
  await run(
    'npm',
    ['install', '--ignore-scripts', '--no-audit', '--no-fund', nativeTarball, cliTarball],
    { cwd: installDirectory }
  )

  return process.platform === 'win32'
    ? join(installDirectory, 'node_modules', '.bin', 'intlify.cmd')
    : join(installDirectory, 'node_modules', '.bin', 'intlify')
}

async function measurePhase(phase, { sampleCount, expectedVersion }) {
  const samples = []
  for (let index = 0; index < sampleCount; index++) {
    const start = performance.now()
    const result = await run(phase.command[0], phase.command.slice(1), { capture: true })
    const duration = performance.now() - start
    assertStdoutEquals(result, expectedVersion)
    samples.push(duration)
  }

  return {
    name: phase.name,
    commandLine: phase.command.map(shellQuote).join(' '),
    nativeBinaryPath: phase.nativeBinaryPath,
    ...(phase.nodeVersion ? { nodeVersion: phase.nodeVersion } : {}),
    ...(phase.npmVersion ? { npmVersion: phase.npmVersion } : {}),
    timing: summarize(samples)
  }
}

function summarize(samples) {
  const sorted = [...samples].sort((left, right) => left - right)
  const mean = samples.reduce((sum, sample) => sum + sample, 0) / samples.length
  return {
    samples: samples.map(round),
    minMs: round(sorted[0]),
    maxMs: round(sorted.at(-1)),
    meanMs: round(mean),
    medianMs: round(percentile(sorted, 0.5)),
    p95Ms: round(percentile(sorted, 0.95))
  }
}

function percentile(sortedSamples, percentileValue) {
  const index = Math.min(
    sortedSamples.length - 1,
    Math.max(0, Math.ceil(sortedSamples.length * percentileValue) - 1)
  )
  return sortedSamples[index]
}

function round(value) {
  return Math.round(value * 1000) / 1000
}

async function readJson(path) {
  return JSON.parse(await readFile(path, 'utf8'))
}

async function readCargoPackageVersion() {
  const manifest = await readFile(cargoManifestPath, 'utf8')
  const match = manifest.match(/^version = "([^"]+)"$/m)
  if (!match) {
    throw new Error('crates/intlify_cli/Cargo.toml is missing package version')
  }
  return match[1]
}

async function cliVersion() {
  return (await readJson(cliPackageJsonPath)).version
}

async function gitCommit() {
  const result = await run('git', ['rev-parse', '--short', 'HEAD'], { capture: true })
  return result.stdout.trim()
}

async function toolVersion(tool) {
  const result = await run(tool, ['--version'], { capture: true })
  return result.stdout.trim()
}

async function run(commandName, args, options = {}) {
  const cwd = options.cwd ?? workspaceRoot
  const allowExitCodes = options.allowExitCodes ?? [0]
  const capture = options.capture ?? false

  return new Promise((resolve, reject) => {
    const child = spawn(commandName, args, {
      cwd,
      stdio: capture ? ['ignore', 'pipe', 'pipe'] : 'inherit'
    })
    let stdout = ''
    let stderr = ''

    if (capture) {
      child.stdout.on('data', chunk => {
        stdout += chunk
      })
      child.stderr.on('data', chunk => {
        stderr += chunk
      })
    }

    child.once('error', reject)
    child.once('close', code => {
      if (!allowExitCodes.includes(code)) {
        reject(
          new Error(
            `${commandName} ${args.join(' ')} failed with exit code ${code}` +
              (capture && stderr ? `\n${stderr.trimEnd()}` : '')
          )
        )
        return
      }
      resolve({ code, stdout, stderr })
    })
  })
}

function assertStdoutEquals(result, expectedVersion) {
  assertEqual(result.stdout, `${expectedVersion}\n`, 'CLI version output')
  assertEqual(result.stderr, '', 'CLI stderr')
}

function assertEqual(actual, expected, label) {
  if (actual !== expected) {
    throw new Error(
      `${label} mismatch: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`
    )
  }
}

function assertJsonEqual(actual, expected, label) {
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(
      `${label} mismatch: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`
    )
  }
}

function shellQuote(value) {
  if (/^[\w./:=@+-]+$/.test(value)) {
    return value
  }
  return `'${value.replaceAll("'", "'\\''")}'`
}

function compareStrings(left, right) {
  return left.localeCompare(right)
}
