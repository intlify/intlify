/**
 * Check the committed Case Schema against this implementation.
 *
 * The Rust crate writes the conformance suite's Case Schema, the closed form
 * of its five manifest kinds, and pins its file digest. This script shares no
 * code with it. It reads the schema strictly and checks that it admits
 * exactly the five kinds, each a closed object of its members, under the
 * vocabularies of its own copy of design 015's tables: the boundary kinds and
 * their failure reasons, the slots of the two admission steps, the
 * verification and handoff owners, the unpaired-entry reasons, the relation
 * kinds, the conditional obligations, and the artifact kinds. It takes the
 * construction failure reasons and bounds from the committed
 * construction-admission body, the Safe Origin Kinds from the committed Safe
 * Origin Kind Registry, and the semantic groups and Snapshot marker reasons
 * from the committed Profile Specification, and reads the pinned digest out
 * of the crate's source. It also checks the schema's top-level
 * `failurePrecedence`: each boundary kind's rows take the positions of the
 * general order without going back and list the reasons of its row in that
 * row's order, and the construction rows are referenced by the Root Package
 * member and the file digest the committed Suite Index declares for it, and
 * the input fixture of each boundary kind: a closed object of exactly its
 * members, the capability value beside the input of a boundary that reads
 * capability bounds. It does not read the design document.
 */

import { createHash } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { isDeepStrictEqual } from 'node:util'

import { repeatsMember } from './strict-json.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const crate = resolve(here, '../../../crates/intlify_config')
const root = resolve(crate, 'conformance')

const DRAFT_7 = 'http://json-schema.org/draft-07/schema#'
// The positions of the general order in which every boundary applies its
// checks, in that order.
const CHECKS = [
  'missing-input',
  'unsafe-host-inspection',
  'container-form',
  'raw-bounds',
  'strict-syntax',
  'count-bounds',
  'closed-shape',
  'repeated-member',
  'unknown-member',
  'reserved-members',
  'value-rules',
  'assertion-comparison'
]
const SCHEMA_PATH = 'schemas/case-v0.schema.json'

/** The five manifest kinds and their definitions, in the index's order. */
const KINDS = [
  ['resolver-construction', 'resolverConstructionCase'],
  ['resolver-input-admission', 'resolverInputAdmissionCase'],
  ['resolver', 'resolverCase'],
  ['handoff-vector', 'handoffVector'],
  ['conditional-obligation', 'conditionalObligation']
]

/** Design 015's boundary kinds, each with its failure reasons in row order. */
const BOUNDARIES = [
  [
    'profile-resolution-specification-input',
    ['missing-input', 'over-bound-member', 'malformed-input', 'duplicate-member', 'unknown-member']
  ],
  [
    'intent-surface-class-vocabulary-artifact',
    [
      'missing-input',
      'over-bound-envelope',
      'malformed-envelope',
      'duplicate-member',
      'unknown-member'
    ]
  ],
  [
    'locale-canonicalization-data-artifact',
    [
      'missing-input',
      'over-bound-envelope',
      'malformed-envelope',
      'duplicate-member',
      'unknown-member'
    ]
  ],
  [
    'submitted-profile-resolution-artifact-collection',
    [
      'missing-input',
      'unsafe-host-inspection',
      'malformed-container',
      'formal-envelope-over-bound',
      'unrepresentable-member'
    ]
  ],
  [
    'resource-limit-policy-verification-input',
    [
      'missing-input',
      'over-bound-input',
      'malformed-input',
      'duplicate-member',
      'unknown-member',
      'secret-bearing-input',
      'ambient-authority',
      'verification-identity-mismatch',
      'verification-revision-mismatch',
      'verification-digest-mismatch'
    ]
  ],
  [
    'safe-origin-kind',
    [
      'missing-safe-origin-kind',
      'unknown-safe-origin-kind',
      'non-canonical-safe-origin-kind',
      'evidence-kind-incompatible'
    ]
  ],
  [
    'external-profile-selector-binding',
    ['unsafe-host-inspection', 'unrepresentable-selector', 'malformed-normalized-selector']
  ],
  [
    'file-source-identity-revision',
    [
      'missing-source-identity',
      'missing-source-revision',
      'over-bound-source-identity',
      'over-bound-source-revision',
      'unstable-source-identity',
      'unbound-source-revision',
      'source-snapshot-mismatch'
    ]
  ],
  [
    'programmatic-entry-identity-revision',
    [
      'missing-entry-identity',
      'missing-entry-revision',
      'over-bound-entry-identity',
      'over-bound-entry-revision',
      'unstable-entry-identity',
      'unbound-entry-revision',
      'snapshot-identity-mismatch'
    ]
  ],
  [
    'programmatic-entry-snapshot',
    [
      'missing-snapshot',
      'unsafe-host-inspection',
      'snapshot-envelope-over-bound',
      'malformed-snapshot',
      'snapshot-content-digest-mismatch'
    ]
  ],
  [
    'programmatic-call-site-evidence-input',
    [
      'unsafe-host-inspection',
      'over-bound-input',
      'malformed-input',
      'invalid-utf8-source',
      'duplicate-mapping',
      'missing-source-identity',
      'missing-source-revision',
      'unstable-source-identity',
      'unbound-source-revision',
      'source-snapshot-mismatch'
    ]
  ],
  [
    'implementation-capability',
    [
      'missing-capability',
      'malformed-capability',
      'capability-specification-identity-mismatch',
      'capability-specification-revision-mismatch',
      'incomplete-capability',
      'non-canonical-bound',
      'non-positive-bound',
      'unrepresentable-bound',
      'below-bootstrap-minimum'
    ]
  ],
  [
    'resolver-construction-identity-reference',
    [
      'missing-construction-identity',
      'malformed-construction-identity',
      'construction-identity-mismatch'
    ]
  ]
]

/** The two admission steps' slots, each with the boundary kinds of its value. */
const COMMON_SLOTS = [
  ['profile-resolution-specification-input', ['profile-resolution-specification-input']],
  ['intent-surface-class-vocabulary', ['intent-surface-class-vocabulary-artifact']],
  ['locale-canonicalization-data', ['locale-canonicalization-data-artifact']],
  ['profile-resolution-artifact-collection', ['submitted-profile-resolution-artifact-collection']],
  ['resource-limit-policy-verification', ['resource-limit-policy-verification-input']],
  ['implementation-capability', ['implementation-capability']]
]
const FILE_ENTRY_SLOTS = [
  ['configuration-file', ['file-source-identity-revision']],
  ['profile-selector-cli', ['external-profile-selector-binding']]
]
const PROGRAMMATIC_ENTRY_SLOTS = [
  [
    'configuration-programmatic',
    [
      'programmatic-entry-identity-revision',
      'programmatic-entry-snapshot',
      'programmatic-call-site-evidence-input'
    ]
  ],
  ['profile-selector-programmatic', ['external-profile-selector-binding']]
]

