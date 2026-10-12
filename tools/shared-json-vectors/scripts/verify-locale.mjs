/**
 * Check the committed locale canonicalization dataset vectors against this
 * implementation.
 *
 * The Rust crate writes a small dataset's parts, its conformance payload as
 * canonical JSON text, and its semantic digest, with edits and refused
 * payloads. This script shares no code with it: it writes the canonical text
 * itself, checks the order design 017 fixes, rebuilds the preimage from the
 * parts, and re-frames and re-hashes it.
 */

import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { digest } from './framing.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const datasetPath = resolve(
  here,
  '../../../crates/intlify_locale_icu4x/fixtures/dataset/vectors.json'
)

/** The four marker families of dataset schema revision "0", sorted by name. */
const MARKERS = [
  'LocaleAliasesV1',
  'LocaleLikelySubtagsExtendedV1',
  'LocaleLikelySubtagsLanguageV1',
  'LocaleLikelySubtagsScriptRegionV1'
]

/**
 * Compare two strings by their unsigned UTF-8 bytes.
 *
 * @param left - The first string.
 * @param right - The second string.
 * @returns Negative, zero, or positive, as for a sort.
 */
function compareText(left, right) {
  return Buffer.compare(Buffer.from(left, 'utf8'), Buffer.from(right, 'utf8'))
}

/**
 * Compare two tuples of strings element by element.
 *
 * @param left - The first tuple.
 * @param right - The second tuple, of the same length.
 * @returns Negative, zero, or positive, as for a sort.
 */
function compareTuple(left, right) {
  for (let index = 0; index < left.length; index += 1) {
    const order = compareText(left[index], right[index])
    if (order !== 0) {
      return order
    }
  }
  return 0
}

/**
 * Quote one string with only the escapes canonical JSON text allows.
 *
 * @param text - The string to quote.
 * @returns The quoted string.
 */
function quote(text) {
  let quoted = '"'
  for (const scalar of text) {
    const code = scalar.codePointAt(0) ?? 0
    if (scalar === '"') {
      quoted += '\\"'
    } else if (scalar === '\\') {
      quoted += '\\\\'
    } else if (code <= 0x1f) {
      quoted += `\\u${code.toString(16).padStart(4, '0')}`
    } else {
      quoted += scalar
    }
  }
  return `${quoted}"`
}

/**
 * Write a value as canonical JSON text revision "0".
 *
 * @param value - A JSON value without numbers.
 * @returns The canonical text.
 */
export function canonicalText(value) {
  if (value === null) {
    return 'null'
  }
  if (value === true || value === false) {
    return String(value)
  }
  if (typeof value === 'string') {
    return quote(value)
  }
  if (typeof value === 'number') {
    throw new TypeError('JSON numbers are outside canonical JSON text')
  }
  if (Array.isArray(value)) {
    return `[${value.map(item => canonicalText(item)).join(',')}]`
  }
  if (typeof value === 'object') {
    const members = /** @type {Record<string, unknown>} */ (value)
    const keys = Object.keys(members).sort(compareText)
    return `{${keys.map(key => `${quote(key)}:${canonicalText(members[key])}`).join(',')}}`
  }
  throw new TypeError(`unsupported value: ${typeof value}`)
}

/**
 * Whether the value framing carries a marker payload unchanged: only strings,
 * arrays, and objects, at any depth.
 *
 * @param value - A marker payload.
 * @returns Whether it is representable.
 */
function representable(value) {
  if (typeof value === 'string') {
    return true
  }
  if (Array.isArray(value)) {
    return value.every(item => representable(item))
  }
  if (value !== null && typeof value === 'object') {
    return Object.values(value).every(item => representable(item))
  }
  return false
}

/**
 * Find the first place a list is not strictly ascending.
 *
 * @param label - The collection, for the message.
 * @param keys - Each item's ordering tuple, in list order.
 * @returns One message per problem found.
 */
function ascending(label, keys) {
  for (let index = 1; index < keys.length; index += 1) {
    const order = compareTuple(keys[index - 1], keys[index])
    if (order === 0) {
      return [`${label} repeats ${keys[index].join('/')}`]
    }
    if (order > 0) {
      return [`${label} is out of order at ${keys[index].join('/')}`]
    }
  }
  return []
}

/**
 * Check the canonical order and uniqueness design 017 fixes for conformance
 * data.
 *
 * @param conformance - The conformance value.
 * @returns One message per problem found.
 */
