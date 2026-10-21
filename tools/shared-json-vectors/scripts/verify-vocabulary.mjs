/**
 * Check the committed Intent Surface-Class Vocabulary Artifact vectors against
 * this implementation.
 *
 * The Rust crate writes sealed vocabularies with their integrity and semantic
 * preimages and semantic digests, and sealed artifacts its reader refuses with
 * the reason it reports. This script shares no code with it: it removes the
 * integrity member itself, rebuilds the semantic preimage from the artifact,
 * re-frames and re-hashes both, and decides which of design 017's reader steps
 * 3 to 6 each refused artifact fails first.
 */

import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { digest, encode } from './framing.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const vectorsPath = resolve(here, '../../../crates/intlify_config/fixtures/vocabulary/vectors.json')

/** The digest domains design 017 registers for the artifact. */
const INTEGRITY_DOMAIN = 'intent-surface-class-vocabulary-integrity'
const SEMANTIC_DOMAIN = 'intent-surface-class-vocabulary-semantic-digest'

/** The complete IdentityToken grammar of design 017. */
const IDENTITY_TOKEN = /^[a-z0-9](?:[a-z0-9._-]*[a-z0-9])?$/

/**
 * Compare two strings by their unsigned UTF-8 bytes.
 *
 * @param left - The first string.
 * @param right - The second string.
 * @returns Negative, zero, or positive, as for a sort.
 */
function compareText(left, right) {
  return Buffer.compare(Buffer.from(left, 'utf8'), Buffer.from(right, 'utf8'))
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
 * The artifact without its top-level integrity member.
 *
 * @param artifact - A sealed artifact.
 * @returns The integrity preimage.
 */
function integrityPreimage(artifact) {
  const { integrityDigest: _, ...preimage } = artifact
  return preimage
}

/**
 * The kind, schema revision, and members, without the name and revision.
 *
 * @param artifact - An artifact.
 * @returns The semantic preimage.
 */
function semanticPreimage(artifact) {
  return {
    kind: artifact.kind,
    schemaRevision: artifact.schemaRevision,
    members: artifact.members
  }
}

/**
 * The reason for the first of reader steps 3 to 6 an artifact fails.
 *
 * @param artifact - An artifact that passed the boundary.
 * @returns The reason, or null when the artifact is intact.
 */
function firstFailure(artifact) {
  if (digest(INTEGRITY_DOMAIN, integrityPreimage(artifact)) !== artifact.integrityDigest) {
    return 'body-integrity-invalid'
  }
  const members = artifact.members
  if (!members.every(member => IDENTITY_TOKEN.test(member))) {
    return 'member-invalid'
  }
  if (new Set(members).size !== members.length) {
    return 'member-duplicate'
  }
  for (let index = 1; index < members.length; index += 1) {
    if (compareText(members[index - 1], members[index]) >= 0) {
      return 'non-canonical-order'
    }
  }
  return null
}

/**
 * Check every intact vocabulary's preimages and digests.
 *
 * @param vectors - The vectors document.
 * @param fail - Records one failed check.
 */
function checkVocabularies(vectors, fail) {
  for (const vector of vectors.vocabularies) {
    const { artifact } = vector
    if (artifact.kind !== 'intent-surface-class-vocabulary' || artifact.schemaRevision !== '0') {
      fail(`${vector.id}: not a revision "0" vocabulary artifact`)
    }
    const integrity = integrityPreimage(artifact)
    if (!same(integrity, vector.integrityPreimage)) {
      fail(`${vector.id}: the integrity preimage is not the artifact without integrityDigest`)
    }
    const sealed = digest(INTEGRITY_DOMAIN, integrity)
    if (sealed !== artifact.integrityDigest) {
      fail(`${vector.id}:\n  committed ${artifact.integrityDigest}\n  produced  ${sealed}`)
    }
    const semantic = semanticPreimage(artifact)
    if (!same(semantic, vector.semanticPreimage)) {
      fail(`${vector.id}: the semantic preimage is not kind, schemaRevision, and members`)
    }
    const produced = digest(SEMANTIC_DOMAIN, semantic)
    if (produced !== vector.semanticDigest) {
      fail(`${vector.id}:\n  committed ${vector.semanticDigest}\n  produced  ${produced}`)
    }
    const failure = firstFailure(artifact)
    if (failure !== null) {
      fail(`${vector.id}: an intact vocabulary fails with ${failure}`)
    }
  }
}

/**
 * Check that renaming or revising keeps the semantic digest and that changing
 * the members changes it.
 *
 * @param vectors - The vectors document.
 * @param fail - Records one failed check.
 */
function checkEdits(vectors, fail) {
  const byId = new Map(vectors.vocabularies.map(vector => [vector.id, vector]))
  for (const [left, right] of vectors.sameSemanticDigest) {
    const [a, b] = [byId.get(left), byId.get(right)]
    const renamed = a.artifact.identity !== b.artifact.identity
    const revised = a.artifact.revision !== b.artifact.revision
    if (!same(a.artifact.members, b.artifact.members) || !(renamed || revised)) {
      fail(`${left} and ${right}: not the same members under another name or revision`)
    }
    if (a.semanticDigest !== b.semanticDigest) {
      fail(`${left} and ${right}: renaming or revising changed the semantic digest`)
    }
  }
  for (const [left, right] of vectors.differentSemanticDigest) {
    const [a, b] = [byId.get(left), byId.get(right)]
    if (same(a.artifact.members, b.artifact.members)) {
      fail(`${left} and ${right}: the members are the same`)
    }
    if (a.semanticDigest === b.semanticDigest) {
      fail(`${left} and ${right}: other members kept the semantic digest`)
    }
  }
}

/**
 * Check that every refused artifact fails where its vector says.
 *
 * @param vectors - The vectors document.
 * @param fail - Records one failed check.
 */
function checkRefused(vectors, fail) {
  for (const vector of vectors.refused) {
    const failure = firstFailure(vector.artifact)
    if (failure !== vector.reason) {
      fail(`${vector.id}:\n  committed ${vector.reason}\n  produced  ${failure}`)
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
checkVocabularies(vectors, fail)
checkEdits(vectors, fail)
checkRefused(vectors, fail)

if (failures > 0) {
  console.error(`\n${failures} vocabulary vector check(s) failed`)
  process.exit(1)
}
console.log(
  `${vectors.vocabularies.length} vocabularies and ${vectors.refused.length} refused artifacts agree with an independent implementation`
)