/** Design 015's verification owners, and those of a Handoff Vector. */
const OWNERS = [
  'intlify_config',
  '016',
  '019',
  '020',
  '021',
  '022',
  '023',
  '024',
  '025',
  '026',
  '027',
  '029',
  '030',
  'product-integration'
]
const HANDOFF_OWNERS = ['016', '019', '020', '021', '022', '023', '024', '025', '026', '027', '030']

/** The closed reasons of an unpaired entry, by the side a case runs. */
const FILE_ONLY = [
  'raw-json-syntax',
  'duplicate-object-member',
  'malformed-encoding',
  'exact-token-span'
]
const PROGRAMMATIC_ONLY = [
  'programmatic-source-label',
  'programmatic-call-site',
  'programmatic-entry-snapshot',
  'safe-host-inspection'
]

/** The kinds of a relation between two Resolver Cases. */
const RELATION_KINDS = [
  'profile-equal',
  'profile-changed',
  'invocation-stale',
  'invocation-not-stale',
  'materialized-input-equal',
  'materialized-input-changed'
]

/** The conditional obligations: ID, activation, reason, and owner. */
const OBLIGATIONS = [
  [
    '015-XV-001',
    'second-configuration-schema-revision',
    'no-second-admitted-configuration-schema-revision',
    'intlify_config'
  ],
  [
    '015-XV-002',
    'extension-admitting-canonicalization-specification-revision',
    'no-extension-admitting-canonicalization-specification-revision',
    'intlify_config'
  ],
  [
    '015-AD-001',
    'trusted-local-editor-navigation-adapter',
    'no-trusted-local-editor-navigation-adapter',
    'product-integration'
  ],
  ['015-AD-002', 'compatibility-conversion-adapter', 'no-compatibility-conversion-adapter', '029'],
  [
    '015-AD-003',
    'baked-canonicalization-provider',
    'no-baked-canonicalization-provider',
    'intlify_config'
  ],
  ['015-AD-004', 'cli-adapter', 'no-cli-adapter', 'product-integration'],
  [
    '015-HV-001',
    'negotiation-adapter-resource-section',
    'no-negotiation-adapter-resource-section',
    '027'
  ],
  ['015-HV-002', 'expanded-fallback-probes', 'no-expanded-fallback-probes', '020']
]

/** The kinds of artifact an exact reference names. */
const ARTIFACT_KINDS = [
  'approval-policy',
  'glossary-set',
  'provider-routing-policy',
  'resource-limit-policy',
  'selection-policy',
  'source-admission-policy',
  'target-profile',
  'trust-policy'
]

const failures = []

/**
 * Record a disagreement.
 *
 * @param message - What disagrees.
 */
function fail(message) {
  failures.push(message)
}

/**
 * Read a committed JSON artifact strictly.
 *
 * @param path - Its path below the suite root.
 * @returns Its bytes and value.
 */
function read(path) {
  const bytes = readFileSync(resolve(root, path))
  const text = bytes.toString('utf8')
  const value = JSON.parse(text)
  if (repeatsMember(text)) {
    fail(`${path}: repeats a member`)
  }
  return { bytes, text, value }
}

/**
 * Whether a schema node is a closed object of exactly these members: the
 * required ones, then the optional ones.
 *
 * @param node - The node.
 * @param names - The required members, in order.
 * @param optional - The optional members, in order.
 * @returns Whether it is.
 */
function closedOf(node, names, optional = []) {
  return (
    node?.type === 'object' &&
    node.additionalProperties === false &&
    isDeepStrictEqual(node.required, names) &&
    isDeepStrictEqual(Object.keys(node.properties ?? {}), [...names, ...optional])
  )
}

/**
 * The strings a schema node's `enum` lists, or its `const` as one string.
 *
 * @param node - The node.
 * @returns The strings.
 */
function strings(node) {
  if (typeof node?.const === 'string') {
    return [node.const]
  }
  return node?.enum ?? node?.allOf?.[0]?.enum ?? []
}

const { bytes, text, value: schema } = read(SCHEMA_PATH)
const admission = read('construction/construction-admission-v0.json').value
const origins = read('registries/safe-origin-kind-v0.json').value.rows.map(row => row.id)
const profile = read('authorities/project-profile-specification-v0.json').value
const capability = read('authorities/resolver-capability-v0.json').value

// The bytes and the pin.
if (bytes.includes(0x0d) || `${JSON.stringify(schema, null, 2)}\n` !== text) {
  fail(`${SCHEMA_PATH}: not the bytes two-space indentation writes`)
}
const digest = `sha256:${createHash('sha256').update(bytes).digest('hex')}`
const source = readFileSync(resolve(crate, 'src/conformance/case.rs'), 'utf8')
if (source.match(/pub\(crate\) const DIGEST: &str =\s*"(sha256:[\da-f]{64})";/)?.[1] !== digest) {
  fail('schema: its file digest is not DIGEST in src/conformance/case.rs')
}

// The schema adds no default that 015 does not name (D15:3241): no schema in
// it has a `default`. A member's name or a constant may spell `default`, and
// neither is a schema.
/**
 * Count the schemas that have a `default`.
 *
 * @param node - A schema, or a list of schemas.
 * @returns How many of them, and of the schemas in them, have one.
 */
function defaults(node) {
  if (Array.isArray(node)) {
    return node.reduce((sum, item) => sum + defaults(item), 0)
  }
  if (node === null || typeof node !== 'object') {
    return 0
  }
  return Object.entries(node).reduce((sum, [name, value]) => {
    if (name === 'default') {
      return sum + 1
    }
    if (name === 'const' || name === 'enum') {
      return sum
    }
    if (name === 'properties' || name === 'definitions') {
      return sum + defaults(Object.values(value))
    }
    return sum + defaults(value)
  }, 0)
}
if (defaults(schema) !== 0) {
  fail('schema: a schema in it has a default')
}

