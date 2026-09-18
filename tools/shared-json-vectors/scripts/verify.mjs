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

const document = JSON.parse(readFileSync(vectorsPath, 'utf8'))
const failures =
  checkFraming() +
  checkRevisions(document.domain, document.vectors) +
  checkDistinctness(document.vectors)

if (failures > 0) {
  console.error(`\n${failures} vector check(s) failed`)
  process.exit(1)
}
console.log(`${document.vectors.length} revision vectors agree with an independent implementation`)
