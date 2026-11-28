/**
 * Check the committed conformance Suite Index against this implementation.
 *
 * The Rust crate writes the Suite Index Schema and the Suite Index below
 * `conformance/`, and its conformance runner pins the schema's file digest
 * as its trust root. This script shares no code with it. It checks that the
 * schema closes every object of the index and gives each value its form,
 * that the index holds exactly the members, roles, identities, and revisions
 * of its own copy of design 015's Suite Index, that every path is
 * suite-root-relative and names a file with the declared digest, that the
 * Root Package, the Configuration Schema Authority Set, and the Registry
 * Package list the digests the index declares for their members, that the
 * Profile Specification has the semantic digest this package's own framing
 * computes, and that every digest is the one the crate pins in its source.
 * For each construction case it checks the fixture closure, the fixture and
 * every body it references outside a decoded JSON value by their digests,
 * and that the case expects what the row its rule ID names refuses with, or,
 * constructing, the committed construction vector's identity. For each input
 * admission case of a boundary kind it checks the same closure, that the
 * case names a constructing case with that identity, that each kind
 * has its admitted case and one case for each row of its failure precedence
 * in the Case Schema, each expecting its row's reason under its pair's rule
 * ID, and that the admitted case expects the normalized input this script
 * derives from the fixture: a decoded input with its members in byte order,
 * a revision this script computes over the bytes, a Snapshot that is one of
 * the committed vectors with its digest, and a call-site sidecar with its
 * mappings in canonical order. For each Resolver Case it checks the same
 * closure for its common input, entry, and expected outcome fixtures, that
 * the locale data it submits is the committed ICU4X artifact, that each
 * expected outcome names the vector's identity and keeps 015's rules for an
 * outcome, its Shared evidence and causes, its rows without an owner, and
 * its bindings' order, that the case traces each check its expected outcomes
 * report under the check's rule, and that its paired entries agree where 015
 * makes them agree. Each Handoff Vector names a listed Resolver Case whose
 * every expected outcome is checked and gives the vector's facts and
 * bindings, and each Group ID selection vector concludes what 015's selection
 * rules conclude from its selectors. Each row of the obligation table is one
 * inactive manifest, and nothing activates an evaluated row. It does not read
 * the design document.
 */