// The root admits exactly the five kinds.
const definitions = schema.definitions ?? {}
if (
  !isDeepStrictEqual(Object.keys(schema), [
    '$schema',
    'oneOf',
    'definitions',
    'failurePrecedence'
  ]) ||
  schema.$schema !== DRAFT_7 ||
  !isDeepStrictEqual(
    schema.oneOf,
    KINDS.map(([, name]) => ({ $ref: `#/definitions/${name}` }))
  )
) {
  fail('schema: its root does not admit exactly the five manifest kinds')
}
for (const [kind, name] of KINDS) {
  const node = definitions[name]
  const rows = node?.properties === undefined ? (node?.oneOf ?? []) : [node]
  for (const row of rows) {
    if (
      row?.properties?.manifestKind?.const !== kind ||
      row.properties.suiteRevision?.const !== '0'
    ) {
      fail(`schema: ${name} is not a revision "0" ${kind} manifest`)
    }
  }
}

// A construction case: success, a closed reason, or the input limit with its
// bound.
const construction = definitions.resolverConstructionCase
const [success, failure, limit] = construction?.properties?.expected?.oneOf ?? []
const reasons = [...new Set(admission.failurePrecedence.map(row => row.reason))]
if (
  !closedOf(construction, [
    'suiteRevision',
    'manifestKind',
    'id',
    'constructionInputFixture',
    'expected',
    'traceability'
  ]) ||
  !closedOf(success, ['constructed', 'resolverConstructionIdentity', 'resolverInvoked']) ||
  !closedOf(failure, [
    'constructed',
    'resolverConstructionIdentity',
    'failureReason',
    'resolverInvoked'
  ]) ||
  !closedOf(limit, [
    'constructed',
    'resolverConstructionIdentity',
    'failureReason',
    'failureDetails',
    'resolverInvoked'
  ])
) {
  fail('schema: a construction case is not closed to its three expectations')
}
if (!isDeepStrictEqual(new Set(strings(failure?.properties?.failureReason)), new Set(reasons))) {
  fail("schema: the construction reasons are not the construction-admission body's")
}
if (
  !isDeepStrictEqual(
    strings(limit?.properties?.failureDetails?.properties?.boundId),
    admission.bounds.map(bound => bound.id)
  )
) {
  fail("schema: the construction bounds are not the construction-admission body's")
}

// An admission case: each boundary kind with its own reasons, and the two
// steps with failures of their own slots.
const admissionCase = definitions.resolverInputAdmissionCase
const branches = admissionCase?.oneOf ?? []
const branch = kind => branches.find(row => row?.properties?.boundaryKind?.const === kind)
const sets = ['common-invocation-input-set', 'entry-admission-input-set']
if (
  !isDeepStrictEqual(strings(admissionCase?.properties?.boundaryKind), [
    ...BOUNDARIES.map(([kind]) => kind),
    ...sets
  ]) ||
  branches.length !== BOUNDARIES.length + sets.length
) {
  fail('schema: the admission case does not close its boundary kinds')
}
for (const [kind, ownReasons] of BOUNDARIES) {
  const refused = branch(kind)?.properties?.expected?.oneOf?.[1]
  if (!isDeepStrictEqual(strings(refused?.properties?.failureReason), ownReasons)) {
    fail(`schema: ${String(kind)} is not refused with exactly its row's reasons`)
  }
}
const own = Object.fromEntries(BOUNDARIES)

// The failure precedence: every boundary kind's rows and the construction
// reference, and nothing for the two admission steps' kinds. A kind's rows
// take the positions of the general order without going back, except that
// the capability compares its specification before its value rules, and list
// its row's reasons in the order of the first position each occupies.
const precedence = schema.failurePrecedence ?? {}
const pinnedRoot = read('suite-index-v0.json').value.resolverConstructionRootPackage
const pinnedBody = pinnedRoot?.members?.['construction-admission-specification']
if (
  !isDeepStrictEqual(
    new Set(Object.keys(precedence)),
    new Set(['resolver-construction', ...BOUNDARIES.map(([kind]) => kind)])
  )
) {
  fail('schema: failurePrecedence does not hold exactly the boundary kinds and the construction')
}
if (
  !isDeepStrictEqual(precedence['resolver-construction'], {
    rootPackageMember: 'construction-admission-specification',
    digest: pinnedBody?.digest
  }) ||
  `sha256:${createHash('sha256')
    .update(read(pinnedBody?.path ?? '').bytes)
    .digest('hex')}` !== pinnedBody?.digest
) {
  fail('schema: the construction precedence is not the body the Suite Index pins')
}
let precedenceRows = 0
for (const [kind, reasons] of BOUNDARIES) {
  const rows = precedence[kind]
  const order =
    kind === 'implementation-capability' ? [...CHECKS.slice(0, 10), CHECKS[11], CHECKS[10]] : CHECKS
  const positions = Array.isArray(rows) ? rows.map(row => order.indexOf(row?.check)) : []
  if (
    !Array.isArray(rows) ||
    !rows.every(
      row =>
        isDeepStrictEqual(Object.keys(row ?? {}), ['check', 'reason']) &&
        typeof row.reason === 'string'
    ) ||
    positions.some((position, at) => position < 0 || (at > 0 && position < positions[at - 1])) ||
    !isDeepStrictEqual([...new Set(rows.map(row => row.reason))], reasons)
  ) {
    fail(`schema: ${String(kind)}'s failurePrecedence is not its row in the general order`)
  }
  precedenceRows += Array.isArray(rows) ? rows.length : 0
}
const lists = []
const commonList = branch('common-invocation-input-set')?.properties?.expected?.oneOf?.[1]
lists.push(['common-invocation-input-set', COMMON_SLOTS, commonList?.properties?.failures])
const entryLists = branch('entry-admission-input-set')?.properties?.expected?.oneOf?.[1]?.properties
  ?.failures?.anyOf
if (!Array.isArray(entryLists) || entryLists.length !== 2) {
  fail('schema: entry-admission-input-set does not list failures of one entry at a time')
}
lists.push(['entry-admission-input-set', FILE_ENTRY_SLOTS, entryLists?.[0]])
lists.push(['entry-admission-input-set', PROGRAMMATIC_ENTRY_SLOTS, entryLists?.[1]])
for (const [set, slots, list] of lists) {
  const items = list?.items?.oneOf
  const pairs = new Set()
  for (const item of items ?? []) {
    for (const slot of strings(item?.properties?.slot)) {
      const kind = item.properties.boundaryKind?.const
      const itemReasons = strings(item.properties.failureReason)
      if (!isDeepStrictEqual(itemReasons, own[kind])) {
        fail(`schema: ${String(set)} lists ${String(kind)} with other reasons`)
      }
      pairs.add(`${slot} ${kind}`)
    }
  }
  const expected = new Set(
    slots.flatMap(([slot, kinds]) => [
      `${slot} safe-origin-kind`,
      ...kinds.map(kind => `${slot} ${kind}`)
    ])
  )
  if (!isDeepStrictEqual(pairs, expected)) {
    fail(`schema: ${String(set)} does not list failures of exactly its own slots`)
  }
}
const admittedOrigin = branch('safe-origin-kind')?.properties?.expected?.oneOf?.[0]?.properties
if (
  !isDeepStrictEqual(strings(admittedOrigin?.normalizedInput?.properties?.safeOriginKind), origins)
) {
  fail("schema: an admitted Safe Origin Kind is not one of the registry's")
}

