/**
 * Check the committed Registry Package members against this implementation.
 *
 * The Rust crate writes the Safe Origin Kind Registry, the Check Registry, and
 * their schemas, and compiles in their file digests. This script shares no
 * code with it. It builds the Safe Origin Kind Registry from its own copy of
 * design 015's table. It checks that the Check Registry's rows agree with each
 * other the way 015's registry rules require and carry 015's five example
 * check IDs, without rebuilding the rows. It checks that each schema closes
 * its container, every row, and its vocabularies, hashes every artifact, and
 * reads the pinned digests out of the crate's source. It does not read the
 * design document.
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
const SEGMENT = '[a-z][a-z0-9]*(-[a-z0-9]+)*'
const CHECK_ID_PATTERN = `^${SEGMENT}\\.${SEGMENT}\\.${SEGMENT}$`
const RULE_ID_PATTERN = `^015\\.${SEGMENT}\\.${SEGMENT}\\.${SEGMENT}$`
const SCOPE_PATTERN = `^${SEGMENT}$`

/** A check ID as the schema's pattern spells it, with no capturing group. */
const CHECK_ID =
  /^[a-z][a-z0-9]*(?:-[a-z0-9]+)*\.[a-z][a-z0-9]*(?:-[a-z0-9]+)*\.[a-z][a-z0-9]*(?:-[a-z0-9]+)*$/

/** Design 015's sixteen phases, in rank order. */
const PHASES = [
  'resolver-bootstrap-admission',
  'entry-admission',
  'structural-admission',
  'profile-selection',
  'resource-policy-admission',
  'specification-admission',
  'project-identity-resolution',
  'locale-resolution',
  'locale-policy-resolution',
  'artifact-reference-resolution',
  'delivery-resolution',
  'target-resolution',
  'group-resolution',
  'hydration-validation',
  'profile-projection',
  'reporting-finalization'
]

/** Design 015's four subject-key strategies and three content projections. */
const STRATEGIES = [
  'singleton',
  'logical-field',
  'canonical-identity-or-content',
  'semantic-position'
]
const PROJECTIONS = ['none', 'non-secret-content', 'redacted-structure']

/**
 * Design 015's five example check IDs, each with the phase and subject kind of
 * the check it names.
 *
 * @type {[string, string, string][]}
 */
const EXAMPLE_CHECKS = [
  ['locale.default-requested.member', 'locale-resolution', 'default-requested-locale'],
  ['target.requested-locales.project-subset', 'target-resolution', 'target'],
  ['group.target.single-group', 'group-resolution', 'assignment'],
  ['hydration.relation.same-requested-locales', 'hydration-validation', 'relation'],
  ['hydration.relation.same-effective-default', 'hydration-validation', 'relation']
]

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
const checkSchemaBytes = read('check-v0.schema.json')
const checkRegistryBytes = read('check-v0.json')
const source = readFileSync(resolve(crate, 'src/construction/registry/safe_origin.rs'), 'utf8')
const checkSource = readFileSync(resolve(crate, 'src/construction/registry/check.rs'), 'utf8')

