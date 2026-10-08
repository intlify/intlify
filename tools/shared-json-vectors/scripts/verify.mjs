/**
 * Check the committed Intent revision vectors against this implementation.
 *
 * The Rust crate writes the vectors, each holding the exact digest preimage and
 * the digest it computed. This script reframes the preimage and re-hashes it
 * from the specification, so a disagreement says which of the two is wrong.
 */

import { createHash } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { digest, encode } from './framing.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const vectorsPath = resolve(
  here,
  '../../../crates/intlify_authoring/fixtures/phase1/revision-vectors.json'
)
const inventoryPath = resolve(
  here,
  '../../../crates/intlify_authoring/fixtures/phase2/inventory-vectors.json'
)
const registryPath = resolve(
  here,
  '../../../crates/intlify_authoring_identity/fixtures/phase3/registry-vectors.json'
)

/** The framing vectors written out in design 017, checked before anything depends on them. */
const framingVectors = [
  { label: 'null', value: null, hex: '010000000000000000' },
  { label: 'false', value: false, hex: '020000000000000000' },
  { label: 'true', value: true, hex: '030000000000000000' },
  { label: 'empty string', value: '', hex: '040000000000000000' },
  { label: 'one character', value: 'a', hex: '04000000000000000161' },
  { label: 'empty array', value: [], hex: '0500000000000000080000000000000000' },
  { label: 'empty object', value: {}, hex: '0600000000000000080000000000000000' }
]

/**
 * Check the framing primitives before the digests that build on them.
 *
 * @returns How many framing vectors disagreed.
 */
function checkFraming() {
  let failures = 0
  for (const vector of framingVectors) {
    const actual = encode(vector.value).toString('hex')
    if (actual !== vector.hex) {
      console.error(`framing ${vector.label}: expected ${vector.hex}, produced ${actual}`)
      failures += 1
    }
  }
  return failures
}

/**
 * Re-hash every committed revision vector from its published preimage.
 *
 * @param domain - The registered digest domain the vectors use.
 * @param vectors - Committed vectors.
 * @returns How many vectors disagreed.
 */
function checkRevisions(domain, vectors) {
  let failures = 0
  for (const vector of vectors) {
    const produced = digest(domain, vector.preimage)
    if (produced !== vector.revision) {
      console.error(`${vector.id}:\n  committed ${vector.revision}\n  produced  ${produced}`)
      failures += 1
    }
  }
  return failures
}

/**
 * Check that only vectors declared equal share a revision.
 *
 * Two messages collapsing into one revision is the failure this whole fixture
 * exists to catch, so it is asserted rather than left to the digests.
 *
 * @param vectors - Committed vectors.
 * @returns How many undeclared collisions were found.
 */
function checkDistinctness(vectors) {
  let failures = 0
  const seen = new Map()
  for (const vector of vectors) {
    const previous = seen.get(vector.revision)
    if (previous !== undefined && !vector.equalTo.includes(previous)) {
      console.error(`${vector.id} and ${previous} share a revision but are not declared equal`)
      failures += 1
    }
    seen.set(vector.revision, vector.id)
  }
  return failures
}

/**
 * Re-derive every sealed inventory's integrity digest from the artifact itself.
 *
 * The preimage is the artifact with only its top-level integrityDigest
 * removed. Doing the removal here, rather than reading a published preimage,
 * checks the exclusion rule as well as the framing and the hash.
 *
 * @param domain - The registered integrity domain.
 * @param vectors - Committed sealed artifacts.
 * @returns How many digests disagreed.
 */
function checkIntegrity(domain, vectors) {
  let failures = 0
  for (const vector of vectors) {
    const { integrityDigest, ...preimage } = vector.artifact
    const produced = digest(domain, preimage)
    if (produced !== integrityDigest) {
      console.error(`${vector.id}:\n  committed ${integrityDigest}\n  produced  ${produced}`)
      failures += 1
    }
  }
  return failures
}

/**
 * Recompute the revision of every declaration in one artifact.
 *
 * @param inventory - The inventory document.
 * @param artifact - One sealed artifact.
 * @returns The revisions in declaration order.
 */
function revisionsOf(inventory, artifact) {
  return artifact.body.declarations.map(facts =>
    digest(inventory.revisionDomain, {
      projectionSpecification: inventory.projectionSpecification,
      projection: facts.projection
    })
  )
}

