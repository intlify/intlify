/**
 * Check the committed Programmatic Entry Snapshot vectors against this
 * implementation.
 *
 * The Rust crate writes canonical Snapshots in a JSON notation, each with
 * design 015's framing S of it, its content digest, and its counts under the
 * nine bootstrapOnly.snapshot bounds. This script shares no code with it: it
 * reads the notation, checks that the notation is canonical, frames and
 * digests every tree from design 015, and counts it under each bound. A
 * notation that is not canonical, a member named twice in one object of the
 * JSON text, or a node after the bootstrap-limit-exceeded marker is refused
 * (plan 025 decision #297).
 */

import { createHash } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const vectorsPath = resolve(here, '../../../crates/intlify_config/fixtures/snapshot/vectors.json')

/** The domain separator of the snapshot-content digest. */
const DOMAIN = 'intlify.programmatic-entry-snapshot.v0'

/** The closed reasons of an ordinary rejected-node marker. */
const REASONS = [
  'unsupported-type',
  'non-finite-number',
  'non-portable-number',
  'invalid-unicode',
  'non-string-key',
  'dynamic-container',
  'dynamic-property',
  'sparse-array',
  'cycle'
]

/** The reason of the marker a bootstrap overrun leaves. */
const OVERRUN = 'bootstrap-limit-exceeded'

/** The bootstrapOnly.snapshot bounds, in the capability table's order. */
const BOUNDS = [
  'maxNodes',
  'maxDepth',
  'maxCollectionEntries',
  'maxTotalStringBytes',
  'maxSingleStringBytes',
  'maxRejectedMarkers',
  'maxEnvelopeCanonicalBytes',
  'maxSingleMarkerCanonicalBytes',
  'maxTraversalUnits'
]

/** The largest magnitude of a portable number. */
const LARGEST = 9007199254740991

/** The tags of S: null, false, true, string, array, object, number, marker. */
const TAG = { null: 1, false: 2, true: 3, string: 4, array: 5, object: 6, number: 7, marker: 8 }

/**
 * Encode an unsigned 64-bit integer, big-endian.
 *
 * @param value - Length, count, or witness to encode.
 * @returns The eight-byte big-endian encoding.
 */
function u64(value) {
  const bytes = Buffer.alloc(8)
  bytes.writeBigUInt64BE(BigInt(value))
  return bytes
}

/**
 * The shortest decimal that reads back as a binary64 value, without an
 * exponent, as the Rust crate writes a number.
 *
 * @param number - A finite number.
 * @returns Its decimal.
 */
function decimal(number) {
  const text = String(number)
  const small = /^(-?)(\d)(?:\.(\d+))?e-(\d+)$/.exec(text)
  if (small === null) {
    return text
  }
  const [, sign, lead, rest = '', power] = small
  return `${sign}0.${'0'.repeat(Number(power) - 1)}${lead}${rest}`
}

/**
 * Refuse a member named twice in one object of a JSON text, whose earlier
 * value JSON.parse would drop unseen.
 *
 * @param text - A JSON text that JSON.parse accepts.
 */
function rejectRepeatedMembers(text) {
  // The names of each open object, and null for each open array.
  const open = []
  let name = false
  for (let index = 0; index < text.length; index += 1) {
    const char = text[index]
    if (char === '"') {
      let end = index + 1
      while (text[end] !== '"') {
        end += text[end] === '\\' ? 2 : 1
      }
      const names = open.at(-1)
      if (names instanceof Set && name) {
        const member = JSON.parse(text.slice(index, end + 1))
        if (names.has(member)) {
          throw new Error(`a member is named twice in one object: ${JSON.stringify(member)}`)
        }
        names.add(member)
        name = false
      }
      index = end
    } else if (char === '{') {
      open.push(new Set())
      name = true
    } else if (char === '[') {
      open.push(null)
    } else if (char === '}' || char === ']') {
      open.pop()
    } else if (char === ',') {
      name = open.at(-1) instanceof Set
    }
  }
}