import { createHash } from 'node:crypto'
import { existsSync, readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { isDeepStrictEqual } from 'node:util'

import { encode, digest as semanticDigest } from './framing.mjs'
import { repeatsMember } from './strict-json.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const crate = resolve(here, '../../../crates/intlify_config')
const root = resolve(crate, 'conformance')

const DRAFT_7 = 'http://json-schema.org/draft-07/schema#'
const REVISION = '0'
const SEMANTIC_DOMAIN = 'project-profile-specification-semantic-digest'
const TOKEN_PATTERN = '^[a-z0-9](?:[a-z0-9._-]*[a-z0-9])?$'
const DIGEST_PATTERN = '^sha256:[0-9a-f]{64}$'
const PATH_PATTERN = '^[A-Za-z0-9][A-Za-z0-9._-]*(?:/[A-Za-z0-9][A-Za-z0-9._-]*)*$'
const INDEX_PATH = 'suite-index-v0.json'
const SCHEMA_PATH = 'schemas/suite-index-v0.schema.json'

/** The Suite Index's members, in the order the crate writes them. */
const MEMBERS = [
  'suiteRevision',
  'suiteIndexSchema',
  'resolverConstructionRootPackage',
  'profileSpecificationSchema',
  'profileSpecification',
  'configurationSchemaAuthoritySet',
  'resolverCapabilitySpecification',
  'caseSchema',
  'registryPackage',
  'resolverConstructionCases',
  'resolverInputAdmissionCases',
  'resolverCases',
  'handoffVectors',
  'conditionalObligations'
]

/** The Root Package's five roles, in their fixed order. */
const ROOT_ROLES = [
  'construction-admission-specification',
  'construction-admission-schema',
  'authority-envelope-schema-package',
  'json-schema-meta-schema-package',
  'cross-authority-rules'
]

/** The Registry Package's six roles, in their fixed order. */
const REGISTRY_ROLES = [
  'safe-origin-kind-registry-schema',
  'check-registry-schema',
  'finding-registry-schema',
  'safe-origin-kind-registry',
  'check-registry',
  'finding-registry'
]

/** The members every entry starts with. */
const ENTRY = ['identity', 'revision', 'path', 'digest']

/**
 * Each member's identity and its own members, in order; a member with roles
 * lists the members of each role.
 */
const SHAPES = {
  suiteIndexSchema: {
    identity: 'intlify-project-profile-conformance-suite-index',
    members: ENTRY
  },
  resolverConstructionRootPackage: {
    identity: 'intlify-resolver-construction-root-package',
    members: [...ENTRY, 'canonicalDigest', 'members'],
    roles: ROOT_ROLES,
    role: ['path', 'digest']
  },
  profileSpecificationSchema: {
    identity: 'intlify-project-profile-specification-schema',
    members: ENTRY
  },
  profileSpecification: {
    identity: 'intlify-project-profile',
    members: [...ENTRY, 'semanticDigest']
  },
  configurationSchemaAuthoritySet: {
    identity: 'intlify-configuration-schema-authority-set',
    members: [...ENTRY, 'profileSpecification', 'members'],
    roles: ['0'],
    role: ['identity', 'revision', 'dialect', 'path', 'digest']
  },
  resolverCapabilitySpecification: {
    identity: 'intlify-project-profile-resolver-capability',
    members: ENTRY
  },
  caseSchema: {
    identity: 'intlify-project-profile-conformance-case',
    members: ENTRY
  },
  registryPackage: {
    identity: 'intlify-project-profile-resolver-registry-package',
    members: [...ENTRY, 'members'],
    roles: REGISTRY_ROLES,
    role: ENTRY
  }
}

/** A case or Handoff Vector ID, and a conditional obligation ID. */
const CASE_ID_PATTERN = '^015-[a-z0-9]+(?:-[a-z0-9]+)*$'
const OBLIGATION_ID_PATTERN = '^015-(?:XV|AD|HV)-[0-9]{3}$'

/** The same ID forms, as this script's own regular expressions. */
const CASE_ID = /^015-[\da-z]+(?:-[\da-z]+)*$/
const OBLIGATION_ID = /^015-(?:XV|AD|HV)-\d{3}$/

/**
 * Each manifest array, with the `manifestKind` of the manifests it lists and
 * the form of their IDs.
 */
const ARRAYS = {
  resolverConstructionCases: ['resolver-construction', CASE_ID_PATTERN, CASE_ID],
  resolverInputAdmissionCases: ['resolver-input-admission', CASE_ID_PATTERN, CASE_ID],
  resolverCases: ['resolver', CASE_ID_PATTERN, CASE_ID],
  handoffVectors: ['handoff-vector', CASE_ID_PATTERN, CASE_ID],
  conditionalObligations: ['conditional-obligation', OBLIGATION_ID_PATTERN, OBLIGATION_ID]
}

/** Where each digest the index declares is pinned in the crate's source. */
const PINS = [
  ['src/conformance/case.rs', 'DIGEST', '/caseSchema/digest'],
  ['src/conformance/index.rs', 'SCHEMA_DIGEST', '/suiteIndexSchema/digest'],
  ['src/construction/root.rs', 'DIGEST', '/resolverConstructionRootPackage/digest'],
  [
    'src/construction/root.rs',
    'CANONICAL_DIGEST',
    '/resolverConstructionRootPackage/canonicalDigest'
  ],
  ['src/construction/profile.rs', 'SCHEMA_DIGEST', '/profileSpecificationSchema/digest'],
  ['src/construction/profile.rs', 'SEMANTIC_DIGEST', '/profileSpecification/semanticDigest'],
  ['src/construction/authority_set.rs', 'DIGEST', '/configurationSchemaAuthoritySet/digest'],
  [
    'src/construction/authority_set.rs',
    'MEMBER_DIGEST',
    '/configurationSchemaAuthoritySet/members/0/digest'
  ],
  ['src/construction/capability.rs', 'DIGEST', '/resolverCapabilitySpecification/digest'],
  ['src/construction/registry.rs', 'DIGEST', '/registryPackage/digest'],
  [
    'src/construction/registry/safe_origin.rs',
    'SCHEMA_DIGEST',
    '/registryPackage/members/safe-origin-kind-registry-schema/digest'
  ],
  [
    'src/construction/registry/check.rs',
    'SCHEMA_DIGEST',
    '/registryPackage/members/check-registry-schema/digest'
  ],
  [
    'src/construction/registry/finding.rs',
    'SCHEMA_DIGEST',
    '/registryPackage/members/finding-registry-schema/digest'
  ],
  [
    'src/construction/registry/safe_origin.rs',
    'DIGEST',
    '/registryPackage/members/safe-origin-kind-registry/digest'
  ],
  [
    'src/construction/registry/check.rs',
    'DIGEST',
    '/registryPackage/members/check-registry/digest'
  ],
  [
    'src/construction/registry/finding.rs',
    'DIGEST',
    '/registryPackage/members/finding-registry/digest'
  ]
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
 * The file digest of exact bytes.
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
 * Follow a JSON Pointer of plain member names.
 *
 * @param value - The document.
 * @param pointer - The pointer.
 * @returns The value it names, or undefined.
 */
function at(value, pointer) {
  return pointer
    .split('/')
    .slice(1)
    .reduce((node, name) => node?.[name], value)
}

/**
 * Read a file's bytes, recording why when it cannot be read, as when its
 * path names a directory.
 *
 * @param file - The file.
 * @param shown - Its path as messages show it.
 * @returns The bytes, or undefined.
 */
function readBytes(file, shown) {
  try {
    return readFileSync(file)
  } catch (error) {
    fail(`${shown}: unreadable (${error.code ?? error.message})`)
    return undefined
  }
}

/**
 * Read a JSON file strictly: its bytes, then its value, which must not
 * repeat a member.
 *
 * @param file - The file.
 * @param shown - Its path as messages show it.
 * @returns The bytes and the value, either undefined when unavailable.
 */
function readJson(file, shown) {
  const bytes = readBytes(file, shown)
  if (bytes === undefined) {
    return { bytes, value: undefined }
  }
  const text = bytes.toString('utf8')
  let value
  try {
    value = JSON.parse(text)
  } catch {
    fail(`${shown}: not JSON`)
    return { bytes, value: undefined }
  }
  if (repeatsMember(text)) {
    fail(`${shown}: repeats a member`)
    return { bytes, value: undefined }
  }
  return { bytes, value }
}

/**
 * Read a committed artifact and check its form: a JSON object, one final
 * newline, no carriage return, and exactly the bytes two-space indentation
 * writes.
 *
 * @param path - Its path below the suite root.
 * @returns Its bytes and value, the value undefined unless it is an object.
 */
function artifact(path) {
  const file = resolve(root, path)
  if (!existsSync(file)) {
    fail(`${path}: missing`)
    return { bytes: undefined, value: undefined }
  }
  const { bytes, value } = readJson(file, path)
  if (value === undefined) {
    return { bytes, value }
  }
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    fail(`${path}: not a JSON object`)
    return { bytes, value: undefined }
  }
  const text = bytes.toString('utf8')
  if (bytes.includes(0x0d) || !text.endsWith('}\n') || text.endsWith('\n\n')) {
    fail(`${path}: not one final newline without carriage returns`)
  }
  if (`${JSON.stringify(value, null, 2)}\n` !== text) {
    fail(`${path}: not the bytes two-space indentation writes`)
  }
  return { bytes, value }
}

// The path grammar, as this script's own regular expression, on its own
// examples.
const path = /^[a-z0-9][\w.-]*(?:\/[a-z0-9][\w.-]*)*$/i
for (const good of [
  'a',
  '0',
  'construction/root-package-v0.json',
  'cases/conditional/015-XV-001.json'
]) {
  if (!path.test(good)) {
    fail(`path grammar: refuses ${good}`)
  }
}
for (const bad of [
  '',
  '/a',
  'a/',
  'a//b',
  '.',
  '..',
  './a',
  'a/../b',
  '.hidden',
  'a\\b',
  'C:/a',
  'https://x/a',
  'a b',
  'é'
]) {
  if (path.test(bad)) {
    fail(`path grammar: admits ${JSON.stringify(bad)}`)
  }
}

/**
 * Read the artifact an entry names, when its path names a JSON file.
 *
 * @param entry - The entry.
 * @returns The artifact's value, or undefined.
 */
function named(entry) {
  if (typeof entry?.path !== 'string' || !path.test(entry.path)) {
    return undefined
  }
  const file = resolve(root, entry.path)
  if (!existsSync(file)) {
    return undefined
  }
  return readJson(file, entry.path).value
}

/**
 * The semantic digest of a value, or undefined when the framing leaves the
 * value out, as it leaves out every number.
 *
 * @param value - The value.
 * @returns The digest, or undefined.
 */
function semantic(value) {
  try {
    return semanticDigest(SEMANTIC_DOMAIN, value)
  } catch {
    return undefined
  }
}

/** Report every disagreement and exit, or return when there is none. */
function finish() {
  if (failures.length > 0) {
    for (const message of failures) {
      console.error(message)
    }
    process.exit(1)
  }
}

const schema = artifact(SCHEMA_PATH)
const index = artifact(INDEX_PATH)
if (schema.value === undefined || index.value === undefined) {
  finish()
}

// The schema closes every object of the index and gives each value its form.
const token = { type: 'string', pattern: TOKEN_PATTERN }
const digestForm = { type: 'string', pattern: DIGEST_PATTERN }
const forms = {
  identity: token,
  revision: token,
  suiteRevision: token,
  path: { type: 'string', pattern: PATH_PATTERN },
  digest: digestForm,
  canonicalDigest: digestForm,
  semanticDigest: digestForm,
  dialect: { type: 'string', minLength: 1 }
}
/** The keywords of a closed object, and nothing that could reopen it. */
const CLOSED = ['type', 'additionalProperties', 'required', 'properties']
let objects = 0
/**
 * Check one object schema of the index and the object schemas below it.
 *
 * @param node - The object schema.
 * @param names - The members the index gives the object, in order.
 * @param where - Where the object is, for messages.
 */
function closes(node, names, where) {
  objects += 1
  if (
    !isDeepStrictEqual(Object.keys(node ?? {}), CLOSED) ||
    node.type !== 'object' ||
    node.additionalProperties !== false ||
    !isDeepStrictEqual(node.required, names) ||
    !isDeepStrictEqual(Object.keys(node.properties ?? {}), names)
  ) {
    fail(`schema: ${where} is not a closed object of ${names.join(', ')}`)
    return
  }
  for (const name of names) {
    if (forms[name] !== undefined && !isDeepStrictEqual(node.properties[name], forms[name])) {
      fail(`schema: ${where}/${name} is not ${JSON.stringify(forms[name])}`)
    }
  }
}
if (schema.value.$schema !== DRAFT_7) {
  fail('schema: not a Draft 7 schema')
}
const { $schema: _, ...top } = schema.value
closes(top, MEMBERS, '#')
for (const [member, shape] of Object.entries(SHAPES)) {
  const node = top.properties?.[member]
  closes(node, shape.members, member)
  if (shape.roles === undefined) {
    continue
  }
  const roles = node?.properties?.members
  if (member === 'configurationSchemaAuthoritySet') {
    closes(
      node?.properties?.profileSpecification,
      ['identity', 'revision', 'semanticDigest'],
      `${member}/profileSpecification`
    )
    if (
      !isDeepStrictEqual(Object.keys(roles ?? {}), [
        'type',
        'minProperties',
        'propertyNames',
        'additionalProperties'
      ]) ||
      roles.type !== 'object' ||
      roles.minProperties !== 1 ||
      !isDeepStrictEqual(roles.propertyNames, { pattern: TOKEN_PATTERN })
    ) {
      fail(`schema: ${member}/members is not a map from schema versions`)
    }
    closes(roles?.additionalProperties, shape.role, `${member}/members/*`)
    continue
  }
  closes(roles, shape.roles, `${member}/members`)
  for (const role of shape.roles) {
    closes(roles?.properties?.[role], shape.role, `${member}/members/${role}`)
  }
}

for (const [member, [, idPattern]] of Object.entries(ARRAYS)) {
  const node = top.properties?.[member]
  if (!isDeepStrictEqual(Object.keys(node ?? {}), ['type', 'items']) || node.type !== 'array') {
    fail(`schema: ${member} is not an array of references`)
    continue
  }
  closes(node.items, ['id', 'path', 'digest'], `${member}/*`)
  if (!isDeepStrictEqual(node.items?.properties?.id, { type: 'string', pattern: idPattern })) {
    fail(`schema: ${member}/*/id is not the ID form of its kind`)
  }
}

// The index holds exactly its own copy's members, roles, identities, and
// revisions, and every path names a file with the declared digest.
if (!isDeepStrictEqual(Object.keys(index.value), MEMBERS)) {
  fail(`index: members are not ${MEMBERS.join(', ')}`)
}
if (index.value.suiteRevision !== REVISION) {
  fail('index: the suite revision is not "0"')
}
let bodies = 0
/**
 * Check that a path is suite-root-relative and names a file with the digest.
 *
 * @param entry - The entry that names it.
 * @param where - Where the entry is, for messages.
 */
function names(entry, where) {
  if (typeof entry?.path !== 'string' || !path.test(entry.path)) {
    fail(`index: ${where} has no suite-root-relative path`)
    return
  }
  const file = resolve(root, entry.path)
  if (!existsSync(file)) {
    fail(`index: ${where} names ${entry.path}, which is missing`)
    return
  }
  bodies += 1
  const bytes = readBytes(file, entry.path)
  if (bytes !== undefined && fileDigest(bytes) !== entry.digest) {
    fail(`index: ${entry.path} does not have the digest ${where} declares`)
  }
}
for (const [member, shape] of Object.entries(SHAPES)) {
  const entry = index.value[member]
  if (!isDeepStrictEqual(Object.keys(entry ?? {}), shape.members)) {
    fail(`index: ${member} does not hold ${shape.members.join(', ')}`)
  }
  if (entry?.identity !== shape.identity || entry?.revision !== REVISION) {
    fail(`index: ${member} is not ${shape.identity} at revision "0"`)
  }
  names(entry, member)
  if (shape.roles === undefined) {
    continue
  }
  if (!isDeepStrictEqual(Object.keys(entry?.members ?? {}), shape.roles)) {
    fail(`index: ${member}'s roles are not ${shape.roles.join(', ')} in order`)
  }
  for (const role of shape.roles) {
    const roleEntry = entry?.members?.[role]
    if (!isDeepStrictEqual(Object.keys(roleEntry ?? {}), shape.role)) {
      fail(`index: ${member}/${role} does not hold ${shape.role.join(', ')}`)
    }
    names(roleEntry, `${member}/${role}`)
  }
}
if (
  !isDeepStrictEqual(index.value.suiteIndexSchema, {
    identity: SHAPES.suiteIndexSchema.identity,
    revision: REVISION,
    path: SCHEMA_PATH,
    digest: fileDigest(schema.bytes)
  })
) {
  fail('index: suiteIndexSchema is not the schema at its fixed path')
}
const registryMembers = index.value.registryPackage?.members ?? {}
for (const role of REGISTRY_ROLES) {
  if (
    registryMembers[role]?.identity !== `intlify-project-profile-${role}` ||
    registryMembers[role]?.revision !== REVISION
  ) {
    fail(`index: registryPackage/${role} is not intlify-project-profile-${role} at revision "0"`)
  }
}
const mapped = index.value.configurationSchemaAuthoritySet?.members?.['0']
if (
  mapped?.identity !== 'intlify-configuration-schema' ||
  mapped?.revision !== REVISION ||
  mapped?.dialect !== DRAFT_7
) {
  fail('index: the mapped schema is not intlify-configuration-schema at revision "0" in Draft 7')
}

// Each manifest is listed once, each array in ascending unsigned byte order,
// and has the ID that lists it and the kind of its array.
const seen = new Set()
let manifests = 0
for (const [member, [kind, , id]] of Object.entries(ARRAYS)) {
  const listed = index.value[member]
  if (!Array.isArray(listed)) {
    fail(`index: ${member} is not an array`)
    continue
  }
  let previous
  for (const entry of listed) {
    if (
      !isDeepStrictEqual(Object.keys(entry ?? {}), ['id', 'path', 'digest']) ||
      typeof entry.id !== 'string' ||
      !id.test(entry.id)
    ) {
      fail(`index: ${member} lists an entry that is no reference of its kind`)
      continue
    }
    if (
      previous !== undefined &&
      Buffer.compare(Buffer.from(previous), Buffer.from(entry.id)) >= 0
    ) {
      fail(`index: ${member} lists ${entry.id} out of ascending byte order`)
    }
    if (seen.has(entry.id)) {
      fail(`index: ${entry.id} is listed twice`)
    }
    seen.add(entry.id)
    previous = entry.id
    manifests += 1
    names(entry, `${member}/${entry.id}`)
    const manifest = named(entry)
    if (manifest?.id !== entry.id || manifest?.manifestKind !== kind) {
      fail(`index: ${entry.path} is not the ${kind} manifest ${entry.id}`)
    }
  }
}

// Each construction case's fixture, and every body the fixture references
// outside a decoded JSON value, have the digests declared for them. A failing
// case names one construction rule, a row of the construction-admission body,
// and expects that row's reason, with its bound only for the bounds' row; the
// constructing case traces every row and expects the identity of the
// committed construction vector.
/**
 * Every `{ path, digest }` reference outside a decoded JSON value.
 *
 * @param value - A fixture or a part of one.
 * @returns The references, in document order.
 */
function references(value) {
  if (Array.isArray(value)) {
    return value.flatMap(references)
  }
  if (value === null || typeof value !== 'object') {
    return []
  }
  const keys = Object.keys(value)
  if (isDeepStrictEqual(keys, ['json'])) {
    return []
  }
  if (keys.length === 2 && typeof value.path === 'string' && typeof value.digest === 'string') {
    return [value]
  }
  return Object.values(value).flatMap(references)
}
/**
 * Check that a reference names a file below the root with its digest.
 *
 * @param reference - The `{ path, digest }` reference.
 * @param where - What holds it, for messages.
 */
function referenced(reference, where) {
  const file = typeof reference?.path === 'string' && path.test(reference.path)
  if (!file || !existsSync(resolve(root, reference.path))) {
    fail(`${where}: references ${String(reference?.path)}, which is missing`)
    return
  }
  const bytes = readBytes(resolve(root, reference.path), reference.path)
  if (bytes !== undefined && fileDigest(bytes) !== reference.digest) {
    fail(`${where}: ${reference.path} does not have the digest it declares`)
  }
}
const admissionBody = named(
  index.value.resolverConstructionRootPackage?.members?.['construction-admission-specification']
)
const precedence = admissionBody?.failurePrecedence ?? []
const ruleOf = id => precedence.find(row => `015.construction.${row.component}.${row.check}` === id)
const vectorPath = 'construction/construction-vectors-v0.json'
const vectorIdentity = readJson(resolve(root, vectorPath), vectorPath).value?.constructionIdentity
  ?.text
let fixtures = 0
for (const entry of index.value.resolverConstructionCases ?? []) {
  const manifest = named(entry)
  const reference = manifest?.constructionInputFixture
  referenced(reference, entry.path)
  const fixture = named(reference)
  if (fixture === undefined) {
    continue
  }
  fixtures += 1
  for (const body of references(fixture)) {
    referenced(body, reference.path)
  }
  const { expected, traceability } = manifest
  const rules = traceability?.ruleIds ?? []
  if (expected?.constructed === true) {
    if (
      expected.resolverConstructionIdentity !== vectorIdentity ||
      rules.length !== precedence.length ||
      !rules.every(ruleOf)
    ) {
      fail(`${entry.path}: does not expect the vector's identity under every rule`)
    }
    continue
  }
  const row = rules.length === 1 ? ruleOf(rules[0]) : undefined
  if (
    row === undefined ||
    expected?.failureReason !== row.reason ||
    (row.check === 'construction-bounds') !== Object.hasOwn(expected, 'failureDetails')
  ) {
    fail(`${entry.path}: does not expect what its rule's row refuses with`)
  }
}

// Each input admission case of a boundary kind names the constructing case
// with the committed vector's identity, and its fixture and every body the
// fixture references have their digests. Each kind has one admitted case,
// which traces each pair of the kind and expects the normalized input this
// script derives from the fixture, and one case for each row of the kind's
// failure precedence in the Case Schema, which its ID names and whose reason
// and pair it expects.
const caseSchemaBody = named(index.value.caseSchema)
const boundaryPrecedence = Object.fromEntries(
  Object.entries(caseSchemaBody?.failurePrecedence ?? {}).filter(([, rows]) => Array.isArray(rows))
)
const SHORT = {
  'profile-resolution-specification-input': 'specification',
  'intent-surface-class-vocabulary-artifact': 'vocabulary',
  'locale-canonicalization-data-artifact': 'locale-data',
  'submitted-profile-resolution-artifact-collection': 'collection',
  'resource-limit-policy-verification-input': 'verification',
  'safe-origin-kind': 'origin',
  'external-profile-selector-binding': 'selector',
  'file-source-identity-revision': 'file-source',
  'programmatic-entry-identity-revision': 'entry-source',
  'programmatic-entry-snapshot': 'snapshot',
  'programmatic-call-site-evidence-input': 'call-site',
  'implementation-capability': 'capability',
  'resolver-construction-identity-reference': 'construction-identity'
}

/**
 * The ID of the case for a row of a kind's precedence: the row's check, and
 * its reason when the check repeats in the precedence.
 *
 * @param kind - The boundary kind.
 * @param row - The row.
 * @param rows - The kind's precedence.
 * @returns The case ID.
 */
const rowCaseId = (kind, row, rows) =>
  rows.filter(other => other.check === row.check).length > 1
    ? `015-admission-${SHORT[kind]}-${row.check}-${row.reason}`
    : `015-admission-${SHORT[kind]}-${row.check}`

/**
 * Order two strings by their UTF-8 bytes.
 *
 * @param left - One string.
 * @param right - The other string.
 * @returns A negative number, zero, or a positive number.
 */
const byBytes = (left, right) => Buffer.compare(Buffer.from(left), Buffer.from(right))

/**
 * A value with every object's members in ascending unsigned byte order of
 * their names.
 *
 * @param value - The value.
 * @returns The value with its objects' members in that order.
 */
function sortedValue(value) {
  if (Array.isArray(value)) {
    return value.map(sortedValue)
  }
  if (value === null || typeof value !== 'object') {
    return value
  }
  const names = Object.keys(value).sort(byBytes)
  return Object.fromEntries(names.map(name => [name, sortedValue(value[name])]))
}

/**
 * The bytes of a raw input: its hexadecimal, or the file it references.
 *
 * @param raw - The raw input, `{ bytes }` or `{ path, digest }`.
 * @returns The bytes, empty when the file cannot be read.
 */
const rawBytes = raw =>
  typeof raw?.bytes === 'string'
    ? Buffer.from(raw.bytes, 'hex')
    : (readBytes(resolve(root, String(raw?.path)), String(raw?.path)) ?? Buffer.alloc(0))

/**
 * The text of bytes that are UTF-8, as the boundaries read them: a leading
 * byte order mark stays in the text.
 *
 * @param bytes - The bytes.
 * @returns The text, or undefined when the bytes are not UTF-8.
 */
function utf8(bytes) {
  try {
    return new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes)
  } catch {
    return undefined
  }
}

