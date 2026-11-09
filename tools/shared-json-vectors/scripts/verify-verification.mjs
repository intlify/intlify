/**
 * Check the committed Resource Limit Policy Verification Input vectors against
 * this implementation.
 *
 * The Rust crate writes sealed inputs with their integrity and semantic
 * preimages and semantic digests, and raw inputs with the bounds, the
 * assertion, and the outcome of 015's resource-limit-policy-verification-input
 * boundary. This script shares no code with it: it removes the integrity
 * member itself, rebuilds the semantic preimage, re-frames and re-hashes both,
 * and reads every raw input in the boundary's order to decide whether it is
 * admitted or which check refuses it first. It does not read the design
 * documents.
 */

import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { digest, encode } from './framing.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const vectorsPath = resolve(
  here,
  '../../../crates/intlify_config/fixtures/verification/vectors.json'
)

/** The digest domains design 017 registers for the input. */
const INTEGRITY_DOMAIN = 'resource-limit-policy-verification-input-integrity'
const SEMANTIC_DOMAIN = 'resource-limit-policy-verification-input-semantic-digest'

/** The one kind, schema revision, and mode of revision "0". */
const KIND = 'resource-limit-policy-verification-input'
const SCHEMA_REVISION = '0'
const EXACT_PIN = 'exact-pin'

/** Design 017's token grammar, for identities and revisions alike. */
const TOKEN = /^[a-z0-9](?:[a-z0-9._-]*[a-z0-9])?$/

/** Design 017's spelling of a digest. */
const DIGEST = /^sha256:[0-9a-f]{64}$/

/** The kinds a 017 `PolicyReference` may name. */
const POLICY_KINDS = new Set([
  'resource-limit-policy',
  'trust-policy',
  'source-admission-policy',
  'approval-policy',
  'selection-policy',
  'provider-routing-policy',
  'glossary-set'
])

/** The closed input's members, and a pin's. */
const MEMBERS = [
  'identity',
  'integrityDigest',
  'kind',
  'mode',
  'pins',
  'revision',
  'schemaRevision'
]
const PIN_MEMBERS = ['identity', 'kind', 'revision', 'semanticDigest', 'specificationRevision']

/** The members design 018 reserves, to be refused. */
const SECRET_BEARING = new Set(['privateKey', 'credential', 'secretLocator'])
const AMBIENT_AUTHORITY = new Set(['trustStore', 'networkSource', 'hostTrust'])

/** The reason 015's boundary row gives each check. */
const REASONS = {
  presence: 'missing-input',
  bytes: 'over-bound-input',
  depth: 'over-bound-input',
  'pin-count': 'over-bound-input',
  syntax: 'malformed-input',
  shape: 'malformed-input',
  'repeated-member': 'duplicate-member',
  'unknown-member': 'unknown-member',
  'secret-bearing-member': 'secret-bearing-input',
  'ambient-authority-member': 'ambient-authority',
  integrity: 'malformed-input',
  mode: 'malformed-input',
  pins: 'malformed-input',
  identity: 'verification-identity-mismatch',
  revision: 'verification-revision-mismatch',
  'semantic-digest': 'verification-digest-mismatch'
}

/**
 * Whether a value is a JSON object.
 *
 * @param value - The value.
 * @returns Whether it is an object that is not an array.
 */
function isObject(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
}

/**
 * Whether two JSON values are the same value, whatever their member order.
 *
 * @param left - The first value.
 * @param right - The second value.
 * @returns Whether their canonical framings are equal.
 */
function same(left, right) {
  return encode(left).equals(encode(right))
}

/**
 * The deepest nesting of objects and arrays in raw bytes, counted outside
 * strings without decoding anything.
 *
 * @param bytes - The raw input.
 * @returns The depth.
 */
function depth(bytes) {
  let [current, deepest, quoted, escaped] = [0, 0, false, false]
  for (const byte of bytes) {
    if (quoted) {
      if (escaped) {
        escaped = false
      } else if (byte === 0x5c) {
        escaped = true
      } else if (byte === 0x22) {
        quoted = false
      }
      continue
    }
    if (byte === 0x22) {
      quoted = true
    } else if (byte === 0x7b || byte === 0x5b) {
      current += 1
      deepest = Math.max(deepest, current)
    } else if (byte === 0x7d || byte === 0x5d) {
      current = Math.max(0, current - 1)
    }
  }
  return deepest
}

/**
 * Whether one object of a valid JSON text names a member twice, comparing
 * the decoded names.
 *
 * @param text - A text that is one JSON value.
 * @returns Whether a member repeats.
 */
