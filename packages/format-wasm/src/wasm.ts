/**
 * @license MIT
 * @author kazuya kawaguchi (a.k.a. kazupon)
 */

import { OxMf2ErrorCode, OxMf2InitializationError } from '@intlify/ox-mf2-shared'

type WasmSpan = {
  readonly start: number
  readonly end: number
}

type WasmSourceLocation = {
  readonly line: number
  readonly column: number
}

type WasmDiagnosticLabel = {
  readonly sourceId: number
  readonly span: WasmSpan
  readonly message: string | null
}

type WasmDiagnostic = {
  readonly rootId: number
  readonly sourceId: number
  readonly severity: number
  readonly code: number
  readonly message: string | null
  readonly span: WasmSpan
  readonly location: WasmSourceLocation | null
  readonly labels: WasmDiagnosticLabel[]
}

type WasmOperationalErrorDetail = {
  readonly key: string
  readonly valueJson: string
}

type WasmOperationalError = {
  readonly kind: string
  readonly code: string
  readonly message: string
  readonly path?: string | null
  readonly details: WasmOperationalErrorDetail[]
}

/** Native WASM result returned by APIs that produce formatted text. */
export type WasmNativeFormatResult = {
  /** Whether the formatter completed successfully. */
  readonly ok: boolean
  /** Formatted source text for successful formatting results. */
  readonly code?: string | null
  /** Whether formatted output differs from the supplied source. */
  readonly changed?: boolean | null
  /** Parser diagnostics returned for failed formatting results. */
  readonly diagnostics: WasmDiagnostic[]
  /** Operational formatter errors returned for failed formatting results. */
  readonly errors: WasmOperationalError[]
}

/** Native WASM result returned by APIs that only report changed state. */
export type WasmNativeFormatCheckResult = {
  /** Whether the formatter completed successfully. */
  readonly ok: boolean
  /** Whether formatting would change the supplied source. */
  readonly changed?: boolean | null
  /** Parser diagnostics returned for failed check results. */
  readonly diagnostics: WasmDiagnostic[]
  /** Operational formatter errors returned for failed check results. */
  readonly errors: WasmOperationalError[]
}

/** Generated wasm-bindgen module surface used after initialization. */
export type WasmBinding = {
  /** Format source text with the native WASM formatter. */
  readonly formatMessage: (source: string, options: unknown) => WasmNativeFormatResult
  /** Check source text with the native WASM formatter. */
  readonly checkFormat: (source: string, options: unknown) => WasmNativeFormatCheckResult
  /** Format source text using serialized snapshot bytes. */
  readonly formatSnapshot: (
    snapshot: Uint8Array,
    source: string,
    options: unknown
  ) => WasmNativeFormatResult
  /** Check source text using serialized snapshot bytes. */
  readonly checkSnapshot: (
    snapshot: Uint8Array,
    source: string,
    options: unknown
  ) => WasmNativeFormatCheckResult
}

let binding: WasmBinding | null = null

/**
 * Store the initialized wasm-bindgen module for synchronous formatter calls.
 *
 * @param value - Generated wasm-bindgen module.
 */
export function setWasmBinding(value: WasmBinding): void {
  binding = value
}

/**
 * Return the initialized wasm-bindgen module.
 *
 * @returns Generated wasm-bindgen module.
 */
export function getWasmBinding(): WasmBinding {
  if (!binding) {
    throw new OxMf2InitializationError({
      code: OxMf2ErrorCode.InitializationWasmNotInitialized,
      message: 'format WASM runtime has not been initialized'
    })
  }
  return binding
}
