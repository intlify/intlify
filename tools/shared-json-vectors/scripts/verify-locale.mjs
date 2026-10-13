/**
 * Check the committed locale canonicalization dataset vectors and fixture
 * artifacts against this implementation.
 *
 * The Rust crate writes a small dataset's parts, its conformance payload as
 * canonical JSON text, and its semantic digest, with edits and refused
 * payloads, and then artifacts that realize the dataset: payload files and
 * sealed manifests. It also writes the committed artifact of revision "0".
 * This script shares no code with it: it writes the canonical text itself,
 * checks the order design 017 fixes, rebuilds the preimage from the parts,
 * re-frames and re-hashes it, recomputes each manifest's integrity, and
 * hashes each payload file and each vendored CLDR source.
 */

import { createHash } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { digest } from './framing.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const crate = resolve(here, '../../../crates/intlify_locale_icu4x')
const fixtures = resolve(crate, 'fixtures')
const datasetPath = resolve(fixtures, 'dataset/vectors.json')
const artifactDirectory = resolve(fixtures, 'artifact')
const dataDirectory = resolve(crate, 'data')
const cldrDirectory = resolve(crate, 'cldr')

/** The fixture manifests, each beside the payload files it names. */
const MANIFESTS = ['manifest-narrow.json', 'manifest-wide.json', 'manifest-vendored.json']

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

/**
 * Whether a list of strings is strictly ascending by UTF-8 bytes.
 *
 * @param texts - The strings, in list order.
 * @returns Whether each is after the one before it.
 */
function strictlyAscending(texts) {
  return texts.every((text, index) => index === 0 || compareText(texts[index - 1], text) < 0)
}

/**
 * Check one manifest on its own: its fixed members, its closed lists, its
 * integrity, and each payload file it names.
 *
 * @param directory - Where the manifest and its payload files are.
 * @param name - The manifest's file name, for messages.
 * @param manifest - The manifest.
 * @param fail - Reports one problem.
 */
function checkManifest(directory, name, manifest, fail) {
  if (manifest.kind !== 'locale-canonicalization-data' || manifest.schemaRevision !== '0') {
    fail(`${name} is not a locale-canonicalization-data manifest revision "0"`)
  }
  if (manifest.payloads.map(payload => payload.role).join() !== 'conformance,markers') {
    fail(`${name} does not have one conformance and one markers payload, in that order`)
  }
  if (!strictlyAscending(manifest.generation.cldr.sources.map(source => source.address))) {
    fail(`${name} does not list its CLDR sources sorted by address, each once`)
  }
  if (!strictlyAscending(manifest.generation.markers)) {
    fail(`${name} does not list its markers sorted, each once`)
  }
  const { integrityDigest, ...rest } = manifest
  const integrity = digest('locale-canonicalization-data-integrity', rest)
  if (integrity !== integrityDigest) {
    fail(`${name}:\n  committed ${integrityDigest}\n  produced  ${integrity}`)
  }
  for (const payload of manifest.payloads) {
    const bytes = readFileSync(resolve(directory, payload.address))
    if (String(bytes.length) !== payload.byteLength) {
      fail(`${name}: ${payload.address} is ${bytes.length} bytes, not ${payload.byteLength}`)
    }
    const file = `sha256:${createHash('sha256').update(bytes).digest('hex')}`
    if (file !== payload.digest) {
      fail(`${name}: ${payload.address}\n  committed ${payload.digest}\n  produced  ${file}`)
    }
  }
}

/**
 * Check the fixture artifacts: each manifest on its own, the conformance
 * payload as the canonical text of the dataset's conformance data, and what
 * the three manifests share and do not.
 *
 * @param vectors - The dataset vectors the artifacts realize.
 * @returns How many checks failed.
 */
function checkArtifacts(vectors) {
  let failures = 0
  const fail = message => {
    console.error(`artifact: ${message}`)
    failures += 1
  }
  const [narrow, wide, vendored] = MANIFESTS.map(name =>
    JSON.parse(readFileSync(resolve(artifactDirectory, name), 'utf8'))
  )
  for (const [index, manifest] of [narrow, wide, vendored].entries()) {
    checkManifest(artifactDirectory, MANIFESTS[index], manifest, fail)
    if (manifest.dataset.semanticDigest !== vectors.semanticDigest) {
      fail(`${MANIFESTS[index]} does not name the dataset vectors' semantic digest`)
    }
  }
  const conformance = readFileSync(resolve(artifactDirectory, 'conformance.json'), 'utf8')
  if (conformance !== canonicalText(vectors.conformance)) {
    fail("conformance.json is not the canonical text of the dataset's conformance data")
  }
  const same = (left, right) => canonicalText(left) === canonicalText(right)
  // Two blob layouts: one dataset, one conformance payload, two markers payloads.
  if (!same(narrow.dataset, wide.dataset) || !same(narrow.payloads[0], wide.payloads[0])) {
    fail('the narrow and wide artifacts do not share their dataset and conformance payload')
  }
  if (narrow.payloads[1].digest === wide.payloads[1].digest) {
    fail('the narrow and wide markers payloads have one file digest')
  }
  // Moved CLDR sources: the same data and payloads, another artifact.
  if (!same(narrow.dataset, vendored.dataset) || !same(narrow.payloads, vendored.payloads)) {
    fail('the vendored artifact does not keep the dataset and payloads')
  }
  if (same(narrow.generation, vendored.generation)) {
    fail('the vendored artifact does not change its provenance')
  }
  const integrities = new Set([narrow, wide, vendored].map(manifest => manifest.integrityDigest))
  if (integrities.size !== 3) {
    fail('two artifacts share an integrity digest')
  }
  return failures
}

/**
 * Check the committed artifact of revision "0": the manifest and its payload
 * files, the conformance payload as canonical text in 017's order, and every
 * CLDR source the manifest records against the vendored file.
 *
 * @returns How many checks failed.
 */
function checkRevisionArtifact() {
  let failures = 0
  const fail = message => {
    console.error(`revision 0: ${message}`)
    failures += 1
  }
  const manifest = JSON.parse(readFileSync(resolve(dataDirectory, 'manifest.json'), 'utf8'))
  checkManifest(dataDirectory, 'manifest.json', manifest, fail)
  const text = readFileSync(resolve(dataDirectory, 'conformance.json'), 'utf8')
  const conformance = JSON.parse(text)
  if (canonicalText(conformance) !== text) {
    fail('conformance.json is not canonical JSON text')
  }
  for (const problem of checkConformance(conformance)) {
    fail(problem)
  }
  for (const source of manifest.generation.cldr.sources) {
    const bytes = readFileSync(resolve(cldrDirectory, source.address))
    const file = `sha256:${createHash('sha256').update(bytes).digest('hex')}`
    if (file !== source.digest) {
      fail(`${source.address}\n  recorded ${source.digest}\n  vendored ${file}`)
    }
  }
  return failures
}

const dataset = JSON.parse(readFileSync(datasetPath, 'utf8'))
const failures = checkDataset(dataset) + checkArtifacts(dataset) + checkRevisionArtifact()

if (failures > 0) {
  console.error(`\n${failures} locale vector check(s) failed`)
  process.exit(1)
}
console.log(
  `a dataset and ${dataset.edits.length} edits of it agree with an independent implementation`
)
console.log(`${MANIFESTS.length} artifacts realizing it agree with an independent implementation`)
console.log('the committed revision 0 artifact agrees with an independent implementation')