function repeats(text) {
  const open = []
  let index = 0
  while (index < text.length) {
    const character = text[index]
    if (character === '"') {
      let end = index + 1
      while (text[end] !== '"') {
        end += text[end] === '\\' ? 2 : 1
      }
      const top = open.at(-1)
      if (top?.names && top.expectName) {
        const name = JSON.parse(text.slice(index, end + 1))
        if (top.names.has(name)) {
          return true
        }
        top.names.add(name)
        top.expectName = false
      }
      index = end + 1
      continue
    }
    if (character === '{') {
      open.push({ names: new Set(), expectName: true })
    } else if (character === '[') {
      open.push({ names: null })
    } else if (character === '}' || character === ']') {
      open.pop()
    } else if (character === ',' && open.at(-1)?.names) {
      open.at(-1).expectName = true
    }
    index += 1
  }
  return false
}

/**
 * Take the members outside the closed shape out of the input and of every pin
 * that is an object, and say which kinds there were.
 *
 * @param value - The decoded input.
 * @returns The input without them, and what was outside.
 */
function sortOut(value) {
  const outside = { unknown: false, secretBearing: false, ambientAuthority: false }
  const keep = (object, shape) =>
    Object.fromEntries(
      Object.entries(object).filter(([name]) => {
        if (shape.includes(name)) {
          return true
        }
        if (SECRET_BEARING.has(name)) {
          outside.secretBearing = true
        } else if (AMBIENT_AUTHORITY.has(name)) {
          outside.ambientAuthority = true
        } else {
          outside.unknown = true
        }
        return false
      })
    )
  if (!isObject(value)) {
    return { inside: value, outside }
  }
  const inside = keep(value, MEMBERS)
  if (Array.isArray(inside.pins)) {
    inside.pins = inside.pins.map(pin => (isObject(pin) ? keep(pin, PIN_MEMBERS) : pin))
  }
  return { inside, outside }
}

/**
 * Whether a value with nothing outside the shape has 017's closed shape: the
 * kind and schema revision, token identity and revision, a mode string, pins
 * that are Policy references, and a digest.
 *
 * @param value - The value.
 * @returns Whether it does.
 */
function shaped(value) {
  const token = text => typeof text === 'string' && TOKEN.test(text)
  const spelled = text => typeof text === 'string' && DIGEST.test(text)
  const reference = pin =>
    isObject(pin) &&
    PIN_MEMBERS.every(name => Object.hasOwn(pin, name)) &&
    POLICY_KINDS.has(pin.kind) &&
    token(pin.identity) &&
    token(pin.revision) &&
    token(pin.specificationRevision) &&
    spelled(pin.semanticDigest)
  return (
    isObject(value) &&
    MEMBERS.every(name => Object.hasOwn(value, name)) &&
    value.kind === KIND &&
    value.schemaRevision === SCHEMA_REVISION &&
    token(value.identity) &&
    token(value.revision) &&
    typeof value.mode === 'string' &&
    Array.isArray(value.pins) &&
    value.pins.every(reference) &&
    spelled(value.integrityDigest)
  )
}

/**
 * The input without its top-level integrity member.
 *
 * @param input - An input.
 * @returns The integrity preimage.
 */
function integrityPreimage(input) {
  const { integrityDigest: _, ...preimage } = input
  return preimage
}

/**
 * The kind, schema revision, mode, and pins, without the name and revision.
 *
 * @param input - An input.
 * @returns The semantic preimage.
 */
function semanticPreimage(input) {
  return {
    kind: input.kind,
    schemaRevision: input.schemaRevision,
    mode: input.mode,
    pins: input.pins
  }
}

/**
 * Decide what the boundary does with one raw input under its bounds and
 * assertion.
 *
 * @param text - The raw input, or null when none was supplied.
 * @param limits - The bounds, as decimal strings.
 * @param assertion - What trusted setup asserts.
 * @returns The check that refuses the input first, or null when it is admitted.
 */
function decide(text, limits, assertion) {
  if (text === null) {
    return 'presence'
  }
  const bytes = Buffer.from(text, 'utf8')
  if (BigInt(bytes.length) > BigInt(limits.bytes)) {
    return 'bytes'
  }
  if (BigInt(depth(bytes)) > BigInt(limits.depth)) {
    return 'depth'
  }
  let value
  try {
    value = JSON.parse(text)
  } catch {
    return 'syntax'
  }
  const pins = isObject(value) && Array.isArray(value.pins) ? value.pins.length : 0
  if (BigInt(pins) > BigInt(limits.pins)) {
    return 'pin-count'
  }
  const { inside, outside } = sortOut(value)
  if (!shaped(inside)) {
    return 'shape'
  }
  if (repeats(text)) {
    return 'repeated-member'
  }
  if (outside.unknown) {
    return 'unknown-member'
  }
  if (outside.secretBearing) {
    return 'secret-bearing-member'
  }
  if (outside.ambientAuthority) {
    return 'ambient-authority-member'
  }
  if (digest(INTEGRITY_DOMAIN, integrityPreimage(inside)) !== inside.integrityDigest) {
    return 'integrity'
  }
  if (inside.mode !== EXACT_PIN) {
    return 'mode'
  }
  const framed = inside.pins.map(pin => encode(pin))
  const ascending = framed.every(
    (bytes, index) => index === 0 || Buffer.compare(framed[index - 1], bytes) < 0
  )
  if (
    inside.pins.length === 0 ||
    !inside.pins.every(pin => pin.kind === 'resource-limit-policy') ||
    !ascending
  ) {
    return 'pins'
  }
  if (inside.identity !== assertion.identity) {
    return 'identity'
  }
  if (inside.revision !== assertion.revision) {
    return 'revision'
  }
  if (digest(SEMANTIC_DOMAIN, semanticPreimage(inside)) !== assertion.semanticDigest) {
    return 'semantic-digest'
  }
  return null
}

