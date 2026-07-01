#!/usr/bin/env node
// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

import { spawn } from 'node:child_process'
import { existsSync, readFileSync, realpathSync } from 'node:fs'
import { createRequire } from 'node:module'
import { constants as osConstants } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const require = createRequire(import.meta.url)
const packageRoot = fileURLToPath(new URL('..', import.meta.url))
const workspacePackagesRoot = dirname(packageRoot)
export const NATIVE_PACKAGE_NAME = '@intlify/cli-native'
export const NATIVE_PACKAGE_DIRECTORY = 'cli-native'
export const NATIVE_PACKAGE_BINARY_DIRECTORY = 'bin'
const PACKAGE_VERSION = JSON.parse(
  readFileSync(new URL('../package.json', import.meta.url), 'utf8')
).version
const OUTPUT_SCHEMA_VERSION = '0'
const SIGNAL_EXIT_CODES = {
  ...Object.fromEntries(
    Object.entries(osConstants.signals).map(([signal, value]) => [signal, 128 + value])
  ),
  SIGHUP: 129,
  SIGINT: 130,
  SIGTERM: 143
}

export const NATIVE_TARGETS = [
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
      ? options.resolvePackageJson(NATIVE_PACKAGE_NAME)
      : resolvePackageJson(exists)
  } catch {
    return {
      ok: false,
      error: wrapperError({
        code: 'native_package_not_found',
        message: `Native package ${NATIVE_PACKAGE_NAME} could not be resolved.`,
        details: platformDetails({
          platform,
          arch,
          libc,
          packageName: NATIVE_PACKAGE_NAME
        })
      })
    }
  }

  const binaryPath = join(
    dirname(packageJsonPath),
    NATIVE_PACKAGE_BINARY_DIRECTORY,
    target.rustTarget,
    target.binaryName
  )
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
          packageName: NATIVE_PACKAGE_NAME,
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

function resolvePackageJson(exists) {
  try {
    return require.resolve(`${NATIVE_PACKAGE_NAME}/package.json`)
  } catch {
    // Source-tree wrapper runs before the native package is installed from npm;
    // PR4 build tasks copy host binaries into this sibling package directory.
    const sourceTreePath = join(workspacePackagesRoot, NATIVE_PACKAGE_DIRECTORY, 'package.json')
    if (exists(sourceTreePath)) {
      return sourceTreePath
    }
    throw new Error(`native package ${NATIVE_PACKAGE_NAME} was not found`)
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
      packageName: NATIVE_PACKAGE_NAME,
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

/**
 * Check whether this module is the process entry point.
 *
 * @param entryPath - Process entry path from argv.
 * @param moduleUrl - Current module URL.
 * @returns Whether the entry path resolves to this module.
 */
export function isMainEntryPath(entryPath, moduleUrl = import.meta.url) {
  if (!entryPath) {
    return false
  }

  const modulePath = fileURLToPath(moduleUrl)
  try {
    return realpathSync(modulePath) === realpathSync(entryPath)
  } catch {
    return modulePath === entryPath
  }
}

if (isMainEntryPath(process.argv[1])) {
  runWrapper()
}
