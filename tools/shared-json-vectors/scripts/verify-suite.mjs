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
 * It does not read the design document.
 */

import { createHash } from 'node:crypto'
import { existsSync, readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { isDeepStrictEqual } from 'node:util'

import { digest as semanticDigest } from './framing.mjs'

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
  'registryPackage'
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
  registryPackage: {
    identity: 'intlify-project-profile-resolver-registry-package',
    members: [...ENTRY, 'members'],
    roles: REGISTRY_ROLES,
    role: ENTRY
  }
}

/** Where each digest the index declares is pinned in the crate's source. */
const PINS = [
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
 * Whether a JSON text repeats a member name within one object, which
 * `JSON.parse` hides by keeping the last value. The text is valid JSON, and
 * names compare as decoded strings.
 *
 * @param text - The JSON text.
 * @returns Whether a member repeats.
 */
function repeatsMember(text) {
  const open = []
  let name = false
  for (let at = 0; at < text.length; at += 1) {
    const character = text[at]
    if (character === '"') {
      let end = at + 1
      while (text[end] !== '"') {
        end += text[end] === '\\' ? 2 : 1
      }
      const names = open.at(-1)
      if (name && names instanceof Set) {
        const decoded = JSON.parse(text.slice(at, end + 1))
        if (names.has(decoded)) {
          return true
        }
        names.add(decoded)
        name = false
      }
      at = end
    } else if (character === '{') {
      open.push(new Set())
      name = true
    } else if (character === '[') {
      open.push(null)
      name = false
    } else if (character === '}' || character === ']') {
      open.pop()
      name = false
    } else if (character === ',') {
      name = open.at(-1) instanceof Set
    }
  }
  return false
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
  `conformance suite index agrees: ${MEMBERS.length} members, ${bodies} bodies, ${objects} closed schema objects, ${PINS.length} pinned digests`
)
