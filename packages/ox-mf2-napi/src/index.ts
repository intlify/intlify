/**
 * @license MIT
 * @author kazuya kawaguchi (a.k.a. kazupon)
 */

import {
  createDecodedSnapshotResult,
  createParseBatchResult,
  createParseMessageResult,
  normalizeParseBatchInput,
  normalizeParseMessageInput,
  validateDecodeSnapshotInput,
  validateParseBatchOptions,
  validateParseMessageOptions
} from '@intlify/ox-mf2-shared'
import { getNativeBinding } from './native.ts'

import type {
  ParseBatchOptions,
  ParseBatchResult,
  ParseInputObject,
  ParseMessageOptions,
  ParseMessageResult,
  DecodedSnapshotResult
} from '@intlify/ox-mf2-shared'

export * from '@intlify/ox-mf2-shared'

/**
 * Parse one MF2 message with the native N-API binding.
 *
 * @param input - Source string or source object to parse.
 * @param options - Optional parse options.
 * @returns Snapshot-backed parse result.
 */
export function parseMessage(
  input: string | ParseInputObject,
  options?: ParseMessageOptions
): ParseMessageResult {
  const normalized = normalizeParseMessageInput(input)
  const native = getNativeBinding().parseMessage(
    toNativeParseInput(normalized),
    validateParseMessageOptions(options)
  )
  return createParseMessageResult({
    bytes: native.bytes,
    externalSources: [normalized.source]
  })
}

/**
 * Parse a batch of MF2 messages with the native N-API binding.
 *
 * @param items - Source objects to parse as one batch.
 * @param options - Optional batch parse options.
 * @returns Snapshot-backed batch parse result.
 */
export function parseBatch(
  items: ParseInputObject[],
  options?: ParseBatchOptions
): ParseBatchResult {
  const normalized = normalizeParseBatchInput(items)
  const native = getNativeBinding().parseBatch(
    normalized.map(toNativeParseInput),
    validateParseBatchOptions(options)
  )
  return createParseBatchResult({
    bytes: native.bytes,
    externalSources: normalized.map(item => item.source),
    execution: native.execution ?? 'sequential',
    degraded: native.degraded ?? false
  })
}

/**
 * Decode Binary AST snapshot bytes with native validation.
 *
 * @param bytes - Snapshot byte array to decode.
 * @returns Snapshot-backed decoded result.
 */
export function decodeSnapshot(bytes: Uint8Array): DecodedSnapshotResult {
  const native = getNativeBinding().decodeSnapshot(validateDecodeSnapshotInput(bytes).slice())
  return createDecodedSnapshotResult(native.bytes)
}

function toNativeParseInput(
  input: ReturnType<typeof normalizeParseMessageInput>
): Record<string, unknown> {
  return {
    source: input.source,
    path: input.path ?? undefined,
    locale: input.locale ?? undefined,
    messageId: input.messageId ?? undefined,
    baseOffset: input.baseOffset
  }
}