/**
 * Frame one payload as its tag, its byte length, and its bytes.
 *
 * @param tag - The one-byte tag.
 * @param payload - The payload bytes.
 * @returns The frame.
 */
function frame(tag, payload) {
  return Buffer.concat([Buffer.from([tag]), u64(payload.length), payload])
}

/**
 * The UTF-8 bytes of a string that must be a sequence of Unicode scalars.
 *
 * @param text - A string or key of the tree.
 * @returns Its UTF-8 bytes.
 */
function utf8(text) {
  if (typeof text !== 'string' || !text.isWellFormed()) {
    throw new Error(`not a Unicode-scalar string: ${JSON.stringify(text)}`)
  }
  return Buffer.from(text, 'utf8')
}

/**
 * The marker record: field 0x01 the reason and, for an overrun, 0x02 the
 * bound's capability path and 0x03 the witness.
 *
 * @param marker - The marker's notation.
 * @returns The record bytes.
 */
function markerRecord(marker) {
  const { reason, bound, witness } = marker
  if (reason === OVERRUN) {
    const name = bound.startsWith('bootstrapOnly.snapshot.') ? bound.slice(23) : null
    if (!BOUNDS.includes(name) || Object.keys(marker).length !== 3) {
      throw new Error(`malformed overrun marker: ${JSON.stringify(marker)}`)
    }
    if (
      typeof witness !== 'string' ||
      !/^(?:0|[1-9]\d*)$/.test(witness) ||
      BigInt(witness) >= 2n ** 64n
    ) {
      throw new Error(`a witness is not a decimal string within U64: ${JSON.stringify(witness)}`)
    }
    return Buffer.concat([
      frame(1, Buffer.from(reason, 'ascii')),
      frame(2, Buffer.from(bound, 'ascii')),
      frame(3, u64(witness))
    ])
  }
  if (!REASONS.includes(reason) || Object.keys(marker).length !== 1) {
    throw new Error(`malformed marker: ${JSON.stringify(marker)}`)
  }
  return frame(1, Buffer.from(reason, 'ascii'))
}

/**
 * Frame a node with S, counting it at its depth.
 *
 * @param node - The node's notation.
 * @param depth - Its tree level, the root at one.
 * @param counts - The counts so far.
 * @param walk - Whether the traversal passed the bootstrap-limit-exceeded
 *   marker, and the marker reasons and bounds the trees hold.
 * @returns The node's frame.
 */
