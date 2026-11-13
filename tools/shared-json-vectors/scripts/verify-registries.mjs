/**
 * Check the committed Registry Package against this implementation.
 *
 * The Rust crate writes the Safe Origin Kind, Check, and Finding Registries,
 * their schemas, and the package's container, and compiles in their file
 * digests. This script shares no code with it. It builds the Safe Origin Kind
 * Registry from its own copy of design 015's table. It checks that the Check
 * Registry's rows agree with each other the way 015's registry rules require
 * and carry 015's five example check IDs, without rebuilding the rows, that
 * the Finding Registry's rows agree with them and with its own copy of 015's
 * code tables, and that its disclosure policies are the ones its own copy of
 * 015's Evidence views and of the capability bound tables gives. It checks
 * that each schema closes its container, every row and policy, and its
 * vocabularies, builds the package's container from the members' file
 * digests, hashes every artifact, and reads the pinned digests out of the
 * crate's source. It does not read the design document.
 */

import { createHash } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { isDeepStrictEqual } from 'node:util'

import { BOOTSTRAP_GROUPS, POLICY_GROUPS } from './resolution-bounds.mjs'

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

/** Design 015's twelve ordinary Evidence record kinds, the Entry domain's first. */
const RECORD_KINDS = [
  'entry-finding',
  'entry-evaluation-status',
  'finding-occurrence',
  'coverage-decision-evidence',
  'evidence-index',
  'entry-dependency',
  'shared-unavailability-cause',
  'shared-finding',
  'shared-evaluation-status',
  'coverage-decision-basis',
  'semantic-dependency',
  'materialized-dependency'
]

/** Design 015's twelve semantic group IDs, in the order of its inventory. */
const SEMANTIC_GROUPS = [
  'profileSpecification',
  'projectIdentity',
  'localeCanonicalization',
  'projectLocales',
  'localeNegotiation',
  'messageFallback',
  'coverage',
  'requiredPolicies',
  'optionalPolicies',
  'targets',
  'deploymentGroups',
  'delivery'
]

/** Design 015's common and materialized input slots, the common ones first. */
const MATERIALIZED_SLOTS = [
  'specification-input',
  'intent-surface-class-vocabulary',
  'locale-canonicalization-data',
  'artifact-collection',
  'verification-input',
  'implementation-capability',
  'configuration-value',
  'profile-selector'
]

/** Design 015's entry input slots. */
const ENTRY_SLOTS = ['entry-source', 'call-site-source', 'profile-selector', 'origins']

/** The IDs each dependency record kind names. */
const DEPENDENCY_IDS = {
  'semantic-dependency': SEMANTIC_GROUPS,
  'materialized-dependency': MATERIALIZED_SLOTS,
  'entry-dependency': ENTRY_SLOTS
}

/** Design 015's forty-eight Finding codes, in the order of its tables. */
const CODES = [
  'project-profile-json-invalid',
  'project-profile-json-member-duplicate',
  'project-profile-input-not-json-compatible',
  'project-profile-schema-version-unsupported',
  'project-profile-required-field-missing',
  'project-profile-field-type-invalid',
  'project-profile-field-value-invalid',
  'project-profile-unknown-field',
  'project-profile-selector-required',
  'project-profile-selector-invalid',
  'project-profile-selector-unknown',
  'project-profile-specification-input-incomplete',
  'project-profile-specification-not-admitted',
  'project-profile-locale-specification-not-admitted',
  'project-profile-locale-data-not-admitted',
  'project-profile-negotiation-specification-not-admitted',
  'project-profile-message-fallback-specification-not-admitted',
  'project-profile-coverage-specification-not-admitted',
  'project-profile-delivery-specification-not-admitted',
  'project-profile-intent-surface-class-vocabulary-not-admitted',
  'project-profile-locale-invalid',
  'project-profile-locale-non-canonical',
  'project-profile-locale-duplicate',
  'project-profile-default-requested-locale-not-in-project',
  'project-profile-target-locales-not-subset',
  'project-profile-target-default-locale-not-supported',
  'project-profile-target-locale-direction-missing',
  'project-profile-negotiation-alias-duplicate',
  'project-profile-negotiation-alias-destination-not-requested',
  'project-profile-message-fallback-source-not-requested',
  'project-profile-message-fallback-sequence-invalid',
  'project-profile-coverage-rule-invalid',
  'project-profile-coverage-rule-duplicate',
  'project-profile-coverage-rule-conflict',
  'project-profile-artifact-reference-not-admitted',
  'project-profile-artifact-collection-not-admitted',
  'project-profile-group-member-unknown',
  'project-profile-group-member-duplicate',
  'project-profile-target-unassigned',
  'project-profile-target-assigned-to-multiple-groups',
  'project-profile-hydration-relation-invalid',
  'project-profile-hydration-locale-policy-incompatible',
  'project-profile-hydration-capability-incompatible',
  'project-profile-resource-policy-not-admitted',
  'project-profile-resource-policy-capability-incompatible',
  'project-profile-bootstrap-limit-exceeded',
  'project-profile-resource-limit-exceeded',
  'project-profile-reporting-limit-exceeded'
]

/** Design 015's closed vocabulary of a Finding's details members, in its order. */
const DETAILS_MEMBERS = [
  'reason',
  'field',
  'component',
  'slot',
  'subject',
  'resourceGroup',
  'bound',
  'limit',
  'actual',
  'required',
  'available',
  'actualRelation'
]

/**
 * The stable reasons design 015's code tables give each code that has any, in
 * the order the tables list them.
 *
 * @type {Record<string, string[]>}
 */
const CODE_REASONS = {
  'project-profile-input-not-json-compatible': [
    'unsupported-type',
    'non-finite-number',
    'non-portable-number',
    'invalid-unicode',
    'non-string-key',
    'dynamic-container',
    'dynamic-property',
    'sparse-array',
    'cycle'
  ],
  'project-profile-specification-input-incomplete': ['missing-required-slot'],
  'project-profile-specification-not-admitted': ['unsupported', 'incompatible'],
  'project-profile-locale-specification-not-admitted': ['unsupported', 'incompatible'],
  'project-profile-negotiation-specification-not-admitted': ['unsupported', 'incompatible'],
  'project-profile-message-fallback-specification-not-admitted': ['unsupported', 'incompatible'],
  'project-profile-coverage-specification-not-admitted': ['unsupported', 'incompatible'],
  'project-profile-delivery-specification-not-admitted': ['unsupported', 'incompatible'],
  'project-profile-intent-surface-class-vocabulary-not-admitted': [
    'body-integrity-invalid',
    'member-invalid',
    'member-duplicate',
    'non-canonical-order',
    'identity-mismatch',
    'revision-mismatch',
    'digest-mismatch'
  ],
  'project-profile-target-default-locale-not-supported': [
    'explicit-override',
    'inherited-project-default'
  ],
  'project-profile-negotiation-alias-duplicate': ['same-destination', 'conflicting-destination'],
  'project-profile-message-fallback-sequence-invalid': [
    'literal-self-reference',
    'literal-duplicate',
    'intent-source-duplicate'
  ],
  'project-profile-coverage-rule-invalid': [
    'locale-not-requested',
    'surface-class-unknown',
    'surface-class-duplicate'
  ],
  'project-profile-artifact-reference-not-admitted': [
    'not-found',
    'kind-mismatch',
    'identity-mismatch',
    'revision-mismatch',
    'specification-revision-mismatch',
    'specification-revision-unsupported',
    'role-kind-mismatch',
    'digest-mismatch'
  ],
  'project-profile-artifact-collection-not-admitted': [
    'envelope-invalid',
    'kind-unsupported',
    'duplicate-envelope',
    'conflicting-content'
  ],
  'project-profile-hydration-relation-invalid': [
    'equal-endpoint',
    'unknown-endpoint',
    'cross-group',
    'duplicate-relation',
    'server-capability-missing',
    'client-capability-missing',
    'dual-role'
  ],
  'project-profile-hydration-locale-policy-incompatible': [
    'requested-locales',
    'effective-default'
  ],
  'project-profile-resource-policy-not-admitted': [
    'body-integrity-invalid',
    'body-schema-invalid',
    'project-profile-resolution-missing',
    'project-profile-resolution-invalid'
  ],
  'project-profile-reporting-limit-exceeded': ['entry', 'shared']
}