function checkConformance(conformance) {
  const problems = []
  const groups = conformance.validity
  problems.push(
    ...ascending(
      'validity',
      groups.map(group => [group.field, group.status])
    )
  )
  const fields = new Map()
  for (const group of groups) {
    problems.push(
      ...ascending(
        `codes of ${group.field}/${group.status}`,
        group.codes.map(code => [code])
      )
    )
    const seen = fields.get(group.field) ?? new Set()
    for (const code of group.codes) {
      if (seen.has(code)) {
        problems.push(`${code} is in two groups of ${group.field}`)
      }
      seen.add(code)
    }
    fields.set(group.field, seen)
  }
  const keys = conformance.extensionKeys
  problems.push(
    ...ascending(
      'extension keys',
      keys.map(key => [key.extension, key.key])
    )
  )
  for (const key of keys) {
    problems.push(
      ...ascending(
        `types of ${key.extension}/${key.key}`,
        key.types.map(type => [type.kind, type.kind === 'class' ? type.class : type.type])
      )
    )
  }
  const corrections = conformance.corrections
  problems.push(
    ...ascending(
      'corrections',
      corrections.map(correction => [correction.extension, correction.key, correction.from])
    )
  )
  const corrected = new Set(
    corrections.map(correction => `${correction.extension}/${correction.key}/${correction.from}`)
  )
  for (const correction of corrections) {
    if (corrected.has(`${correction.extension}/${correction.key}/${correction.to}`)) {
      problems.push(`the correction of ${correction.from} is not flattened`)
    }
  }
  return problems
}

/**
 * Replace the value a JSON pointer names.
 *
 * @param root - The value to edit in place.
 * @param pointer - An RFC 6901 pointer to an existing value.
 * @param value - The replacement.
 */
function replaceAt(root, pointer, value) {
  const tokens = pointer
    .split('/')
    .slice(1)
    .map(token => token.replaceAll('~1', '/').replaceAll('~0', '~'))
  const last = tokens.pop()
  if (last === undefined) {
    throw new TypeError('the pointer names the whole value')
  }
  let cursor = root
  for (const token of tokens) {
    cursor = cursor[token]
  }
  if (!(last in cursor)) {
    throw new TypeError(`nothing at ${pointer}`)
  }
  cursor[last] = value
}

/**
 * Check the dataset vectors: the canonical text, the order, the digest of the
 * dataset and of every edit, and every refused payload.
 *
 * @param vectors - The committed vectors.
 * @returns How many checks failed.
 */
function checkDataset(vectors) {
  let failures = 0
  const fail = message => {
    console.error(`dataset: ${message}`)
    failures += 1
  }

  const names = vectors.markers.map(entry => entry.marker)
  if (names.join() !== MARKERS.join()) {
    fail(`the markers are ${names.join(', ')}, not the four in name order`)
  }
  for (const entry of vectors.markers) {
    if (!representable(entry.payload)) {
      fail(`the payload of ${entry.marker} is not representable`)
    }
  }
  for (const problem of checkConformance(vectors.conformance)) {
    fail(problem)
  }
  const written = canonicalText(vectors.conformance)
  if (written !== vectors.conformanceText) {
    fail(
      `the conformance text is not canonical:\n  committed ${vectors.conformanceText}\n  written   ${written}`
    )
  }

  const preimage = {
    datasetSchema: vectors.datasetSchema,
    markers: vectors.markers,
    conformance: vectors.conformance
  }
  const produced = digest(vectors.domain, preimage)
  if (produced !== vectors.semanticDigest) {
    fail(`committed ${vectors.semanticDigest}\n  produced  ${produced}`)
  }

  const seen = new Set([produced])
  for (const edit of vectors.edits) {
    const edited = structuredClone(preimage)
    replaceAt(edited, edit.pointer, edit.value)
    for (const problem of checkConformance(edited.conformance)) {
      fail(`${edit.id}: ${problem}`)
    }
    const editedDigest = digest(vectors.domain, edited)
    if (editedDigest !== edit.semanticDigest) {
      fail(`${edit.id}:\n  committed ${edit.semanticDigest}\n  produced  ${editedDigest}`)
    }
    if (seen.has(editedDigest)) {
      fail(`${edit.id} does not change the digest`)
    }
    seen.add(editedDigest)
  }

  for (const refused of vectors.refused) {
    if (!MARKERS.includes(refused.marker)) {
      fail(`${refused.id} names no marker of the dataset schema`)
    }
    if (representable(refused.payload)) {
      fail(`${refused.id} holds nothing the framing refuses`)
    }
  }
  return failures
}

const dataset = JSON.parse(readFileSync(datasetPath, 'utf8'))
const failures = checkDataset(dataset)

if (failures > 0) {
  console.error(`\n${failures} locale vector check(s) failed`)
  process.exit(1)
}
console.log(
  `a dataset and ${dataset.edits.length} edits of it agree with an independent implementation`
)
