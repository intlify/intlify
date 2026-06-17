import { parse as parseIcuMessage } from '@formatjs/icu-messageformat-parser'
import { MessageFormat, parseMessage } from 'messageformat'
import { messageFromCST, parseCST } from 'messageformat/cst'

export const jsParserTargets = {
  'messageformat-parse-message': source => parseMessage(source),
  'messageformat-parse-cst': source => parseCST(source),
  'messageformat-cst-to-message': source => messageFromCST(parseCST(source)),
  'messageformat-constructor': source => new MessageFormat('en', source),
  'formatjs-icu-parse': source => parseIcuMessage(source)
}

/**
 * Run one JavaScript parser target for a source message.
 *
 * @param targetName - Parser target name.
 * @param source - Message source.
 * @returns Parser result.
 */
export function runJsParserTarget(targetName, source) {
  const parser = jsParserTargets[targetName]
  if (!parser) {
    throw new Error(`Unknown JS parser target: ${targetName}`)
  }
  return parser(source)
}

/**
 * Compute a small checksum contribution from a parser result.
 *
 * @param value - Parser result.
 * @returns Numeric checksum contribution.
 */
export function checksumValue(value) {
  if (value == null) {
    return 1
  }
  if (typeof value === 'string') {
    return hashString(value)
  }
  if (typeof value === 'number' || typeof value === 'boolean') {
    return Number(value) || 1
  }
  if (Array.isArray(value)) {
    return value.length + checksumValue(value[0])
  }
  if (typeof value === 'object') {
    const keys = Object.keys(value)
    let checksum = keys.length
    const type = value.type
    if (typeof type === 'string') {
      checksum += hashString(type)
    }
    return checksum
  }
  return 7
}

/**
 * Count parser diagnostics exposed on a result object.
 *
 * @param value - Parser result.
 * @returns Number of diagnostics.
 */
export function diagnosticsCount(value) {
  if (value && typeof value === 'object' && Array.isArray(value.errors)) {
    return value.errors.length
  }
  return 0
}

function hashString(value) {
  let hash = 0
  for (let index = 0; index < value.length; index++) {
    hash = (hash * 31 + value.charCodeAt(index)) >>> 0
  }
  return hash
}