// A Resolver Case: paired entries, or one unpaired with a reason of the side
// the case runs; and its relations.
const resolverCase = definitions.resolverCase
const entries = resolverCase?.properties?.entries?.oneOf ?? []
const unpaired = node => strings(node?.properties?.unpaired?.properties?.reason)
if (
  entries.length !== 3 ||
  !isDeepStrictEqual(unpaired(entries[1]?.properties?.programmatic), FILE_ONLY) ||
  !isDeepStrictEqual(unpaired(entries[2]?.properties?.file), PROGRAMMATIC_ONLY)
) {
  fail('schema: a Resolver Case does not close its unpaired entries')
}
const relations = resolverCase?.properties?.relations?.items?.oneOf ?? []
const groups = profile.semanticGroups
if (
  relations.length !== 2 ||
  relations[0]?.properties?.role?.const !== 'equivalence' ||
  relations[1]?.properties?.role?.const !== 'mutation' ||
  !isDeepStrictEqual(
    strings(relations[0]?.properties?.kind),
    RELATION_KINDS.filter(kind => kind !== 'profile-changed')
  ) ||
  !isDeepStrictEqual(strings(relations[1]?.properties?.kind), RELATION_KINDS) ||
  !isDeepStrictEqual(
    strings(relations[1]?.properties?.class),
    groups.filter(group => group !== 'profileSpecification')
  )
) {
  fail('schema: a relation is not closed to its kinds and mutable groups')
}

// A Handoff Vector and its owners, and the verification owners.
const vector = definitions.handoffVector
if (
  !isDeepStrictEqual(strings(vector?.properties?.verificationOwner), HANDOFF_OWNERS) ||
  !isDeepStrictEqual(strings(vector?.properties?.producerProfileFacts?.propertyNames), groups) ||
  !isDeepStrictEqual(strings(definitions.exactReference?.properties?.kind), ARTIFACT_KINDS)
) {
  fail('schema: a Handoff Vector is not closed to its owners, groups, and artifact kinds')
}
if (!isDeepStrictEqual(strings(definitions.verificationOwner), OWNERS)) {
  fail("schema: the verification owners are not design 015's")
}

// A conditional obligation is one row of the table, inactive with its reason
// or active without it.
const rows = definitions.conditionalObligation?.oneOf ?? []
const states = rows.map(row => [
  row?.properties?.id?.const,
  row?.properties?.activation?.const,
  row?.properties?.activationState?.const,
  row?.properties?.notApplicableReason?.const ?? null,
  row?.properties?.verificationOwner?.const
])
const table = OBLIGATIONS.flatMap(([id, activation, reason, owner]) => [
  [id, activation, 'inactive', reason, owner],
  [id, activation, 'active', null, owner]
])
if (!isDeepStrictEqual(states, table)) {
  fail("schema: the conditional obligations are not design 015's eight rows")
}

// An admitted Snapshot's markers.
const marker = definitions.snapshotNode?.oneOf?.[6]?.properties?.marker?.oneOf
if (
  !isDeepStrictEqual(
    strings(marker?.[0]?.properties?.reason),
    profile.snapshot.markerReasons.filter(reason => reason !== 'bootstrap-limit-exceeded')
  ) ||
  marker?.[1]?.properties?.reason?.const !== 'bootstrap-limit-exceeded'
) {
  fail("schema: the Snapshot markers are not the Profile Specification's")
}
const snapshotBounds = capability.bootstrapOnly
  .map(bound => bound.path)
  .filter(path => path.startsWith('snapshot.'))
  .map(path => `bootstrapOnly.${String(path)}`)
if (!isDeepStrictEqual(strings(marker?.[1]?.properties?.boundPath), snapshotBounds)) {
  fail(
    "schema: a terminal marker's bound is not one of the Capability Specification's Snapshot bounds"
  )
}

// A collection member is any value without numbers.
const numberFree = { $ref: '#/definitions/numberFree' }
const collectionInput = branch('submitted-profile-resolution-artifact-collection')?.properties
  ?.expected?.oneOf?.[0]?.properties?.normalizedInput
if (
  !isDeepStrictEqual(definitions.numberFree, {
    anyOf: [
      { type: ['null', 'boolean', 'string'] },
      { type: 'array', items: numberFree },
      { type: 'object', additionalProperties: numberFree }
    ]
  }) ||
  !closedOf(collectionInput, ['members']) ||
  !closedOf(collectionInput.properties.members?.items, ['json']) ||
  !isDeepStrictEqual(collectionInput.properties.members.items.properties.json, numberFree)
) {
  fail('schema: a collection member is not a value without numbers')
}

// Every manifest form, and each of its alternatives, is a closed object with
// exactly its members.
const HEADING = ['suiteRevision', 'manifestKind', 'id']
const PAIRED = ['entryAdmissionInputFixture', 'expectedOutcomeFixture']
const side = node =>
  closedOf(node, PAIRED) ||
  (closedOf(node, ['unpaired']) &&
    closedOf(node.properties.unpaired, ['reason', 'verificationOwner']))
