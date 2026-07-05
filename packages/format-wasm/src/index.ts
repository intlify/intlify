/**
 * @license MIT
 * @author kazuya kawaguchi (a.k.a. kazupon)
 */

import { init } from './init.ts'
import { getWasmBinding } from './wasm.ts'

import type { DiagnosticView } from '@intlify/ox-mf2-shared'
import type {
  FormatCheckResult,
  FormatFailure,
  FormatOptions,
  FormatResult,
  FormatterOperationalError
} from './types.ts'
import type { WasmNativeFormatCheckResult, WasmNativeFormatResult } from './wasm.ts'

export { OxMf2ErrorCode, OxMf2InitializationError } from './errors.ts'
export { init }
export type {
  DiagnosticView,
  FormatCheckResult,
  FormatFailure,
  FormatMode,
  FormatOptions,
  FormatResult,
  FormatterOperationalError
} from './types.ts'

const ALLOWED_OPTION_FIELDS = ['mode'] as const
const ALLOWED_MODES = ['standard', 'preserve'] as const

type WasmOperationalError = WasmNativeFormatResult['errors'][number]
type WasmDiagnostic = WasmNativeFormatResult['diagnostics'][number]
type WasmDiagnosticLabel = WasmDiagnostic['labels'][number]
type NormalizedFormatOptions = {
  readonly mode: (typeof ALLOWED_MODES)[number]
}

/**
 * Format one complete MF2 message with the WASM formatter.
 *
 * @param source - Complete MF2 message source text.
 * @param options - Optional formatter options.
 * @returns Formatted output or parser/operational failures.
 */
export function formatMessage(source: string, options?: FormatOptions): FormatResult {
  const sourceError = validateSource(source)
  if (sourceError) {
    return sourceError
  }
  const normalizedOptions = validateFormatOptions(options)
  if (isFormatFailure(normalizedOptions)) {
    return normalizedOptions
  }
  return createFormatResult(getWasmBinding().formatMessage(source, normalizedOptions))
}

/**
 * Check whether one complete MF2 message would change after formatting.
 *
 * @param source - Complete MF2 message source text.
 * @param options - Optional formatter options.
 * @returns Change status or parser/operational failures.
 */
export function checkFormat(source: string, options?: FormatOptions): FormatCheckResult {
  const sourceError = validateSource(source)
  if (sourceError) {
    return sourceError
  }
  const normalizedOptions = validateFormatOptions(options)
  if (isFormatFailure(normalizedOptions)) {
    return normalizedOptions
  }
  return createCheckResult(getWasmBinding().checkFormat(source, normalizedOptions))
}

/**
 * Format one complete MF2 message using serialized Binary AST snapshot bytes.
 *
 * @param snapshot - Serialized Binary AST snapshot bytes.
 * @param source - Complete MF2 message source text corresponding to the snapshot.
 * @param options - Optional formatter options.
 * @returns Formatted output or parser/operational failures.
 */
export function formatSnapshot(
  snapshot: Uint8Array,
  source: string,
  options?: FormatOptions
): FormatResult {
  const inputError = validateSnapshotInput(snapshot, source)
  if (inputError) {
    return inputError
  }
  const normalizedOptions = validateFormatOptions(options)
  if (isFormatFailure(normalizedOptions)) {
    return normalizedOptions
  }
  return createFormatResult(
    getWasmBinding().formatSnapshot(snapshot.slice(), source, normalizedOptions)
  )
}

/**
 * Check snapshot-backed formatting without returning formatted text.
 *
 * @param snapshot - Serialized Binary AST snapshot bytes.
 * @param source - Complete MF2 message source text corresponding to the snapshot.
 * @param options - Optional formatter options.
 * @returns Change status or parser/operational failures.
 */
export function checkSnapshot(
  snapshot: Uint8Array,
  source: string,
  options?: FormatOptions
): FormatCheckResult {
  const inputError = validateSnapshotInput(snapshot, source)
  if (inputError) {
    return inputError
  }
  const normalizedOptions = validateFormatOptions(options)
  if (isFormatFailure(normalizedOptions)) {
    return normalizedOptions
  }
  return createCheckResult(
    getWasmBinding().checkSnapshot(snapshot.slice(), source, normalizedOptions)
  )
}

