/**
 * Check the committed Resolver Construction Root Package against this implementation.
 *
 * The Rust crate writes the five Root Package members and the Root Package
 * artifact, and compiles in two digests: the artifact's file digest and the
 * package's canonical digest. This script shares no code with it: it hashes
 * every artifact itself, frames the canonical digest from design 015's record
 * shape, lists the roles, bounds, units, and failure precedence from its own
 * copy of the design's tables, and reads the two pinned digests out of the
 * crate's source. It does not read the design document.
 */

import { createHash } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { isDeepStrictEqual } from 'node:util'

const here = dirname(fileURLToPath(import.meta.url))
const crate = resolve(here, '../../../crates/intlify_config')
const construction = resolve(crate, 'conformance/construction')

const IDENTITY = 'intlify-resolver-construction-root-package'
const REVISION = '0'
const DOMAIN = 'intlify.resolver-construction-root-package.v0'
const DRAFT_7 = 'http://json-schema.org/draft-07/schema#'
const META_SCHEMA_DIGEST = 'sha256:9e786e2198debe6717675ce48e199199f2dba8e3a3e1ffe35cfc0959efd691cb'

/**
 * Design 015's Root Package roles in their fixed order, with each member's
 * file below the suite root.
 *
 * @type {[string, string][]}
 */
const ROLES = [
  ['construction-admission-specification', 'construction-admission-v0.json'],
  ['construction-admission-schema', 'construction-admission-v0.schema.json'],
  ['authority-envelope-schema-package', 'authority-envelope-schemas-v0.json'],
  ['json-schema-meta-schema-package', 'json-schema-meta-schemas-v0.json'],
  ['cross-authority-rules', 'cross-authority-rules-v0.json']
]

/**
 * Design 015's construction-root bounds in their canonical order, each with
 * its counting unit.
 *
 * @type {[string, string][]}
 */
const BOUNDS = [
  ['input.maxTotalArtifactBytes', 'artifact-byte'],
  ['input.maxSingleArtifactBytes', 'artifact-byte'],
  ['input.maxDepth', 'json-level'],
  ['input.maxNodes', 'json-node'],
  ['input.maxCollectionEntries', 'json-collection-entry'],
  ['input.maxTotalStringBytes', 'string-byte'],
  ['input.maxSingleStringBytes', 'string-byte'],
  ['schema.maxConfigurationSchemas', 'configuration-schema'],
  ['schema.maxSingleArtifactBytes', 'artifact-byte'],
  ['schema.maxTotalArtifactBytes', 'artifact-byte'],
  ['schema.maxNodes', 'schema-node'],
  ['schema.maxReferences', 'schema-reference'],
  ['schema.maxValidationWorkUnits', 'validation-pair'],
  ['registry.maxRows', 'registry-row'],
  ['registry.maxSingleMemberArtifactBytes', 'artifact-byte'],
  ['registry.maxTotalArtifactBytes', 'artifact-byte'],
  ['registry.maxValidationWorkUnits', 'validation-pair'],
  ['crossAuthority.maxChecks', 'cross-authority-check']
]

/**
 * Design 015's construction-failure precedence: each row's component, the
 * implementation's spelling of its check cell, and its reason.
 *
 * @type {[string, string, string][]}
 */
