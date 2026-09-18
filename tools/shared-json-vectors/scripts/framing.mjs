/**
 * An independent implementation of design 017's canonical value framing and
 * domain-separated digest.
 *
 * This deliberately shares no code with the Rust implementation. It exists so a
 * committed vector is checked by two implementations reading the same
 * specification, rather than by one implementation checking its own output.
 */

import { createHash } from 'node:crypto'

const TAG = {
  null: 0x01,
  false: 0x02,
  true: 0x03,
  string: 0x04,
  array: 0x05,
  object: 0x06
}

/**
 * Encode an unsigned 64-bit length, big-endian.
 *
 * @param value - Length or element count to encode.
 * @returns The eight-byte big-endian encoding.
 */
function u64(value) {
  const bytes = Buffer.alloc(8)
  bytes.writeBigUInt64BE(BigInt(value))
  return bytes
}

/**
 * Frame one tagged payload as tag, payload byte length, payload.
 *
 * @param tag - The one-byte type tag.
 * @param payload - The already encoded payload bytes.
 * @returns The complete frame.
 */
function frame(tag, payload) {
  return Buffer.concat([Buffer.from([tag]), u64(payload.length), payload])
}

/**
 * Apply the complete canonical value function C.
 *
 * @param value - An admitted JSON value; numbers are outside this encoding.
 * @returns The canonical bytes of the value.
 */
export function encode(value) {
  if (value === null) {
    return frame(TAG.null, Buffer.alloc(0))
  }
  if (value === false) {
    return frame(TAG.false, Buffer.alloc(0))
  }
  if (value === true) {
    return frame(TAG.true, Buffer.alloc(0))
  }
  if (typeof value === 'string') {
    return frame(TAG.string, Buffer.from(value, 'utf8'))
  }
  if (typeof value === 'number') {
    throw new TypeError('JSON numbers are outside this encoding')
  }
  if (Array.isArray(value)) {
    const parts = value.map(element => encode(element))
    return frame(TAG.array, Buffer.concat([u64(value.length), ...parts]))
  }
  if (typeof value === 'object') {
    const members = /** @type {Record<string, unknown>} */ (value)
    // Object keys are ordered by ascending unsigned UTF-8 bytes.
    const keys = Object.keys(members).sort((left, right) =>
      Buffer.compare(Buffer.from(left, 'utf8'), Buffer.from(right, 'utf8'))
    )
    const parts = keys.flatMap(key => [encode(key), encode(members[key])])
    return frame(TAG.object, Buffer.concat([u64(keys.length), ...parts]))
  }
  throw new TypeError(`unsupported value: ${typeof value}`)
}

/**
 * Compute the domain-separated digest H(D, V).
 *
 * @param domain - The registered digest domain.
 * @param value - The admitted value to hash.
 * @returns The `sha256:` presentation of the digest.
 */
export function digest(domain, value) {
  const hash = createHash('sha256')
  hash.update(Buffer.from('intlify.shared-json.v0', 'utf8'))
  hash.update(Buffer.from([0x00]))
  hash.update(encode(domain))
  hash.update(encode(value))
  return `sha256:${hash.digest('hex')}`
}
