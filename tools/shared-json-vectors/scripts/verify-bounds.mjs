/**
 * Check the committed bound vectors against this implementation.
 *
 * The Rust crate writes design 015's bound values as decimal strings with the
 * eight bytes of their semantic form, strings and byte forms that are no bound
 * with the reason the capability boundary gives for each, and every capability
 * bound path with its minimum. This script shares no code with it: it reads
 * every string with BigInt, encodes every value itself, and lists the
 * capability bounds from the checkers' copy of design 015's tables. It does
 * not read the design document, so a design change can land before the code
 * follows.
 */

import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { BOOTSTRAP_GROUPS, POLICY_GROUPS } from './resolution-bounds.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const vectorsPath = resolve(here, '../../../crates/intlify_config/fixtures/bounds/vectors.json')

/** The largest bound: one below the largest unsigned 64-bit count. */
const LARGEST = 2n ** 64n - 2n

/** A count's shortest decimal spelling: zero alone, or no leading zero. */
const CANONICAL = /^(?:0|[1-9]\d*)$/

/**
 * Decide the reason the capability boundary gives a string that is no bound.
 *
 * @param text - The string.
 * @returns The reason, or null when the string is a bound's.
 */
function textFailure(text) {
  if (!CANONICAL.test(text)) {
    return 'non-canonical-bound'
  }
  const value = BigInt(text)
  if (value === 0n) {
    return 'non-positive-bound'
  }
  return value > LARGEST ? 'unrepresentable-bound' : null
}

/**
 * Decide the reason the capability boundary gives eight bytes that are no
 * bound's semantic form.
 *
 * @param value - The count the bytes hold.
 * @returns The reason, or null when the count is a bound.
 */
function countFailure(value) {
  if (value === 0n) {
    return 'non-positive-bound'
  }
  return value > LARGEST ? 'unrepresentable-bound' : null
}

/**
 * Encode a count as eight bytes, big-endian.
 *
 * @param value - The count.
 * @returns The bytes in hexadecimal.
 */
function semantic(value) {
  const bytes = Buffer.alloc(8)
  bytes.writeBigUInt64BE(value)
  return bytes.toString('hex')
}

/**
 * List every capability bound path with its minimum, in canonical order.
 *
 * @returns The bounds as `{ path, minimum }`.
 */
function capabilityBounds() {
  const policy = POLICY_GROUPS.flatMap(([group, bounds]) =>
    bounds.map(([bound]) => ({ path: `policyComparable.${group}.${bound}`, minimum: '1' }))
  )
  const bootstrap = BOOTSTRAP_GROUPS.flatMap(([group, bounds]) =>
    bounds.map(([bound, , minimum]) => ({ path: `bootstrapOnly.${group}.${bound}`, minimum }))
  )
  return [...policy, ...bootstrap]
}

const vectors = JSON.parse(readFileSync(vectorsPath, 'utf8'))
const failures = []
const fail = message => failures.push(message)

for (const { text, semantic: bytes } of vectors.values) {
  const failure = textFailure(text)
  if (failure !== null) {
    fail(`value ${text}: refused as ${failure}`)
    continue
  }
  if (semantic(BigInt(text)) !== bytes) {
    fail(`value ${text}: expected ${bytes}, encoded ${semantic(BigInt(text))}`)
  }
  if (BigInt(text).toString() !== text) {
    fail(`value ${text}: not the shortest spelling`)
  }
}

for (const { id, text, reason } of vectors.refusedTexts) {
  const failure = textFailure(text)
  if (failure !== reason) {
    fail(`refused string ${id}: expected ${reason}, decided ${failure}`)
  }
}

for (const { semantic: bytes, reason } of vectors.refusedSemantics) {
  const failure = countFailure(Buffer.from(bytes, 'hex').readBigUInt64BE(0))
  if (bytes.length !== 16 || failure !== reason) {
    fail(`refused bytes ${bytes}: expected ${reason}, decided ${failure}`)
  }
}

const expected = capabilityBounds()
if (vectors.capabilityBounds.length !== expected.length) {
  fail(`capability bounds: expected ${expected.length}, listed ${vectors.capabilityBounds.length}`)
}
for (const [index, { path, minimum }] of expected.entries()) {
  const listed = vectors.capabilityBounds[index]
  if (listed?.path !== path || listed?.minimum !== minimum) {
    fail(`capability bound ${index}: expected ${path} ${minimum}, listed ${JSON.stringify(listed)}`)
  }
  if (textFailure(minimum) !== null) {
    fail(`capability bound ${path}: its minimum ${minimum} is no bound`)
  }
}

if (failures.length > 0) {
  for (const message of failures) {
    console.error(message)
  }
  process.exit(1)
}

const policy = expected.filter(({ path }) => path.startsWith('policyComparable.')).length
console.log(
  `bound vectors agree: ${vectors.values.length} values, ${vectors.refusedTexts.length + vectors.refusedSemantics.length} refused, ${expected.length} capability bounds (${policy} policyComparable, ${expected.length - policy} bootstrapOnly)`
)