for (const [name, bytes] of [
  ['safe-origin-kind-v0.schema.json', schemaBytes],
  ['safe-origin-kind-v0.json', registryBytes],
  ['check-v0.schema.json', checkSchemaBytes],
  ['check-v0.json', checkRegistryBytes]
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

// The Check Registry: rows that agree with each other as 015's registry rules
// require. The script does not rebuild the rows; their values are pinned by
// the registry's file digest.
const checkRegistry = JSON.parse(checkRegistryBytes.toString('utf8'))
const checkRows = checkRegistry.rows ?? []
const checkIds = new Set()
const kinds = new Map()
const phasesWithRows = new Set()
let previous
for (const row of checkRows) {
  const where = `check registry: ${row.checkId}`
  if (!CHECK_ID.test(row.checkId)) {
    fail(`${where}: not three kebab-case segments`)
  }
  if (row.ruleId !== `015.${row.checkId}`) {
    fail(`${where}: the rule ID ${row.ruleId} is not the check ID under 015`)
  }
  if (checkIds.has(row.checkId)) {
    fail(`${where}: repeated`)
  }
  checkIds.add(row.checkId)
  const phase = PHASES.indexOf(row.phaseId)
  if (phase < 0) {
    fail(`${where}: the phase ${row.phaseId} is no phase of 015`)
  }
  phasesWithRows.add(row.phaseId)
  // Rows follow canonical unit order: phase, kind rank, then check rank.
  const order = [phase, Number(row.subjectKindRank), Number(row.checkRank)]
  if (
    previous !== undefined &&
    !(
      order[0] > previous[0] ||
      (order[0] === previous[0] && order[1] > previous[1]) ||
      (order[0] === previous[0] && order[1] === previous[1] && order[2] > previous[2])
    )
  ) {
    fail(`${where}: out of phase, kind-rank, and check-rank order`)
  }
  previous = order
  // One subject kind, one shape: the same phase, strategy, projection,
  // owning scope, parent, and rank on every row of it.
  const shape = JSON.stringify([
    row.phaseId,
    row.subjectKeyStrategy,
    row.subjectContentProjection,
    row.owningScope,
    row.parentSubject ?? null,
    row.subjectKindRank
  ])
  const kind = kinds.get(row.subjectKind)
  if (kind === undefined) {
    kinds.set(row.subjectKind, { shape, row, checks: 1 })
  } else {
    if (kind.shape !== shape) {
      fail(`${where}: its kind ${row.subjectKind} has another shape on another row`)
    }
    kind.checks += 1
  }
  if (row.checkRank !== `${(kind?.checks ?? 1) - 1}`) {
    fail(`${where}: the check ranks of ${row.subjectKind} do not count up from zero`)
  }
  // A kind keyed by an identity or content projects it, and no row
  // classifies content as non-secret.
  const identity = row.subjectKeyStrategy === 'canonical-identity-or-content'
  if (identity !== (row.subjectContentProjection !== 'none')) {
    fail(
      `${where}: the projection ${row.subjectContentProjection} does not fit ${row.subjectKeyStrategy}`
    )
  }
  if (row.subjectContentProjection === 'non-secret-content') {
    fail(`${where}: classifies content as non-secret`)
  }
  // A singleton's owning scope is its own kind, and only a singleton's.
  const singleton = row.subjectKeyStrategy === 'singleton'
  if (singleton !== (row.owningScope === row.subjectKind)) {
    fail(`${where}: the owning scope ${row.owningScope} does not fit ${row.subjectKeyStrategy}`)
  }
  if (singleton && row.parentSubject !== undefined) {
    fail(`${where}: a singleton is scoped by a parent`)
  }
}
// Each phase's kinds count their ranks up from zero.
const ranksByPhase = new Map()
for (const [, { row }] of kinds) {
  const ranks = ranksByPhase.get(row.phaseId) ?? []
  ranks.push(Number(row.subjectKindRank))
  ranksByPhase.set(row.phaseId, ranks)
}
for (const [phase, ranks] of ranksByPhase) {
  if (
    !isDeepStrictEqual(
      ranks,
      ranks.map((_, index) => index)
    )
  ) {
    fail(`check registry: the kind ranks of ${phase} are ${JSON.stringify(ranks)}`)
  }
}
// A parent is a kind keyed by an identity or content, with rows in the same
// or an earlier phase.
for (const [name, { row }] of kinds) {
  const parent = row.parentSubject
  if (parent === undefined) {
    continue
  }
  const own = kinds.get(parent.subjectKind)
  if (own === undefined || own.row.phaseId !== parent.phaseId) {
    fail(`check registry: ${name}'s parent ${JSON.stringify(parent)} has no rows`)
    continue
  }
  if (PHASES.indexOf(parent.phaseId) > PHASES.indexOf(row.phaseId)) {
    fail(`check registry: ${name}'s parent comes in a later phase`)
  }
  if (own.row.subjectKeyStrategy !== 'canonical-identity-or-content') {
    fail(`check registry: ${name}'s parent is not keyed by an identity or content`)
  }
}
// Every phase but profile projection has rows.
const expectedPhases = PHASES.filter(phase => phase !== 'profile-projection')
if (!isDeepStrictEqual([...phasesWithRows], expectedPhases)) {
  fail(`check registry: the phases with rows are ${JSON.stringify([...phasesWithRows])}`)
}
for (const [id, phase, kind] of EXAMPLE_CHECKS) {
  const row = checkRows.find(candidate => candidate.checkId === id)
  if (row?.phaseId !== phase || row?.subjectKind !== kind) {
    fail(`check registry: 015's example ${id} is not the check of ${kind} in ${phase}`)
  }
}
if (kinds.get('diagnostic-domain')?.row.phaseId !== 'reporting-finalization') {
  fail('check registry: no diagnostic-domain kind in reporting-finalization')
}
if (fileDigest(checkRegistryBytes) !== pinned(checkSource, 'DIGEST')) {
  fail(
    `check registry: the file digest ${fileDigest(checkRegistryBytes)} is not the pinned ${pinned(checkSource, 'DIGEST')}`
  )
}

// The Check Registry's schema: Draft 7, closing the container, every row, its
// parent, and the vocabularies of phases, kinds, strategies, and projections.
const checkSchema = JSON.parse(checkSchemaBytes.toString('utf8'))
if (checkSchema.$schema !== DRAFT_7) {
  fail(`check registry schema: $schema ${checkSchema.$schema}`)
}
if (!closes(checkSchema, ['rows'])) {
  fail('check registry schema: the container is not closed')
}
const checkRowsSchema = checkSchema.properties?.rows
if (checkRowsSchema?.type !== 'array' || checkRowsSchema.minItems !== 1) {
  fail('check registry schema: the rows are not a non-empty array')
}
const checkRow = follow(checkSchema, checkRowsSchema?.items)
const required = [
  'checkId',
  'phaseId',
  'ruleId',
  'subjectKind',
  'subjectKeyStrategy',
  'subjectContentProjection',
  'owningScope',
  'subjectKindRank',
  'checkRank'
]
const members = [...required.slice(0, 7), 'parentSubject', ...required.slice(7)]
if (
  checkRow?.type !== 'object' ||
  checkRow.additionalProperties !== false ||
  !isDeepStrictEqual(checkRow.required, required) ||
  !isDeepStrictEqual(Object.keys(checkRow.properties ?? {}), members)
) {
  fail('check registry schema: a row is not closed')
}
const property = name => checkRow?.properties?.[name]
for (const [name, pattern] of [
  ['checkId', CHECK_ID_PATTERN],
  ['ruleId', RULE_ID_PATTERN],
  ['owningScope', SCOPE_PATTERN],
  ['subjectKindRank', RANK_PATTERN],
  ['checkRank', RANK_PATTERN]
]) {
  if (property(name)?.type !== 'string' || property(name)?.pattern !== pattern) {
    fail(`check registry schema: ${name} is not a string of ${pattern}`)
  }
}
const vocabulary = node => {
  const definition = follow(checkSchema, node)
  return definition?.type === 'string' ? definition.enum : undefined
}
if (!isDeepStrictEqual(vocabulary(property('phaseId')), PHASES)) {
  fail("check registry schema: the phases are not 015's sixteen")
}
if (!isDeepStrictEqual(vocabulary(property('subjectKeyStrategy')), STRATEGIES)) {
  fail("check registry schema: the strategies are not 015's four")
}
if (!isDeepStrictEqual(vocabulary(property('subjectContentProjection')), PROJECTIONS)) {
  fail("check registry schema: the projections are not 015's three")
}
const kindIds = vocabulary(property('subjectKind')) ?? []
if (!isDeepStrictEqual(new Set(kindIds), new Set(kinds.keys())) || kindIds.length !== kinds.size) {
  fail('check registry schema: the kinds are not the ones the rows name, each once')
}
const parentSchema = follow(checkSchema, property('parentSubject')?.allOf?.[0])
if (!closes(parentSchema, ['phaseId', 'subjectKind'])) {
  fail('check registry schema: a parent is not closed')
}
if (fileDigest(checkSchemaBytes) !== pinned(checkSource, 'SCHEMA_DIGEST')) {
  fail(
    `check registry schema: the file digest ${fileDigest(checkSchemaBytes)} is not the pinned ${pinned(checkSource, 'SCHEMA_DIGEST')}`
  )
}

if (failures.length > 0) {
  for (const message of failures) {
    console.error(message)
  }
  process.exit(1)
}

console.log(
  `registries agree: ${expected.rows.length} safe origin kinds, ${EVIDENCE_KINDS.length} evidence kinds, ${checkRows.length} checks of ${kinds.size} subject kinds`
)
