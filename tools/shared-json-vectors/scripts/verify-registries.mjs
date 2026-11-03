/**
 * Check the committed Registry Package members against this implementation.
 *
 * The Rust crate writes the Safe Origin Kind Registry and its schema, and
 * compiles in both file digests. This script shares no code with it: it builds
 * the registry from its own copy of design 015's Safe Origin Kind table,
 * checks that the schema closes the container, every row, and both
 * vocabularies, hashes both artifacts, and reads the two pinned digests out of
 * the crate's source. It does not read the design document.
 */

import { createHash } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { isDeepStrictEqual } from 'node:util'

const here = dirname(fileURLToPath(import.meta.url))
const crate = resolve(here, '../../../crates/intlify_config')
const registries = resolve(crate, 'conformance/registries')

const DRAFT_7 = 'http://json-schema.org/draft-07/schema#'
const RANK_PATTERN = '^(0|[1-9][0-9]*)$'

/** Design 015's seven evidence kinds, in the order it lists them. */
const EVIDENCE_KINDS = [
  'entry-source',
  'configuration-value',
  'profile-selector',
  'specification-input',
  'artifact-input',
  'verification-input',
  'implementation-capability'
]

/**
 * Design 015's Safe Origin Kind table: each kind's ID and the evidence kinds
 * it permits, in rank order from zero.
 *
 * @type {[string, string[]][]}
 */
const SAFE_ORIGIN_KINDS = [
  ['configuration-file', ['entry-source', 'configuration-value']],
  ['configuration-programmatic', ['entry-source', 'configuration-value']],
  ['profile-selector-cli', ['profile-selector']],
  ['profile-selector-programmatic', ['profile-selector']],
  ['profile-resolution-specification-input', ['specification-input']],
  ['intent-surface-class-vocabulary', ['artifact-input']],
  ['locale-canonicalization-data', ['artifact-input']],
  ['profile-resolution-artifact-collection', ['artifact-input']],
  ['resource-limit-policy-verification', ['verification-input']],
  ['implementation-capability', ['implementation-capability']]
]

const failures = []
const fail = message => failures.push(message)

/**
 * Hash bytes and present the digest as the repository writes digests.
 *
 * @param bytes - The bytes.
 * @returns `sha256:` and 64 lowercase hexadecimal digits.
 */
function fileDigest(bytes) {
  return `sha256:${createHash('sha256').update(bytes).digest('hex')}`
}

/**
 * Read one digest the crate pins out of a module's source.
 *
 * @param source - The module's source text.
 * @param name - The constant's name.
 * @returns The digest, or undefined when it is not there.
 */
function pinned(source, name) {
  const match = source.match(
    new RegExp(`pub\\(crate\\) const ${name}: &str =\\s*"(sha256:[0-9a-f]{64})";`)
  )
  return match?.[1]
}

/**
 * Follow a schema object's reference to a definition of the same document.
 *
 * @param schema - The document.
 * @param node - A schema object.
 * @returns The definition it refers to, or the object itself.
 */