const PRECEDENCE = [
  ['all', 'construction-bounds', 'construction-input-limit-exceeded'],
  ['profileSpecificationSchema', 'presence', 'profile-specification-schema-missing'],
  ['profileSpecificationSchema', 'envelope', 'profile-specification-schema-invalid'],
  ['profileSpecificationSchema', 'identity', 'profile-specification-schema-identity-mismatch'],
  ['profileSpecificationSchema', 'revision', 'profile-specification-schema-revision-unsupported'],
  ['profileSpecificationSchema', 'file-digest', 'profile-specification-schema-digest-mismatch'],
  ['profileSpecificationSchema', 'meta-schema', 'profile-specification-schema-invalid'],
  ['profileSpecification', 'presence', 'profile-specification-missing'],
  ['profileSpecification', 'envelope', 'profile-specification-body-schema-invalid'],
  ['profileSpecification', 'identity', 'profile-specification-identity-mismatch'],
  ['profileSpecification', 'revision', 'profile-specification-revision-unsupported'],
  ['profileSpecification', 'file-digest', 'profile-specification-digest-mismatch'],
  ['profileSpecification', 'body-schema', 'profile-specification-body-schema-invalid'],
  ['profileSpecification', 'semantic-digest', 'profile-specification-digest-mismatch'],
  ['configurationSchemaAuthoritySet', 'presence', 'configuration-schema-authority-set-missing'],
  [
    'configurationSchemaAuthoritySet',
    'envelope',
    'configuration-schema-authority-set-schema-invalid'
  ],
  [
    'configurationSchemaAuthoritySet',
    'identity',
    'configuration-schema-authority-set-identity-mismatch'
  ],
  [
    'configurationSchemaAuthoritySet',
    'revision',
    'configuration-schema-authority-set-revision-unsupported'
  ],
  [
    'configurationSchemaAuthoritySet',
    'file-digest',
    'configuration-schema-authority-set-digest-mismatch'
  ],
  ['configurationSchemaMember', 'identity', 'configuration-schema-member-identity-mismatch'],
  ['configurationSchemaMember', 'revision', 'configuration-schema-member-revision-unsupported'],
  ['configurationSchemaMember', 'dialect', 'configuration-schema-member-dialect-unsupported'],
  ['configurationSchemaMember', 'file-digest', 'configuration-schema-member-digest-mismatch'],
  ['configurationSchemaMember', 'meta-schema', 'configuration-schema-member-schema-invalid'],
  ['resolverCapabilitySpecification', 'presence', 'resolver-capability-specification-missing'],
  [
    'resolverCapabilitySpecification',
    'envelope',
    'resolver-capability-specification-schema-invalid'
  ],
  [
    'resolverCapabilitySpecification',
    'identity',
    'resolver-capability-specification-identity-mismatch'
  ],
  [
    'resolverCapabilitySpecification',
    'revision',
    'resolver-capability-specification-revision-unsupported'
  ],
  [
    'resolverCapabilitySpecification',
    'file-digest',
    'resolver-capability-specification-digest-mismatch'
  ],
  [
    'resolverCapabilitySpecification',
    'body-schema',
    'resolver-capability-specification-schema-invalid'
  ],
  ['registryPackage', 'presence', 'registry-package-incomplete'],
  ['registryPackage', 'envelope', 'registry-package-schema-invalid'],
  ['registryPackage', 'identity', 'registry-package-identity-mismatch'],
  ['registryPackage', 'revision', 'registry-package-revision-unsupported'],
  ['registryPackage', 'file-digest', 'registry-package-digest-mismatch'],
  ['registryMember', 'presence', 'registry-package-incomplete'],
  ['registryMember', 'identity', 'registry-member-identity-mismatch'],
  ['registryMember', 'revision', 'registry-member-revision-unsupported'],
  ['registryMember', 'file-digest', 'registry-member-digest-mismatch'],
  ['registryMember', 'member-schema', 'registry-member-schema-invalid'],
  ['registryPackage', 'cross-references', 'registry-cross-reference-invalid'],
  ['all', 'cross-authority-rules', 'construction-cross-authority-reference-invalid'],
  ['all', 'implemented-authority', 'construction-authority-not-implemented']
]

/** Design 015's five construction components and the identity each admits. */
const COMPONENTS = [
  ['profileSpecificationSchema', 'intlify-project-profile-specification-schema'],
  ['profileSpecification', 'intlify-project-profile'],
  ['configurationSchemaAuthoritySet', 'intlify-configuration-schema-authority-set'],
  ['resolverCapabilitySpecification', 'intlify-project-profile-resolver-capability'],
  ['registryPackage', 'intlify-project-profile-resolver-registry-package']
]

/** Design 015's six Registry Package roles in their fixed order. */
const REGISTRY_ROLES = [
  'safe-origin-kind-registry-schema',
  'check-registry-schema',
  'finding-registry-schema',
  'safe-origin-kind-registry',
  'check-registry',
  'finding-registry'
]

/** Design 015's four cross-authority rules in their order. */
const RULES = [
  'profile-association',
  'profile-authorities',
  'capability-bound-mirror',
  'registry-phases'
]

const LARGEST = 2n ** 64n - 2n
const CANONICAL = /^(?:0|[1-9]\d*)$/

const failures = []
const fail = message => failures.push(message)

/**
 * Hash bytes and present the digest as the repository writes digests.
 *
 * @param bytes - The bytes.
 * @returns `sha256:` and 64 lowercase hexadecimal digits.
 */
function digest(bytes) {
  return `sha256:${createHash('sha256').update(bytes).digest('hex')}`
}