function validateSource(source: unknown): FormatFailure | null {
  if (typeof source === 'string') {
    return null
  }
  return invalidOptions('formatter source must be a string', {
    pointer: '/source',
    expectedType: 'string',
    actualType: valueType(source)
  })
}

function validateSnapshotInput(snapshot: unknown, source: unknown): FormatFailure | null {
  if (!(snapshot instanceof Uint8Array)) {
    return invalidOptions('formatter snapshot must be a Uint8Array', {
      pointer: '/snapshot',
      expectedType: 'Uint8Array',
      actualType: valueType(snapshot)
    })
  }
  return validateSource(source)
}

function validateFormatOptions(options: unknown): NormalizedFormatOptions | FormatFailure {
  if (options === undefined) {
    return { mode: 'standard' }
  }
  if (options === null || typeof options !== 'object' || Array.isArray(options)) {
    return invalidOptions('formatter options must be an object', {
      pointer: '/options',
      expectedType: 'object',
      actualType: valueType(options)
    })
  }

  const rawOptions = options as Record<string, unknown>

  for (const field of Object.keys(rawOptions)) {
    if (!ALLOWED_OPTION_FIELDS.includes(field as (typeof ALLOWED_OPTION_FIELDS)[number])) {
      return invalidOptions('formatter options contain an unknown field', {
        pointer: `/${field}`,
        reason: 'unknown_field',
        allowedFields: [...ALLOWED_OPTION_FIELDS]
      })
    }
  }

  if (rawOptions.mode === undefined) {
    return { mode: 'standard' }
  }
  if (!ALLOWED_MODES.includes(rawOptions.mode as (typeof ALLOWED_MODES)[number])) {
    return invalidOptions('formatter mode must be "standard" or "preserve"', {
      pointer: '/mode',
      value: rawOptions.mode,
      allowedValues: [...ALLOWED_MODES]
    })
  }
  return { mode: rawOptions.mode as NormalizedFormatOptions['mode'] }
}

function createFormatResult(native: WasmNativeFormatResult): FormatResult {
  if (native.ok) {
    return {
      ok: true,
      code: native.code ?? '',
      changed: native.changed ?? false
    }
  }
  return {
    ok: false,
    diagnostics: native.diagnostics.map(createDiagnostic),
    errors: native.errors.map(createOperationalError)
  }
}

function createCheckResult(native: WasmNativeFormatCheckResult): FormatCheckResult {
  if (native.ok) {
    return {
      ok: true,
      changed: native.changed ?? false
    }
  }
  return {
    ok: false,
    diagnostics: native.diagnostics.map(createDiagnostic),
    errors: native.errors.map(createOperationalError)
  }
}

function createDiagnostic(diagnostic: WasmDiagnostic): DiagnosticView {
  return {
    rootId: diagnostic.rootId,
    sourceId: diagnostic.sourceId,
    severity: diagnostic.severity as DiagnosticView['severity'],
    code: diagnostic.code as DiagnosticView['code'],
    message: diagnostic.message ?? null,
    span: diagnostic.span,
    location: diagnostic.location ?? null,
    labels: diagnostic.labels.map(createDiagnosticLabel)
  }
}

function createDiagnosticLabel(label: WasmDiagnosticLabel): DiagnosticView['labels'][number] {
  return {
    span: label.span,
    message: label.message ?? null
  }
}

function createOperationalError(error: WasmOperationalError): FormatterOperationalError {
  const details = Object.fromEntries(error.details.map(detail => [detail.key, detail.value]))
  return {
    kind: error.kind,
    code: error.code,
    message: error.message,
    path: error.path ?? undefined,
    ...(Object.keys(details).length > 0 ? { details } : {})
  }
}

function invalidOptions(message: string, details: Record<string, unknown>): FormatFailure {
  return {
    ok: false,
    diagnostics: [],
    errors: [
      {
        kind: 'input',
        code: 'invalid_options',
        message,
        details
      }
    ]
  }
}

function isFormatFailure(value: NormalizedFormatOptions | FormatFailure): value is FormatFailure {
  return 'ok' in value && value.ok === false
}

function valueType(value: unknown): string {
  if (value === null) {
    return 'null'
  }
  if (value instanceof Uint8Array) {
    return 'Uint8Array'
  }
  if (Array.isArray(value)) {
    return 'array'
  }
  return typeof value
}