function snapshot(node, depth, counts, walk) {
  if (walk.overrun) {
    throw new Error(`a node follows the ${OVERRUN} marker: ${JSON.stringify(node)}`)
  }
  const entries = Object.entries(node)
  if (entries.length !== 1) {
    throw new Error(`a node names one variant: ${JSON.stringify(node)}`)
  }
  const [[variant, value]] = entries
  counts.maxNodes += 1
  counts.maxDepth = Math.max(counts.maxDepth, depth)
  const string = text => {
    const bytes = utf8(text)
    counts.maxTotalStringBytes += bytes.length
    counts.maxSingleStringBytes = Math.max(counts.maxSingleStringBytes, bytes.length)
    return frame(TAG.string, bytes)
  }
  switch (variant) {
    case 'null':
      if (value !== null) {
        throw new Error(`a null holds ${JSON.stringify(value)}`)
      }
      return frame(TAG.null, Buffer.alloc(0))
    case 'boolean':
      if (typeof value !== 'boolean') {
        throw new Error(`a boolean holds ${JSON.stringify(value)}`)
      }
      return frame(value ? TAG.true : TAG.false, Buffer.alloc(0))
    case 'number': {
      const number = Number(value)
      if (
        !Number.isFinite(number) ||
        Math.abs(number) > LARGEST ||
        Object.is(number, -0) ||
        decimal(number) !== value
      ) {
        throw new Error(`not a canonical portable number: ${JSON.stringify(value)}`)
      }
      const bytes = Buffer.alloc(8)
      bytes.writeDoubleBE(number)
      return frame(TAG.number, bytes)
    }
    case 'string':
      return string(value)
    case 'array': {
      counts.maxCollectionEntries += value.length
      const elements = value.map(element => snapshot(element, depth + 1, counts, walk))
      return frame(TAG.array, Buffer.concat([u64(value.length), ...elements]))
    }
    case 'object': {
      counts.maxCollectionEntries += value.length
      const members = value.flatMap((pair, index) => {
        if (pair.length !== 2) {
          throw new Error(`an object member is not one [key, node] pair: ${JSON.stringify(pair)}`)
        }
        const [key, member] = pair
        if (index > 0 && Buffer.compare(utf8(value[index - 1][0]), utf8(key)) >= 0) {
          throw new Error(`keys are not unique and ascending at ${JSON.stringify(key)}`)
        }
        return [string(key), snapshot(member, depth + 1, counts, walk)]
      })
      return frame(TAG.object, Buffer.concat([u64(value.length), ...members]))
    }
    case 'marker': {
      counts.maxRejectedMarkers += 1
      const framed = frame(TAG.marker, markerRecord(value))
      if (value.reason === OVERRUN) {
        walk.overrun = true
        walk.bounds.add(value.bound.slice(23))
      } else {
        walk.reasons.add(value.reason)
      }
      counts.maxSingleMarkerCanonicalBytes = Math.max(
        counts.maxSingleMarkerCanonicalBytes,
        framed.length
      )
      return framed
    }
    default:
      throw new Error(`unknown variant ${variant}`)
  }
}

const text = readFileSync(vectorsPath, 'utf8')
const vectors = JSON.parse(text)
const failures = []
const fail = message => failures.push(message)
try {
  rejectRepeatedMembers(text)
} catch (error) {
  fail(error.message)
}

// The marker reasons and bounds the trees hold.
const covered = { reasons: new Set(), bounds: new Set() }
for (const { id, snapshot: root, bytes, digest, counts } of vectors.snapshots) {
  const counted = Object.fromEntries(BOUNDS.map(bound => [bound, 0]))
  const walk = { overrun: false, reasons: covered.reasons, bounds: covered.bounds }
  let framed
  try {
    framed = snapshot(root, 1, counted, walk)
  } catch (error) {
    fail(`${id}: ${error.message}`)
    continue
  }
  counted.maxEnvelopeCanonicalBytes = framed.length
  counted.maxTraversalUnits = counted.maxNodes + counted.maxCollectionEntries
  if (framed.toString('hex') !== bytes) {
    fail(`${id}: expected ${bytes}, framed ${framed.toString('hex')}`)
  }
  const hash = createHash('sha256')
  hash.update(Buffer.from(DOMAIN, 'ascii'))
  hash.update(framed)
  const computed = `sha256:${hash.digest('hex')}`
  if (computed !== digest) {
    fail(`${id}: expected digest ${digest}, computed ${computed}`)
  }
  const expected = BOUNDS.map(bound => `${bound}=${counts[bound]}`).join(' ')
  const actual = BOUNDS.map(bound => `${bound}=${counted[bound]}`).join(' ')
  if (Object.keys(counts).join() !== BOUNDS.join() || actual !== expected) {
    fail(`${id}: expected counts ${expected}, counted ${actual}`)
  }
}

for (const reason of REASONS) {
  if (!covered.reasons.has(reason)) {
    fail(`no vector holds a ${reason} marker`)
  }
}
for (const bound of BOUNDS) {
  if (!covered.bounds.has(bound)) {
    fail(`no vector holds an overrun of ${bound}`)
  }
}

if (failures.length > 0) {
  for (const message of failures) {
    console.error(message)
  }
  process.exit(1)
}

console.log(
  `snapshot vectors agree: ${vectors.snapshots.length} canonical Snapshots framed, digested, and counted under ${BOUNDS.length} bounds`
)
