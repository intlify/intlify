/**
 * Check the committed construction authorities against this implementation.
 *
 * The Rust crate writes the Profile Specification Schema, the Profile
 * Specification, the Configuration Schema Authority Set, and the schema the
 * set maps to `schemaVersion` `"0"`, and compiles in four digests: the Profile
 * Specification's semantic digest and the file digests of the other three.
 * This script shares no code with it: it builds the Profile Specification from
 * its own copy of design 015's tables and the locale data's manifest, computes
 * the semantic digest with this package's own framing, hashes every artifact,
 * compares the mapped schema with the published configuration schema, and
 * reads the four pinned digests out of the crate's source. It does not read
 * the design document.
 */

import { createHash } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { digest as semanticDigest } from './framing.mjs'
import { POLICY_GROUPS } from './resolution-bounds.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const crate = resolve(here, '../../../crates/intlify_config')
const authorities = resolve(crate, 'conformance/authorities')
const manifestPath = resolve(here, '../../../crates/intlify_locale_icu4x/data/manifest.json')

const DRAFT_7 = 'http://json-schema.org/draft-07/schema#'
const REVISION = '0'
const SEMANTIC_DOMAIN = 'project-profile-specification-semantic-digest'

/**
 * Design 015's eight specification slots in their fixed order, each with the
 * identity it supports at revision `"0"`, or none when it is asserted.
 *
 * @type {[string, string | null][]}
 */
const SLOTS = [
  ['project-profile', 'intlify-project-profile'],
  ['locale-canonicalization', 'intlify-locale-canonicalization'],
  ['locale-negotiation', 'intlify-locale-negotiation'],
  ['message-locale-fallback', 'intlify-message-locale-fallback'],
  ['coverage', 'intlify-localization-coverage'],
  ['intent-surface-class-vocabulary', null],
  ['delivery-graph', 'intlify-delivery-graph'],
  ['delivery-placement', 'intlify-delivery-placement']
]

/** Design 015's sixteen phase IDs in rank order. */
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

/** The profile view's members, one per semantic group, in table order. */
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

/** The four other construction authorities and their identities. */
const AUTHORITIES = [
  ['profileSpecificationSchema', 'intlify-project-profile-specification-schema'],
  ['configurationSchemaAuthoritySet', 'intlify-configuration-schema-authority-set'],
  ['resolverCapabilitySpecification', 'intlify-project-profile-resolver-capability'],
  ['registryPackage', 'intlify-project-profile-resolver-registry-package']
]

