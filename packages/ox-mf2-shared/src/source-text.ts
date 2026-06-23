/**
 * @license MIT
 * @author kazuya kawaguchi (a.k.a. kazupon)
 */

import type { Span } from './types.ts'

const TEXT_ENCODER = new TextEncoder()
const TEXT_DECODER = new TextDecoder('utf-8', { fatal: true })

/**
 * Return whether a JavaScript string contains an unpaired surrogate.
 *
 * @param source - JavaScript string to inspect.
 * @returns True when the string contains an invalid surrogate pair.
 */
export function hasUnpairedSurrogate(source: string): boolean {
  for (let i = 0; i < source.length; i++) {
    const unit = source.charCodeAt(i)
    if (isHighSurrogate(unit)) {
      const next = source.charCodeAt(i + 1)
      if (!isLowSurrogate(next)) {
        return true
      }
      i++
      continue
    }
    if (isLowSurrogate(unit)) {
      return true
    }
  }
  return false
}

/**
 * Validate that a string can be encoded to UTF-8 without replacement.
 *
 * @param source - JavaScript string to validate.
 * @param label - Human-readable label used in error messages.
 */
export function assertNoUnpairedSurrogates(source: string, label = 'source'): void {
  if (hasUnpairedSurrogate(source)) {
    throw new TypeError(`${label} must not contain unpaired surrogates`)
  }
}

/**
 * Encode source text as UTF-8 after surrogate validation.
 *
 * @param source - Source text to encode.
 * @param label - Human-readable label used in error messages.
 * @returns UTF-8 encoded source bytes.
 */
export function encodeUtf8Source(source: string, label = 'source'): Uint8Array {
  assertNoUnpairedSurrogates(source, label)
  return TEXT_ENCODER.encode(source)
}

/**
 * Return the UTF-8 byte length of source text.
 *
 * @param source - Source text to measure.
 * @param label - Human-readable label used in error messages.
 * @returns UTF-8 byte length of the source text.
 */
export function utf8ByteLength(source: string, label = 'source'): number {
  return encodeUtf8Source(source, label).byteLength
}

/**
 * Return whether an offset falls on a UTF-8 code point boundary.
 *
 * @param bytes - UTF-8 encoded source bytes.
 * @param offset - Byte offset to inspect.
 * @returns True when the offset is a valid code point boundary.
 */
export function isUtf8Boundary(bytes: Uint8Array, offset: number): boolean {
  if (!Number.isInteger(offset) || offset < 0 || offset > bytes.byteLength) {
    return false
  }
  if (offset === 0 || offset === bytes.byteLength) {
    return true
  }
  return (bytes[offset] & 0xc0) !== 0x80
}

/**
 * Validate that a span is inside the byte array and aligned to UTF-8.
 *
 * @param bytes - UTF-8 encoded source bytes.
 * @param span - Half-open byte span to validate.
 */
export function assertUtf8Span(bytes: Uint8Array, span: Span): void {
  if (!Number.isInteger(span.start) || !Number.isInteger(span.end)) {
    throw new RangeError('span offsets must be integers')
  }
  if (span.start < 0 || span.end < span.start || span.end > bytes.byteLength) {
    throw new RangeError('span is outside source text bounds')
  }
  if (!isUtf8Boundary(bytes, span.start) || !isUtf8Boundary(bytes, span.end)) {
    throw new RangeError('span does not align to UTF-8 code point boundaries')
  }
}

/**
 * Decode a UTF-8 slice after validating its span.
 *
 * @param bytes - UTF-8 encoded source bytes.
 * @param span - Half-open byte span to decode.
 * @returns Decoded JavaScript string for the requested span.
 */
export function decodeUtf8Slice(bytes: Uint8Array, span: Span): string {
  assertUtf8Span(bytes, span)
  return TEXT_DECODER.decode(bytes.subarray(span.start, span.end))
}

function isHighSurrogate(unit: number): boolean {
  return unit >= 0xd800 && unit <= 0xdbff
}

function isLowSurrogate(unit: number): boolean {
  return unit >= 0xdc00 && unit <= 0xdfff
}