/**
 * Check each vector's revisions against the Rust implementation, then check
 * that vectors declared to share revisions do while their artifacts differ.
 *
 * The second half is the claim that source evidence is part of what an
 * artifact records and not part of what a message means. It only means
 * something once the first half has tied these revisions to Rust's.
 *
 * @param inventory - The inventory document.
 * @returns How many declared relations failed.
 */
function checkSharedRevisions(inventory) {
  let failures = 0
  const byId = new Map(inventory.vectors.map(vector => [vector.id, vector]))
  for (const vector of inventory.vectors) {
    // Compare with what the Rust implementation computed first. Without this,
    // the relations below compare this implementation's answers with each
    // other, and equal projections agree under any deterministic function.
    const produced = revisionsOf(inventory, vector.artifact)
    if (JSON.stringify(produced) !== JSON.stringify(vector.revisions)) {
      console.error(
        `${vector.id}:\n  committed revisions ${JSON.stringify(vector.revisions)}\n  produced  revisions ${JSON.stringify(produced)}`
      )
      failures += 1
    }
    for (const otherId of vector.sameRevisionsAs) {
      const other = byId.get(otherId)
      if (other === undefined) {
        console.error(`${vector.id} names unknown vector ${otherId}`)
        failures += 1
        continue
      }
      const mine = revisionsOf(inventory, vector.artifact)
      const theirs = revisionsOf(inventory, other.artifact)
      if (JSON.stringify(mine) !== JSON.stringify(theirs)) {
        console.error(`${vector.id} and ${otherId} are declared to share revisions but do not`)
        failures += 1
      }
      if (vector.artifact.integrityDigest === other.artifact.integrityDigest) {
        console.error(`${vector.id} and ${otherId} differ in content but share an artifact digest`)
        failures += 1
      }
    }
  }
  return failures
}

/**
 * The kind each reference member has to name, by the kind of the artifact
 * holding it.
 */
const referenceMembers = {
  'intent-registry': { base: 'intent-registry', update: 'intent-registry-update' },
  'intent-registry-update': { base: 'intent-registry', inventory: 'authoring-inventory' }
}

/**
 * Find every source snapshot inside a value.
 *
 * @param value - Any part of an artifact.
 * @param found - Where to collect the snapshots.
 * @returns The snapshots found, in document order.
 */
function snapshotsIn(value, found = []) {
  if (Array.isArray(value)) {
    for (const element of value) {
      snapshotsIn(element, found)
    }
  } else if (value !== null && typeof value === 'object') {
    if (typeof value.utf8Digest === 'string' && typeof value.unit === 'string') {
      found.push(value)
    }
    for (const member of Object.values(value)) {
      snapshotsIn(member, found)
    }
  }
  return found
}

/**
 * Check every snapshot against the retained text it names.
 *
 * A snapshot that names no retained text, or names text with another digest
 * or length, would make every position recorded in it unverifiable.
 *
 * @param registry - The registry document.
 * @param texts - Retained texts by unit and revision.
 * @returns How many snapshots disagreed.
 */
function checkSnapshots(registry, texts) {
  let failures = 0
  for (const vector of registry.artifacts) {
    for (const snapshot of snapshotsIn(vector.artifact.body)) {
      const text = texts.get(`${snapshot.unit}@${snapshot.revision}`)
      if (text === undefined) {
        console.error(`${vector.id}: ${snapshot.unit}@${snapshot.revision} names no retained text`)
        failures += 1
        continue
      }
      const digest = `sha256:${createHash('sha256').update(text).digest('hex')}`
      if (digest !== snapshot.utf8Digest || String(text.length) !== snapshot.byteLength) {
        console.error(`${vector.id}: ${snapshot.unit}@${snapshot.revision} does not name its text`)
        failures += 1
      }
    }
  }
  return failures
}

/**
 * Check that every reference names exactly one earlier artifact of its kind.
 *
 * Earlier, because 017 builds a chain in one direction: an update names its
 * base and inventory, a snapshot names its base and update, and nothing names
 * its own result. A snapshot's update also has to be planned against that
 * snapshot's base, and a chain keeps its owner, scope and registry identity.
 *
 * @param registry - The registry document.
 * @returns How many references or chain links disagreed.
 */