/**
 * The value of a raw input that is strict UTF-8 JSON.
 *
 * @param raw - The raw input.
 * @returns The value, or undefined when the bytes are not UTF-8 or not
 *   JSON, or repeat a member.
 */
function rawValue(raw) {
  const text = utf8(rawBytes(raw))
  try {
    return text === undefined || repeatsMember(text) ? undefined : JSON.parse(text)
  } catch {
    return undefined
  }
}

/**
 * A content-derived revision: SHA-256 over the domain, then one framed field
 * of the bytes.
 *
 * @param domain - The revision domain.
 * @param bytes - The content.
 * @returns `sha256:` and 64 lowercase hexadecimal digits.
 */
function contentRevision(domain, bytes) {
  const length = Buffer.alloc(8)
  length.writeBigUInt64BE(BigInt(bytes.length))
  const preimage = Buffer.concat([Buffer.from(String(domain)), Buffer.from([1]), length, bytes])
  return `sha256:${createHash('sha256').update(preimage).digest('hex')}`
}
const domains = named(index.value.profileSpecification)?.revisionDomains ?? {}
// The construction cases that construct, which every input admission case
// and Resolver Case must name (D15:3158, 3219).
const constructionIds = new Set(
  (index.value.resolverConstructionCases ?? [])
    .filter(entry => named(entry)?.expected?.constructed === true)
    .map(entry => entry.id)
)
const json = value => ({ json: sortedValue(value) })