// The construction input fixture: each component a closed `{ tuple, bytes }`,
// the containers with their members, the Registry Package's closed to the
// roles of the committed container, and bytes hexadecimal or a reference.
const fixtureForm = definitions.constructionInputFixture
const component = name => fixtureForm?.properties?.[name]
const roles = Object.keys(read('registries/package-v0.json').value.members ?? {})
const bytesForm = definitions.bytesInput?.oneOf ?? []
const closures = [
  ['traceability', closedOf(definitions.traceability, ['decisionIds', 'ruleIds', 'checkIds'])],
  [
    'constructionInputFixture',
    closedOf(
      fixtureForm,
      [],
      [
        'profileSpecificationSchema',
        'profileSpecification',
        'configurationSchemaAuthoritySet',
        'resolverCapabilitySpecification',
        'registryPackage'
      ]
    ) &&
      [
        'profileSpecificationSchema',
        'profileSpecification',
        'resolverCapabilitySpecification'
      ].every(name => closedOf(component(name), ['tuple', 'bytes'])) &&
      closedOf(component('configurationSchemaAuthoritySet'), ['tuple', 'bytes', 'members']) &&
      closedOf(component('registryPackage'), ['tuple', 'bytes', 'members']) &&
      roles.length === 6 &&
      closedOf(component('registryPackage')?.properties?.members, [], roles)
  ],
  [
    'bytesInput',
    bytesForm.length === 2 &&
      closedOf(bytesForm[0], ['bytes']) &&
      bytesForm[0].properties.bytes.pattern === '^(?:[0-9a-f]{2})*$' &&
      isDeepStrictEqual(bytesForm[1], { $ref: '#/definitions/reference' })
  ],
  [
    'resolverConstruction',
    closedOf(definitions.resolverConstruction, ['caseId', 'identityReference'])
  ],
  ['reference', closedOf(definitions.reference, ['path', 'digest'])],
  [
    'resolverInputAdmissionCase',
    closedOf(admissionCase, [
      ...HEADING,
      'resolverConstruction',
      'boundaryKind',
      'inputFixture',
      'expected',
      'verificationOwner',
      'traceability'
    ]) &&
      branches.every(row => {
        const kind = row?.properties?.boundaryKind?.const
        const [admitted, other] = row?.properties?.expected?.oneOf ?? []
        return (
          isDeepStrictEqual(Object.keys(row ?? {}), ['properties']) &&
          isDeepStrictEqual(Object.keys(row.properties), ['boundaryKind', 'expected']) &&
          row.properties.expected.oneOf.length === 2 &&
          closedOf(admitted, ['admitted', 'normalizedInput', 'resolverInvoked']) &&
          closedOf(other, [
            'admitted',
            'normalizedInput',
            sets.includes(kind) ? 'failures' : 'failureReason',
            'resolverInvoked'
          ])
        )
      })
  ],
  [
    'resolverCase',
    closedOf(
      resolverCase,
      [
        ...HEADING,
        'resolverConstruction',
        'commonInvocationInputFixture',
        'entries',
        'traceability'
      ],
      ['relations']
    ) &&
      entries.every(
        row =>
          closedOf(row, ['file', 'programmatic']) &&
          side(row.properties.file) &&
          side(row.properties.programmatic)
      ) &&
      relations.every(relation => closedOf(relation, ['role', 'kind', 'case', 'class']))
  ],
  [
    'handoffVector',
    closedOf(vector, [
      ...HEADING,
      'producerCaseId',
      'producerProfileFacts',
      'producerArtifactBindings',
      'consumerInputs',
      'expectedRelationship',
      'verificationOwner',
      'traceability'
    ]) &&
      closedOf(vector.properties.producerArtifactBindings?.items, ['key']) &&
      closedOf(definitions.exactReference, [
        'kind',
        'identity',
        'revision',
        'specificationRevision',
        'semanticDigest'
      ])
  ],
  [
    'conditionalObligation',
    rows.every((row, index) =>
      closedOf(row, [
        ...HEADING,
        'activation',
        'activationState',
        ...(index % 2 === 0 ? ['notApplicableReason'] : []),
        'verificationOwner',
        'traceability'
      ])
    )
  ]
]
// The input fixtures of the boundary kinds: a boundary that reads capability
// bounds holds the decoded capability value beside its input, an omitted
// input or candidate is an absent one, the selector's report and the
// call-site input are always present, and the collection, Snapshot, and
// call-site inputs may be an unsafe host inspection.
const ref = name => ({ $ref: `#/definitions/${name}` })
const inputOf = name => definitions[name]?.properties?.input
const holdsCapability = name =>
  isDeepStrictEqual(definitions[name]?.properties?.capability, ref('json'))
const reading = (name, form) =>
  closedOf(definitions[name], ['capability'], ['input']) &&
  holdsCapability(name) &&
  form(inputOf(name))
const always = (name, form) =>
  closedOf(definitions[name], ['capability', 'input']) &&
  holdsCapability(name) &&
  form(inputOf(name))
const unsafeHost = node =>
  closedOf(node, ['hostInspection']) && node.properties.hostInspection.const === 'unsafe'
