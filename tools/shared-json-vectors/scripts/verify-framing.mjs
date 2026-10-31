/**
 * Check the committed framing vectors against this implementation.
 *
 * The Rust crate writes records and collections in design 015's field framing,
 * byte strings its strict readers refuse with the failure each reports,
 * unpadded base64url texts, and Resolver Construction Identity texts. This
 * script shares no code with it: it frames every record and collection from
 * design 015, decides which failure each refused byte string has first,
 * encodes and decodes base64url with Node's own codec, and decides which
 * defect each refused text has.
 */

import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const vectorsPath = resolve(here, '../../../crates/intlify_config/fixtures/framing/vectors.json')

/** The unpadded base64url alphabet of RFC 4648, section 5: letters, digits, `_`, and `-`. */
const BASE64URL = /^[\w-]*$/

/**
 * Encode an unsigned 64-bit length, big-endian.
 *
 * @param value - Length or item count to encode.
 * @returns The eight-byte big-endian encoding.
 */
function u64(value) {
  const bytes = Buffer.alloc(8)
  bytes.writeBigUInt64BE(BigInt(value))
  return bytes
}

/**
 * Frame one field as its tag, its payload's byte length, and its payload.
 *
 * @param tag - The record-local tag.
 * @param payload - The field's payload.
 * @returns The field frame.
 */
function frame(tag, payload) {
  return Buffer.concat([Buffer.from([tag]), u64(payload.length), payload])
}

/**
 * Frame a record's fields with the tags 0x01, 0x02, and so on; a record is
 * not itself framed.
 *
 * @param fields - The field payloads in order.
 * @returns The record's bytes.
 */
function record(fields) {
  return Buffer.concat(fields.map((payload, index) => frame(index + 1, payload)))
}

/**
 * Frame a collection as its item count, then each item's length and bytes.
 *
 * @param items - The items in order.
 * @returns The collection payload.
 */
function collection(items) {
  return Buffer.concat([u64(items.length), ...items.flatMap(item => [u64(item.length), item])])
}

/**
 * Read one length-prefixed span starting at an offset.
 *
 * @param bytes - The whole input.
 * @param offset - Where the eight length bytes start.
 * @returns The offset after the span, or null when the input ends first.
 */
function skipSpan(bytes, offset) {
  if (bytes.length - offset < 8) {
    return null
  }
  const length = bytes.readBigUInt64BE(offset)
  const end = BigInt(offset + 8) + length
  return end > BigInt(bytes.length) ? null : Number(end)
}

/**
 * Decide which failure design 015's strict reader reports for a record.
 *
 * @param bytes - The record's bytes.
 * @param fields - How many fields the record has.
 * @returns The failure, or null when the record is read.
 */
function recordFailure(bytes, fields) {
  const tags = []
  let offset = 0
  while (offset < bytes.length) {
    const end = skipSpan(bytes, offset + 1)
    if (end === null) {
      return 'truncated'
    }
    tags.push(bytes[offset])
    offset = end
  }
  const seen = new Set()
  let last = 0
  for (const tag of tags) {
    if (tag === 0) {
      return 'zero-tag'
    }
    if (tag > fields) {
      return 'unknown-tag'
    }
    if (seen.has(tag)) {
      return 'repeated-tag'
    }
    if (tag < last) {
      return 'out-of-order'
    }
    seen.add(tag)
    last = tag
  }
  for (let tag = 1; tag <= fields; tag++) {
    if (!seen.has(tag)) {
      return 'missing-tag'
    }
  }
  return null
}

/**
 * Decide which failure design 015's strict reader reports for a collection.
 *
 * @param bytes - The collection payload.
 * @returns The failure, or null when the collection is read.
 */
function collectionFailure(bytes) {
  if (bytes.length < 8) {
    return 'truncated'
  }
  const count = bytes.readBigUInt64BE(0)
  let items = 0n
  let offset = 8
  while (offset < bytes.length) {
    if (items === count) {
      return 'count-mismatch'
    }
    const end = skipSpan(bytes, offset)
    if (end === null) {
      return 'truncated'
    }
    items++
    offset = end
  }
  return items === count ? null : 'count-mismatch'
}