/** A Snapshot's rejected-node marker reasons, in the order 015 lists them. */
const MARKER_REASONS = [
  'unsupported-type',
  'non-finite-number',
  'non-portable-number',
  'invalid-unicode',
  'non-string-key',
  'dynamic-container',
  'dynamic-property',
  'sparse-array',
  'cycle',
  'bootstrap-limit-exceeded'
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
 * Pin a specification or an authority at revision `"0"`.
 *
 * @param identity - Its identity.
 * @returns The pin.
 */
function pin(identity) {
  return { identity, revision: REVISION }
}

/**
 * Visit every schema object of a Draft 7 schema document's subschema
 * positions that this schema uses.
 *
 * @param schema - The document.
 * @param visit - Called with each schema object and its location.
 */
function walk(schema, visit) {
  const pending = [[schema, '#']]
  while (pending.length > 0) {
    const [node, at] = pending.pop()
    visit(node, at)
    for (const keyword of ['properties', 'definitions']) {
      for (const [name, child] of Object.entries(node[keyword] ?? {})) {
        pending.push([child, `${at}/${keyword}/${name}`])
      }
    }
    if (node.items !== undefined) {
      pending.push([node.items, `${at}/items`])
    }
    for (const [index, child] of (node.anyOf ?? []).entries()) {
      pending.push([child, `${at}/anyOf/${index}`])
    }
  }
}

const read = name => readFileSync(resolve(authorities, name))
const schemaBytes = read('project-profile-specification-v0.schema.json')
const bodyBytes = read('project-profile-specification-v0.json')
const setBytes = read('configuration-schema-authority-set-v0.json')
const memberBytes = read('configuration-v0.schema.json')
const published = readFileSync(resolve(crate, 'schema/project-profile-config-v0.schema.json'))
const profileSource = readFileSync(resolve(crate, 'src/construction/profile.rs'), 'utf8')
const setSource = readFileSync(resolve(crate, 'src/construction/authority_set.rs'), 'utf8')

for (const [name, bytes] of [
  ['project-profile-specification-v0.schema.json', schemaBytes],
  ['project-profile-specification-v0.json', bodyBytes],
  ['configuration-schema-authority-set-v0.json', setBytes],
  ['configuration-v0.schema.json', memberBytes]
]) {
  if (
    bytes.includes(0x0d) ||
    !bytes.toString('utf8').endsWith('}\n') ||
    bytes.toString('utf8').endsWith('\n\n')
  ) {
    fail(`${name}: not written with one final newline and no carriage return`)
  }
}

// The Profile Specification, member for member and in order.
const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'))
const expected = {
  specificationSlots: Object.fromEntries(
    SLOTS.map(([slot, identity]) => [slot, identity === null ? 'asserted' : pin(identity)])
  ),
  localeCanonicalizationDataset: {
    identity: manifest.dataset.identity,
    semanticDigest: manifest.dataset.semanticDigest
  },
  phases: PHASES,
  semanticGroups: SEMANTIC_GROUPS,
  resourceBounds: POLICY_GROUPS.flatMap(([group, bounds]) =>
    bounds.map(bound => `${group}.${bound}`)
  ),
  authorities: Object.fromEntries(AUTHORITIES.map(([name, identity]) => [name, pin(identity)])),
  tokenDomains: {
    subject: 'intlify.project-profile.subject',
    evidence: 'intlify.project-profile.evidence'
  },
  revisionDomains: {
    fileSource: 'intlify.project-profile.file-source-revision.v0',
    callSiteSource: 'intlify.project-profile.call-site-source-revision.v0'
  },
  snapshot: {
    domain: 'intlify.programmatic-entry-snapshot.v0',
    markerReasons: MARKER_REASONS
  }
}
const body = JSON.parse(bodyBytes.toString('utf8'))
if (JSON.stringify(body) !== JSON.stringify(expected)) {
  fail(
    `profile specification: the body differs from 015's tables\n  committed ${JSON.stringify(body)}\n  expected  ${JSON.stringify(expected)}`
  )
}
if (expected.resourceBounds.length !== 52) {
  fail(`the transcribed bounds are ${expected.resourceBounds.length}, not 52`)
}
const semantic = semanticDigest(SEMANTIC_DOMAIN, body)
if (semantic !== pinned(profileSource, 'SEMANTIC_DIGEST')) {
  fail(
    `profile specification: the semantic digest ${semantic} is not the pinned ${pinned(profileSource, 'SEMANTIC_DIGEST')}`
  )
}

// The schema: Draft 7, pinned, and no position of it admits a number.
const schema = JSON.parse(schemaBytes.toString('utf8'))
if (schema.$schema !== DRAFT_7) {
  fail(`profile specification schema: $schema ${schema.$schema}`)
}
if (fileDigest(schemaBytes) !== pinned(profileSource, 'SCHEMA_DIGEST')) {
  fail(
    `profile specification schema: the file digest ${fileDigest(schemaBytes)} is not the pinned ${pinned(profileSource, 'SCHEMA_DIGEST')}`
  )
}
// Each position names one type that is not a number, refers to a definition,
// or is a union of definitions; each object is closed and each array says what
// its items are. No number then fits anywhere.
let positions = 0
walk(schema, (node, at) => {
  positions += 1
  if (node.type === undefined && node.$ref === undefined && node.anyOf === undefined) {
    fail(`profile specification schema: ${at} constrains no type`)
  }
  if (node.type !== undefined && !['object', 'array', 'string'].includes(node.type)) {
    fail(`profile specification schema: ${at} admits ${JSON.stringify(node.type)}`)
  }
  if (node.type === 'object' && node.additionalProperties !== false) {
    fail(`profile specification schema: ${at} is not closed`)
  }
  if (node.type === 'array' && node.items === undefined) {
    fail(`profile specification schema: ${at} leaves its items open`)
  }
  const target = node.$ref?.match(/^#\/definitions\/([A-Za-z]+)$/)?.[1]
  if (node.$ref !== undefined && schema.definitions?.[target] === undefined) {
    fail(`profile specification schema: ${at} refers to ${node.$ref}`)
  }
})

// The mapped schema is the published configuration schema.
if (!memberBytes.equals(published)) {
  fail('configuration schema: the mapped bytes are not the published configuration schema')
}
if (JSON.parse(memberBytes.toString('utf8')).$schema !== DRAFT_7) {
  fail('configuration schema: the mapped schema does not name Draft 7')
}
if (fileDigest(memberBytes) !== pinned(setSource, 'MEMBER_DIGEST')) {
  fail(
    `configuration schema: the file digest ${fileDigest(memberBytes)} is not the pinned ${pinned(setSource, 'MEMBER_DIGEST')}`
  )
}

// The set belongs to the Profile Specification and maps the schema.
const set = JSON.parse(setBytes.toString('utf8'))
const expectedSet = {
  profileSpecification: { ...pin('intlify-project-profile'), semanticDigest: semantic },
  members: {
    0: {
      identity: 'intlify-configuration-schema',
      revision: REVISION,
      dialect: DRAFT_7,
      digest: fileDigest(memberBytes)
    }
  }
}
if (JSON.stringify(set) !== JSON.stringify(expectedSet)) {
  fail(
    `configuration schema authority set: differs\n  committed ${JSON.stringify(set)}\n  expected  ${JSON.stringify(expectedSet)}`
  )
}
if (fileDigest(setBytes) !== pinned(setSource, 'DIGEST')) {
  fail(
    `configuration schema authority set: the file digest ${fileDigest(setBytes)} is not the pinned ${pinned(setSource, 'DIGEST')}`
  )
}

if (failures.length > 0) {
  for (const message of failures) {
    console.error(message)
  }
  process.exit(1)
}

console.log(
  `construction authorities agree: ${SLOTS.length} slots, ${PHASES.length} phases, ${SEMANTIC_GROUPS.length} semantic groups, ${expected.resourceBounds.length} bounds, ${positions} schema positions, semantic digest ${semantic}`
)