const raw = node => isDeepStrictEqual(node, ref('bytesInput'))
const REPORTS = [
  ['absent'],
  ['string', 'value'],
  ['over-limit-string'],
  ['invalid-type', 'type'],
  ['unrepresentable'],
  ['unsafe']
]
const reports = inputOf('selectorInputFixture')?.oneOf ?? []
const collectionForms = inputOf('collectionInputFixture')?.oneOf ?? []
const snapshotForms = inputOf('snapshotInputFixture')?.oneOf ?? []
const [sidecar, sidecarUnsafe] = inputOf('callSiteInputFixture')?.oneOf ?? []
const mapping = sidecar?.properties?.mappings?.items
const edges = mapping?.properties?.path?.items?.oneOf ?? []
const originFixture = definitions.originInputFixture
const verificationFixture = definitions.verificationInputFixture
closures.push(
  ['specificationInputFixture', reading('specificationInputFixture', raw)],
  ['vocabularyInputFixture', reading('vocabularyInputFixture', raw)],
  [
    'canonicalizationDataInputFixture',
    reading(
      'canonicalizationDataInputFixture',
      node =>
        closedOf(node, ['manifest', 'payloads']) &&
        raw(node.properties.manifest) &&
        isDeepStrictEqual(node.properties.payloads, {
          type: 'object',
          additionalProperties: ref('reference')
        })
    )
  ],
  [
    'collectionInputFixture',
    reading(
      'collectionInputFixture',
      () =>
        collectionForms.length === 3 &&
        closedOf(collectionForms[0], ['members']) &&
        isDeepStrictEqual(collectionForms[0].properties.members, {
          type: 'array',
          items: ref('bytesInput')
        }) &&
        closedOf(collectionForms[1], ['container']) &&
        collectionForms[1].properties.container.const === 'malformed' &&
        unsafeHost(collectionForms[2])
    )
  ],
  [
    'verificationInputFixture',
    closedOf(verificationFixture, ['assertion', 'capability'], ['input']) &&
      holdsCapability('verificationInputFixture') &&
      closedOf(verificationFixture.properties.assertion, [
        'identity',
        'revision',
        'semanticDigest'
      ]) &&
      raw(inputOf('verificationInputFixture'))
  ],
  [
    'originInputFixture',
    closedOf(originFixture, ['slot'], ['candidate']) &&
      isDeepStrictEqual(strings(originFixture.properties.slot), origins) &&
      isDeepStrictEqual(originFixture.properties.candidate, {})
  ],
  [
    'selectorInputFixture',
    always(
      'selectorInputFixture',
      () =>
        reports.length === REPORTS.length &&
        reports.every(
          (node, index) =>
            closedOf(node, ['report', ...REPORTS[index].slice(1)]) &&
            node.properties.report.const === REPORTS[index][0]
        )
    )
  ],
  [
    'fileSourceInputFixture',
    always(
      'fileSourceInputFixture',
      node =>
        closedOf(node, ['nonSecret', 'bytes'], ['identity', 'revision']) &&
        raw(node.properties.bytes)
    )
  ],
  [
    'entrySourceInputFixture',
    always(
      'entrySourceInputFixture',
      node =>
        closedOf(node, ['nonSecret', 'snapshot'], ['identity', 'revision']) &&
        isDeepStrictEqual(node.properties.snapshot, ref('snapshot'))
    )
  ],
  [
    'snapshotInputFixture',
    reading(
      'snapshotInputFixture',
      () =>
        snapshotForms.length === 2 &&
        closedOf(snapshotForms[0], ['snapshot', 'digest']) &&
        unsafeHost(snapshotForms[1])
    )
  ],
  [
    'callSiteInputFixture',
    always(
      'callSiteInputFixture',
      () =>
        closedOf(
          sidecar,
          ['nonSecret', 'source', 'mappings'],
          ['identity', 'revision', 'locator']
        ) &&
        raw(sidecar.properties.source) &&
        isDeepStrictEqual(sidecar.properties.locator, ref('locatorCandidate')) &&
        closedOf(mapping, ['path', 'role', 'span']) &&
        closedOf(mapping.properties.span, ['start', 'end']) &&
        edges.length === 3 &&
        closedOf(edges[0], ['key']) &&
        closedOf(edges[1], ['keyUtf16']) &&
        closedOf(edges[2], ['index']) &&
        unsafeHost(sidecarUnsafe)
    )
  ],
  ['locatorCandidate', closedOf(definitions.locatorCandidate, ['variant', 'payload', 'nonSecret'])],
  ...['capabilityInputFixture', 'constructionIdentityInputFixture'].map(name => [
    name,
    closedOf(definitions[name], [], ['input']) && isDeepStrictEqual(inputOf(name), ref('json'))
  ])
)
// The input fixtures of the two admission steps (D15:3166, 3168, 3252): the
// common one holds the Verification Input assertion and one member per slot,
// keyed by the Safe Origin Kind ID the slot fixes, with the slot's candidate
// and, as its input, what the slot's boundary fixture holds as its input,
// either omitted for absence. An entry's holds its two slots: a file's input
// with its raw bytes, which are always present, its mark, its pair, and its
// locator candidate; a programmatic entry's with its mark and, each omitted
// for absence, its Snapshot, its pair, its locator candidate, and its
// call-site input; and a selector slot's input its report. The entry step's
// fixture holds the entry beside the capability value.
const COMMON_SLOT_FIXTURES = [
  ['profile-resolution-specification-input', 'specificationInputFixture'],
  ['intent-surface-class-vocabulary', 'vocabularyInputFixture'],
  ['locale-canonicalization-data', 'canonicalizationDataInputFixture'],
  ['profile-resolution-artifact-collection', 'collectionInputFixture'],
  ['resource-limit-policy-verification', 'verificationInputFixture'],
  ['implementation-capability', 'capabilityInputFixture']
]
const TEXT = { type: 'string' }
const MARK = { type: 'boolean' }
const commonFixture = definitions.commonInvocationInputFixture
const entryForms = definitions.entryAdmissionInputFixture?.oneOf ?? []
const [fileEntry, programmaticEntry] = entryForms
const optionalSlot = (node, input) =>
  closedOf(node, [], ['candidate', 'input']) &&
  isDeepStrictEqual(node.properties.candidate, {}) &&
  isDeepStrictEqual(node.properties.input, input)
const presentSlot = (node, form) =>
  closedOf(node, ['input'], ['candidate']) &&
  isDeepStrictEqual(node.properties.candidate, {}) &&
  form(node.properties.input)
const selectorSlot = node =>
  presentSlot(node, input => isDeepStrictEqual(input, inputOf('selectorInputFixture')))
const fileInput = input =>
  closedOf(input, ['bytes', 'nonSecret'], ['identity', 'revision', 'locator']) &&
  raw(input.properties.bytes) &&
  isDeepStrictEqual(input.properties.nonSecret, MARK) &&
  isDeepStrictEqual(input.properties.identity, TEXT) &&
  isDeepStrictEqual(input.properties.revision, TEXT) &&
  isDeepStrictEqual(input.properties.locator, ref('locatorCandidate'))
const programmaticInput = input =>
  closedOf(input, ['nonSecret'], ['snapshot', 'identity', 'revision', 'locator', 'callSite']) &&
  isDeepStrictEqual(input.properties.nonSecret, MARK) &&
  isDeepStrictEqual(input.properties.snapshot, inputOf('snapshotInputFixture')) &&
  isDeepStrictEqual(input.properties.identity, TEXT) &&
  isDeepStrictEqual(input.properties.revision, TEXT) &&
  isDeepStrictEqual(input.properties.locator, ref('locatorCandidate')) &&
  isDeepStrictEqual(input.properties.callSite, inputOf('callSiteInputFixture'))
closures.push(
  [
    'commonInvocationInputFixture',
    closedOf(commonFixture, ['assertion', ...COMMON_SLOT_FIXTURES.map(([slot]) => slot)]) &&
      isDeepStrictEqual(
        commonFixture.properties.assertion,
        verificationFixture?.properties?.assertion
      ) &&
      COMMON_SLOT_FIXTURES.every(([slot, name]) =>
        optionalSlot(commonFixture.properties[slot], inputOf(name))
      )
  ],
  [
    'entryAdmissionInputFixture',
    entryForms.length === 2 &&
      closedOf(fileEntry, ['configuration-file', 'profile-selector-cli']) &&
      presentSlot(fileEntry.properties['configuration-file'], fileInput) &&
      selectorSlot(fileEntry.properties['profile-selector-cli']) &&
      closedOf(programmaticEntry, [
        'configuration-programmatic',
        'profile-selector-programmatic'
      ]) &&
      presentSlot(programmaticEntry.properties['configuration-programmatic'], programmaticInput) &&
      selectorSlot(programmaticEntry.properties['profile-selector-programmatic'])
  ],
  [
    'entryInputFixture',
    closedOf(definitions.entryInputFixture, ['capability', 'entry']) &&
      holdsCapability('entryInputFixture') &&
      isDeepStrictEqual(
        definitions.entryInputFixture.properties.entry,
        ref('entryAdmissionInputFixture')
      )
  ]
)
for (const [name, closedForm] of closures) {
  if (!closedForm) {
    fail(`schema: ${name} is not a closed object of exactly its members`)
  }
}