/**
 * Write one field frame: the tag, the payload length as eight bytes
 * big-endian, and the payload.
 *
 * @param tag - The record-local tag.
 * @param payload - The payload.
 * @returns The frame.
 */
function frame(tag, payload) {
  const header = Buffer.alloc(9)
  header.writeUInt8(tag, 0)
  header.writeBigUInt64BE(BigInt(payload.length), 1)
  return Buffer.concat([header, payload])
}

/**
 * Write a collection: the count, then each item's length and bytes.
 *
 * @param items - The item records.
 * @returns The collection payload.
 */
function collection(items) {
  const count = Buffer.alloc(8)
  count.writeBigUInt64BE(BigInt(items.length))
  const parts = [count]
  for (const item of items) {
    const length = Buffer.alloc(8)
    length.writeBigUInt64BE(BigInt(item.length))
    parts.push(length, item)
  }
  return Buffer.concat(parts)
}

/**
 * Read one digest the crate pins out of its source.
 *
 * @param source - The source text of the Root Package module.
 * @param name - The constant's name.
 * @returns The digest, or undefined when it is not there.
 */
function pinned(source, name) {
  const match = source.match(
    new RegExp(`pub\\(crate\\) const ${name}: &str =\\s*"(sha256:[0-9a-f]{64})";`)
  )
  return match?.[1]
}

const rootBytes = readFileSync(resolve(construction, 'root-package-v0.json'))
const memberBytes = ROLES.map(([, file]) => readFileSync(resolve(construction, file)))
const source = readFileSync(resolve(crate, 'src/construction/root.rs'), 'utf8')

for (const [name, bytes] of [
  ['root-package-v0.json', rootBytes],
  ...ROLES.map(([, file], index) => [file, memberBytes[index]])
]) {
  if (
    bytes.includes(0x0d) ||
    !bytes.toString('utf8').endsWith('}\n') ||
    bytes.toString('utf8').endsWith('\n\n')
  ) {
    fail(`${name}: not written with one final newline and no carriage return`)
  }
}

if (digest(rootBytes) !== pinned(source, 'DIGEST')) {
  fail(
    `root package: the file digest ${digest(rootBytes)} is not the pinned ${pinned(source, 'DIGEST')}`
  )
}

const root = JSON.parse(rootBytes.toString('utf8'))
if (!isDeepStrictEqual(Object.keys(root), ['members'])) {
  fail(`root package: top-level members ${JSON.stringify(Object.keys(root))}`)
}
if (
  !isDeepStrictEqual(
    Object.keys(root.members ?? {}),
    ROLES.map(([role]) => role)
  )
) {
  fail(`root package: roles ${JSON.stringify(Object.keys(root.members ?? {}))}`)
}
const items = []
for (const [index, [role]] of ROLES.entries()) {
  const entry = root.members?.[role]
  if (!isDeepStrictEqual(Object.keys(entry ?? {}), ['digest'])) {
    fail(`root package: ${role} is not exactly { digest }`)
    continue
  }
  if (entry.digest !== digest(memberBytes[index])) {
    fail(
      `root package: ${role} lists ${entry.digest}, its bytes hash to ${digest(memberBytes[index])}`
    )
  }
  const hash = createHash('sha256').update(memberBytes[index]).digest()
  items.push(Buffer.concat([frame(1, Buffer.from(role, 'utf8')), frame(2, hash)]))
}
const record = Buffer.concat([
  frame(1, Buffer.from(IDENTITY, 'utf8')),
  frame(2, Buffer.from(REVISION, 'utf8')),
  frame(3, collection(items))
])
const canonical = digest(Buffer.concat([Buffer.from(DOMAIN, 'utf8'), record]))
if (canonical !== pinned(source, 'CANONICAL_DIGEST')) {
  fail(
    `root package: the canonical digest ${canonical} is not the pinned ${pinned(source, 'CANONICAL_DIGEST')}`
  )
}

const [admissionBytes, schemaBytes, envelopeBytes, metaBytes, rulesBytes] = memberBytes
const admission = JSON.parse(admissionBytes.toString('utf8'))

const bounds = admission.bounds ?? []
if (bounds.length !== BOUNDS.length) {
  fail(`admission: ${bounds.length} bounds, expected ${BOUNDS.length}`)
}
for (const [index, [id, unit]] of BOUNDS.entries()) {
  const bound = bounds[index]
  if (bound?.id !== id || bound?.unit !== unit) {
    fail(`admission: bound ${index} is ${JSON.stringify(bound)}, expected ${id} in ${unit}`)
    continue
  }
  if (!CANONICAL.test(bound.value) || BigInt(bound.value) === 0n || BigInt(bound.value) > LARGEST) {
    fail(`admission: bound ${id} has the value ${JSON.stringify(bound.value)}, which is no bound`)
  }
}