/**
 * Decide which defect makes text something no byte string encodes to.
 *
 * Node's base64url decoder accepts non-canonical text, so the canonical check
 * re-encodes what it decodes and compares.
 *
 * @param text - The text.
 * @returns The defect, or null when the text is canonical.
 */
function textFailure(text) {
  if (!BASE64URL.test(text)) {
    return 'character'
  }
  if (text.length % 4 === 1) {
    return 'length'
  }
  const bytes = Buffer.from(text, 'base64url')
  return bytes.toString('base64url') === text ? null : 'unused-bits'
}

/**
 * Decide which defect makes text something no Resolver Construction Identity
 * is presented as.
 *
 * @param text - The text.
 * @returns The defect, or null when the text is an identity's.
 */
function identityFailure(text) {
  if (!text.startsWith('rci0_')) {
    return 'prefix'
  }
  const rest = text.slice('rci0_'.length)
  if (rest.length !== 43) {
    return 'length'
  }
  return textFailure(rest)
}

const vectors = JSON.parse(readFileSync(vectorsPath, 'utf8'))
const failures = []
const fail = message => failures.push(message)
const hex = text => Buffer.from(text, 'hex')

for (const { id, fields, bytes } of vectors.records) {
  const framed = record(fields.map(hex))
  if (framed.toString('hex') !== bytes) {
    fail(`record ${id}: expected ${bytes}, framed ${framed.toString('hex')}`)
  }
  const failure = recordFailure(framed, fields.length)
  if (failure !== null) {
    fail(`record ${id}: its own framing is refused as ${failure}`)
  }
}

for (const { id, items, bytes } of vectors.collections) {
  const framed = collection(items.map(hex))
  if (framed.toString('hex') !== bytes) {
    fail(`collection ${id}: expected ${bytes}, framed ${framed.toString('hex')}`)
  }
  const failure = collectionFailure(framed)
  if (failure !== null) {
    fail(`collection ${id}: its own framing is refused as ${failure}`)
  }
}

for (const { id, bytes, fields, reason } of vectors.refusedRecords) {
  const failure = recordFailure(hex(bytes), Number(fields))
  if (failure !== reason) {
    fail(`refused record ${id}: expected ${reason}, decided ${failure}`)
  }
}

for (const { id, bytes, reason } of vectors.refusedCollections) {
  const failure = collectionFailure(hex(bytes))
  if (failure !== reason) {
    fail(`refused collection ${id}: expected ${reason}, decided ${failure}`)
  }
}

for (const { bytes, text } of vectors.base64url) {
  const encoded = hex(bytes).toString('base64url')
  if (encoded !== text) {
    fail(`base64url of ${bytes}: expected ${text}, encoded ${encoded}`)
  }
  if (textFailure(text) !== null) {
    fail(`base64url ${text}: refused as ${textFailure(text)}`)
  }
}

for (const { text, reason } of vectors.refusedBase64url) {
  const failure = textFailure(text)
  if (failure !== reason) {
    fail(`refused base64url ${JSON.stringify(text)}: expected ${reason}, decided ${failure}`)
  }
}

for (const { digest, text } of vectors.constructionIdentities) {
  const presented = `rci0_${hex(digest).toString('base64url')}`
  if (hex(digest).length !== 32 || presented !== text) {
    fail(`identity of ${digest}: expected ${text}, presented ${presented}`)
  }
  if (identityFailure(text) !== null) {
    fail(`identity ${text}: refused as ${identityFailure(text)}`)
  }
}

for (const { text, reason } of vectors.refusedConstructionIdentities) {
  const failure = identityFailure(text)
  if (failure !== reason) {
    fail(`refused identity ${JSON.stringify(text)}: expected ${reason}, decided ${failure}`)
  }
}

if (failures.length > 0) {
  for (const message of failures) {
    console.error(message)
  }
  process.exit(1)
}

const counted = [
  vectors.records.length + vectors.collections.length,
  vectors.refusedRecords.length + vectors.refusedCollections.length,
  vectors.base64url.length + vectors.constructionIdentities.length,
  vectors.refusedBase64url.length + vectors.refusedConstructionIdentities.length
]
console.log(
  `framing vectors agree: ${counted[0]} written, ${counted[1]} refused frames, ${counted[2]} texts, ${counted[3]} refused texts`
)
