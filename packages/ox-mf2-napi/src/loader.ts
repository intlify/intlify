/**
 * @license MIT
 * @author kazuya kawaguchi (a.k.a. kazupon)
 */

import { createRequire } from 'node:module'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

/**
 * Native N-API functions loaded from the platform binary.
 */
export type NativeBinding = {
  /**
   * Parse one normalized input into snapshot bytes.
   *
   * @param input - Normalized source object.
   * @param options - Normalized parse options.
   * @returns Native snapshot result.
   */
  parseMessage(input: unknown, options?: unknown): NativeSnapshotResult

  /**
   * Parse normalized inputs into one batch snapshot.
   *
   * @param items - Normalized source objects.
   * @param options - Normalized batch parse options.
   * @returns Native snapshot result.
   */
  parseBatch(items: unknown[], options?: unknown): NativeSnapshotResult

  /**
   * Validate and normalize existing snapshot bytes.
   *
   * @param bytes - Snapshot bytes to decode.
   * @param options - Optional native decode options.
   * @returns Native snapshot result.
   */
  decodeSnapshot(bytes: Uint8Array, options?: unknown): NativeSnapshotResult

  /**
   * Copy snapshot bytes from a native snapshot object.
   *
   * @param snapshot - Native snapshot object.
   * @returns Copied snapshot bytes.
   */
  snapshotToBytes(snapshot: unknown): Uint8Array
}

/** Snapshot result returned by the native binding boundary. */
export type NativeSnapshotResult = {
  /** Snapshot bytes produced or validated by native code. */
  readonly bytes: Uint8Array
  /** Optional root ids reported by native code. */
  readonly roots?: number[]
  /** Optional effective batch execution mode. */
  readonly execution?: 'sequential' | 'parallel'
  /** Optional indication that requested execution degraded. */
  readonly degraded?: boolean
}

type NativeLoadResult = {
  readonly binding: NativeBinding | null
  readonly error: unknown
}

const require = createRequire(import.meta.url)
let cached: NativeLoadResult | undefined

/**
 * Load the native binding for the current platform.
 *
 * @returns Loaded binding or the collected load error.
 */
export function loadNativeBinding(): NativeLoadResult {
  if (process.env.OX_MF2_NAPI_FORCE_MISSING === '1') {
    return {
      binding: null,
      error: new Error('native binding load was forced off')
    }
  }
  cached ??= tryLoadNativeBinding()
  return cached
}

function tryLoadNativeBinding(): NativeLoadResult {
  const errors: unknown[] = []
  for (const specifier of candidateSpecifiers()) {
    try {
      return {
        binding: require(specifier) as NativeBinding,
        error: null
      }
    } catch (error) {
      errors.push(error)
    }
  }
  return {
    binding: null,
    error: errors
  }
}

function candidateSpecifiers(): string[] {
  const specifiers = [
    optionalPackageName(),
    join(bindingDir(), localBinaryName()),
    join(bindingDir(), 'ox_mf2_napi.node')
  ]

  if (process.env.OX_MF2_NAPI_ALLOW_LOCAL_TARGET === '1') {
    specifiers.push(
      join(packageDir(), '..', '..', 'target', 'debug', 'ox_mf2_napi.node'),
      join(packageDir(), '..', '..', 'target', 'release', 'ox_mf2_napi.node')
    )
  }

  return specifiers
}

function bindingDir(): string {
  return fileURLToPath(new URL('../dist/', import.meta.url))
}

function packageDir(): string {
  return fileURLToPath(new URL('..', import.meta.url))
}

function optionalPackageName(): string {
  const platform = process.platform
  const arch = process.arch

  if (platform === 'darwin' && arch === 'arm64') {
    return '@intlify/ox-mf2-napi-darwin-arm64'
  }
  if (platform === 'darwin' && arch === 'x64') {
    return '@intlify/ox-mf2-napi-darwin-x64'
  }
  if (platform === 'linux' && arch === 'x64') {
    return isMusl() ? '@intlify/ox-mf2-napi-linux-x64-musl' : '@intlify/ox-mf2-napi-linux-x64-gnu'
  }
  if (platform === 'linux' && arch === 'arm64') {
    return isMusl() ? '@intlify/ox-mf2-napi-unsupported' : '@intlify/ox-mf2-napi-linux-arm64-gnu'
  }
  if (platform === 'win32' && arch === 'x64') {
    return '@intlify/ox-mf2-napi-win32-x64-msvc'
  }
  return '@intlify/ox-mf2-napi-unsupported'
}

function localBinaryName(): string {
  const platform = process.platform
  const arch = process.arch
  if (platform === 'darwin') {
    return `ox_mf2_napi.darwin-${arch}.node`
  }
  if (platform === 'linux') {
    const libc = isMusl() ? 'musl' : 'gnu'
    return `ox_mf2_napi.linux-${arch}-${libc}.node`
  }
  if (platform === 'win32') {
    return `ox_mf2_napi.win32-${arch}-msvc.node`
  }
  return 'ox_mf2_napi.node'
}

function isMusl(): boolean {
  const report = process.report?.getReport() as
    | { header?: { glibcVersionRuntime?: string } }
    | undefined
  return !report?.header?.glibcVersionRuntime
}