/**
 * The admitted pair of a fixture's identity, mark, and revision.
 *
 * @param input - The fixture's input.
 * @returns The pair as the boundary admits it.
 */
const pairOf = input => ({
  identity: input?.identity,
  nonSecret: input?.nonSecret,
  revision: input?.revision
})
const snapshotVectorsPath = resolve(crate, 'fixtures/snapshot/vectors.json')
const snapshotVectors =
  readJson(snapshotVectorsPath, 'fixtures/snapshot/vectors.json').value?.snapshots ?? []

/**
 * A Snapshot node in the vectors' notation, which names a terminal marker's
 * bound path `bound`.
 *
 * @param node - The node in the Case Schema's notation.
 * @returns The node in the vectors' notation.
 */
function vectorNotation(node) {
  if (Array.isArray(node)) {
    return node.map(vectorNotation)
  }
  if (node === null || typeof node !== 'object') {
    return node
  }
  return Object.fromEntries(
    Object.entries(node).map(([name, value]) => [
      name === 'boundPath' ? 'bound' : name,
      vectorNotation(value)
    ])
  )
}

/**
 * Whether a fixture's Snapshot is a committed vector, with that vector's
 * digest.
 *
 * @param snapshot - The fixture's Snapshot.
 * @returns Whether it is.
 */
const committedSnapshot = snapshot =>
  snapshotVectors.some(
    vector =>
      vector.digest === snapshot?.digest &&
      isDeepStrictEqual(vector.snapshot, vectorNotation(snapshot?.snapshot))
  )
const ROLES = ['node', 'member-key', 'member-value']

/**
 * A path edge as admitted: code units as the key they spell, and an index as
 * a number.
 *
 * @param edge - The fixture's edge.
 * @returns The admitted edge.
 */
const admittedEdge = edge =>
  Array.isArray(edge.keyUtf16)
    ? { key: String.fromCharCode(...edge.keyUtf16) }
    : 'index' in edge
      ? { index: edge.index === 0 ? 0 : edge.index }
      : { key: edge.key }

/**
 * Order two admitted edges at one position: a key before an index, keys by
 * their bytes, and indexes by value.
 *
 * @param left - One edge.
 * @param right - The other edge.
 * @returns A negative number, zero, or a positive number.
 */
function compareEdges(left, right) {
  if ('key' in left !== 'key' in right) {
    return 'key' in left ? -1 : 1
  }
  return 'key' in left ? byBytes(left.key, right.key) : left.index - right.index
}

/**
 * Order two admitted mappings canonically: by path, edge by edge, with a
 * path that is a prefix of the other first, then by role.
 *
 * @param left - One mapping.
 * @param right - The other mapping.
 * @returns A negative number, zero, or a positive number.
 */
function canonicalMappings(left, right) {
  for (let at = 0; at < Math.min(left.path.length, right.path.length); at += 1) {
    const order = compareEdges(left.path[at], right.path[at])
    if (order !== 0) {
      return order
    }
  }
  return (
    left.path.length - right.path.length || ROLES.indexOf(left.role) - ROLES.indexOf(right.role)
  )
}

/**
 * The admitted sidecar of a fixture's call-site input.
 *
 * @param input - The fixture's input.
 * @returns The sidecar, or undefined when its source is not UTF-8 or its
 *   revision is not the source's.
 */
function callSite(input) {
  const bytes = rawBytes(input?.source)
  const source = utf8(bytes)
  if (source === undefined || input.revision !== contentRevision(domains.callSiteSource, bytes)) {
    return undefined
  }
  const value = {
    identity: input.identity,
    nonSecret: input.nonSecret,
    revision: input.revision,
    source,
    mappings: (input.mappings ?? [])
      .map(mapping => ({
        path: mapping.path.map(admittedEdge),
        role: mapping.role,
        span: mapping.span
      }))
      .sort(canonicalMappings)
  }
  if (input.locator !== undefined) {
    value.locator = { [input.locator.variant]: input.locator.payload }
  }
  return json(value)
}

/**
 * The admitted locale data of a fixture: the manifest, and the payloads at
 * the addresses it names.
 *
 * @param input - The fixture's input.
 * @returns The locale data.
 */
function localeData(input) {
  const manifest = rawValue(input?.manifest)
  const named = new Set((manifest?.payloads ?? []).map(payload => payload.address))
  const payloads = Object.fromEntries(
    Object.entries(input?.payloads ?? {}).filter(([address]) => named.has(address))
  )
  return json({ manifest, payloads })
}

