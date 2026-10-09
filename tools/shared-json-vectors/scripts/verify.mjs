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
const compilationPath = resolve(
  here,
  '../../../crates/intlify_authoring_identity/fixtures/phase3/compile-vectors.json'
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
  'intent-registry-update': { base: 'intent-registry', inventory: 'authoring-inventory' },
  'message-intent': { inventory: 'authoring-inventory', registry: 'intent-registry' },
  'message-reference': { inventory: 'authoring-inventory' }
}

/**
 * Spell a value with every object's members in sorted order, so two equal
 * values spell the same whatever order their members were written in.
 *
 * @param value - Any JSON value.
 * @returns The canonical spelling.
 */
function canonical(value) {
  if (Array.isArray(value)) {
    return `[${value.map(canonical).join(',')}]`
  }
  if (value !== null && typeof value === 'object') {
    const members = Object.keys(value)
      .sort()
      .map(key => `${JSON.stringify(key)}:${canonical(value[key])}`)
    return `{${members.join(',')}}`
  }
  return JSON.stringify(value)
}

/**
 * Compare two values completely.
 *
 * @param left - One value.
 * @param right - The other.
 * @returns Whether they are equal.
 */
function same(left, right) {
  return canonical(left) === canonical(right)
}

/**
 * Return whether a reference names exactly this artifact.
 *
 * @param artifact - A sealed artifact.
 * @param reference - An artifact reference.
 * @returns Whether every field of the reference matches the artifact.
 */
function names(artifact, reference) {
  return (
    artifact.kind === reference.kind &&
    artifact.schemaRevision === reference.schemaRevision &&
    artifact.authoringSpecification.identity === reference.authoringSpecification.identity &&
    artifact.authoringSpecification.revision === reference.authoringSpecification.revision &&
    artifact.integrityDigest === reference.integrityDigest
  )
}

/**
 * Find the one earlier artifact a reference names.
 *
 * @param seen - The artifacts before the one holding the reference.
 * @param reference - An artifact reference.
 * @returns The artifact, or undefined when none or several match.
 */
function resolveIn(seen, reference) {
  const matches = seen.filter(earlier => names(earlier.artifact, reference))
  return matches.length === 1 ? matches[0].artifact : undefined
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
      const artifact = resolveIn(seen, reference)
      if (reference.kind !== expected || artifact === undefined) {
        console.error(`${vector.id}: ${member} does not name exactly one earlier ${expected}`)
        failures += 1
        continue
      }
      resolved[member] = artifact
    }
    for (const [index, target] of (kind === 'message-reference' ? body.targets : []).entries()) {
      if (
        target.intentArtifact.kind !== 'message-intent' ||
        resolveIn(seen, target.intentArtifact) === undefined
      ) {
        console.error(
          `${vector.id}: target ${index} does not name exactly one earlier message-intent`
        )
        failures += 1
      }
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
    const { kind, body } = vector.artifact
    const carried =
      kind === 'intent-registry-update'
        ? body.decisions.map(decision => [decision.intentId, decision.basis])
        : kind === 'message-intent' && body.continuity !== undefined
          ? [[body.intentId, body.continuity.basis]]
          : []
    for (const [intentId, basis] of carried) {
      for (const edit of basis.changes ?? []) {
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
          console.error(`${vector.id}: an edit of ${intentId.value} does not replay`)
          failures += 1
        }
      }
    }
  }
  return failures
}

/**
 * Carry a range of an edit's before bytes across its replacements, under the
 * edit-replay profile's rules: a replacement before the range shifts it, one
 * after leaves it, one strictly inside moves its end, and one that touches
 * or crosses an end leaves nothing to carry.
 *
 * @param replacements - The edit's replacements, in before order.
 * @param range - A range of the before bytes.
 * @returns The carried range, or undefined when it cannot be carried.
 */
function carry(replacements, range) {
  const start = Number(range.start)
  const end = Number(range.end)
  let shift = 0
  let inner = 0
  for (const replacement of replacements) {
    const from = Number(replacement.range.start)
    const to = Number(replacement.range.end)
    const delta = Buffer.byteLength(replacement.text, 'utf8') - (to - from)
    if (from === to ? from < start : to < start) {
      shift += delta
    } else if (from > end) {
      continue
    } else if (start < from && to < end) {
      inner += delta
    } else {
      return undefined
    }
  }
  return { start: String(start + shift), end: String(end + shift + inner) }
}

/**
 * Compare two Intent IDs in 017's order: owner kind, owner identity, then
 * value, each by unsigned UTF-8 bytes.
 *
 * @param left - One Intent ID.
 * @param right - The other.
 * @returns Negative, zero or positive.
 */
function compareIds(left, right) {
  for (const [a, b] of [
    [left.owner.kind, right.owner.kind],
    [left.owner.identity, right.owner.identity],
    [left.value, right.value]
  ]) {
    const order = Buffer.compare(Buffer.from(a, 'utf8'), Buffer.from(b, 'utf8'))
    if (order !== 0) {
      return order
    }
  }
  return 0
}

/**
 * Check each Intent against the inventory and registry it names.
 *
 * Its declaration is one of the inventory's, its revision is recomputed from
 * that declaration's projection, and its ID is active in the registry. With
 * no continuity, the entry holds the declaration itself. With one, the entry
 * holds where it starts, its one edit under the edit-replay profile runs from
 * that snapshot to the declaration's and carries the range exactly onto the
 * declaration, and no other active entry is carried onto it too. One
 * compilation gives each declaration one Intent.
 *
 * @param document - The compilation document.
 * @returns How many Intents disagreed.
 */
