/**
 * @license MIT
 * @author kazuya kawaguchi (a.k.a. kazupon)
 */

import { createRequire } from 'node:module'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

type NativeFormatOptions = {
  readonly mode?: 'standard' | 'preserve'
}

type NativeSpan = {
  readonly start: number
  readonly end: number
}

type NativeSourceLocation = {
  readonly line: number
  readonly column: number
}

type NativeDiagnosticLabel = {
  readonly sourceId: number
  readonly span: NativeSpan
  readonly message?: string | null
}

type NativeDiagnostic = {
  readonly rootId: number
  readonly sourceId: number
  readonly severity: number
  readonly code: number
  readonly message?: string | null
  readonly span: NativeSpan
  readonly location?: NativeSourceLocation | null
  readonly labels: NativeDiagnosticLabel[]
}

type NativeOperationalErrorDetail = {
  readonly key: string
  readonly valueJson: string
}

type NativeOperationalError = {
  readonly kind: string
  readonly code: string
  readonly message: string
  readonly path?: string | null
  readonly details: NativeOperationalErrorDetail[]
}

type NativeFormatResult = {
  readonly ok: boolean
  readonly code?: string | null
  readonly changed?: boolean | null
  readonly diagnostics: NativeDiagnostic[]
  readonly errors: NativeOperationalError[]
}

type NativeFormatCheckResult = {
  readonly ok: boolean
  readonly changed?: boolean | null
  readonly diagnostics: NativeDiagnostic[]
  readonly errors: NativeOperationalError[]
}

/** Native N-API functions loaded from the platform binary. */
export type NativeBinding = {
  /** Format one complete MF2 message. */
  formatMessage(source: string, options?: NativeFormatOptions): NativeFormatResult

  /** Check whether one complete MF2 message would change. */
  checkFormat(source: string, options?: NativeFormatOptions): NativeFormatCheckResult

  /** Format one complete MF2 message using serialized snapshot bytes. */
  formatSnapshot(
    snapshot: Uint8Array,
    source: string,
    options?: NativeFormatOptions
  ): NativeFormatResult

  /** Check snapshot-backed formatting without returning formatted text. */
  checkSnapshot(
    snapshot: Uint8Array,
    source: string,
    options?: NativeFormatOptions
  ): NativeFormatCheckResult
}

type NativeLoadResult = {
  readonly binding: NativeBinding | null
  readonly error: unknown
}

const require = createRequire(import.meta.url)
let cached: NativeLoadResult | undefined

/**
 * Load the formatter native binding for the current platform.
 *
 * @returns Loaded binding or the collected load error.
 */
export function loadNativeBinding(): NativeLoadResult {
  if (process.env.INTLIFY_FORMAT_NAPI_FORCE_MISSING === '1') {
    return {
      binding: null,
      error: new Error('formatter native binding load was forced off')
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
    join(bindingDir(), 'intlify_format_napi.node')
  ]

  if (process.env.INTLIFY_FORMAT_NAPI_ALLOW_LOCAL_TARGET === '1') {
    specifiers.push(
      join(packageDir(), '..', '..', 'target', 'debug', 'intlify_format_napi.node'),
      join(packageDir(), '..', '..', 'target', 'release', 'intlify_format_napi.node')
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
    return '@intlify/format-napi-darwin-arm64'
  }
  if (platform === 'darwin' && arch === 'x64') {
    return '@intlify/format-napi-darwin-x64'
  }
  if (platform === 'linux' && arch === 'x64') {
    return isMusl() ? '@intlify/format-napi-linux-x64-musl' : '@intlify/format-napi-linux-x64-gnu'
  }
  if (platform === 'linux' && arch === 'arm64') {
    return isMusl() ? '@intlify/format-napi-unsupported' : '@intlify/format-napi-linux-arm64-gnu'
  }
  if (platform === 'win32' && arch === 'x64') {
    return '@intlify/format-napi-win32-x64-msvc'
  }
  return '@intlify/format-napi-unsupported'
}

function localBinaryName(): string {
  const platform = process.platform
  const arch = process.arch
  if (platform === 'darwin') {
    return `intlify_format_napi.darwin-${arch}.node`
  }
  if (platform === 'linux') {
    const libc = isMusl() ? 'musl' : 'gnu'
    return `intlify_format_napi.linux-${arch}-${libc}.node`
  }
  if (platform === 'win32') {
    return `intlify_format_napi.win32-${arch}-msvc.node`
  }
  return 'intlify_format_napi.node'
}

function isMusl(): boolean {
  const report = process.report?.getReport() as
    | { header?: { glibcVersionRuntime?: string } }
    | undefined
  return !report?.header?.glibcVersionRuntime
}
