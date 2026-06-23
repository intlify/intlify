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
import { init } from './init.ts'
import { getWasmBinding } from './wasm.ts'

import type {
  DecodedSnapshotResult,
  ParseBatchOptions,
  ParseBatchResult,
  ParseInputObject,
  ParseMessageOptions,
  ParseMessageResult
} from '@intlify/ox-mf2-shared'

export * from '@intlify/ox-mf2-shared'
export { init }

/**
 * Parse one MF2 message with the initialized WASM binding.
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
  const bytes = getWasmBinding().parseMessageToSnapshot(
    toWasmParseInput(normalized),
    validateParseMessageOptions(options)
  )
  return createParseMessageResult({
    bytes,
    externalSources: [normalized.source]
  })
}

/**
 * Parse a batch of MF2 messages with the initialized WASM binding.
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
  const parseOptions = validateParseBatchOptions(options)
  const bytes = getWasmBinding().parseBatchToSnapshot(
    normalized.map(toWasmParseInput),
    parseOptions
  )
  return createParseBatchResult({
    bytes,
    externalSources: normalized.map(item => item.source),
    execution: 'sequential',
    degraded: parseOptions.batchExecution === 'parallel'
  })
}

/**
 * Decode Binary AST snapshot bytes with WASM validation.
 *
 * @param bytes - Snapshot byte array to decode.
 * @returns Snapshot-backed decoded result.
 */
export function decodeSnapshot(bytes: Uint8Array): DecodedSnapshotResult {
  return createDecodedSnapshotResult(
    getWasmBinding().decodeSnapshotBytes(validateDecodeSnapshotInput(bytes).slice())
  )
}

function toWasmParseInput(
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
