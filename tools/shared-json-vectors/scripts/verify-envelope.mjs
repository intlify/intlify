/**
 * Check the committed artifact envelope vectors against this implementation.
 *
 * The Rust crate writes members of a Submitted Profile Resolution Artifact
 * Collection with what each declares, the exact-reference tuple it
 * establishes, whether its integrity holds, how its body admission ends, and
 * its canonical bytes, and the digests, spellings, orders, content changes, and
 * references that depend on them. This script shares no code with it: it
 * decides each of these from design 017's rules and its own framing, digest,
 * and reading of the closed contents, so a disagreement says which of the two
 * is wrong.
 */

import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { digest, encode } from './framing.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const vectorsPath = resolve(here, '../../../crates/intlify_config/fixtures/envelope/vectors.json')

/** The integrity domain design 017 registers for localization artifacts. */
const INTEGRITY_DOMAIN = 'localization-artifact-integrity'

/** The eight kinds the collection admits, with their governing specification and digest domain. */
const KINDS = new Map([
  ['approval-policy', ['intlify-design-021', 'policy-semantic-digest']],
  ['glossary-set', ['intlify-design-022', 'policy-semantic-digest']],
  ['provider-routing-policy', ['intlify-design-022', 'policy-semantic-digest']],
  ['resource-limit-policy', ['intlify-design-018', 'policy-semantic-digest']],
  ['selection-policy', ['intlify-design-021', 'policy-semantic-digest']],
  ['source-admission-policy', ['intlify-design-018', 'policy-semantic-digest']],
  ['target-profile', ['intlify-design-024', 'target-profile-semantic-digest']],
  ['trust-policy', ['intlify-design-018', 'policy-semantic-digest']]
])

/** The complete IdentityToken and RevisionToken grammar of design 017. */
const TOKEN = /^[a-z0-9](?:[a-z0-9._-]*[a-z0-9])?$/

/** The complete `sha256:` digest spelling. */
const DIGEST = /^sha256:[0-9a-f]{64}$/

/** The one specification revision a body is admitted under. */
const SPECIFICATION_REVISION = '0'

/** The largest `UInt64`. */
const MAX_UINT64 = 18446744073709551615n

/**
 * The closed contents of design 017's Policy and Target Profile bodies, as
 * data that `conforms` reads: an object lists its members, `oneOf` its
 * spellings, and `ordered` an array of entries strictly ascending by the
 * UTF-8 bytes of one member. The other three kinds have no entry: 018 and 022
 * admit their contents.
 */
const token = { type: 'token' }
const pair = { type: 'object', members: { identity: token, revision: token } }
const oneOf = (...values) => ({ type: 'oneOf', values })
const pinned = (identity, revision) => ({
  type: 'object',
  members: { identity: oneOf(identity), revision: oneOf(revision) }
})
const object = members => ({ type: 'object', members })
const positive = { type: 'positive' }
const LIMIT_NAMES = [
  'maxMessages',
  'maxUses',
  'maxLocales',
  'maxFiles',
  'maxFileBytes',
  'maxOutputBytes',
  'maxParameterBytes',
  'maxTextBytes'
]
const CONTENTS = new Map([
  ['approval-policy', object({ localizedApproval: oneOf('required', 'not-required') })],
  ['selection-policy', object({ mode: oneOf('explicit') })],
  [
    'source-admission-policy',
    object({ sourceApproval: oneOf('required', 'authenticated-sufficient') })
  ],
  [
    'provider-routing-policy',
    object({
      adapter: pair,
      configuration: object({ identity: token, semanticDigest: { type: 'digest' } })
    })
  ],
  [
    'target-profile',
    object({
      host: object({ kind: oneOf('javascript-esm-dom-text'), grammar: pair, moduleProfile: pair }),
      mode: oneOf('runtime-backed', 'aot'),
      adapter: pair,
      execution: object({
        specification: pinned('intlify-design-023', '0'),
        profile: pinned('intlify-text-interpolation-minimum', '0')
      }),
      mf2Specification: pair,
      codecs: object({
        bindingTable: pair,
        localePayload: pair,
        descriptor: pair,
        sourceMap: pinned('ecma-426-source-map', '3')
      }),
      locale: object({
        selection: oneOf('direct-only'),
        services: oneOf('absent'),
        directions: {
          type: 'ordered',
          by: 'locale',
          entry: object({ locale: { type: 'text' }, direction: oneOf('LTR', 'RTL', 'unknown') })
        }
      }),
      delivery: object({ unit: oneOf('main'), loading: oneOf('eager') }),
      limits: object(Object.fromEntries(LIMIT_NAMES.map(name => [name, positive])))
    })
  ]
])