/** Design 015's stable reasons, in the order of its tables, each where it first appears. */
const REASONS = [...new Set(CODES.flatMap(code => CODE_REASONS[code] ?? []))]

/** What a row's Finding payloads may derive from: its key alone, or the one value it names. */
const DERIVATIONS = ['key-only', 'exact']

/**
 * The details members design 015's code tables give each code that carries
 * any, in the vocabulary's order: a stable reason wherever a code's row names
 * reasons, the schema field, the missing slot, the delivery component, and a
 * bound's subject kind, group, ID, value, and total, or what the policy
 * requires and the capability makes available. Every other code's details are
 * empty.
 *
 * @type {Record<string, string[]>}
 */
const CODE_DETAILS = {
  'project-profile-input-not-json-compatible': ['reason'],
  'project-profile-required-field-missing': ['field'],
  'project-profile-field-type-invalid': ['field'],
  'project-profile-field-value-invalid': ['field'],
  'project-profile-unknown-field': ['field'],
  'project-profile-specification-input-incomplete': ['reason', 'slot'],
  'project-profile-specification-not-admitted': ['reason'],
  'project-profile-locale-specification-not-admitted': ['reason'],
  'project-profile-negotiation-specification-not-admitted': ['reason'],
  'project-profile-message-fallback-specification-not-admitted': ['reason'],
  'project-profile-coverage-specification-not-admitted': ['reason'],
  'project-profile-delivery-specification-not-admitted': ['reason', 'component'],
  'project-profile-intent-surface-class-vocabulary-not-admitted': ['reason'],
  'project-profile-target-default-locale-not-supported': ['reason'],
  'project-profile-negotiation-alias-duplicate': ['reason'],
  'project-profile-message-fallback-sequence-invalid': ['reason'],
  'project-profile-coverage-rule-invalid': ['reason'],
  'project-profile-artifact-reference-not-admitted': ['reason'],
  'project-profile-artifact-collection-not-admitted': ['reason'],
  'project-profile-hydration-relation-invalid': ['reason'],
  'project-profile-hydration-locale-policy-incompatible': ['reason'],
  'project-profile-resource-policy-not-admitted': ['reason'],
  'project-profile-resource-policy-capability-incompatible': [
    'resourceGroup',
    'bound',
    'required',
    'available'
  ],
  'project-profile-bootstrap-limit-exceeded': ['bound', 'limit', 'actual', 'actualRelation'],
  'project-profile-resource-limit-exceeded': [
    'subject',
    'resourceGroup',
    'bound',
    'limit',
    'actual',
    'actualRelation'
  ],
  'project-profile-reporting-limit-exceeded': ['reason', 'bound']
}

/**
 * The singleton checks that see each occurrence on its own, as 015's bound
 * table says of the longest string, and as the duplicate members and the
 * values outside the JSON domain each are: their payloads derive from the key
 * alone.
 */
const INDEPENDENT_SINGLETON_CHECKS = [
  'entry.input.unique-members',
  'entry.input.json-compatible',
  'entry.input.within-single-string-bytes',
  'resource-policy.root-longest-string.within-bound'
]

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

/** The members of design 015's Evidence views, in view order. */
const MEMBERS = ['source', 'locator', 'path', 'value', 'span', 'selector', 'component']

/**
 * The components of design 015's Evidence views: each member's leaf
 * variants, in the order of the view's members and of each member's
 * variants, with the projection whose token replaces a redacted one.
 *
 * @type {[string, string, string | undefined][]}
 */
const COMPONENTS = [
  ['source', 'source.safe-opaque', undefined],
  ['source', 'source.redacted', 'source-identity'],
  ['locator', 'locator.relative-path', undefined],
  ['locator', 'locator.safe-label', undefined],
  ['locator', 'locator.safe-uri', undefined],
  ['path', 'path.safe-member', undefined],
  ['path', 'path.array-index', undefined],
  ['path', 'path.redacted-member', 'logical-input-path-member'],
  ['value', 'value.exact-json-pointer', undefined],
  ['value', 'value.redacted-logical-path.safe-member', undefined],
  ['value', 'value.redacted-logical-path.array-index', undefined],
  ['value', 'value.redacted-logical-path.redacted-member', 'value-path-member'],
  ['span', 'span', undefined],
  ['selector', 'selector.absent', undefined],
  ['selector', 'selector.safe-profile-id', undefined],
  ['selector', 'selector.redacted-value', 'profile-selector'],
  ['component', 'component.fixed-role', undefined],
  ['component', 'component.safe-key', undefined],
  ['component', 'component.redacted-key', 'resolver-input-component-key']
]

/**
 * For each evidence kind, the members every Evidence of the kind has and the
 * members its view can have.
 *
 * @type {Record<string, [string[], string[]]>}
 */
const VIEW_MEMBERS = {
  'entry-source': [['source'], ['source', 'locator', 'path', 'span']],
  'configuration-value': [
    ['source', 'value'],
    ['source', 'locator', 'value', 'span']
  ],
  'profile-selector': [['selector'], ['selector']],
  'specification-input': [['component'], ['component']],
  'artifact-input': [['component'], ['component']],
  'verification-input': [['component'], ['component']],
  'implementation-capability': [['component'], ['component']]
}

/**
 * The registry's names for 015's conditions under which a literal
 * component's content counts as non-secret: an owner-marked identity, an
 * adapter-classified locator, a schema-defined name, an identifier key, a
 * canonical locale key, a declared Profile ID, a vocabulary identity, and an
 * admitted exact reference.
 */
const NON_SECRET_CLASSES = [
  'owner-marked-identity',
  'adapter-classified-locator',
  'schema-defined-name',
  'identifier-key',
  'canonical-locale-key',
  'declared-profile-id',
  'vocabulary-identity',
  'admitted-exact-reference'
]