// The expected-outcome fixture (D15:3271-3303): a checked outcome with its
// profile view and binding keys, or a blocked one with neither, each with
// the construction identity and the Resolution Evidence of both projections,
// the Shared one absent after an entry failure. Each projection's arrays
// list `{ owner, record }` rows, and only the terminal Finding and the Entry
// reserve's cause have no owner (D15:3303).
const OUTCOME_MEMBERS = [
  'outcome',
  'resolverConstructionIdentity',
  'profile',
  'resolvedProfileArtifactBindings',
  'resolutionEvidence'
]
const NULL = { type: 'null' }

/**
 * What a node admits besides null, when it is a `oneOf` of it and null.
 *
 * @param node - The node.
 * @returns The other form, or undefined.
 */
function orNull(node) {
  return node?.oneOf?.length === 2 && isDeepStrictEqual(node.oneOf[1], NULL)
    ? node.oneOf[0]
    : undefined
}

const outcomes = definitions.expectedOutcomeFixture?.oneOf ?? []
if (outcomes.length !== 2 || !outcomes.every(form => closedOf(form, OUTCOME_MEMBERS))) {
  fail('expectedOutcomeFixture: not a checked and a blocked closed outcome')
} else {
  const [checked, blocked] = outcomes.map(form => form.properties)
  const bindings = checked.resolvedProfileArtifactBindings
  if (
    checked.outcome.const !== 'checked' ||
    !isDeepStrictEqual(checked.profile, ref('profileView')) ||
    bindings?.type !== 'array' ||
    !closedOf(bindings.items, ['key']) ||
    !isDeepStrictEqual(bindings.items.properties.key, ref('exactReference'))
  ) {
    fail('expectedOutcomeFixture: the checked outcome is not a profile view and binding keys')
  }
  if (
    blocked.outcome.const !== 'blocked' ||
    !isDeepStrictEqual(blocked.profile, NULL) ||
    !isDeepStrictEqual(blocked.resolvedProfileArtifactBindings, NULL)
  ) {
    fail('expectedOutcomeFixture: the blocked outcome holds a profile or bindings')
  }
  for (const form of [checked, blocked]) {
    if (
      !isDeepStrictEqual(form.resolverConstructionIdentity, ref('constructionIdentity')) ||
      !isDeepStrictEqual(form.resolutionEvidence, ref('resolutionEvidence'))
    ) {
      fail('expectedOutcomeFixture: an outcome without its identity or Resolution Evidence')
    }
  }
}
const evidence = definitions.resolutionEvidence
if (
  !closedOf(evidence, [
    'complete',
    'terminalStates',
    'shared',
    'sharedUnavailableCauseKeys',
    'entry'
  ]) ||
  !isDeepStrictEqual(evidence.properties.complete, { type: 'boolean' }) ||
  !isDeepStrictEqual(orNull(evidence.properties.shared), ref('sharedProjection')) ||
  !isDeepStrictEqual(evidence.properties.entry, ref('entryProjection')) ||
  !isDeepStrictEqual(evidence.properties.terminalStates?.items, ref('terminalStateView')) ||
  !isDeepStrictEqual(evidence.properties.sharedUnavailableCauseKeys?.items, ref('findingKeyView'))
) {
  fail('resolutionEvidence: not the bundle of both projections')
}
const PROJECTIONS = [
  [
    'sharedProjection',
    [
      ['findings', 'findingView', true],
      ['evaluationStatus', 'evaluationStatusView', false],
      ['coverageDecisionBases', 'coverageDecisionBasisView', false],
      ['semanticDependencies', 'semanticDependencyView', false],
      ['materializedDependencies', 'materializedDependencyView', false]
    ]
  ],
  [
    'entryProjection',
    [
      ['findings', 'findingView', true],
      ['evaluationStatus', 'evaluationStatusView', false],
      ['occurrences', 'findingOccurrenceView', false],
      ['coverageDecisionEvidence', 'coverageDecisionEvidenceView', false],
      ['evidenceIndex', 'evidenceView', false],
      ['entryDependencies', 'entryDependencyView', false],
      ['sharedUnavailabilityCauses', 'sharedUnavailabilityCauseView', true]
    ]
  ]
]
for (const [name, arrays] of PROJECTIONS) {
  const node = definitions[name]
  if (
    !closedOf(
      node,
      arrays.map(([member]) => member)
    )
  ) {
    fail(`${String(name)}: not its arrays`)
    continue
  }
  for (const [member, view, reserved] of arrays) {
    const rows = node.properties[member]
    const owner = rows?.items?.properties?.owner
    if (
      rows?.type !== 'array' ||
      !closedOf(rows.items, ['owner', 'record']) ||
      !isDeepStrictEqual(reserved ? orNull(owner) : owner, ref('unitView')) ||
      !isDeepStrictEqual(rows.items.properties.record, ref(view))
    ) {
      fail(
        `${String(name)}.${String(member)}: not rows of an owner${reserved ? ' or none' : ''} and a ${String(view)}`
      )
    }
  }
}

// Each code's Finding and Finding Key take the checks, reasons, severity,
// blocking flag, and suggestion of its Finding Registry row, and the
// `details` members its Check Registry rows give it (D15:2230, 2232, 2309).
const findingRows = read('registries/finding-v0.json').value.rows
const checkRows = read('registries/check-v0.json').value.rows
const findings = definitions.findingView?.oneOf ?? []
const keys = definitions.findingKeyView?.oneOf ?? []
if (findings.length !== findingRows.length || keys.length !== findingRows.length) {
  fail('findingView: not one branch per Finding Registry code')
}
for (const [index, row] of findingRows.entries()) {
  const finding = findings[index]
  const given = checkRows
    .flatMap(check => check.findingPayloadStrategy.details)
    .filter(entry => entry.code === row.code)
  const members = given[0]?.members ?? []
  if (given.some(entry => !isDeepStrictEqual(entry.members, members))) {
    fail(`${row.code}: the Check Registry gives its details different members`)
  }
  const reasoned = row.allowedReasons.length > 0
  const keyNames = [
    'specification',
    'phase',
    'check',
    'code',
    'subject',
    ...(reasoned ? ['reason'] : [])
  ]
  // Both views name the code's one Finding Key definition.
  const name = `findingKey:${String(row.code)}`
  const form = definitions[name]
  if (
    !isDeepStrictEqual(keys[index], ref(name)) ||
    !isDeepStrictEqual(finding?.properties?.key, ref(name)) ||
    !closedOf(form, keyNames) ||
    !isDeepStrictEqual(form.properties.specification, ref('versionedIdentity')) ||
    !isDeepStrictEqual(form.properties.phase, ref('phaseId')) ||
    form.properties.code.const !== row.code ||
    !isDeepStrictEqual(strings(form.properties.check), row.allowedCheckIds) ||
    (reasoned && !isDeepStrictEqual(strings(form.properties.reason), row.allowedReasons))
  ) {
    fail(`${name}: not its key, or not the key both views name`)
  }
  const names = [
    'key',
    'severity',
    'blocking',
    ...(members.length > 0 ? ['details'] : []),
    ...(row.suggestionKind === 'none' ? [] : ['suggestion'])
  ]
  if (
    !closedOf(finding, names) ||
    finding.properties.severity.const !== row.severity ||
    finding.properties.blocking.const !== row.blocking ||
    (members.length > 0 && !closedOf(finding.properties.details, members))
  ) {
    fail(`findingView ${row.code}: not its severity, blocking flag, details, or suggestion`)
  }
}