if (
  !isDeepStrictEqual(
    Object.keys(admission.components ?? {}),
    COMPONENTS.map(([role]) => role)
  )
) {
  fail(`admission: components ${JSON.stringify(Object.keys(admission.components ?? {}))}`)
}
for (const [role, identity] of COMPONENTS) {
  const component = admission.components?.[role]
  if (component?.identity !== identity || !isDeepStrictEqual(component?.revisions, [REVISION])) {
    fail(`admission: component ${role} admits ${JSON.stringify(component)}`)
  }
}
if (
  !isDeepStrictEqual(admission.configurationSchemas, {
    identity: 'intlify-configuration-schema',
    revisions: [REVISION],
    dialects: [DRAFT_7]
  })
) {
  fail(`admission: configuration schemas ${JSON.stringify(admission.configurationSchemas)}`)
}
if (!isDeepStrictEqual(Object.keys(admission.registryMembers ?? {}), REGISTRY_ROLES)) {
  fail(
    `admission: registry members ${JSON.stringify(Object.keys(admission.registryMembers ?? {}))}`
  )
}
for (const role of REGISTRY_ROLES) {
  const member = admission.registryMembers?.[role]
  if (
    member?.identity !== `intlify-project-profile-${role}` ||
    !isDeepStrictEqual(member?.revisions, [REVISION])
  ) {
    fail(`admission: registry member ${role} admits ${JSON.stringify(member)}`)
  }
}

const rows = admission.failurePrecedence ?? []
if (rows.length !== PRECEDENCE.length) {
  fail(`admission: ${rows.length} precedence rows, expected ${PRECEDENCE.length}`)
}
for (const [index, [component, check, reason]] of PRECEDENCE.entries()) {
  if (!isDeepStrictEqual(rows[index], { component, check, reason })) {
    fail(
      `admission: precedence row ${index + 1} is ${JSON.stringify(rows[index])}, expected ${component} ${check} ${reason}`
    )
  }
}
if (new Set(PRECEDENCE.map(([, , reason]) => reason)).size !== 38) {
  fail('the transcribed precedence does not hold 38 reasons')
}

if (!isDeepStrictEqual(admission.crossAuthorityRules, RULES)) {
  fail(`admission: cross-authority rules ${JSON.stringify(admission.crossAuthorityRules)}`)
}
const rules = JSON.parse(rulesBytes.toString('utf8'))
if (
  !isDeepStrictEqual(
    rules.rules?.map(rule => rule.id),
    RULES
  )
) {
  fail(`cross-authority rules: ${JSON.stringify(rules.rules?.map(rule => rule.id))}`)
}

const schema = JSON.parse(schemaBytes.toString('utf8'))
if (schema.$schema !== DRAFT_7) {
  fail(`admission schema: $schema ${schema.$schema}`)
}

const envelope = JSON.parse(envelopeBytes.toString('utf8'))
for (const [name, value] of Object.entries(envelope.schemas ?? {})) {
  if (value.$schema !== DRAFT_7 || value.additionalProperties !== false) {
    fail(`envelope schema ${name}: not a closed Draft 7 schema`)
  }
}

const metaSchemaText = readFileSync(resolve(crate, 'data/json-schema-draft-07.json'))
if (digest(metaSchemaText) !== META_SCHEMA_DIGEST) {
  fail(`meta-schema: the published text hashes to ${digest(metaSchemaText)}`)
}
const meta = JSON.parse(metaBytes.toString('utf8'))
if (
  meta.dialects?.length !== 1 ||
  meta.dialects[0].dialect !== DRAFT_7 ||
  !isDeepStrictEqual(meta.dialects[0].metaSchema, JSON.parse(metaSchemaText.toString('utf8')))
) {
  fail('meta-schema package: not exactly Draft 7 with its published meta-schema')
}

if (failures.length > 0) {
  for (const message of failures) {
    console.error(message)
  }
  process.exit(1)
}

console.log(
  `construction root agrees: ${ROLES.length} members, ${bounds.length} bounds, ${rows.length} precedence rows, ${RULES.length} cross-authority rules, canonical digest ${canonical}`
)