/**
 * The evidence kinds a Redacted evidence token's record can name: 015's four
 * component projections, in its order, then a selector's.
 */
const TOKEN_KINDS = [
  'source-identity',
  'logical-input-path-member',
  'value-path-member',
  'resolver-input-component-key',
  'profile-selector'
]

/** Design 015's Specification Input slots, in slot order. */
const SLOTS = [
  'project-profile',
  'locale-canonicalization',
  'locale-negotiation',
  'message-locale-fallback',
  'coverage',
  'intent-surface-class-vocabulary',
  'delivery-graph',
  'delivery-placement'
]

/** Design 015's six Registry Package roles, in role order. */
const ROLES = [
  'safe-origin-kind-registry-schema',
  'check-registry-schema',
  'finding-registry-schema',
  'safe-origin-kind-registry',
  'check-registry',
  'finding-registry'
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
 * members and may also have the optional ones after them.
 *
 * @param node - The schema object.
 * @param members - The required member names, in order.
 * @param optional - The optional member names, in order.
 * @returns Whether it closes them.
 */
function closes(node, members, optional = []) {
  return (
    node?.type === 'object' &&
    node.additionalProperties === false &&
    isDeepStrictEqual(node.required, members) &&
    isDeepStrictEqual(Object.keys(node.properties ?? {}), [...members, ...optional])
  )
}

const read = name => readFileSync(resolve(registries, name))
const schemaBytes = read('safe-origin-kind-v0.schema.json')
const registryBytes = read('safe-origin-kind-v0.json')
const checkSchemaBytes = read('check-v0.schema.json')
const checkRegistryBytes = read('check-v0.json')
const findingSchemaBytes = read('finding-v0.schema.json')
const findingRegistryBytes = read('finding-v0.json')
const packageBytes = read('package-v0.json')
const source = readFileSync(resolve(crate, 'src/construction/registry/safe_origin.rs'), 'utf8')
const checkSource = readFileSync(resolve(crate, 'src/construction/registry/check.rs'), 'utf8')
const findingSource = readFileSync(resolve(crate, 'src/construction/registry/finding.rs'), 'utf8')
const packageSource = readFileSync(resolve(crate, 'src/construction/registry.rs'), 'utf8')

for (const [name, bytes] of [
  ['safe-origin-kind-v0.schema.json', schemaBytes],
  ['safe-origin-kind-v0.json', registryBytes],
  ['check-v0.schema.json', checkSchemaBytes],
  ['check-v0.json', checkRegistryBytes],
  ['finding-v0.schema.json', findingSchemaBytes],
  ['finding-v0.json', findingRegistryBytes],
  ['package-v0.json', packageBytes]
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
// Following parents from any kind ends at an unscoped kind, so no kind
// scopes itself.
for (const [name] of kinds) {
  const seen = new Set()
  let current = name
  while (current !== undefined && !seen.has(current)) {
    seen.add(current)
    current = kinds.get(current)?.row.parentSubject?.subjectKind
  }
  if (current !== undefined) {
    fail(`check registry: ${name}'s parents lead back to ${current}`)
  }
}
// A row's prerequisites name earlier rows, so none is a cycle, each once and
// in row order, and none is one another listed prerequisite already waits on.
const rowIndex = new Map(checkRows.map((row, index) => [row.checkId, index]))
const requirements = new Map()
let prerequisiteCount = 0
for (const [index, row] of checkRows.entries()) {
  const where = `check registry: ${row.checkId}`
  const listed = row.prerequisiteCheckIds ?? []
  prerequisiteCount += listed.length
  const places = listed.map(id => rowIndex.get(id))
  if (places.some(place => place === undefined || place >= index)) {
    fail(`${where}: a prerequisite is no earlier row`)
    requirements.set(row.checkId, new Set())
    continue
  }
  if (places.some((place, at) => at > 0 && place <= places[at - 1])) {
    fail(`${where}: the prerequisites are not in row order, each once`)
  }
  const reached = new Set()
  for (const id of listed) {
    for (const required of requirements.get(id)) {
      reached.add(required)
    }
  }
  for (const id of listed) {
    if (reached.has(id)) {
      fail(`${where}: lists ${id}, which another prerequisite waits on`)
    }
  }
  requirements.set(row.checkId, new Set([...reached, ...listed]))
}
// A row's Finding codes are 015's, each once and in the order of its tables,
// and every code is some row's.
const reportedCodes = new Set()
for (const row of checkRows) {
  const places = (row.possibleFindingCodes ?? []).map(code => CODES.indexOf(code))
  if (places.some(place => place < 0)) {
    fail(`check registry: ${row.checkId}: a code is not one of 015's`)
  }
  if (places.some((place, at) => at > 0 && place <= places[at - 1])) {
    fail(`check registry: ${row.checkId}: the codes are not in 015's order, each once`)
  }
  for (const code of row.possibleFindingCodes ?? []) {
    reportedCodes.add(code)
  }
}
for (const code of CODES) {
  if (!reportedCodes.has(code)) {
    fail(`check registry: no row reports ${code}`)
  }
}
// A row's record kinds are 015's, each once and in its order, exactly those
// 015's ownership rules give its check, and every kind is some row's. Every
// dependency record has exactly one owner, which lists its record kind and
// is where 015 puts it.
const ownedKinds = new Set()
const owners = new Map()
const ENTRY_PHASES = ['resolver-bootstrap-admission', 'entry-admission']

/**
 * The ordinary Evidence record kinds 015 lets a check's units own, in its
 * order: none in reporting finalization, whose terminal records take reserved
 * slots; otherwise the check's status in its phase's domain, and where it
 * reports codes a Finding in that domain with the Entry domain's occurrences
 * and evidence index, in entry admission also the causes of a Shared
 * projection that cannot materialize; both coverage decision records for the
 * decision table, with the evidence index its Coverage Decision Evidence
 * refers to; and the kinds of the dependency records the check owns.
 *
 * @param row - A Check Registry row.
 * @returns The record kinds, in 015's order.
 */
function expectedRecordKinds(row) {
  if (row.phaseId === 'reporting-finalization') {
    return []
  }
  const entry = ENTRY_PHASES.includes(row.phaseId)
  const expected = new Set([entry ? 'entry-evaluation-status' : 'shared-evaluation-status'])
  if ((row.possibleFindingCodes ?? []).length > 0) {
    expected.add(entry ? 'entry-finding' : 'shared-finding')
    expected.add('finding-occurrence')
    expected.add('evidence-index')
    if (row.phaseId === 'entry-admission') {
      expected.add('shared-unavailability-cause')
    }
  }
  if (row.subjectKind === 'coverage-table') {
    expected.add('coverage-decision-evidence')
    expected.add('evidence-index')
    expected.add('coverage-decision-basis')
  }
  for (const owned of row.ownedDependencies ?? []) {
    expected.add(owned.recordKind)
  }
  return RECORD_KINDS.filter(kind => expected.has(kind))
}

/** The phase whose first kind, a singleton, owns each slot's dependency records. */
const SLOT_OWNERS = {
  'entry-dependency': 'entry-admission',
  'materialized-dependency': 'structural-admission'
}
for (const row of checkRows) {
  const where = `check registry: ${row.checkId}`
  const kinds = row.possibleEvidenceRecordKinds ?? []
  const places = kinds.map(kind => RECORD_KINDS.indexOf(kind))
  if (places.some(place => place < 0)) {
    fail(`${where}: a record kind is not one of 015's`)
  }
  if (places.some((place, at) => at > 0 && place <= places[at - 1])) {
    fail(`${where}: the record kinds are not in 015's order, each once`)
  }
  for (const kind of kinds) {
    ownedKinds.add(kind)
  }
  if (!isDeepStrictEqual(kinds, expectedRecordKinds(row))) {
    fail(`${where}: the record kinds are not the ones 015 gives its check`)
  }
  for (const owned of row.ownedDependencies ?? []) {
    const ids = DEPENDENCY_IDS[owned.recordKind]
    if (ids === undefined || !ids.includes(owned.id)) {
      fail(`${where}: owns ${JSON.stringify(owned)}, which is no dependency record of 015's`)
      continue
    }
    const key = `${owned.recordKind} ${owned.id}`
    if (owners.has(key)) {
      fail(`${where}: owns ${key}, which ${owners.get(key)} owns`)
    }
    owners.set(key, row.checkId)
    if (!kinds.includes(owned.recordKind)) {
      fail(`${where}: owns ${key} but may not own its kind`)
    }
    const singleton = row.subjectKeyStrategy === 'singleton'
    const slotOwner = SLOT_OWNERS[owned.recordKind]
    if (
      slotOwner !== undefined &&
      (row.phaseId !== slotOwner || row.subjectKindRank !== '0' || !singleton)
    ) {
      fail(`${where}: owns ${key}, which the first kind of ${slotOwner} owns`)
    }
    if (
      owned.recordKind === 'semantic-dependency' &&
      (ENTRY_PHASES.includes(row.phaseId) ||
        !singleton ||
        (row.prerequisiteCheckIds ?? []).length === 0)
    ) {
      fail(`${where}: owns ${key}, which a Shared singleton waiting on its group's checks owns`)
    }
  }
}
for (const kind of RECORD_KINDS) {
  if (!ownedKinds.has(kind)) {
    fail(`check registry: no row may own ${kind}`)
  }
}
for (const [recordKind, ids] of Object.entries(DEPENDENCY_IDS)) {
  for (const id of ids) {
    if (!owners.has(`${recordKind} ${id}`)) {
      fail(`check registry: no row owns ${recordKind} ${id}`)
    }
  }
}
// A row's payload strategy derives in one of the two ways and names each code
// the row may report, in their order, with exactly the details members 015's
// code tables give it. A singleton's payloads derive from its key alone
// exactly where its check sees each occurrence on its own; structural
// admission's other keys, whose paths hold `*` and whose failed map keys are
// keyed by their content, always do; and so does a key under a parent whose
// key does.
const keyOnlyKinds = new Set()
const reachedReasons = new Map()
let keyOnlyRows = 0
for (const row of checkRows) {
  const where = `check registry: ${row.checkId}`
  const strategy = row.findingPayloadStrategy
  if (!DERIVATIONS.includes(strategy?.derivation)) {
    fail(`${where}: the payloads derive in no way of the registry's`)
    continue
  }
  const codes = (strategy.details ?? []).map(details => details.code)
  if (!isDeepStrictEqual(codes, row.possibleFindingCodes ?? [])) {
    fail(`${where}: the payload strategy does not name the row's codes in their order`)
  }
  for (const details of strategy.details ?? []) {
    if (!isDeepStrictEqual(details.members, CODE_DETAILS[details.code] ?? [])) {
      fail(`${where}: ${details.code} does not carry the details 015 gives it`)
    }
    const allowed = CODE_REASONS[details.code] ?? []
    const places = (details.reasons ?? []).map(reason => allowed.indexOf(reason))
    if (
      places.some((place, at) => place < 0 || (at > 0 && place <= places[at - 1])) ||
      (places.length === 0) !== !(details.members ?? []).includes('reason')
    ) {
      fail(`${where}: ${details.code} is not reported with 015's reasons for it, in their order`)
    }
    const reached = reachedReasons.get(details.code) ?? new Set()
    for (const reason of details.reasons ?? []) {
      reached.add(reason)
    }
    reachedReasons.set(details.code, reached)
  }
  const keyOnly = strategy.derivation === 'key-only'
  if (keyOnly) {
    keyOnlyKinds.add(row.subjectKind)
    keyOnlyRows += 1
  }
  const singleton = row.subjectKeyStrategy === 'singleton'
  if (singleton && keyOnly !== INDEPENDENT_SINGLETON_CHECKS.includes(row.checkId)) {
    fail(
      `${where}: a singleton derives from its key alone only where it sees each occurrence on its own`
    )
  }
  if (row.phaseId === 'structural-admission' && !singleton && !keyOnly) {
    fail(`${where}: a structural admission key can stand for several values`)
  }
}
// Every reason 015 gives a code is one some row reports the code with.
for (const code of CODES) {
  const reached = reachedReasons.get(code) ?? new Set()
  const allowed = CODE_REASONS[code] ?? []
  if (reached.size !== allowed.length || allowed.some(reason => !reached.has(reason))) {
    fail(`check registry: ${code} is not reported with every reason 015 gives it`)
  }
}
for (const row of checkRows) {
  const parent = row.parentSubject?.subjectKind
  if (keyOnlyKinds.has(parent) && row.findingPayloadStrategy?.derivation !== 'key-only') {
    fail(
      `check registry: ${row.checkId}: its parent's key can stand for several values, and so can its own`
    )
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
// parent, its payload strategy, and the vocabularies of phases, kinds,
// strategies, projections, codes, record kinds, and details members.
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
  'checkRank',
  'prerequisiteCheckIds',
  'possibleFindingCodes',
  'possibleEvidenceRecordKinds',
  'ownedDependencies',
  'findingPayloadStrategy'
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
const prerequisites = property('prerequisiteCheckIds')
if (
  prerequisites?.type !== 'array' ||
  prerequisites.uniqueItems !== true ||
  prerequisites.items?.type !== 'string' ||
  prerequisites.items?.pattern !== CHECK_ID_PATTERN
) {
  fail('check registry schema: the prerequisites are not check IDs, each once')
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
const findingCodes = property('possibleFindingCodes')
if (
  findingCodes?.type !== 'array' ||
  findingCodes.uniqueItems !== true ||
  !isDeepStrictEqual(vocabulary(findingCodes.items), CODES)
) {
  fail("check registry schema: the codes are not 015's forty-eight, each once")
}
const recordKinds = property('possibleEvidenceRecordKinds')
if (
  recordKinds?.type !== 'array' ||
  recordKinds.uniqueItems !== true ||
  !isDeepStrictEqual(vocabulary(recordKinds.items), RECORD_KINDS)
) {
  fail("check registry schema: the record kinds are not 015's twelve, each once")
}
const ownedDependencies = property('ownedDependencies')
const dependency = follow(checkSchema, ownedDependencies?.items)
const variants = (dependency?.oneOf ?? []).map(variant => [
  variant.properties?.recordKind?.const,
  closes(variant, ['recordKind', 'id']) ? vocabulary(variant.properties?.id) : undefined
])
if (
  ownedDependencies?.type !== 'array' ||
  ownedDependencies.uniqueItems !== true ||
  !isDeepStrictEqual(Object.fromEntries(variants), DEPENDENCY_IDS) ||
  variants.length !== Object.keys(DEPENDENCY_IDS).length
) {
  fail("check registry schema: the dependencies are not 015's three kinds of record, each closed")
}
const strategySchema = follow(checkSchema, property('findingPayloadStrategy')?.allOf?.[0])
const detailsSchema = strategySchema?.properties?.details
const codeDetails = follow(checkSchema, detailsSchema?.items)
const membersSchema = codeDetails?.properties?.members
const reasonsSchema = codeDetails?.properties?.reasons
if (
  !closes(strategySchema, ['derivation', 'details']) ||
  !isDeepStrictEqual(vocabulary(strategySchema?.properties?.derivation), DERIVATIONS) ||
  detailsSchema?.type !== 'array' ||
  !closes(codeDetails, ['code', 'members', 'reasons']) ||
  !isDeepStrictEqual(vocabulary(codeDetails?.properties?.code), CODES) ||
  membersSchema?.type !== 'array' ||
  membersSchema.uniqueItems !== true ||
  !isDeepStrictEqual(vocabulary(membersSchema.items), DETAILS_MEMBERS) ||
  reasonsSchema?.type !== 'array' ||
  reasonsSchema.uniqueItems !== true ||
  !isDeepStrictEqual(vocabulary(reasonsSchema.items), REASONS)
) {
  fail(
    "check registry schema: a payload strategy is not closed over 015's codes, details members, and reasons"
  )
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

// The Finding Registry: one row for each of 015's codes, in table order, whose
// checks are exactly the Check Registry rows that list the code, in row order,
// whose subject kinds are those rows' kinds, in the order they first appear,
// and whose reasons are the ones 015's tables give the code, in their order.
// Every code but the non-canonical spelling blocks and is an error; the
// non-canonical spelling is a warning that suggests its canonical replacement,
// and no other code suggests anything.
const NON_CANONICAL = 'project-profile-locale-non-canonical'
const findingRegistry = JSON.parse(findingRegistryBytes.toString('utf8'))
const findingRows = findingRegistry.rows ?? []
if (
  !isDeepStrictEqual(
    findingRows.map(row => row.code),
    CODES
  )
) {
  fail("finding registry: the rows are not one for each of 015's codes, in table order")
}
for (const row of findingRows) {
  const where = `finding registry: ${row.code}`
  const listing = checkRows.filter(check => (check.possibleFindingCodes ?? []).includes(row.code))
  if (
    !isDeepStrictEqual(
      row.allowedCheckIds,
      listing.map(check => check.checkId)
    )
  ) {
    fail(`${where}: the checks are not the rows that list the code, in row order`)
  }
  const kinds = [...new Set(listing.map(check => check.subjectKind))]
  if (!isDeepStrictEqual(row.allowedSubjectKinds, kinds)) {
    fail(`${where}: the subject kinds are not those of its checks, in the order they first appear`)
  }
  if (!isDeepStrictEqual(row.allowedReasons, CODE_REASONS[row.code] ?? [])) {
    fail(`${where}: the reasons are not the ones 015 gives the code, in its order`)
  }
  const warning = row.code === NON_CANONICAL
  if (row.blocking !== !warning || row.severity !== (warning ? 'warning' : 'error')) {
    fail(`${where}: the code does not block and report as 015 assigns it`)
  }
  if (row.suggestionKind !== (warning ? 'canonical-locale-replacement' : 'none')) {
    fail(`${where}: the suggestion kind is not 015's`)
  }
}

/**
 * The classifications under which a literal component counts as non-secret:
 * only Entry Source Evidence has path members, and it precedes the
 * canonicalization data, so none of them is a canonical locale key; only the
 * artifact input has keys.
 *
 * @param component - A component.
 * @returns The classifications, none for fixed or redacted content.
 */
function classes(component) {
  switch (component) {
    case 'source.safe-opaque':
      return ['owner-marked-identity']
    case 'locator.relative-path':
    case 'locator.safe-label':
    case 'locator.safe-uri':
      return ['adapter-classified-locator']
    case 'path.safe-member':
      return ['schema-defined-name', 'identifier-key']
    case 'value.exact-json-pointer':
    case 'value.redacted-logical-path.safe-member':
      return ['schema-defined-name', 'identifier-key', 'canonical-locale-key']
    case 'selector.safe-profile-id':
      return ['declared-profile-id']
    case 'component.safe-key':
      return ['vocabulary-identity', 'admitted-exact-reference']
    default:
      return []
  }
}

/**
 * A fixed role of a component path shape.
 *
 * @param name - The role ID.
 * @param rank - Its rank within its parent path.
 * @returns The role.
 */
const role = (name, rank) => ({ role: name, rank: String(rank) })

/**
 * The component path shapes some retained Evidence of a kind can carry, in
 * Evidence order: every Specification Input slot; the vocabulary's root and
 * members, the canonicalization data's root, and the artifact collection's
 * root and members, each member with an admitted key and then a redacted
 * one; the verification input's `pins`; and every `policyComparable` bound
 * with the `bootstrapOnly` bounds of `rawInput` and `artifactBootstrap`.
 * The `diagnostics` groups share one parent role, ranked where the first of
 * them stands.
 *
 * @param kind - An evidence kind.
 * @returns The shapes, none for a kind that is no resolver input.
 */
function reachablePaths(kind) {
  switch (kind) {
    case 'specification-input':
      return SLOTS.map((slot, rank) => ({
        origin: 'profile-resolution-specification-input',
        roles: [role(slot, rank)]
      }))
    case 'artifact-input': {
      const member = (origin, name) => [
        { origin, roles: [] },
        { origin, roles: [role(name, 0)], key: 'safe-key' },
        { origin, roles: [role(name, 0)], key: 'redacted-key' }
      ]
      return [
        ...member('intent-surface-class-vocabulary', 'vocabulary-member'),
        { origin: 'locale-canonicalization-data', roles: [] },
        ...member('profile-resolution-artifact-collection', 'artifact-member')
      ]
    }
    case 'verification-input':
      return [{ origin: 'resource-limit-policy-verification', roles: [role('pins', 1)] }]
    case 'implementation-capability': {
      const origin = 'implementation-capability'
      const diagnostics = POLICY_GROUPS.findIndex(([path]) => path.startsWith('diagnostics.'))
      const comparable = POLICY_GROUPS.flatMap(([path, bounds], group) => {
        const parent = path.startsWith('diagnostics.')
          ? [
              role('diagnostics', diagnostics),
              role(path.slice('diagnostics.'.length), group - diagnostics)
            ]
          : [role(path, group)]
        return bounds.map(([name], rank) => ({
          origin,
          roles: [role('policyComparable', 0), ...parent, role(name, rank)]
        }))
      })
      const bootstrap = BOOTSTRAP_GROUPS.flatMap(([path, bounds], group) =>
        ['rawInput', 'artifactBootstrap'].includes(path)
          ? bounds.map(([name], rank) => ({
              origin,
              roles: [role('bootstrapOnly', 1), role(path, group), role(name, rank)]
            }))
          : []
      )
      return [...comparable, ...bootstrap]
    }
    default:
      return []
  }
}

/**
 * One evidence kind's disclosure policy as 015 gives it: the members every
 * Evidence of the kind has, each component its view can retain, in view
 * order, disclosed literally unless a projection's token replaces it, and,
 * for a resolver-input kind, the path shapes its Evidence can carry.
 *
 * @param kind - An evidence kind.
 * @returns The policy.
 */
function policyOf(kind) {
  const [required, members] = VIEW_MEMBERS[kind]
  const components = COMPONENTS.filter(
    ([member, component]) =>
      members.includes(member) &&
      (kind === 'artifact-input' ||
        !['component.safe-key', 'component.redacted-key'].includes(component))
  ).map(([, component, projection]) => {
    const policy = {
      component,
      disclosure: projection ? 'redacted-structure' : 'non-secret-literal'
    }
    const when = classes(component)
    if (when.length > 0) {
      policy.nonSecretWhen = when
    }
    if (projection) {
      policy.tokenProjection = projection
    }
    return policy
  })
  const policy = { evidenceKind: kind, required, components }
  const paths = reachablePaths(kind)
  if (paths.length > 0) {
    policy.paths = paths
  }
  return policy
}

// The Finding Registry's disclosure policies: one for each of 015's evidence
// kinds, in its order, as `policyOf` gives them, and no path shape into an
// origin the Safe Origin Kind table does not let the kind use.
const findingPolicies = findingRegistry.policies ?? []
if (!isDeepStrictEqual(findingPolicies, EVIDENCE_KINDS.map(policyOf))) {
  fail(
    "finding registry: the disclosure policies are not one for each of 015's evidence kinds, as 015 discloses them"
  )
}
for (const policy of findingPolicies) {
  for (const shape of policy.paths ?? []) {
    const permitted = SAFE_ORIGIN_KINDS.find(([id]) => id === shape.origin)?.[1] ?? []
    if (!permitted.includes(policy.evidenceKind)) {
      fail(
        `finding registry: ${policy.evidenceKind} pins a path into ${shape.origin}, which it may not use`
      )
    }
  }
}
const pinnedShapes = findingPolicies.reduce(
  (count, policy) => count + (policy.paths?.length ?? 0),
  0
)

// The rows' occurrence Evidence. For each check, in the row's check order,
// the primary kind is one of the seven or, only for the terminal reporting
// code, `none` with no related kind, and the related kinds are some of the
// seven, in the admitted order without repeats. The policies and the origins name exactly
// those kinds, in that order. A resolver-input kind's shapes are ones its
// policy pins, in Evidence order, and its origins exactly theirs; another
// kind has no shape. Every origin is one the Safe Origin Kind table lets the
// kind use, in rank order. Every shape a policy pins is one some row carries,
// or Coverage Decision Evidence: the Coverage Specification input. Of the
// kinds 015 names, a root-value bound before materialization is
// `entry-source`, an over-limit selector `profile-selector`, and a selector
// Finding relates the declarations, `configuration-value`.
const TERMINAL = 'project-profile-reporting-limit-exceeded'
const RESOURCE_LIMIT = 'project-profile-resource-limit-exceeded'
const RESOLVER_INPUTS = EVIDENCE_KINDS.slice(3)
const ORIGIN_IDS = SAFE_ORIGIN_KINDS.map(([id]) => id)
const increasing = (items, rank) =>
  items.every((item, index) => index === 0 || rank(items[index - 1]) < rank(item))
const shapeKey = shape => JSON.stringify(shape)
const pinnedByKind = new Map(
  findingPolicies.map(policy => [policy.evidenceKind, (policy.paths ?? []).map(shapeKey)])
)
const carried = new Set()
let occurrenceChecks = 0
for (const row of findingRows) {
  const where = `finding registry: ${row.code}`
  const primary = row.occurrencePrimaryEvidenceKindByCheck ?? []
  const related = row.occurrenceRelatedEvidenceKindsByCheck ?? []
  occurrenceChecks += primary.length
  if (
    !isDeepStrictEqual(
      primary.map(entry => entry.checkId),
      row.allowedCheckIds
    ) ||
    !isDeepStrictEqual(
      related.map(entry => entry.checkId),
      row.allowedCheckIds
    )
  ) {
    fail(`${where}: the per-check columns do not list its checks in their order`)
  }
  const named = new Set()
  for (const [index, entry] of primary.entries()) {
    const kinds = related[index]?.kinds ?? []
    if (
      ![...EVIDENCE_KINDS, 'none'].includes(entry.kind) ||
      (entry.kind === 'none') !== (row.code === TERMINAL) ||
      (entry.kind === 'none' && kinds.length > 0)
    ) {
      fail(`${where}: ${entry.checkId} has a primary kind 015 does not give it`)
    }
    if (!kinds.every(kind => EVIDENCE_KINDS.includes(kind))) {
      fail(`${where}: ${entry.checkId} has a related kind that is none of the seven`)
    }
    if (!increasing(kinds, kind => EVIDENCE_KINDS.indexOf(kind))) {
      fail(
        `${where}: ${entry.checkId}'s related kinds are not in the admitted order without repeats`
      )
    }
    for (const kind of [entry.kind, ...kinds]) {
      if (kind !== 'none') {
        named.add(kind)
      }
    }
    if (
      row.code === RESOURCE_LIMIT &&
      ((entry.checkId.startsWith('entry.input.') && entry.kind !== 'entry-source') ||
        (entry.checkId.includes('.selector.') && entry.kind !== 'profile-selector'))
    ) {
      fail(`${where}: ${entry.checkId} does not report with the kind 015 names`)
    }
    if (entry.kind === 'profile-selector' && !kinds.includes('configuration-value')) {
      fail(`${where}: ${entry.checkId} does not relate the profile declarations`)
    }
  }
  const kinds = EVIDENCE_KINDS.filter(kind => named.has(kind))
  const policies = row.evidenceDisclosurePolicies ?? []
  const origins = row.allowedSafeOriginKindsByEvidenceKind ?? []
  if (
    !isDeepStrictEqual(
      policies.map(entry => entry.evidenceKind),
      kinds
    ) ||
    !isDeepStrictEqual(
      origins.map(entry => entry.evidenceKind),
      kinds
    )
  ) {
    fail(
      `${where}: the policies and origins do not name exactly the kinds of its checks, in their order`
    )
  }
  for (const entry of policies) {
    const paths = (entry.paths ?? []).map(shapeKey)
    const pins = pinnedByKind.get(entry.evidenceKind) ?? []
    const closed = RESOLVER_INPUTS.includes(entry.evidenceKind)
      ? paths.length > 0 &&
        paths.every(path => pins.includes(path)) &&
        increasing(paths, path => pins.indexOf(path))
      : paths.length === 0
    if (!closed) {
      fail(
        `${where}: the shapes of ${entry.evidenceKind} are not ones its policy pins, in their order`
      )
    }
    for (const path of paths) {
      carried.add(path)
    }
  }
  for (const entry of origins) {
    const shaped = (
      policies.find(policy => policy.evidenceKind === entry.evidenceKind)?.paths ?? []
    )
      .map(shape => shape.origin)
      .filter((origin, index, all) => all.indexOf(origin) === index)
      .sort((left, right) => ORIGIN_IDS.indexOf(left) - ORIGIN_IDS.indexOf(right))
    const permitted = (entry.origins ?? []).every(origin =>
      (SAFE_ORIGIN_KINDS.find(([id]) => id === origin)?.[1] ?? []).includes(entry.evidenceKind)
    )
    if (
      (entry.origins ?? []).length === 0 ||
      !permitted ||
      !increasing(entry.origins, origin => ORIGIN_IDS.indexOf(origin)) ||
      (RESOLVER_INPUTS.includes(entry.evidenceKind) && !isDeepStrictEqual(entry.origins, shaped))
    ) {
      fail(
        `${where}: the origins of ${entry.evidenceKind} are not those 015 lets it use, in rank order`
      )
    }
  }
}
const coverageDecision = shapeKey({
  origin: 'profile-resolution-specification-input',
  roles: [role('coverage', SLOTS.indexOf('coverage'))]
})
for (const policy of findingPolicies) {
  for (const shape of policy.paths ?? []) {
    const key = shapeKey(shape)
    if (!carried.has(key) && key !== coverageDecision) {
      fail(`finding registry: ${policy.evidenceKind} pins a shape no Evidence carries`)
    }
  }
}
if (fileDigest(findingRegistryBytes) !== pinned(findingSource, 'DIGEST')) {
  fail(
    `finding registry: the file digest ${fileDigest(findingRegistryBytes)} is not the pinned ${pinned(findingSource, 'DIGEST')}`
  )
}

// The Finding Registry's schema: Draft 7, closing the container, every
// policy, and every row, and the vocabularies of evidence kinds, members,
// components, disclosures, classifications, projections, origins, keys,
// codes, severities, subject kinds, reasons, and suggestion kinds.
const findingSchema = JSON.parse(findingSchemaBytes.toString('utf8'))
const findingVocabulary = node => {
  const definition = follow(findingSchema, node)
  return definition?.type === 'string' ? definition.enum : undefined
}
if (findingSchema.$schema !== DRAFT_7) {
  fail(`finding registry schema: $schema ${findingSchema.$schema}`)
}
if (!closes(findingSchema, ['policies', 'rows'])) {
  fail('finding registry schema: the container is not closed')
}
const findingPoliciesSchema = findingSchema.properties?.policies
if (findingPoliciesSchema?.type !== 'array' || findingPoliciesSchema.minItems !== 1) {
  fail('finding registry schema: the policies are not a non-empty array')
}
const policySchema = follow(findingSchema, findingPoliciesSchema?.items)
const componentSchema = follow(findingSchema, policySchema?.properties?.components?.items)
const shapeSchema = follow(findingSchema, policySchema?.properties?.paths?.items)
const roleSchema = follow(findingSchema, shapeSchema?.properties?.roles?.items)
if (
  !closes(policySchema, ['evidenceKind', 'required', 'components'], ['paths']) ||
  !closes(componentSchema, ['component', 'disclosure'], ['nonSecretWhen', 'tokenProjection']) ||
  !closes(shapeSchema, ['origin', 'roles'], ['key']) ||
  !closes(roleSchema, ['role', 'rank'])
) {
  fail('finding registry schema: a policy is not closed')
}
const constants = node =>
  follow(findingSchema, node)?.oneOf?.map(variant =>
    variant.type === 'string' ? variant.const : undefined
  )
const policyProperty = name => policySchema?.properties?.[name]
const componentProperty = name => componentSchema?.properties?.[name]
const shapeProperty = name => shapeSchema?.properties?.[name]
const requiredMembers = policyProperty('required')
const policyComponents = policyProperty('components')
const policyPaths = policyProperty('paths')
const nonSecretWhen = componentProperty('nonSecretWhen')
const shapeRoles = shapeProperty('roles')
const roleName = roleSchema?.properties?.role
const roleRank = roleSchema?.properties?.rank
if (
  !isDeepStrictEqual(findingVocabulary(policyProperty('evidenceKind')), EVIDENCE_KINDS) ||
  requiredMembers?.type !== 'array' ||
  requiredMembers.minItems !== 1 ||
  requiredMembers.uniqueItems !== true ||
  !isDeepStrictEqual(findingVocabulary(requiredMembers.items), MEMBERS) ||
  policyComponents?.type !== 'array' ||
  policyComponents.minItems !== 1 ||
  policyPaths?.type !== 'array' ||
  policyPaths.minItems !== 1 ||
  !isDeepStrictEqual(
    findingVocabulary(componentProperty('component')),
    COMPONENTS.map(([, component]) => component)
  ) ||
  !isDeepStrictEqual(findingVocabulary(componentProperty('disclosure')), [
    'non-secret-literal',
    'redacted-structure'
  ]) ||
  nonSecretWhen?.type !== 'array' ||
  nonSecretWhen.minItems !== 1 ||
  nonSecretWhen.uniqueItems !== true ||
  !isDeepStrictEqual(constants(nonSecretWhen.items), NON_SECRET_CLASSES) ||
  !isDeepStrictEqual(
    findingVocabulary(componentProperty('tokenProjection')?.allOf?.[0]),
    TOKEN_KINDS
  ) ||
  !isDeepStrictEqual(
    findingVocabulary(shapeProperty('origin')),
    SAFE_ORIGIN_KINDS.map(([id]) => id)
  ) ||
  shapeRoles?.type !== 'array' ||
  !isDeepStrictEqual(findingVocabulary(shapeProperty('key')), ['safe-key', 'redacted-key']) ||
  roleName?.type !== 'string' ||
  roleName.minLength !== 1 ||
  roleRank?.type !== 'string' ||
  roleRank.pattern !== RANK_PATTERN
) {
  fail("finding registry schema: a policy's members are not closed over 015's vocabularies")
}
const findingRowsSchema = findingSchema.properties?.rows
if (findingRowsSchema?.type !== 'array' || findingRowsSchema.minItems !== 1) {
  fail('finding registry schema: the rows are not a non-empty array')
}
const findingRow = follow(findingSchema, findingRowsSchema?.items)
if (
  !closes(findingRow, [
    'code',
    'allowedCheckIds',
    'severity',
    'blocking',
    'allowedSubjectKinds',
    'allowedReasons',
    'occurrencePrimaryEvidenceKindByCheck',
    'occurrenceRelatedEvidenceKindsByCheck',
    'evidenceDisclosurePolicies',
    'allowedSafeOriginKindsByEvidenceKind',
    'suggestionKind'
  ])
) {
  fail('finding registry schema: a row is not closed')
}
const findingProperty = name => findingRow?.properties?.[name]
const allowedChecks = findingProperty('allowedCheckIds')
const allowedKinds = findingProperty('allowedSubjectKinds')
const allowedReasons = findingProperty('allowedReasons')
if (
  !isDeepStrictEqual(findingVocabulary(findingProperty('code')), CODES) ||
  allowedChecks?.type !== 'array' ||
  allowedChecks.minItems !== 1 ||
  allowedChecks.uniqueItems !== true ||
  allowedChecks.items?.type !== 'string' ||
  allowedChecks.items?.pattern !== CHECK_ID_PATTERN ||
  !isDeepStrictEqual(findingVocabulary(findingProperty('severity')), ['error', 'warning']) ||
  findingProperty('blocking')?.type !== 'boolean' ||
  allowedKinds?.type !== 'array' ||
  allowedKinds.minItems !== 1 ||
  allowedKinds.uniqueItems !== true ||
  !isDeepStrictEqual(findingVocabulary(allowedKinds.items), kindIds) ||
  allowedReasons?.type !== 'array' ||
  allowedReasons.uniqueItems !== true ||
  !isDeepStrictEqual(findingVocabulary(allowedReasons.items), REASONS) ||
  !isDeepStrictEqual(findingVocabulary(findingProperty('suggestionKind')), [
    'none',
    'canonical-locale-replacement'
  ])
) {
  fail("finding registry schema: a row's members are not closed over 015's vocabularies")
}
// The occurrence Evidence columns: a per-check entry closes over its check
// and its kind or kinds, a policy entry over its kind and shapes, and an
// origin entry over its kind and origins.
const primaryColumn = findingProperty('occurrencePrimaryEvidenceKindByCheck')
const relatedColumn = findingProperty('occurrenceRelatedEvidenceKindsByCheck')
const policyColumn = findingProperty('evidenceDisclosurePolicies')
const originColumn = findingProperty('allowedSafeOriginKindsByEvidenceKind')
const primaryEntry = follow(findingSchema, primaryColumn?.items)
const relatedEntry = follow(findingSchema, relatedColumn?.items)
const policyEntry = follow(findingSchema, policyColumn?.items)
const originEntry = follow(findingSchema, originColumn?.items)
if (
  !closes(primaryEntry, ['checkId', 'kind']) ||
  !closes(relatedEntry, ['checkId', 'kinds']) ||
  !closes(policyEntry, ['evidenceKind'], ['paths']) ||
  !closes(originEntry, ['evidenceKind', 'origins'])
) {
  fail('finding registry schema: an occurrence Evidence entry is not closed')
}
const relatedKinds = relatedEntry?.properties?.kinds
const entryPaths = policyEntry?.properties?.paths
const entryOrigins = originEntry?.properties?.origins
if (
  primaryColumn?.type !== 'array' ||
  primaryColumn.minItems !== 1 ||
  relatedColumn?.type !== 'array' ||
  relatedColumn.minItems !== 1 ||
  policyColumn?.type !== 'array' ||
  originColumn?.type !== 'array' ||
  primaryEntry?.properties?.checkId?.pattern !== CHECK_ID_PATTERN ||
  relatedEntry?.properties?.checkId?.pattern !== CHECK_ID_PATTERN ||
  !isDeepStrictEqual(findingVocabulary(primaryEntry?.properties?.kind), [
    ...EVIDENCE_KINDS,
    'none'
  ]) ||
  relatedKinds?.type !== 'array' ||
  relatedKinds.uniqueItems !== true ||
  !isDeepStrictEqual(findingVocabulary(relatedKinds.items), EVIDENCE_KINDS) ||
  !isDeepStrictEqual(findingVocabulary(policyEntry?.properties?.evidenceKind), EVIDENCE_KINDS) ||
  entryPaths?.type !== 'array' ||
  entryPaths.minItems !== 1 ||
  follow(findingSchema, entryPaths.items) !== shapeSchema ||
  !isDeepStrictEqual(findingVocabulary(originEntry?.properties?.evidenceKind), EVIDENCE_KINDS) ||
  entryOrigins?.type !== 'array' ||
  entryOrigins.minItems !== 1 ||
  entryOrigins.uniqueItems !== true ||
  !isDeepStrictEqual(findingVocabulary(entryOrigins.items), ORIGIN_IDS)
) {
  fail(
    "finding registry schema: the occurrence Evidence columns are not closed over 015's vocabularies"
  )
}
if (fileDigest(findingSchemaBytes) !== pinned(findingSource, 'SCHEMA_DIGEST')) {
  fail(
    `finding registry schema: the file digest ${fileDigest(findingSchemaBytes)} is not the pinned ${pinned(findingSource, 'SCHEMA_DIGEST')}`
  )
}

// The package's container: under each of the six roles, in role order, the
// identity revision "0" gives the role's member, that revision, and the file
// digest of the member's committed bytes.
const memberBytes = [
  schemaBytes,
  checkSchemaBytes,
  findingSchemaBytes,
  registryBytes,
  checkRegistryBytes,
  findingRegistryBytes
]
const expectedContainer = {
  members: Object.fromEntries(
    ROLES.map((role, index) => [
      role,
      {
        identity: `intlify-project-profile-${role}`,
        revision: '0',
        digest: fileDigest(memberBytes[index])
      }
    ])
  )
}
const packageContainer = JSON.parse(packageBytes.toString('utf8'))
if (JSON.stringify(packageContainer) !== JSON.stringify(expectedContainer)) {
  fail("registry package: the container is not each role's member tuple, in role order")
}
if (fileDigest(packageBytes) !== pinned(packageSource, 'DIGEST')) {
  fail(
    `registry package: the file digest ${fileDigest(packageBytes)} is not the pinned ${pinned(packageSource, 'DIGEST')}`
  )
}

if (failures.length > 0) {
  for (const message of failures) {
    console.error(message)
  }
  process.exit(1)
}

console.log(
  `registries agree: ${expected.rows.length} safe origin kinds, ${EVIDENCE_KINDS.length} evidence kinds, ${checkRows.length} checks of ${kinds.size} subject kinds with ${prerequisiteCount} prerequisites, reporting ${reportedCodes.size} codes with ${new Set([...reachedReasons.values()].flatMap(reached => [...reached])).size} reasons, owning ${owners.size} dependency records, and deriving ${keyOnlyRows} rows' payloads from their keys alone; ${findingRows.length} Finding Registry rows agree with them, beside ${findingPolicies.length} disclosure policies pinning ${pinnedShapes} path shapes, and give ${occurrenceChecks} checks' occurrence Evidence, carrying every pinned shape; the package lists its ${ROLES.length} members`
)