/**
 * Check every sealed input's preimages and digests, and that the boundary
 * admits it against its own assertion.
 *
 * @param vectors - The vectors document.
 * @param fail - Records one failed check.
 */
function checkInputs(vectors, fail) {
  const largest = '18446744073709551614'
  for (const vector of vectors.inputs) {
    const { input } = vector
    const integrity = integrityPreimage(input)
    if (!same(integrity, vector.integrityPreimage)) {
      fail(`${vector.id}: the integrity preimage is not the input without integrityDigest`)
    }
    const sealed = digest(INTEGRITY_DOMAIN, integrity)
    if (sealed !== input.integrityDigest) {
      fail(`${vector.id}:\n  committed ${input.integrityDigest}\n  produced  ${sealed}`)
    }
    const semantic = semanticPreimage(input)
    if (!same(semantic, vector.semanticPreimage)) {
      fail(`${vector.id}: the semantic preimage is not kind, schemaRevision, mode, and pins`)
    }
    const produced = digest(SEMANTIC_DOMAIN, semantic)
    if (produced !== vector.semanticDigest) {
      fail(`${vector.id}:\n  committed ${vector.semanticDigest}\n  produced  ${produced}`)
    }
    const assertion = {
      identity: input.identity,
      revision: input.revision,
      semanticDigest: produced
    }
    const limits = { bytes: largest, depth: largest, pins: largest }
    const refused = decide(JSON.stringify(input), limits, assertion)
    if (refused !== null) {
      fail(`${vector.id}: the boundary refuses a sealed input at ${refused}`)
    }
  }
}

/**
 * Check that renaming or revising keeps the semantic digest and that changing
 * the pins changes it.
 *
 * @param vectors - The vectors document.
 * @param fail - Records one failed check.
 */
function checkEdits(vectors, fail) {
  const byId = new Map(vectors.inputs.map(vector => [vector.id, vector]))
  for (const [left, right] of vectors.sameSemanticDigest) {
    const [a, b] = [byId.get(left), byId.get(right)]
    const renamed = a.input.identity !== b.input.identity
    const revised = a.input.revision !== b.input.revision
    if (!same(semanticPreimage(a.input), semanticPreimage(b.input)) || !(renamed || revised)) {
      fail(`${left} and ${right}: not the same pins under another name or revision`)
    }
    if (a.semanticDigest !== b.semanticDigest) {
      fail(`${left} and ${right}: renaming or revising changed the semantic digest`)
    }
  }
  for (const [left, right] of vectors.differentSemanticDigest) {
    const [a, b] = [byId.get(left), byId.get(right)]
    if (same(a.input.pins, b.input.pins)) {
      fail(`${left} and ${right}: the pins are the same`)
    }
    if (a.semanticDigest === b.semanticDigest) {
      fail(`${left} and ${right}: other pins kept the semantic digest`)
    }
  }
}

/**
 * Check that the boundary reaches every raw input's stated outcome.
 *
 * @param vectors - The vectors document.
 * @param fail - Records one failed check.
 */
function checkBoundary(vectors, fail) {
  for (const vector of vectors.boundary) {
    const check = decide(vector.text, vector.limits, vector.assertion)
    const outcome = check === null ? { admitted: true } : { reason: REASONS[check], check }
    if (!same(outcome, vector.outcome)) {
      fail(
        `${vector.id}:\n  committed ${JSON.stringify(vector.outcome)}\n  produced  ${JSON.stringify(outcome)}`
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
if (vectors.integrityDomain !== INTEGRITY_DOMAIN || vectors.semanticDomain !== SEMANTIC_DOMAIN) {
  fail('the vectors do not use the digest domains design 017 registers')
}
checkInputs(vectors, fail)
checkEdits(vectors, fail)
checkBoundary(vectors, fail)

if (failures > 0) {
  console.error(`\n${failures} verification input vector check(s) failed`)
  process.exit(1)
}
console.log(
  `${vectors.inputs.length} inputs and ${vectors.boundary.length} boundary cases agree with an independent implementation`
)
