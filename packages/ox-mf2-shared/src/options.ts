/**
 * @license MIT
 * @author kazuya kawaguchi (a.k.a. kazupon)
 */

import { assertNoUnpairedSurrogates } from './source-text.ts'

import type {
  BatchExecution,
  NormalizedParseBatchOptions,
  NormalizedParseInputObject,
  NormalizedParseMessageOptions,
  ParseBatchOptions,
  ParseInputObject,
  ParseMessageOptions
} from './types.ts'

const U32_MAX = 0xffff_ffff

const DEFAULT_PARSE_OPTIONS: NormalizedParseMessageOptions = {
  collectTrivia: true,
  includeTrivia: true,
  includeDiagnostics: true,
  includeSourceText: false
}

const PARSE_MESSAGE_OPTION_KEYS = new Set([
  'collectTrivia',
  'includeTrivia',
  'includeDiagnostics',
  'includeSourceText'
])

const PARSE_BATCH_OPTION_KEYS = new Set([...PARSE_MESSAGE_OPTION_KEYS, 'batchExecution'])

const PARSE_INPUT_KEYS = new Set(['source', 'path', 'locale', 'messageId', 'baseOffset'])

/**
 * Validate and normalize options for parsing one message.
 *
 * @param options - User-provided parse options.
 * @returns Normalized parse options with defaults applied.
 */
export function validateParseMessageOptions(
  options?: ParseMessageOptions
): NormalizedParseMessageOptions {
  const input = normalizeOptionsObject(options, 'options')
  assertKnownKeys(input, PARSE_MESSAGE_OPTION_KEYS, 'parseMessage options')
  const normalized = normalizeCommonParseOptions(input)
  if (!normalized.collectTrivia && normalized.includeTrivia) {
    throw new TypeError('includeTrivia cannot be true when collectTrivia is false')
  }
  return normalized
}

/**
 * Validate and normalize options for parsing a batch of messages.
 *
 * @param options - User-provided batch parse options.
 * @returns Normalized batch parse options with defaults applied.
 */
export function validateParseBatchOptions(
  options?: ParseBatchOptions
): NormalizedParseBatchOptions {
  const input = normalizeOptionsObject(options, 'options')
  assertKnownKeys(input, PARSE_BATCH_OPTION_KEYS, 'parseBatch options')
  const normalized = normalizeCommonParseOptions(input)
  if (!normalized.collectTrivia && normalized.includeTrivia) {
    throw new TypeError('includeTrivia cannot be true when collectTrivia is false')
  }
  return {
    ...normalized,
    batchExecution: readBatchExecution(input.batchExecution)
  }
}

/**
 * Validate and normalize a single-message parse input.
 *
 * @param input - Source string or source object to parse.
 * @returns Normalized source object.
 */
export function normalizeParseMessageInput(
  input: string | ParseInputObject
): NormalizedParseInputObject {
  if (typeof input === 'string') {
    assertNoUnpairedSurrogates(input)
    return {
      source: input,
      path: null,
      locale: null,
      messageId: null,
      baseOffset: 0
    }
  }

  const object = normalizeInputObject(input, 'parseMessage input')
  return normalizeParseInputObject(object, 'parseMessage input')
}

/**
 * Validate and normalize batch parse inputs.
 *
 * @param items - Source objects to parse as one batch.
 * @returns Normalized source objects in input order.
 */
export function normalizeParseBatchInput(
  items: readonly ParseInputObject[]
): NormalizedParseInputObject[] {
  if (!Array.isArray(items)) {
    throw new TypeError('parseBatch input must be an array')
  }
  if (items.length === 0) {
    throw new RangeError('parseBatch input must not be empty')
  }
  return items.map((item, index) => {
    const object = normalizeInputObject(item, `parseBatch input item ${index}`)
    return normalizeParseInputObject(object, `parseBatch input item ${index}`)
  })
}

/**
 * Validate snapshot bytes passed to decode APIs.
 *
 * @param bytes - Snapshot byte array to decode.
 * @returns The same byte array after validation.
 */
