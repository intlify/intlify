/**
 * Check the committed Intent revision vectors against this implementation.
 *
 * The Rust crate writes the vectors, each holding the exact digest preimage and
 * the digest it computed. This script reframes the preimage and re-hashes it
 * from the specification, so a disagreement says which of the two is wrong.
 */

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

const document = JSON.parse(readFileSync(vectorsPath, 'utf8'))
const inventory = JSON.parse(readFileSync(inventoryPath, 'utf8'))
const failures =
  checkFraming() +
  checkRevisions(document.domain, document.vectors) +
  checkDistinctness(document.vectors) +
  checkIntegrity(inventory.domain, inventory.vectors) +
  checkSharedRevisions(inventory)

if (failures > 0) {
  console.error(`\n${failures} vector check(s) failed`)
  process.exit(1)
}
console.log(`${document.vectors.length} revision vectors agree with an independent implementation`)
console.log(
  `${inventory.vectors.length} sealed inventories agree with an independent implementation`
)