function checkReferences(registry) {
  let failures = 0
  const seen = []
  for (const vector of registry.artifacts) {
    const { kind, body } = vector.artifact
    const resolved = {}
    for (const [member, expected] of Object.entries(referenceMembers[kind] ?? {})) {
      const reference = body[member]
      if (reference === undefined) {
        continue
      }
      const matches = seen.filter(
        earlier =>
          earlier.artifact.kind === reference.kind &&
          earlier.artifact.schemaRevision === reference.schemaRevision &&
          earlier.artifact.authoringSpecification.identity ===
            reference.authoringSpecification.identity &&
          earlier.artifact.authoringSpecification.revision ===
            reference.authoringSpecification.revision &&
          earlier.artifact.integrityDigest === reference.integrityDigest
      )
      if (reference.kind !== expected || matches.length !== 1) {
        console.error(`${vector.id}: ${member} does not name exactly one earlier ${expected}`)
        failures += 1
        continue
      }
      resolved[member] = matches[0].artifact
    }
    if (kind === 'intent-registry' && resolved.base !== undefined) {
      const base = resolved.base.body
      if (
        JSON.stringify(base.owner) !== JSON.stringify(body.owner) ||
        base.scope !== body.scope ||
        base.registryIdentity !== body.registryIdentity
      ) {
        console.error(`${vector.id}: the chain does not keep its owner, scope and identity`)
        failures += 1
      }
      if (resolved.update?.body.base.integrityDigest !== body.base.integrityDigest) {
        console.error(`${vector.id}: its update was planned against another base`)
        failures += 1
      }
    }
    seen.push(vector)
  }
  return failures
}

/**
 * Replay every source edit over the retained bytes.
 *
 * Replacements use the coordinates of the bytes before the edit and apply in
 * order. An absent side is an empty buffer.
 *
 * @param registry - The registry document.
 * @param texts - Retained texts by unit and revision.
 * @returns How many edits did not reproduce their after bytes.
 */
function checkEdits(registry, texts) {
  let failures = 0
  const bytesOf = snapshot =>
    snapshot === undefined
      ? Buffer.alloc(0)
      : (texts.get(`${snapshot.unit}@${snapshot.revision}`) ?? Buffer.alloc(0))
  for (const vector of registry.artifacts) {
    if (vector.artifact.kind !== 'intent-registry-update') {
      continue
    }
    for (const decision of vector.artifact.body.decisions) {
      for (const edit of decision.basis.changes ?? []) {
        const before = bytesOf(edit.before)
        const parts = []
        let position = 0
        for (const replacement of edit.replacements) {
          const start = Number(replacement.range.start)
          parts.push(before.subarray(position, start), Buffer.from(replacement.text, 'utf8'))
          position = Number(replacement.range.end)
        }
        parts.push(before.subarray(position))
        if (!Buffer.concat(parts).equals(bytesOf(edit.after))) {
          console.error(`${vector.id}: an edit of ${decision.intentId.value} does not replay`)
          failures += 1
        }
      }
    }
  }
  return failures
}

/**
 * Check the registry chain: integrity, references, retained text and edits.
 *
 * @param registry - The registry document.
 * @returns How many checks failed.
 */
function checkRegistry(registry) {
  const texts = new Map(
    registry.sources.map(source => [
      `${source.unit}@${source.revision}`,
      Buffer.from(source.text, 'utf8')
    ])
  )
  return (
    checkIntegrity(registry.domain, registry.artifacts) +
    checkSnapshots(registry, texts) +
    checkReferences(registry) +
    checkEdits(registry, texts)
  )
}

const document = JSON.parse(readFileSync(vectorsPath, 'utf8'))
const inventory = JSON.parse(readFileSync(inventoryPath, 'utf8'))
const registry = JSON.parse(readFileSync(registryPath, 'utf8'))
const failures =
  checkFraming() +
  checkRevisions(document.domain, document.vectors) +
  checkDistinctness(document.vectors) +
  checkIntegrity(inventory.domain, inventory.vectors) +
  checkSharedRevisions(inventory) +
  checkRegistry(registry)

if (failures > 0) {
  console.error(`\n${failures} vector check(s) failed`)
  process.exit(1)
}
console.log(`${document.vectors.length} revision vectors agree with an independent implementation`)
console.log(
  `${inventory.vectors.length} sealed inventories agree with an independent implementation`
)
console.log(
  `${registry.artifacts.length} artifacts of one registry chain agree with an independent implementation`
)