function follow(schema, node) {
  const name = node?.$ref?.match(/^#\/definitions\/([A-Za-z]+)$/)?.[1]
  return name === undefined ? node : schema.definitions?.[name]
}

/**
 * Whether a schema object is a closed object that requires exactly these
 * members.
 *
 * @param node - The schema object.
 * @param members - The member names, in order.
 * @returns Whether it closes them.
 */
function closes(node, members) {
  return (
    node?.type === 'object' &&
    node.additionalProperties === false &&
    isDeepStrictEqual(node.required, members) &&
    isDeepStrictEqual(Object.keys(node.properties ?? {}), members)
  )
}

const read = name => readFileSync(resolve(registries, name))
const schemaBytes = read('safe-origin-kind-v0.schema.json')
const registryBytes = read('safe-origin-kind-v0.json')
const source = readFileSync(resolve(crate, 'src/construction/registry/safe_origin.rs'), 'utf8')

for (const [name, bytes] of [
  ['safe-origin-kind-v0.schema.json', schemaBytes],
  ['safe-origin-kind-v0.json', registryBytes]
]) {
  if (
    bytes.includes(0x0d) ||
    !bytes.toString('utf8').endsWith('}\n') ||
    bytes.toString('utf8').endsWith('\n\n')
  ) {
    fail(`${name}: not written with one final newline and no carriage return`)
  }
}

// The registry: every kind in rank order, with its rank and permitted kinds.
const expected = {
  rows: SAFE_ORIGIN_KINDS.map(([id, kinds], rank) => ({
    id,
    rank: `${rank}`,
    permittedEvidenceKinds: kinds
  }))
}
const registry = JSON.parse(registryBytes.toString('utf8'))
if (JSON.stringify(registry) !== JSON.stringify(expected)) {
  fail(
    `safe origin kind registry: differs from 015's table\n  committed ${JSON.stringify(registry)}\n  expected  ${JSON.stringify(expected)}`
  )
}
if (expected.rows.length !== 10) {
  fail(`the transcribed safe origin kinds are ${expected.rows.length}, not 10`)
}
for (const [, kinds] of SAFE_ORIGIN_KINDS) {
  for (const kind of kinds) {
    if (!EVIDENCE_KINDS.includes(kind)) {
      fail(`the transcribed table permits ${kind}, which is no evidence kind`)
    }
  }
}
if (fileDigest(registryBytes) !== pinned(source, 'DIGEST')) {
  fail(
    `safe origin kind registry: the file digest ${fileDigest(registryBytes)} is not the pinned ${pinned(source, 'DIGEST')}`
  )
}

// The schema: Draft 7, closing the container, every row, the IDs, the
// evidence kinds, and a rank's spelling.
const schema = JSON.parse(schemaBytes.toString('utf8'))
if (schema.$schema !== DRAFT_7) {
  fail(`safe origin kind registry schema: $schema ${schema.$schema}`)
}
if (!closes(schema, ['rows'])) {
  fail('safe origin kind registry schema: the container is not closed')
}
const rows = schema.properties?.rows
if (rows?.type !== 'array' || rows.minItems !== 1) {
  fail('safe origin kind registry schema: the rows are not a non-empty array')
}
const row = follow(schema, rows?.items)
if (!closes(row, ['id', 'rank', 'permittedEvidenceKinds'])) {
  fail('safe origin kind registry schema: a row is not closed')
}
const id = follow(schema, row?.properties?.id)
if (
  id?.type !== 'string' ||
  !isDeepStrictEqual(
    id.enum,
    SAFE_ORIGIN_KINDS.map(([kind]) => kind)
  )
) {
  fail(`safe origin kind registry schema: the IDs are ${JSON.stringify(id?.enum)}`)
}
const rank = row?.properties?.rank
if (rank?.type !== 'string' || rank.pattern !== RANK_PATTERN) {
  fail('safe origin kind registry schema: a rank is not a shortest decimal string')
}
const permitted = row?.properties?.permittedEvidenceKinds
const kind = follow(schema, permitted?.items)
if (
  permitted?.type !== 'array' ||
  permitted.minItems !== 1 ||
  permitted.uniqueItems !== true ||
  kind?.type !== 'string' ||
  !isDeepStrictEqual(kind.enum, EVIDENCE_KINDS)
) {
  fail('safe origin kind registry schema: the permitted evidence kinds are not closed')
}
if (fileDigest(schemaBytes) !== pinned(source, 'SCHEMA_DIGEST')) {
  fail(
    `safe origin kind registry schema: the file digest ${fileDigest(schemaBytes)} is not the pinned ${pinned(source, 'SCHEMA_DIGEST')}`
  )
}

if (failures.length > 0) {
  for (const message of failures) {
    console.error(message)
  }
  process.exit(1)
}

console.log(
  `registries agree: ${expected.rows.length} safe origin kinds, ${EVIDENCE_KINDS.length} evidence kinds`
)