/**
 * Whether a value is a JSON object.
 *
 * @param value - A JSON value.
 * @returns Whether it is an object rather than an array or null.
 */
function isObject(value) {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

/**
 * The object a value is when it has exactly the given members.
 *
 * @param value - A JSON value.
 * @param members - The member names it must have and no others.
 * @returns The object, or null.
 */
function closed(value, members) {
  if (!isObject(value)) {
    return null
  }
  const names = Object.keys(value)
  return names.length === members.length && members.every(member => names.includes(member))
    ? value
    : null
}

/**
 * Whether a value is a token string.
 *
 * @param value - A JSON value.
 * @returns Whether it is a string matching the token grammar.
 */
function isToken(value) {
  return typeof value === 'string' && TOKEN.test(value)
}

/**
 * What a member declares about its kind.
 *
 * @param member - A collection member.
 * @returns The admitted kind, "unsupported", or null when it declares none.
 */
function declaration(member) {
  if (!isObject(member) || typeof member.kind !== 'string') {
    return null
  }
  if (!KINDS.has(member.kind)) {
    return 'unsupported'
  }
  if (typeof member.schemaRevision === 'string' && member.schemaRevision !== '0') {
    return 'unsupported'
  }
  return member.kind
}

/**
 * The exact-reference tuple a member establishes.
 *
 * @param member - A collection member.
 * @returns The tuple, or null when the member is no closed envelope.
 */
function tuple(member) {
  const kind = declaration(member)
  if (kind === null || kind === 'unsupported') {
    return null
  }
  const envelope = closed(member, [
    'kind',
    'schemaRevision',
    'specification',
    'body',
    'integrityDigest'
  ])
  if (envelope === null || envelope.schemaRevision !== '0') {
    return null
  }
  const specification = closed(envelope.specification, ['identity', 'revision'])
  const body = closed(envelope.body, ['identity', 'revision', 'content'])
  const [governing, domain] = KINDS.get(kind)
  if (
    specification === null ||
    body === null ||
    !isToken(specification.identity) ||
    !isToken(specification.revision) ||
    specification.identity !== governing ||
    !isToken(body.identity) ||
    !isToken(body.revision) ||
    !isObject(body.content) ||
    typeof envelope.integrityDigest !== 'string' ||
    !DIGEST.test(envelope.integrityDigest)
  ) {
    return null
  }
  return {
    kind,
    identity: body.identity,
    revision: body.revision,
    specificationRevision: specification.revision,
    semanticDigest: digest(domain, {
      kind,
      specificationRevision: specification.revision,
      content: body.content
    })
  }
}

/**
 * Whether an envelope's integrity digest is the digest of the rest of it.
 *
 * @param member - A member that establishes a tuple.
 * @returns Whether the integrity holds.
 */
function integrityHolds(member) {
  const { integrityDigest, ...rest } = member
  return digest(INTEGRITY_DOMAIN, rest) === integrityDigest
}

/**
 * Whether a value conforms to a closed content description.
 *
 * @param description - An entry of `CONTENTS` or a part of one.
 * @param value - A JSON value.
 * @returns Whether the value is what the description allows.
 */
function conforms(description, value) {
  switch (description.type) {
    case 'object': {
      const names = Object.keys(description.members)
      return (
        closed(value, names) !== null &&
        names.every(name => conforms(description.members[name], value[name]))
      )
    }
    case 'oneOf': {
      return typeof value === 'string' && description.values.includes(value)
    }
    case 'token': {
      return isToken(value)
    }
    case 'digest': {
      return typeof value === 'string' && DIGEST.test(value)
    }
    case 'text': {
      return typeof value === 'string' && value.length > 0
    }
    case 'positive': {
      return typeof value === 'string' && /^[1-9]\d*$/.test(value) && BigInt(value) <= MAX_UINT64
    }
    case 'ordered': {
      if (!Array.isArray(value) || !value.every(entry => conforms(description.entry, entry))) {
        return false
      }
      const keys = value.map(entry => Buffer.from(entry[description.by], 'utf8'))
      return keys.every((key, index) => index === 0 || Buffer.compare(keys[index - 1], key) < 0)
    }
    default: {
      throw new Error(`no such description: ${description.type}`)
    }
  }
}

/**
 * Whether a content is the closed content of its kind.
 *
 * @param kind - An admitted kind.
 * @param content - A content object.
 * @returns Whether design 017's closed content admits it, or true for a kind
 *   whose content 018 or 022 admits.
 */
function contentAdmitted(kind, content) {
  const description = CONTENTS.get(kind)
  return description === undefined || conforms(description, content)
}

/**
 * How the body admission of a member ends.
 *
 * @param member - A collection member.
 * @returns null without a tuple; otherwise the first of the integrity, the
 *   specification revision, and the content to fail, or "admitted".
 */
function admission(member) {
  const established = tuple(member)
  if (established === null) {
    return null
  }
  if (!integrityHolds(member)) {
    return 'integrity-invalid'
  }
  if (established.specificationRevision !== SPECIFICATION_REVISION) {
    return 'specification-revision-unsupported'
  }
  return contentAdmitted(established.kind, member.body.content) ? 'admitted' : 'content-invalid'
}

/**
 * Whether a JSON value holds a number anywhere.
 *
 * @param value - A JSON value.
 * @returns Whether a number occurs in it.
 */
function holdsNumber(value) {
  if (typeof value === 'number') {
    return true
  }
  if (Array.isArray(value)) {
    return value.some(holdsNumber)
  }
  return isObject(value) && Object.values(value).some(holdsNumber)
}

/**
 * Whether two tuples are the same exact reference.
 *
 * @param left - A tuple or null.
 * @param right - A tuple or null.
 * @returns Whether both are tuples with equal parts.
 */
function sameTuple(left, right) {
  return (
    left !== null &&
    right !== null &&
    ['kind', 'identity', 'revision', 'specificationRevision', 'semanticDigest'].every(
      part => left[part] === right[part]
    )
  )
}

/**
 * Order member ids by the unsigned bytes of their canonical bodies.
 *
 * @param ids - Member ids.
 * @param members - Members by id.
 * @returns The ids in that order.
 */
function byCanonicalBody(ids, members) {
  return [...ids].sort((left, right) =>
    Buffer.compare(encode(members.get(left)), encode(members.get(right)))
  )
}

/**
 * Check every recorded fact against this implementation's decision.
 *
 * @param vectors - The vectors document.
 * @param fail - Records one failed check.
 */
function checkMembers(vectors, fail) {
  for (const vector of vectors.members) {
    const { member } = vector
    const declared = declaration(member)
    if (declared !== vector.declaration) {
      fail(`${vector.id}: declares ${declared}, committed ${vector.declaration}`)
    }
    const established = tuple(member)
    const agrees =
      established === null ? vector.tuple === null : sameTuple(established, vector.tuple)
    if (!agrees) {
      fail(
        `${vector.id}:\n  committed ${JSON.stringify(vector.tuple)}\n  produced  ${JSON.stringify(established)}`
      )
    }
    const holds = established === null ? null : integrityHolds(member)
    if (holds !== vector.integrityHolds) {
      fail(`${vector.id}: integrity ${holds}, committed ${vector.integrityHolds}`)
    }
    const ends = admission(member)
    if (ends !== vector.admission) {
      fail(`${vector.id}: body admission ${ends}, committed ${vector.admission}`)
    }
    const bytes = encode(member).length
    if (bytes !== vector.canonicalBytes) {
      fail(`${vector.id}: ${bytes} canonical bytes, committed ${vector.canonicalBytes}`)
    }
  }
}

/**
 * Check the pairs, spellings, refusals, orders, content changes, and
 * references.
 *
 * @param vectors - The vectors document.
 * @param fail - Records one failed check.
 */
function checkRelations(vectors, fail) {
  const members = new Map(vectors.members.map(vector => [vector.id, vector.member]))
  const tuples = new Map(vectors.members.map(vector => [vector.id, tuple(vector.member)]))
  for (const [left, right] of vectors.sameSemanticDigest) {
    const [a, b] = [tuples.get(left), tuples.get(right)]
    if (a === null || b === null || a.semanticDigest !== b.semanticDigest) {
      fail(`${left} and ${right}: the semantic digests differ`)
    }
  }
  for (const [left, right] of vectors.differentSemanticDigest) {
    const [a, b] = [tuples.get(left), tuples.get(right)]
    if (a === null || b === null || a.semanticDigest === b.semanticDigest) {
      fail(`${left} and ${right}: the semantic digests agree`)
    }
  }
  const framed = vectors.spellings.map(text => encode(JSON.parse(text)))
  if (framed.length < 2 || framed.some(bytes => !bytes.equals(framed[0]))) {
    fail('the spellings do not frame to one canonical body')
  }
  for (const [left, right] of vectors.unequalBodies) {
    if (encode(members.get(left)).equals(encode(members.get(right)))) {
      fail(`${left} and ${right}: the canonical bodies are equal`)
    }
  }
  for (const text of vectors.unrepresentable) {
    if (!holdsNumber(JSON.parse(text))) {
      fail(`a text holds no number: ${text}`)
    }
  }
  const one = vectors.withinOneTuple
  if (one.some(id => !sameTuple(tuples.get(id), tuples.get(one[0])))) {
    fail('the members within one tuple establish different tuples')
  }
  if (JSON.stringify(byCanonicalBody(one, members)) !== JSON.stringify(one)) {
    fail(`the members within one tuple are out of order: ${one.join(', ')}`)
  }
  const without = [...tuples].filter(([, established]) => established === null).map(([id]) => id)
  if (JSON.stringify(byCanonicalBody(without, members)) !== JSON.stringify(vectors.withoutTuple)) {
    fail(
      `the members without a tuple are not these, in this order: ${vectors.withoutTuple.join(', ')}`
    )
  }
  for (const vector of vectors.contents) {
    const base = members.get(vector.base)
    const content = structuredClone(base.body.content)
    const names = vector.pointer.split('/').slice(1)
    const name = names.pop()
    const parent = names.reduce((node, part) => node[part], content)
    if ('value' in vector) {
      parent[name] = vector.value
    } else {
      delete parent[name]
    }
    const admitted = contentAdmitted(base.kind, content)
    if (admitted !== vector.admitted) {
      fail(`${vector.id}: content admitted ${admitted}, committed ${vector.admitted}`)
    }
  }
  for (const vector of vectors.references) {
    if (!sameTuple(vector.reference, tuples.get(vector.resolves))) {
      fail(`the reference does not name the tuple of ${vector.resolves}`)
    }
    const bytes = encode(vector.reference).length
    if (bytes !== vector.canonicalBytes) {
      fail(
        `the reference to ${vector.resolves}: ${bytes} canonical bytes, committed ${vector.canonicalBytes}`
      )
    }
  }
}

const vectors = JSON.parse(readFileSync(vectorsPath, 'utf8'))
let failures = 0
const fail = message => {
  console.error(message)
  failures += 1
}
if (vectors.integrityDomain !== INTEGRITY_DOMAIN) {
  fail('the vectors do not use the integrity domain design 017 registers')
}
const table = vectors.kinds.map(entry => [
  entry.kind,
  [entry.governingSpecification, entry.semanticDomain]
])
if (JSON.stringify(table) !== JSON.stringify([...KINDS])) {
  fail('the vectors do not list the eight kinds design 017 registers')
}
checkMembers(vectors, fail)
checkRelations(vectors, fail)

if (failures > 0) {
  console.error(`\n${failures} envelope vector check(s) failed`)
  process.exit(1)
}
console.log(
  `${vectors.members.length} members, ${vectors.contents.length} content changes, and ${vectors.references.length} references agree with an independent implementation`
)