export function validateDecodeSnapshotInput(bytes: Uint8Array): Uint8Array {
  if (!(bytes instanceof Uint8Array)) {
    throw new TypeError('decodeSnapshot input must be a Uint8Array')
  }
  return bytes
}

/**
 * Validate external source texts attached to a decoded snapshot.
 *
 * @param sources - Source texts ordered by source id.
 * @returns Validated source texts.
 */
export function validateWithSourcesInput(sources: readonly string[]): string[] {
  if (!Array.isArray(sources)) {
    throw new TypeError('withSources input must be a string array')
  }
  return sources.map((source, index) => {
    if (typeof source !== 'string') {
      throw new TypeError(`withSources input item ${index} must be a string`)
    }
    assertNoUnpairedSurrogates(source, `withSources input item ${index}`)
    return source
  })
}

function normalizeCommonParseOptions(
  options: Record<string, unknown>
): NormalizedParseMessageOptions {
  return {
    collectTrivia: readBooleanOption(options.collectTrivia, 'collectTrivia', true),
    includeTrivia: readBooleanOption(options.includeTrivia, 'includeTrivia', true),
    includeDiagnostics: readBooleanOption(options.includeDiagnostics, 'includeDiagnostics', true),
    includeSourceText: readBooleanOption(
      options.includeSourceText,
      'includeSourceText',
      DEFAULT_PARSE_OPTIONS.includeSourceText
    )
  }
}

function normalizeParseInputObject(
  input: Record<string, unknown>,
  label: string
): NormalizedParseInputObject {
  assertKnownKeys(input, PARSE_INPUT_KEYS, label)
  if (typeof input.source !== 'string') {
    throw new TypeError(`${label}.source must be a string`)
  }
  assertNoUnpairedSurrogates(input.source, `${label}.source`)
  return {
    source: input.source,
    path: readOptionalString(input.path, `${label}.path`),
    locale: readOptionalString(input.locale, `${label}.locale`),
    messageId: readOptionalString(input.messageId, `${label}.messageId`),
    baseOffset: readBaseOffset(input.baseOffset, `${label}.baseOffset`)
  }
}

function normalizeOptionsObject(
  value: ParseMessageOptions | ParseBatchOptions | undefined,
  label: string
): Record<string, unknown> {
  if (value === undefined) {
    return {}
  }
  return normalizeInputObject(value, label)
}

function normalizeInputObject(value: unknown, label: string): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new TypeError(`${label} must be an object`)
  }
  return value as Record<string, unknown>
}

function assertKnownKeys(
  object: Record<string, unknown>,
  knownKeys: ReadonlySet<string>,
  label: string
): void {
  for (const key of Object.keys(object)) {
    if (!knownKeys.has(key)) {
      throw new TypeError(`${label} contains unknown option '${key}'`)
    }
  }
}

function readBooleanOption(value: unknown, name: string, defaultValue: boolean): boolean {
  if (value === undefined) {
    return defaultValue
  }
  if (typeof value !== 'boolean') {
    throw new TypeError(`${name} must be a boolean`)
  }
  return value
}

function readOptionalString(value: unknown, label: string): string | null {
  if (value === undefined) {
    return null
  }
  if (typeof value !== 'string') {
    throw new TypeError(`${label} must be a string`)
  }
  return value
}

function readBaseOffset(value: unknown, label: string): number {
  if (value === undefined) {
    return 0
  }
  if (typeof value !== 'number' || !Number.isInteger(value) || value < 0 || value > U32_MAX) {
    throw new RangeError(`${label} must be an integer in 0..u32::MAX`)
  }
  return value
}

function readBatchExecution(value: unknown): BatchExecution {
  if (value === undefined) {
    return 'sequential'
  }
  if (value === 'sequential' || value === 'parallel') {
    return value
  }
  throw new TypeError("batchExecution must be 'sequential' or 'parallel'")
}