function checkIntents(document) {
  let failures = 0
  const fail = (vector, message) => {
    console.error(`${vector.id}: ${message}`)
    failures += 1
  }
  const seen = []
  const compiled = new Map()
  for (const vector of document.artifacts) {
    const { kind, body } = vector.artifact
    const inventory = kind === 'message-intent' && resolveIn(seen, body.inventory)
    const registry = kind === 'message-intent' && resolveIn(seen, body.registry)
    seen.push(vector)
    if (!inventory || !registry) {
      continue
    }
    const facts = inventory.body.declarations.find(candidate =>
      same(candidate.occurrence, body.declaration)
    )
    if (facts === undefined) {
      fail(vector, "its declaration is not one of the inventory's")
      continue
    }
    const revision = digest(document.revisionDomain, {
      projectionSpecification: document.projectionSpecification,
      projection: facts.projection
    })
    if (revision !== body.intentRevision) {
      fail(vector, `its revision is not its declaration's: ${revision}`)
    }
    const key = canonical([body.inventory, body.registry, body.declaration])
    if (compiled.has(key)) {
      fail(vector, `its declaration already has ${compiled.get(key)}`)
    }
    compiled.set(key, vector.id)
    const active = registry.body.entries.filter(entry => entry.state === 'active')
    const entry = active.find(candidate => same(candidate.intentId, body.intentId))
    if (entry === undefined) {
      fail(vector, 'its ID is not active in the registry')
      continue
    }
    if (body.continuity === undefined) {
      if (!same(entry.declaration, body.declaration)) {
        fail(vector, 'its entry holds another declaration')
      }
      continue
    }
    const { from, basis } = body.continuity
    const [edit, ...others] = basis.changes ?? []
    if (
      !same(entry.declaration, from) ||
      basis.kind !== 'verified-edit' ||
      !same(basis.profile, document.editProfile) ||
      edit === undefined ||
      others.length > 0 ||
      !same(edit.before, from.source) ||
      !same(edit.after, body.declaration.source)
    ) {
      fail(vector, 'its continuity is not one verified edit from its entry to it')
      continue
    }
    const carried = carry(edit.replacements, from.range)
    if (
      carried === undefined ||
      !same(carried, body.declaration.range) ||
      from.role !== body.declaration.role
    ) {
      fail(vector, 'its edit does not carry its entry onto it')
    }
    const rivals = active.filter(
      other =>
        !same(other.intentId, body.intentId) &&
        (same(other.declaration, body.declaration) ||
          (same(other.declaration.source, edit.before) &&
            other.declaration.role === body.declaration.role &&
            same(carry(edit.replacements, other.declaration.range), body.declaration.range)))
    )
    if (rivals.length > 0) {
      fail(vector, 'another entry is held or carried onto its declaration too')
    }
  }
  return failures
}

/**
 * Check each reference against the inventory and the Intents it names.
 *
 * Its use site is one of the inventory's references, its targets are in
 * Intent ID order with no ID twice, each target is the ID and revision of
 * the Intent it names, and those Intents' declarations are exactly the
 * declarations the use site may use.
 *
 * @param document - The compilation document.
 * @returns How many references disagreed.
 */
function checkReferenceTargets(document) {
  let failures = 0
  const fail = (vector, message) => {
    console.error(`${vector.id}: ${message}`)
    failures += 1
  }
  const seen = []
  for (const vector of document.artifacts) {
    const { kind, body } = vector.artifact
    const inventory = kind === 'message-reference' && resolveIn(seen, body.inventory)
    seen.push(vector)
    if (!inventory) {
      continue
    }
    const facts = inventory.body.references.find(candidate =>
      same(candidate.occurrence, body.occurrence)
    )
    if (facts === undefined) {
      fail(vector, "its use site is not one of the inventory's references")
      continue
    }
    for (let index = 1; index < body.targets.length; index += 1) {
      if (compareIds(body.targets[index - 1].intentId, body.targets[index].intentId) >= 0) {
        fail(vector, 'its targets are not in Intent ID order, or repeat an ID')
      }
    }
    const declarations = []
    for (const target of body.targets) {
      const intent = resolveIn(seen, target.intentArtifact)
      if (
        intent === undefined ||
        !same(intent.body.intentId, target.intentId) ||
        intent.body.intentRevision !== target.intentRevision ||
        !same(intent.body.inventory, body.inventory)
      ) {
        fail(vector, `the target ${target.intentId.value} is not the Intent it names`)
        continue
      }
      declarations.push(canonical(intent.body.declaration))
    }
    const expected = facts.declarations.map(canonical)
    if (canonical(declarations.sort()) !== canonical(expected.sort())) {
      fail(vector, 'its targets are not the declarations its use site may use')
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

/**
 * Check the compilation of design 028's module: the registry chain it
 * starts with, then every Intent and reference compiled from it.
 *
 * @param compilation - The compilation document.
 * @returns How many checks failed.
 */
function checkCompilation(compilation) {
  return checkRegistry(compilation) + checkIntents(compilation) + checkReferenceTargets(compilation)
}

const document = JSON.parse(readFileSync(vectorsPath, 'utf8'))
const inventory = JSON.parse(readFileSync(inventoryPath, 'utf8'))
const registry = JSON.parse(readFileSync(registryPath, 'utf8'))
const compilation = JSON.parse(readFileSync(compilationPath, 'utf8'))
const failures =
  checkFraming() +
  checkRevisions(document.domain, document.vectors) +
  checkDistinctness(document.vectors) +
  checkIntegrity(inventory.domain, inventory.vectors) +
  checkSharedRevisions(inventory) +
  checkRegistry(registry) +
  checkCompilation(compilation)

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
console.log(
  `${compilation.artifacts.length} artifacts compiling design 028's module agree with an independent implementation`
)