// Each evidence kind's Evidence: its members, and the origins whose row
// permits it (D15:2493).
const EVIDENCE = [
  ['entry-source', ['kind', 'origin', 'source'], ['locator', 'path', 'span']],
  ['configuration-value', ['kind', 'origin', 'source', 'value'], ['locator', 'span']],
  ['profile-selector', ['kind', 'origin', 'selector'], []],
  ['specification-input', ['kind', 'origin', 'component'], []],
  ['artifact-input', ['kind', 'origin', 'component'], []],
  ['verification-input', ['kind', 'origin', 'component'], []],
  ['implementation-capability', ['kind', 'origin', 'component'], []]
]
const originRows = read('registries/safe-origin-kind-v0.json').value.rows
const evidenceForms = definitions.evidenceView?.oneOf ?? []
if (evidenceForms.length !== EVIDENCE.length) {
  fail('evidenceView: not one branch per evidence kind')
}
for (const [index, [kind, required, optional]] of EVIDENCE.entries()) {
  const form = evidenceForms[index]
  const permitting = originRows
    .filter(row => row.permittedEvidenceKinds.includes(kind))
    .map(row => row.id)
  if (
    !closedOf(form, required, optional) ||
    form.properties.kind.const !== kind ||
    !isDeepStrictEqual(strings(form.properties.origin), permitting)
  ) {
    fail(`evidenceView ${String(kind)}: not its members and the origins that permit it`)
  }
}

// The profile view: the twelve semantic groups in their order, and its
// policy section exactly the specification's bounds, each a count
// (D15:1214-1237).
const profileView = definitions.profileView
if (!closedOf(profileView, profile.semanticGroups)) {
  fail('profileView: not the semantic groups in their order')
}
const bounds = []

/**
 * Collect the bounds of a closed policy section.
 *
 * @param node - A node of the section.
 * @param path - Its dotted path.
 */
function section(node, path) {
  if (node?.type === 'object') {
    const names = Object.keys(node.properties ?? {})
    if (node.additionalProperties !== false || !isDeepStrictEqual(node.required, names)) {
      fail(`profileView: ${path} is not a closed object`)
    }
    for (const name of names) {
      section(node.properties[name], path ? `${path}.${name}` : name)
    }
  } else if (node?.pattern === '^(0|[1-9][0-9]*)$') {
    bounds.push(path)
  } else {
    fail(`profileView: ${path} is not a count`)
  }
}
section(profileView?.properties?.requiredPolicies?.properties?.projectProfileResolution, '')
if (
  !isDeepStrictEqual(
    bounds.toSorted((a, b) => a.localeCompare(b)),
    profile.resourceBounds.toSorted((a, b) => a.localeCompare(b))
  )
) {
  fail('profileView: the policy section does not hold exactly the specification bounds')
}
// The materialized slots, the common ones first (D15:2508), and each group's
// semantic inputs in their order (D15:2517): never a slot that only decides
// whether the group is established, such as `profile-selector`,
// `verification-input`, or `implementation-capability`.
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
const LOCALE_INPUTS = [
  'specification-input',
  'locale-canonicalization-data',
  'artifact-collection',
  'configuration-value'
]
const POLICY_INPUTS = ['artifact-collection', 'configuration-value']
const SEMANTIC_INPUTS = {
  profileSpecification: [],
  projectIdentity: ['configuration-value'],
  localeCanonicalization: ['specification-input', 'locale-canonicalization-data'],
  projectLocales: LOCALE_INPUTS,
  localeNegotiation: LOCALE_INPUTS,
  messageFallback: LOCALE_INPUTS,
  coverage: [
    'specification-input',
    'intent-surface-class-vocabulary',
    'locale-canonicalization-data',
    'artifact-collection',
    'configuration-value'
  ],
  requiredPolicies: POLICY_INPUTS,
  optionalPolicies: POLICY_INPUTS,
  targets: LOCALE_INPUTS,
  deploymentGroups: LOCALE_INPUTS,
  delivery: ['specification-input', 'configuration-value']
}
if (
  !isDeepStrictEqual(
    strings(definitions.materializedDependencyView?.properties?.slot),
    MATERIALIZED_SLOTS
  )
) {
  fail('materializedDependencyView: not the materialized slots in their order')
}
const semantic = definitions.semanticDependencyView?.oneOf ?? []
if (
  semantic.length !== profile.semanticGroups.length ||
  !isDeepStrictEqual(Object.keys(SEMANTIC_INPUTS), profile.semanticGroups)
) {
  fail('semanticDependencyView: not one branch per semantic group')
}
for (const [index, group] of profile.semanticGroups.entries()) {
  const form = semantic[index]
  if (
    !closedOf(form, ['group', 'inputs']) ||
    form.properties.group.const !== group ||
    !isDeepStrictEqual(form.properties.inputs.const, SEMANTIC_INPUTS[group])
  ) {
    fail(`semanticDependencyView ${group}: not its group and semantic inputs`)
  }
}

if (failures.length > 0) {
  for (const message of failures) {
    console.error(message)
  }
  process.exit(1)
}

console.log(
  `case schema agrees: ${KINDS.length} manifest kinds, ${BOUNDARIES.length} boundary kinds with ${BOUNDARIES.reduce((sum, [, row]) => sum + row.length, 0)} reasons in ${precedenceRows} precedence rows, ${reasons.length} construction reasons, ${OBLIGATIONS.length} obligations, and the expected outcome with ${findingRows.length} Finding codes, ${EVIDENCE.length} evidence kinds, and ${bounds.length} policy bounds`
)