/** What each kind's admitted case expects of its fixture, or undefined. */
const NORMALIZED = {
  'profile-resolution-specification-input': fixture => json(rawValue(fixture.input)),
  'intent-surface-class-vocabulary-artifact': fixture => json(rawValue(fixture.input)),
  'resource-limit-policy-verification-input': fixture => json(rawValue(fixture.input)),
  'locale-canonicalization-data-artifact': fixture => localeData(fixture.input),
  'submitted-profile-resolution-artifact-collection': fixture => ({
    members: (fixture.input?.members ?? []).map(member => ({ json: rawValue(member) }))
  }),
  'programmatic-entry-identity-revision': fixture =>
    fixture.input?.revision === fixture.input?.snapshot?.digest &&
    committedSnapshot(fixture.input?.snapshot)
      ? pairOf(fixture.input)
      : undefined,
  'programmatic-entry-snapshot': fixture =>
    committedSnapshot(fixture.input) ? fixture.input : undefined,
  'programmatic-call-site-evidence-input': fixture => callSite(fixture.input),
  'implementation-capability': fixture => ({ json: fixture.input?.json }),
  'safe-origin-kind': fixture =>
    fixture.candidate === fixture.slot ? { safeOriginKind: fixture.slot } : undefined,
  'external-profile-selector-binding': fixture => ({ selector: fixture.input }),
  'file-source-identity-revision': fixture =>
    fixture.input?.revision === contentRevision(domains.fileSource, rawBytes(fixture.input?.bytes))
      ? pairOf(fixture.input)
      : undefined,
  'resolver-construction-identity-reference': fixture =>
    fixture.input?.json === vectorIdentity
      ? { resolverConstructionIdentity: vectorIdentity }
      : undefined
}
// An admission step's case (D15:3164-3168). An admitted one expects each
// slot's normalized input, which this script derives from the fixture as the
// slot's boundary kind would: the common step's through the kinds' own
// derivations; an entry's from its raw bytes as hexadecimal, its pair, its
// locator, which a file entry admits as a canonical `relative-path` and a
// programmatic one as a non-empty `safe-label`, each marked non-secret and
// within the capability's locator bound, its selector, its Snapshot, and its
// call-site sidecar or its absence. A refused one expects its failures in
// slot rank, an origin's first and then the slot's values in the boundary
// table's order, at most one for each boundary of a slot; a refused
// capability leaves no other value of the common step evaluated, and a
// refused Snapshot leaves the identity and revision unevaluated. A refused
// common step has a failure, while a refused entry has none when only its
// capability is refused.
const STEPS = ['common-invocation-input-set', 'entry-admission-input-set']
const ORIGIN = 'safe-origin-kind'
const COMMON_SLOTS = [
  ['profile-resolution-specification-input', 'profile-resolution-specification-input'],
  ['intent-surface-class-vocabulary', 'intent-surface-class-vocabulary-artifact'],
  ['locale-canonicalization-data', 'locale-canonicalization-data-artifact'],
  ['profile-resolution-artifact-collection', 'submitted-profile-resolution-artifact-collection'],
  ['resource-limit-policy-verification', 'resource-limit-policy-verification-input'],
  ['implementation-capability', 'implementation-capability']
]
/** Each step form's slots in rank order, each with its value boundaries in the table's order. */
const STEP_FORMS = {
  common: COMMON_SLOTS.map(([slot, kind]) => [slot, [kind]]),
  'entry-file': [
    ['configuration-file', ['file-source-identity-revision']],
    ['profile-selector-cli', ['external-profile-selector-binding']]
  ],
  'entry-programmatic': [
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
}
const originRows = named(registryMembers['safe-origin-kind-registry'])?.rows ?? []
const ranks = new Map(originRows.map(row => [row.id, Number(row.rank)]))

/**
 * Whether a payload is a canonical `relative-path`: no drive, no NUL or
 * backslash, and no empty, `.`, or `..` segment.
 *
 * @param payload - The payload.
 * @returns Whether it is.
 */
function relativePath(payload) {
  return (
    !/^[a-z]:/i.test(payload) &&
    !payload.includes('\0') &&
    !payload.includes('\\') &&
    payload.split('/').every(segment => !['', '.', '..'].includes(segment))
  )
}

/**
 * An entry's admitted locator: the candidate, marked non-secret and within
 * the capability's locator bound, of a variant and payload the entry admits.
 *
 * @param candidate - The locator candidate, or undefined.
 * @param capability - The capability value that bounds it.
 * @param admits - Whether the entry admits a variant and its payload.
 * @returns The locator's canonical form, or null for absence.
 */
function locatorOf(candidate, capability, admits) {
  const bound = Number(capability?.bootstrapOnly?.evidenceMetadata?.maxLocatorBytes)
  return candidate?.nonSecret === true &&
    Buffer.byteLength(String(candidate.payload)) <= bound &&
    admits(candidate.variant, String(candidate.payload))
    ? { [candidate.variant]: candidate.payload }
    : null
}

/**
 * The common step's admitted set: each slot's normalized input, derived as
 * its boundary kind's admitted case is.
 *
 * @param fixture - The step's fixture.
 * @returns The set, or undefined.
 */
function commonNormalized(fixture) {
  const set = {}
  for (const [slot, kind] of COMMON_SLOTS) {
    const value =
      fixture[slot]?.candidate === slot
        ? NORMALIZED[kind]?.({ assertion: fixture.assertion, input: fixture[slot]?.input })
        : undefined
    if (value === undefined) {
      return undefined
    }
    set[slot] = value
  }
  return set
}

/**
 * An entry step's admitted set: a file's raw bytes as hexadecimal, pair, and
 * locator, or a programmatic entry's Snapshot, pair, locator, and call-site
 * sidecar or its absence; and the selector's report.
 *
 * @param fixture - The step's fixture.
 * @returns The set, or undefined. A `safe-uri` locator is not derived here.
 */
function entryNormalized(fixture) {
  const entry = fixture.entry ?? {}
  const capability = fixture.capability?.json
  const file = entry['configuration-file']
  if (file !== undefined) {
    const cli = entry['profile-selector-cli']
    const input = file.input ?? {}
    const pair = NORMALIZED['file-source-identity-revision']({ input })
    if (
      file.candidate !== 'configuration-file' ||
      cli?.candidate !== 'profile-selector-cli' ||
      pair === undefined
    ) {
      return undefined
    }
    return {
      'configuration-file': {
        bytes: { bytes: rawBytes(input.bytes).toString('hex') },
        ...pair,
        locator: locatorOf(
          input.locator,
          capability,
          (variant, payload) => variant === 'relative-path' && relativePath(payload)
        )
      },
      'profile-selector-cli': { selector: cli.input }
    }
  }
  const programmatic = entry['configuration-programmatic']
  const selector = entry['profile-selector-programmatic']
  const input = programmatic?.input ?? {}
  const sidecar = input.callSite === undefined ? null : callSite(input.callSite)
  if (
    programmatic?.candidate !== 'configuration-programmatic' ||
    selector?.candidate !== 'profile-selector-programmatic' ||
    input.locator?.variant === 'safe-uri' ||
    !committedSnapshot(input.snapshot) ||
    input.revision !== input.snapshot?.digest ||
    sidecar === undefined
  ) {
    return undefined
  }
  return {
    'configuration-programmatic': {
      snapshot: input.snapshot.snapshot,
      digest: input.snapshot.digest,
      ...pairOf(input),
      locator: locatorOf(
        input.locator,
        capability,
        (variant, payload) => variant === 'safe-label' && payload !== ''
      ),
      callSite: sidecar
    },
    'profile-selector-programmatic': { selector: selector.input }
  }
}

/**
 * What is wrong with a refused step case's failures, given its form's slots.
 *
 * @param form - The step form's name.
 * @param failures - The failures the case expects.
 * @returns The first wrong thing, or undefined.
 */
function stepFailuresWrong(form, failures) {
  const slots = new Map(STEP_FORMS[form])
  let previous = [-1, -1]
  const seen = new Set()
  for (const failure of failures) {
    const values = slots.get(failure?.slot)
    const index = values?.indexOf(failure?.boundaryKind) ?? -1
    const position = failure?.boundaryKind === ORIGIN ? 0 : index < 0 ? -1 : 1 + index
    const at = [ranks.get(failure?.slot) ?? -1, position]
    if (values === undefined || position < 0 || at[0] < 0) {
      return 'names a slot or a boundary outside its step'
    }
    if (at[0] < previous[0] || (at[0] === previous[0] && at[1] <= previous[1])) {
      return 'does not list its failures in slot rank, an origin first'
    }
    previous = at
    seen.add(`${failure.slot} ${failure.boundaryKind}`)
  }
  const has = (slot, kind) => seen.has(`${slot} ${kind}`)
  if (
    has('implementation-capability', 'implementation-capability') &&
    failures.some(
      failure => failure.slot !== 'implementation-capability' && failure.boundaryKind !== ORIGIN
    )
  ) {
    return 'evaluates a value after a refused capability'
  }
  if (
    has('configuration-programmatic', 'programmatic-entry-snapshot') &&
    has('configuration-programmatic', 'programmatic-entry-identity-revision')
  ) {
    return 'evaluates the identity and revision after a refused Snapshot'
  }
  if (form === 'common' && failures.length === 0) {
    return 'refuses the common step without a failure'
  }
  return undefined
}

let admissionFixtures = 0
let stepCases = 0
const stepForms = new Map()
const admissionIds = new Map()
for (const entry of index.value.resolverInputAdmissionCases ?? []) {
  const manifest = named(entry)
  const reference = manifest?.inputFixture
  referenced(reference, entry.path)
  const fixture = named(reference)
  if (fixture === undefined) {
    continue
  }
  admissionFixtures += 1
  for (const body of references(fixture)) {
    referenced(body, reference.path)
  }
  const { boundaryKind: kind, resolverConstruction, expected, traceability } = manifest
  if (
    !constructionIds.has(resolverConstruction?.caseId) ||
    resolverConstruction.identityReference !== vectorIdentity
  ) {
    fail(`${entry.path}: does not name a constructing case with the vector's identity`)
  }
  if (STEPS.includes(kind)) {
    stepCases += 1
    const form =
      kind === STEPS[0]
        ? 'common'
        : fixture.entry?.['configuration-file'] === undefined
          ? 'entry-programmatic'
          : 'entry-file'
    const pairs = list => [
      ...new Set(list.map(([boundary, reason]) => `015.input-admission.${boundary}.${reason}`))
    ]
    const boundaries = [ORIGIN, ...STEP_FORMS[form].flatMap(([, values]) => values)]
    const own = stepForms.get(form) ?? { admitted: 0, refused: 0 }
    stepForms.set(form, own)
    const rules = traceability?.ruleIds ?? []
    if (!entry.id.startsWith(`015-admission-${form}-`)) {
      fail(`${entry.path}: does not name its step's form`)
    }
    if (expected?.admitted === true) {
      own.admitted += 1
      const normalized = form === 'common' ? commonNormalized(fixture) : entryNormalized(fixture)
      const all = pairs(
        boundaries.flatMap(boundary =>
          (boundaryPrecedence[boundary] ?? []).map(row => [boundary, row.reason])
        )
      )
      if (
        normalized === undefined ||
        !isDeepStrictEqual(rules, all) ||
        JSON.stringify(expected.normalizedInput) !== JSON.stringify(normalized)
      ) {
        fail(`${entry.path}: does not expect its fixture's admitted set under each pair`)
      }
      continue
    }
    own.refused += 1
    const failures = Array.isArray(expected?.failures) ? expected.failures : []
    const wrong = stepFailuresWrong(form, failures)
    if (wrong !== undefined) {
      fail(`${entry.path}: ${wrong}`)
    }
    if (
      !isDeepStrictEqual(
        rules,
        pairs(failures.map(failure => [failure.boundaryKind, failure.failureReason]))
      )
    ) {
      fail(`${entry.path}: does not trace the pair of each of its failures`)
    }
    continue
  }
  const rows = boundaryPrecedence[kind]
  if (rows === undefined) {
    fail(`${entry.path}: names no boundary kind with a failure precedence`)
    continue
  }
  admissionIds.set(kind, [...(admissionIds.get(kind) ?? []), entry.id])
  const rules = traceability?.ruleIds ?? []
  const pair = reason => `015.input-admission.${kind}.${reason}`
  if (expected?.admitted === true) {
    const normalized = NORMALIZED[kind]?.(fixture)
    if (
      entry.id !== `015-admission-${SHORT[kind]}-admitted` ||
      !isDeepStrictEqual(rules, [...new Set(rows.map(row => pair(row.reason)))]) ||
      normalized === undefined ||
      JSON.stringify(expected.normalizedInput) !== JSON.stringify(normalized)
    ) {
      fail(`${entry.path}: does not expect its fixture's normalized input under each pair`)
    }
    continue
  }
  const row = rows.find(own => rowCaseId(kind, own, rows) === entry.id)
  if (
    row === undefined ||
    expected?.failureReason !== row.reason ||
    !isDeepStrictEqual(rules, [pair(row.reason)])
  ) {
    fail(`${entry.path}: does not expect what the row its ID names refuses with`)
  }
}
for (const [kind, rows] of Object.entries(boundaryPrecedence)) {
  const ids = [
    `015-admission-${SHORT[kind]}-admitted`,
    ...rows.map(row => rowCaseId(kind, row, rows))
  ].sort(byBytes)
  if (!isDeepStrictEqual([...(admissionIds.get(kind) ?? [])].sort(byBytes), ids)) {
    fail(`index: ${kind} does not have its admitted case and one case for each row`)
  }
}
for (const form of Object.keys(STEP_FORMS)) {
  const own = stepForms.get(form)
  if (!(own?.admitted > 0 && own?.refused > 0)) {
    fail(`index: the ${form} step does not have an admitted case and a refused one`)
  }
}

// Each Resolver Case names the constructing case with the committed vector's
// identity, and its fixtures and every body they reference have their
// digests (D15:3174-3216); the locale data its common input submits is the
// committed ICU4X artifact. Each expected outcome names that identity; it is
// checked only with a complete bundle, Shared evidence, and no blocking
// Finding, and is otherwise blocked, without a profile or bindings
// (D15:1153-1158, 1186); it lists one terminal state for each exhausted
// domain, Entry first, exactly when its bundle is incomplete; its Shared
// evidence is absent exactly when it has causes, which are its Entry cause
// records in order; only an exhausted domain's terminal Finding and, when
// Entry exhausts without Shared evidence, the reserve's cause have no owner,
// each last in its array and naming the terminal state's cause
// (D15:3271-3303); and its bindings are in ascending unsigned byte order of
// their keys' framing. A case traces each check whose Finding its expected
// outcomes report, in byte order, under that check's rule; a file-only case
// expects no Shared evidence for its file entry (D15:3235); and paired
// entries carry one identity and, when both carry Shared evidence, one
// Shared projection and Shared terminal state, and when both bundles are
// also complete, one outcome, profile, and bindings (D15:3233).
const checkRegistry = named(registryMembers['check-registry'])
const ruleOfCheck = new Map((checkRegistry?.rows ?? []).map(row => [row.checkId, row.ruleId]))
const icu4xData = resolve(crate, '../intlify_locale_icu4x/data')
const dataAddresses = (
  readJson(resolve(icu4xData, 'manifest.json'), 'manifest.json').value?.payloads ?? []
)
  .map(payload => payload.address)
  .toSorted(byBytes)
// Each side's own adapter, by the slot its entry fixture submits through.
const SOURCES = { file: 'configuration-file', programmatic: 'configuration-programmatic' }
const SELECTORS = { file: 'profile-selector-cli', programmatic: 'profile-selector-programmatic' }

/**
 * The value a Snapshot node in the Case Schema's notation spells, or
 * undefined for a refused node.
 *
 * @param node - The node.
 * @returns The value.
 */
function snapshotValue(node) {
  const [[kind, value]] = Object.entries(node ?? { refused: null })
  switch (kind) {
    case 'null':
      return null
    case 'boolean':
    case 'string':
      return value
    case 'number':
      return Number(value)
    case 'array':
      return value.map(snapshotValue)
    case 'object':
      return Object.fromEntries(value.map(([name, member]) => [name, snapshotValue(member)]))
    default:
      return undefined
  }
}

/**
 * The value an entry fixture submits, and its selector: the file's bytes read
 * as JSON, or the programmatic entry's Snapshot.
 *
 * @param input - The entry fixture.
 * @param side - Its side.
 * @returns The value and the selector, the value undefined when the bytes
 *   are no JSON.
 */
function submitted(input, side) {
  const source = input?.[SOURCES[side]]?.input
  let value
  if (side === 'file') {
    try {
      value = JSON.parse(utf8(rawBytes(source?.bytes)) ?? '')
    } catch {
      value = undefined
    }
  } else {
    value = snapshotValue(source?.snapshot?.snapshot)
  }
  return { value, selector: input?.[SELECTORS[side]]?.input }
}
const DOMAINS = ['entry', 'shared']
const SIDES = ['file', 'programmatic']
let resolverCases = 0
let expectedOutcomes = 0
let relations = 0
// Each listed Resolver Case by its ID: its path, construction case, expected
// outcomes by side, and relations.
const resolverListing = new Map()

/**
 * The Finding rows of an expected outcome's projections.
 *
 * @param outcome - The expected outcome.
 * @returns The rows, Entry's first.
 */
function findingRows(outcome) {
  const evidence = outcome?.resolutionEvidence ?? {}
  return DOMAINS.flatMap(domain => evidence[domain]?.findings ?? [])
}

/**
 * What an expected outcome breaks of 015's rules for an outcome, its
 * terminal states, causes, rows without an owner, and bindings.
 *
 * @param outcome - The expected outcome.
 * @returns The first rule it breaks, or undefined.
 */
function outcomeWrong(outcome) {
  const evidence = outcome?.resolutionEvidence
  if (evidence === null || typeof evidence !== 'object' || Array.isArray(evidence)) {
    return 'has no Resolution Evidence'
  }
  if (outcome.resolverConstructionIdentity !== vectorIdentity) {
    return "does not name the construction vector's identity"
  }
  const states = Array.isArray(evidence.terminalStates) ? evidence.terminalStates : []
  const exhausted = DOMAINS.filter(domain => states.some(state => state.domain === domain))
  if (
    !isDeepStrictEqual(
      states.map(state => state.domain),
      exhausted
    )
  ) {
    return 'does not list one terminal state for each exhausted domain, Entry first'
  }
  if (evidence.complete !== (states.length === 0)) {
    return 'is complete with a terminal state, or incomplete without one'
  }
  if (exhausted.some(domain => evidence[domain] === null || typeof evidence[domain] !== 'object')) {
    return 'has a terminal state for a domain without its projection'
  }
  const blocking = findingRows(outcome).some(row => row.record?.blocking === true)
  if (outcome.outcome === 'checked') {
    if (evidence.complete !== true || evidence.shared === null || blocking) {
      return 'is checked without a complete bundle, Shared evidence, and only non-blocking Findings'
    }
    if (outcome.profile === null || !Array.isArray(outcome.resolvedProfileArtifactBindings)) {
      return 'is checked without its profile and bindings'
    }
  } else if (outcome.profile !== null || outcome.resolvedProfileArtifactBindings !== null) {
    return 'is blocked with a profile or bindings'
  }
  const causes = (evidence.entry?.sharedUnavailabilityCauses ?? []).map(row => row.record?.cause)
  if (!isDeepStrictEqual(evidence.sharedUnavailableCauseKeys, causes)) {
    return 'lists cause keys other than its Entry cause records'
  }
  if ((evidence.shared === null) !== causes.length > 0) {
    return 'has both Shared evidence and causes, or neither'
  }
  for (const domain of DOMAINS) {
    const projection = evidence[domain]
    if (projection === null || typeof projection !== 'object') {
      continue
    }
    const cause = states.find(state => state.domain === domain)?.cause
    const reserved = {
      findings: cause,
      sharedUnavailabilityCauses: domain === 'entry' && evidence.shared === null ? cause : undefined
    }
    for (const [name, rows] of Object.entries(projection)) {
      const list = Array.isArray(rows) ? rows : []
      const key = reserved[name]
      const ownerless = list.flatMap((row, at) => (row?.owner === null ? [at] : []))
      if (!isDeepStrictEqual(ownerless, key === undefined ? [] : [list.length - 1])) {
        return `has a row without an owner in ${domain}.${name} other than its reserved last one`
      }
      const record = list.at(-1)?.record
      if (key !== undefined && !isDeepStrictEqual(record?.key ?? record?.cause, key)) {
        return `does not end ${domain}.${name} with the terminal state's cause`
      }
    }
  }
  const keys = (outcome.resolvedProfileArtifactBindings ?? []).map(row => encode(row.key))
  if (keys.some((key, at) => at > 0 && Buffer.compare(keys[at - 1], key) >= 0)) {
    return "does not list its bindings in ascending order of their keys' framing"
  }
  return undefined
}

for (const entry of index.value.resolverCases ?? []) {
  const manifest = named(entry)
  if (manifest === undefined) {
    continue
  }
  resolverCases += 1
  const { resolverConstruction, commonInvocationInputFixture: common, traceability } = manifest
  if (
    !constructionIds.has(resolverConstruction?.caseId) ||
    resolverConstruction.identityReference !== vectorIdentity
  ) {
    fail(`${entry.path}: does not name a constructing case with the vector's identity`)
  }
  referenced(common, entry.path)
  const commonFixture = named(common)
  for (const body of references(commonFixture ?? {})) {
    referenced(body, common.path)
  }
  // The manifest and a payload at each address the committed manifest
  // names, each the committed artifact's bytes for that address.
  const data = commonFixture?.['locale-canonicalization-data']?.input ?? {}
  const supplied = data.payloads ?? {}
  const sameBytes = (reference, name) => {
    const copy = resolve(root, String(reference?.path))
    return existsSync(copy) && readFileSync(resolve(icu4xData, name)).equals(readFileSync(copy))
  }
  if (
    !isDeepStrictEqual(Object.keys(supplied).toSorted(byBytes), dataAddresses) ||
    !sameBytes(data.manifest, 'manifest.json') ||
    dataAddresses.some(address => !sameBytes(supplied[address], address))
  ) {
    fail(`${entry.path}: does not submit the committed locale data at each of its addresses`)
  }
  const outcomes = {}
  const values = {}
  for (const side of SIDES) {
    const own = manifest.entries?.[side]
    if (own?.unpaired !== undefined) {
      continue
    }
    referenced(own?.entryAdmissionInputFixture, entry.path)
    const input = named(own?.entryAdmissionInputFixture)
    for (const body of references(input ?? {})) {
      referenced(body, own.entryAdmissionInputFixture.path)
    }
    if (input?.[SOURCES[side]] === undefined) {
      fail(`${entry.path}: its ${side} side does not submit through its own adapter`)
    }
    values[side] = submitted(input, side)
    referenced(own?.expectedOutcomeFixture, entry.path)
    const expected = named(own?.expectedOutcomeFixture)
    if (expected === undefined) {
      continue
    }
    expectedOutcomes += 1
    const wrong = outcomeWrong(expected)
    if (wrong !== undefined) {
      fail(`${own.expectedOutcomeFixture.path}: ${wrong}`)
    }
    outcomes[side] = expected
  }
  resolverListing.set(manifest.id, {
    path: entry.path,
    construction: resolverConstruction?.caseId,
    outcomes,
    relations: Array.isArray(manifest.relations) ? manifest.relations : []
  })
  const checks = [
    ...new Set(
      Object.values(outcomes).flatMap(outcome =>
        findingRows(outcome).map(row => row.record?.key?.check)
      )
    )
  ].sort(byBytes)
  if (
    !isDeepStrictEqual(traceability?.checkIds, checks) ||
    !isDeepStrictEqual(
      traceability?.ruleIds,
      checks.map(check => ruleOfCheck.get(check))
    )
  ) {
    fail(`${entry.path}: does not trace each check its expected outcomes report under its rule`)
  }
  const { file, programmatic } = outcomes
  if (
    manifest.entries?.programmatic?.unpaired !== undefined &&
    file?.resolutionEvidence?.shared !== null
  ) {
    fail(`${entry.path}: a file-only case expects Shared evidence for its file entry`)
  }
  if (file === undefined || programmatic === undefined) {
    continue
  }
  // Two entries that spell one value under one selector, beside the one
  // common input, reach one Materialized Resolution Input Set.
  const oneSet =
    values.file?.value !== undefined &&
    isDeepStrictEqual(values.file.value, values.programmatic?.value) &&
    isDeepStrictEqual(values.file.selector, values.programmatic?.selector)
  const sharedStates = outcome =>
    (outcome.resolutionEvidence?.terminalStates ?? []).filter(state => state.domain === 'shared')
  const bothComplete =
    file.resolutionEvidence?.complete === true && programmatic.resolutionEvidence?.complete === true
  if (
    file.resolverConstructionIdentity !== programmatic.resolverConstructionIdentity ||
    (oneSet &&
      (!isDeepStrictEqual(
        file.resolutionEvidence?.shared,
        programmatic.resolutionEvidence?.shared
      ) ||
        !isDeepStrictEqual(sharedStates(file), sharedStates(programmatic)))) ||
    (oneSet &&
      bothComplete &&
      ['outcome', 'profile', 'resolvedProfileArtifactBindings'].some(
        member => !isDeepStrictEqual(file[member], programmatic[member])
      ))
  ) {
    fail(`${entry.path}: its paired entries disagree where 015 makes them agree`)
  }
}

/**
 * Whether an expected outcome reached materialization, which its Shared
 * projection marks.
 *
 * @param outcome - The expected outcome.
 * @returns Whether it has Shared evidence.
 */
function materialized(outcome) {
  const shared = outcome?.resolutionEvidence?.shared
  return shared !== null && typeof shared === 'object'
}

// Each relation names a listed case under the same construction case that
// pairs one of the declaring case's sides (D15:3305). On each such side, a
// profile relation holds between two checked expected outcomes whose
// profile views are equal or not, and a materialized-input relation between
// two that reached materialization. Staleness and the sets' equality are
// left to the runner, which resolves both entries.
for (const own of resolverListing.values()) {
  for (const relation of own.relations) {
    relations += 1
    const other = resolverListing.get(relation?.case)
    const sides = SIDES.filter(
      side => own.outcomes[side] !== undefined && other?.outcomes[side] !== undefined
    )
    if (other === undefined || other.construction !== own.construction || sides.length === 0) {
      fail(
        `${own.path}: relates to ${String(relation?.case)}, which is no listed case under its construction case pairing one of its sides`
      )
      continue
    }
    for (const side of sides) {
      const earlier = other.outcomes[side]
      const later = own.outcomes[side]
      const checked = earlier.outcome === 'checked' && later.outcome === 'checked'
      const equal = isDeepStrictEqual(earlier.profile, later.profile)
      const both = materialized(earlier) && materialized(later)
      const holds = {
        'profile-equal': checked && equal,
        'profile-changed': checked && !equal,
        'materialized-input-equal': both,
        'materialized-input-changed': both
      }[relation.kind]
      if (holds === false) {
        fail(
          `${own.path}: its ${String(relation.kind)} relation to ${String(relation.case)} cannot hold for the ${side} entries' expected outcomes`
        )
      }
    }
  }
}

/**
 * Whether a value is a key or holds one at any depth.
 *
 * @param value - The value.
 * @param key - The key.
 * @returns Whether it holds the key.
 */
function holds(value, key) {
  if (isDeepStrictEqual(value, key)) {
    return true
  }
  if (Array.isArray(value)) {
    return value.some(item => holds(item, key))
  }
  return (
    value !== null &&
    typeof value === 'object' &&
    Object.values(value).some(member => holds(member, key))
  )
}

/**
 * What 015's Group ID selection rules conclude from the groups and targets a
 * vector hands over and its selectors: one group permits omission, several
 * require one exact Group ID, and an unknown Group ID, several selectors, a
 * Target ID, or a subset of a group is rejected.
 *
 * @param facts - The vector's facts.
 * @param inputs - Its consumer inputs.
 * @returns The relationship.
 */
function selection(facts, inputs) {
  const groups = facts?.deploymentGroups ?? {}
  const targets = facts?.targets ?? {}
  const selectors = Array.isArray(inputs?.groupIdSelectors) ? inputs.groupIdSelectors : []
  const rejected = rule => ({ outcome: 'rejected', rule })
  let group
  if (selectors.length === 0) {
    if (Object.keys(groups).length !== 1) {
      return rejected('selector-required')
    }
    group = Object.keys(groups)[0]
  } else if (selectors.length > 1) {
    return rejected('multiple-selectors')
  } else if (Object.hasOwn(groups, selectors[0])) {
    group = selectors[0]
  } else if (Object.hasOwn(targets, selectors[0])) {
    return rejected('target-id-selector')
  } else {
    return rejected('unknown-group-id')
  }
  if (inputs?.targetIdSelectors !== null) {
    return rejected('group-subset')
  }
  return { outcome: 'selected-group', groupId: group, plannedTargetIds: groups[group]?.members }
}

// Each Handoff Vector names a listed Resolver Case whose every expected
// outcome is checked, holds whole semantic groups of each such outcome's
// profile as its facts, and as its bindings exactly the outcome's rows whose
// key a fact holds, in the outcome's order (D15:3330, 3348). A Group ID
// selection vector hands over the groups and the targets, and expects what
// 015's selection rules conclude from its selectors (D15:2085).
let vectors = 0
const SELECTION = '015-handoff-group-selection-'
for (const entry of index.value.handoffVectors ?? []) {
  const vector = named(entry)
  if (vector === undefined) {
    continue
  }
  vectors += 1
  const facts = vector.producerProfileFacts ?? {}
  const producer = resolverListing.get(vector.producerCaseId)
  const outcomes = Object.values(producer?.outcomes ?? {})
  const reproduced =
    outcomes.length > 0 &&
    outcomes.every(
      outcome =>
        outcome.outcome === 'checked' &&
        Object.entries(facts).every(([group, value]) =>
          isDeepStrictEqual(outcome.profile?.[group], value)
        ) &&
        isDeepStrictEqual(
          vector.producerArtifactBindings,
          (outcome.resolvedProfileArtifactBindings ?? []).filter(row => holds(facts, row.key))
        )
    )
  if (!reproduced) {
    fail(
      `${entry.path}: does not hold the groups and bindings its producer's expected outcomes give`
    )
  }
  if (
    entry.id.startsWith(SELECTION) &&
    (!isDeepStrictEqual(Object.keys(facts), ['deploymentGroups', 'targets']) ||
      vector.verificationOwner !== '020' ||
      !isDeepStrictEqual(
        vector.expectedRelationship?.json,
        selection(facts, vector.consumerInputs?.json)
      ))
  ) {
    fail(`${entry.path}: does not expect what 015's Group ID selection rules conclude`)
  }
}

// Each row of the obligation table is one manifest, inactive with its
// not-applicable reason, since an active one would need cases no
// revision-"0" manifest names; and nothing activates an evaluated row: the
// Configuration Schema Authority Set maps one configuration schema revision,
// and the Profile Specification names canonicalization specification
// revision "0", which admits no extension but u and t (D15:1288, 3371).
const obligationRows = [
  ...new Set(
    (caseSchemaBody?.definitions?.conditionalObligation?.oneOf ?? []).map(
      row => row?.properties?.id?.const
    )
  )
].toSorted(byBytes)
const listedObligations = (index.value.conditionalObligations ?? []).map(entry => entry?.id)
if (obligationRows.length === 0 || !isDeepStrictEqual(listedObligations, obligationRows)) {
  fail('index: the conditional obligations are not one manifest for each row of the table')
}
for (const entry of index.value.conditionalObligations ?? []) {
  const obligation = named(entry)
  if (obligation !== undefined && obligation.activationState !== 'inactive') {
    fail(`${entry.path}: is active, though no case its row names can be listed`)
  }
}
const schemaRevisions = Object.keys(index.value.configurationSchemaAuthoritySet?.members ?? {})
const canonicalization = named(index.value.profileSpecification)?.specificationSlots?.[
  'locale-canonicalization'
]
if (schemaRevisions.length >= 2 || canonicalization?.revision !== '0') {
  fail('index: an evaluated activation holds, which no manifest of the suite can follow')
}

// The containers list the digests the index declares for their members.
const without = (entry, name) =>
  Object.fromEntries(Object.entries(entry ?? {}).filter(([key]) => key !== name))
const rootPackage = index.value.resolverConstructionRootPackage
const rootArtifact = named(rootPackage)
for (const role of ROOT_ROLES) {
  if (rootArtifact?.members?.[role]?.digest !== rootPackage?.members?.[role]?.digest) {
    fail(`index: the Root Package lists another digest for ${role}`)
  }
}
const registryArtifact = named(index.value.registryPackage)
for (const role of REGISTRY_ROLES) {
  if (
    !isDeepStrictEqual(registryArtifact?.members?.[role], without(registryMembers[role], 'path'))
  ) {
    fail(`index: the Registry Package's container lists another ${role}`)
  }
}
const set = index.value.configurationSchemaAuthoritySet
const setArtifact = named(set)
const profile = index.value.profileSpecification
const profileTuple = {
  identity: profile?.identity,
  revision: profile?.revision,
  semanticDigest: profile?.semanticDigest
}
if (
  !isDeepStrictEqual(set?.profileSpecification, profileTuple) ||
  !isDeepStrictEqual(setArtifact?.profileSpecification, profileTuple)
) {
  fail('index: the Configuration Schema Authority Set belongs to another Profile Specification')
}
if (!isDeepStrictEqual(setArtifact?.members?.['0'], without(mapped, 'path'))) {
  fail('index: the Configuration Schema Authority Set maps another schema')
}
const body = named(profile)
if (body === undefined || semantic(body) !== profile.semanticDigest) {
  fail('index: the Profile Specification does not have the semantic digest the index declares')
}

// Every digest is the one the crate pins.
for (const [module, name, pointer] of PINS) {
  const pin = pinned(readFileSync(resolve(crate, module), 'utf8'), name)
  if (pin === undefined || pin !== at(index.value, pointer)) {
    fail(`index: ${pointer} is not ${name} in ${module}`)
  }
}

finish()

console.log(
  `conformance suite index agrees: ${MEMBERS.length} members, ${bodies} bodies of which ${manifests} manifests, ${fixtures} construction fixtures, ${admissionFixtures} input admission fixtures (${stepCases} of the admission steps), ${resolverCases} Resolver Cases with ${expectedOutcomes} expected outcomes and ${relations} relations, ${vectors} Handoff Vectors, ${listedObligations.length} conditional obligations, ${objects} closed schema objects, ${PINS.length} pinned digests`
)
