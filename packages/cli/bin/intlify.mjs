#!/usr/bin/env node
// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

import { spawn } from 'node:child_process'
import { existsSync, readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const require = createRequire(import.meta.url)
const packageRoot = fileURLToPath(new URL('..', import.meta.url))
const workspacePackagesRoot = dirname(packageRoot)
const PACKAGE_VERSION = JSON.parse(
  readFileSync(new URL('../package.json', import.meta.url), 'utf8')
).version
const OUTPUT_SCHEMA_VERSION = '0'
const SIGNAL_EXIT_CODES = {
  SIGHUP: 129,
  SIGINT: 130,
  SIGTERM: 143
}

export const NATIVE_TARGETS = [
  {
    platform: 'darwin',
    arch: 'x64',
    packageName: '@intlify/cli-darwin-x64',
    packageDirectory: 'cli-darwin-x64',
    binaryName: 'intlify'
  },
  {
    platform: 'darwin',
    arch: 'arm64',
    packageName: '@intlify/cli-darwin-arm64',
    packageDirectory: 'cli-darwin-arm64',
    binaryName: 'intlify'
  },
  {
    platform: 'linux',
    arch: 'x64',
    libc: 'glibc',
    packageName: '@intlify/cli-linux-x64-gnu',
    packageDirectory: 'cli-linux-x64-gnu',
    binaryName: 'intlify'
  },
  {
    platform: 'linux',
    arch: 'arm64',
    libc: 'glibc',
    packageName: '@intlify/cli-linux-arm64-gnu',
    packageDirectory: 'cli-linux-arm64-gnu',
    binaryName: 'intlify'
  },
  {
    platform: 'linux',
    arch: 'x64',
    libc: 'musl',
    packageName: '@intlify/cli-linux-x64-musl',
    packageDirectory: 'cli-linux-x64-musl',
    binaryName: 'intlify'
  },
  {
    platform: 'win32',
    arch: 'x64',
    packageName: '@intlify/cli-win32-x64-msvc',
    packageDirectory: 'cli-win32-x64-msvc',
    binaryName: 'intlify.exe'
  }
]

/**
 * Resolve and execute the target-specific native CLI binary.
 *
 * @param options - Runtime hooks used by tests and by the real wrapper.
 * @returns Spawned native process, or undefined when startup fails.
 */
export function runWrapper(options = {}) {
  const argv = options.argv ?? process.argv.slice(2)
  const cwd = options.cwd ?? process.cwd()
  const env = options.env ?? process.env
  const stdout = options.stdout ?? process.stdout
  const stderr = options.stderr ?? process.stderr
  const exit = options.exit ?? process.exit
  const spawnProcess = options.spawn ?? spawn
  const resolution = resolveNativeBinary(options)

  if (!resolution.ok) {
    writeWrapperError(resolution.error, { argv, cwd, stdout, stderr })
    exit(2)
    return undefined
  }

  let child
  try {
    child = spawnProcess(resolution.binaryPath, argv, {
      env,
      stdio: options.stdio ?? 'inherit'
    })
  } catch (cause) {
    writeWrapperError(
      nativeBinaryFailedError({
        target: resolution.target,
        binaryPath: resolution.binaryPath,
        cause
      }),
      { argv, cwd, stdout, stderr }
    )
    exit(2)
    return undefined
  }

  child.once('error', cause => {
    writeWrapperError(
      nativeBinaryFailedError({
        target: resolution.target,
        binaryPath: resolution.binaryPath,
        cause
      }),
      { argv, cwd, stdout, stderr }
    )
    exit(2)
  })

  child.once('exit', (code, signal) => {
    exit(exitCodeForNativeProcess(code, signal))
  })

  if (options.forwardSignals !== false) {
    for (const signal of ['SIGHUP', 'SIGINT', 'SIGTERM']) {
      process.once(signal, () => {
        child.kill(signal)
      })
    }
  }

  return child
}

/**
 * Resolve the native package and binary path for the current platform tuple.
 *
 * @param options - Runtime hooks and platform overrides.
 * @returns Successful binary resolution or a wrapper-level operational error.
 */
export function resolveNativeBinary(options = {}) {
  const platform = options.platform ?? process.platform
  const arch = options.arch ?? process.arch
  const libc = platform === 'linux' ? (options.libc ?? detectLinuxLibc(options)) : undefined
  const target = resolveNativeTarget({ platform, arch, libc })

  if (!target) {
    return {
      ok: false,
      error: wrapperError({
        code: 'native_platform_unsupported',
        message: 'No intlify native CLI package is available for this platform.',
        details: platformDetails({ platform, arch, libc })
      })
    }
  }

  let packageJsonPath
  const exists = options.existsSync ?? existsSync
  try {
    packageJsonPath = options.resolvePackageJson
      ? options.resolvePackageJson(target.packageName)
      : resolvePackageJson(target, exists)
  } catch {
    return {
      ok: false,
      error: wrapperError({
        code: 'native_package_not_found',
        message: `Native package ${target.packageName} could not be resolved.`,
        details: platformDetails({
          platform,
          arch,
          libc,
          packageName: target.packageName
        })
      })
    }
  }

  const binaryPath = join(dirname(packageJsonPath), target.binaryName)
  if (!exists(binaryPath)) {
    return {
      ok: false,
      error: wrapperError({
        code: 'native_binary_not_found',
        message: `Native CLI binary was not found at ${binaryPath}.`,
        details: platformDetails({
          platform,
          arch,
          libc,
          packageName: target.packageName,
          binaryPath
        })
      })
    }
  }

  return {
    ok: true,
    target,
    binaryPath
  }
}

function resolvePackageJson(target, exists) {
  try {
    return require.resolve(`${target.packageName}/package.json`)
  } catch {
    // Source-tree wrapper runs before native packages are installed from npm;
    // PR4 build tasks copy the binary into this sibling package directory.
    const sourceTreePath = join(workspacePackagesRoot, target.packageDirectory, 'package.json')
    if (exists(sourceTreePath)) {
      return sourceTreePath
    }
    throw new Error(`native package ${target.packageName} was not found`)
  }
}

/**
 * Resolve the package metadata row for a platform, architecture, and libc tuple.
 *
 * @param target - Platform tuple to resolve.
 * @returns Matching native target metadata, or null when unsupported.
 */
export function resolveNativeTarget({ platform, arch, libc } = {}) {
  return (
    NATIVE_TARGETS.find(target => {
      if (target.platform !== platform || target.arch !== arch) {
        return false
      }
      return target.libc === undefined || target.libc === libc
    }) ?? null
  )
}

/**
 * Detect the Linux libc family from Node's runtime report.
 *
 * @param options - Runtime hooks used by tests.
 * @returns Detected libc, or undefined when detection is unavailable.
 */
export function detectLinuxLibc(options = {}) {
  const getReport = options.getReport ?? process.report?.getReport?.bind(process.report)
  try {
    const report = getReport?.()
    if (!report?.header) {
      return undefined
    }
    if (report?.header?.glibcVersionRuntime) {
      return 'glibc'
    }
    return 'musl'
  } catch {
    return undefined
  }
}

/**
 * Check whether argv explicitly requests the JSON reporter.
 *
 * @param argv - Command line arguments forwarded to the native CLI.
 * @returns Whether wrapper-level failures should use JSON output.
 */
export function usesJsonReporter(argv) {
  return argv.some((arg, index) => {
    return arg === '--reporter=json' || (arg === '--reporter' && argv[index + 1] === 'json')
  })
}

/**
 * Build the standard JSON output envelope for wrapper-level startup failures.
 *
 * @param error - Operational error payload.
 * @param context - Envelope context.
 * @returns Machine-readable CLI output envelope.
 */
export function buildErrorEnvelope(error, { cwd, version = PACKAGE_VERSION } = {}) {
  return {
    schemaVersion: OUTPUT_SCHEMA_VERSION,
    command: 'intlify',
    version,
    projectRoot: slashNormalizePath(cwd ?? process.cwd()),
    summary: {
      status: 'error'
    },
    results: [],
    errors: [error]
  }
}

/**
 * Normalize a filesystem path for machine-readable output.
 *
 * @param path - Filesystem path.
 * @returns Slash-normalized path.
 */
export function slashNormalizePath(path) {
  return path.replaceAll('\\', '/')
}

function writeWrapperError(error, { argv, cwd, stdout, stderr }) {
  if (usesJsonReporter(argv)) {
    stdout.write(`${JSON.stringify(buildErrorEnvelope(error, { cwd }))}\n`)
  } else {
    stderr.write(`error: ${error.message}\n`)
  }
}

function wrapperError({ code, message, details }) {
  return {
    kind: 'io',
    code,
    message,
    details
  }
}

function nativeBinaryFailedError({ target, binaryPath, cause }) {
  return wrapperError({
    code: 'native_binary_failed',
    message: `Native CLI binary could not be executed at ${binaryPath}.`,
    details: platformDetails({
      platform: target.platform,
      arch: target.arch,
      libc: target.libc,
      packageName: target.packageName,
      binaryPath,
      cause: cause?.message
    })
  })
}

function platformDetails({ platform, arch, libc, packageName, binaryPath, cause }) {
  return {
    projectRootSource: 'cwd-fallback',
    platform,
    arch,
    ...(libc ? { libc } : {}),
    ...(packageName ? { packageName } : {}),
    ...(binaryPath ? { binaryPath: slashNormalizePath(binaryPath) } : {}),
    ...(cause ? { cause } : {})
  }
}

function exitCodeForNativeProcess(code, signal) {
  if (typeof code === 'number') {
    return code
  }
  return SIGNAL_EXIT_CODES[signal] ?? 1
}

const isMain = process.argv[1]
  ? import.meta.url === pathToFileURL(fileURLToPath(import.meta.url)).href &&
    fileURLToPath(import.meta.url) === process.argv[1]
  : false

if (isMain) {
  runWrapper()
}
