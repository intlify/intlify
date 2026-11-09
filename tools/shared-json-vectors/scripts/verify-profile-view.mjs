/**
 * Check the committed profile view vectors against this implementation.
 *
 * The Rust crate writes the profile view of each checked test profile with its
 * profile semantic digest, and names the view each case's inputs project. This
 * script shares no code with it: it checks every view against its own copy of
 * design 015's closed shape and canonical orders and against the committed
 * Profile Specification's semantic groups and bounds, re-frames and re-hashes
 * every view, and checks that distinct views have distinct digests. It does
 * not read the design document.
 */

import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { digest, encode } from './framing.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const crate = resolve(here, '../../../crates/intlify_config')
const vectorsPath = resolve(crate, 'fixtures/profile-view/vectors.json')
const specificationPath = resolve(
  crate,
  'conformance/authorities/project-profile-specification-v0.json'
)

/** The digest domain design 017 registers for the profile view. */
const DOMAIN = 'localization-project-profile-semantic-digest'

/** Design 017's spelling of a semantic digest. */
const SEMANTIC_DIGEST = /^sha256:[0-9a-f]{64}$/

/** A bound's shortest decimal digits; zero is no bound. */
const BOUND = /^[1-9]\d*$/

/** The largest bound: one below the largest unsigned 64-bit count. */
const LARGEST = 2n ** 64n - 2n

/** The five members of an exact reference. */
const REFERENCE = ['kind', 'identity', 'revision', 'specificationRevision', 'semanticDigest']

/** The kind design 017 expects of each required policy reference. */
const REQUIRED_POLICIES = {
  resourceLimits: 'resource-limit-policy',
  trust: 'trust-policy',
  sourceAdmission: 'source-admission-policy',
  approval: 'approval-policy',
  selection: 'selection-policy'
}

/** The kind design 017 expects of each optional policy reference. */
const OPTIONAL_POLICIES = {
  providerRouting: 'provider-routing-policy',
  glossarySet: 'glossary-set'
}

/** Design 015's coverage modes. */
const MODES = new Set(['direct-required', 'fallback-allowed'])

/** The only placement mode revision "0" admits. */
const PLACEMENT_MODES = new Set(['duplicate'])

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
 * Whether a value is a JSON object.
 *
 * @param value - The value.
 * @returns Whether it is an object that is not an array.
 */
function isObject(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
}

/**
 * Whether a value is a non-empty string.
 *
 * @param value - The value.
 * @returns Whether it is a string with at least one character.
 */
function isText(value) {
  return typeof value === 'string' && value.length > 0
}

/**
 * Whether two lists hold the same strings in the same order.
 *
 * @param left - The first list.
 * @param right - The second list.
 * @returns Whether they are equal.
 */
function sameList(left, right) {
  return left.length === right.length && left.every((item, index) => item === right[index])
}

/**
 * The checks of one view, each reporting under the view's ID and a path.
 *
 * @param id - The view's ID.
 * @param fail - Records one failed check.
 * @returns The checks.
 */
function checks(id, fail) {
  const report = (path, message) => fail(`${id}: ${path}: ${message}`)
  /**
   * Check that a value is an object with exactly the given members.
   *
   * @param value - The value.
   * @param members - Its expected members, in any order.
   * @param path - Where the value is.
   * @returns Whether it is.
   */
  const closed = (value, members, path) => {
    if (!isObject(value)) {
      report(path, 'not an object')
      return false
    }
    const keys = Object.keys(value)
    if (keys.length !== members.length || !members.every(member => Object.hasOwn(value, member))) {
      report(path, `members ${keys.join(', ')}, not ${members.join(', ')}`)
      return false
    }
    return true
  }
  /**
   * Check that a value is a non-empty string.
   *
   * @param value - The value.
   * @param path - Where the value is.
   * @returns Whether it is.
   */
  const text = (value, path) => {
    if (!isText(value)) {
      report(path, 'not a non-empty string')
      return false
    }
    return true
  }
  /**
   * Check that a value is a semantic digest in design 017's spelling.
   *
   * @param value - The value.
   * @param path - Where the value is.
   */
  const semanticDigest = (value, path) => {
    if (typeof value !== 'string' || !SEMANTIC_DIGEST.test(value)) {
      report(path, 'not a sha256 semantic digest')
    }
  }
  /**
   * Check a specification's identity and revision.
   *
   * @param value - The value.
   * @param path - Where the value is.
   */
  const specification = (value, path) => {
    if (closed(value, ['identity', 'revision'], path)) {
      text(value.identity, `${path}/identity`)
      text(value.revision, `${path}/revision`)
    }
  }
  /**
   * Check an exact reference of the expected kind.
   *
   * @param value - The value.
   * @param kind - The kind design 017 expects in its slot.
   * @param path - Where the value is.
   */
  const reference = (value, kind, path) => {
    if (!closed(value, REFERENCE, path)) {
      return
    }
    if (value.kind !== kind) {
      report(`${path}/kind`, `${value.kind}, not ${kind}`)
    }
    for (const member of ['identity', 'revision', 'specificationRevision']) {
      text(value[member], `${path}/${member}`)
    }
    semanticDigest(value.semanticDigest, `${path}/semanticDigest`)
  }
  /**
   * Check that a value is a set: strings in ascending UTF-8 byte order, each
   * once.
   *
   * @param value - The value.
   * @param path - Where the value is.
   * @returns Whether it is a non-empty set.
   */
  const set = (value, path) => {
    if (!Array.isArray(value) || value.length === 0 || !value.every(isText)) {
      report(path, 'not a non-empty array of strings')
      return false
    }
    for (let index = 1; index < value.length; index += 1) {
      if (compareText(value[index - 1], value[index]) >= 0) {
        report(path, `${value[index - 1]} before ${value[index]}`)
        return false
      }
    }
    return true
  }
  return { report, closed, text, semanticDigest, specification, reference, set }
}

/**
 * List the bounds of a `projectProfileResolution` object as dotted paths.
 *
 * @param value - The object, or one of its groups.
 * @param prefix - The path of the group.
 * @param bounds - Collects each bound's path and value.
 */
function flatten(value, prefix, bounds) {
  for (const [name, member] of Object.entries(value)) {
    const path = prefix === '' ? name : `${prefix}.${name}`
    if (isObject(member)) {
      flatten(member, path, bounds)
    } else {
      bounds.push([path, member])
    }
  }
}

/**
 * Find the path of the first number in a value.
 *
 * @param value - The value.
 * @param path - Where the value is.
 * @returns The path, or null when the value holds no number.
 */
function numberIn(value, path) {
  if (typeof value === 'number') {
    return path
  }
  if (value !== null && typeof value === 'object') {
    for (const [name, member] of Object.entries(value)) {
      const found = numberIn(member, `${path}/${name}`)
      if (found !== null) {
        return found
      }
    }
  }
  return null
}

/**
 * Check one view against design 015's closed shape and canonical orders.
 *
 * @param id - The view's ID.
 * @param view - The view.
 * @param profile - The committed Profile Specification.
 * @param fail - Records one failed check.
 */
function checkView(id, view, profile, fail) {
  const { report, closed, text, semanticDigest, specification, reference, set } = checks(id, fail)
  const number = numberIn(view, '')
  if (number !== null) {
    report(number, 'a number, which the view never holds')
    return
  }
  if (!isObject(view) || !sameList(Object.keys(view), profile.semanticGroups)) {
    report('/', "the members are not the Profile Specification's semanticGroups, in order")
    return
  }

  if (
    closed(
      view.profileSpecification,
      ['identity', 'revision', 'semanticDigest'],
      '/profileSpecification'
    )
  ) {
    text(view.profileSpecification.identity, '/profileSpecification/identity')
    text(view.profileSpecification.revision, '/profileSpecification/revision')
    semanticDigest(view.profileSpecification.semanticDigest, '/profileSpecification/semanticDigest')
  }
  if (closed(view.projectIdentity, ['projectId', 'selectionScope'], '/projectIdentity')) {
    text(view.projectIdentity.projectId, '/projectIdentity/projectId')
    text(view.projectIdentity.selectionScope, '/projectIdentity/selectionScope')
  }
  const canonicalization = view.localeCanonicalization
  if (closed(canonicalization, ['specification', 'dataset'], '/localeCanonicalization')) {
    specification(canonicalization.specification, '/localeCanonicalization/specification')
    const dataset = canonicalization.dataset
    if (closed(dataset, ['identity', 'semanticDigest'], '/localeCanonicalization/dataset')) {
      text(dataset.identity, '/localeCanonicalization/dataset/identity')
      text(dataset.semanticDigest, '/localeCanonicalization/dataset/semanticDigest')
    }
  }

  const locales = view.projectLocales
  let requested = []
  let projectDefault = null
  if (
    closed(
      locales,
      ['requestedLocales', 'defaultRequestedLocale', 'defaultSourceLocale'],
      '/projectLocales'
    )
  ) {
    if (set(locales.requestedLocales, '/projectLocales/requestedLocales')) {
      requested = locales.requestedLocales
    }
    projectDefault = locales.defaultRequestedLocale
    if (!requested.includes(projectDefault)) {
      report('/projectLocales/defaultRequestedLocale', 'not a requested locale')
    }
    if (locales.defaultSourceLocale !== null) {
      text(locales.defaultSourceLocale, '/projectLocales/defaultSourceLocale')
    }
  }

  const negotiation = view.localeNegotiation
  if (closed(negotiation, ['specification', 'aliases'], '/localeNegotiation')) {
    specification(negotiation.specification, '/localeNegotiation/specification')
    if (isObject(negotiation.aliases)) {
      for (const [alias, target] of Object.entries(negotiation.aliases)) {
        text(alias, '/localeNegotiation/aliases')
        text(target, `/localeNegotiation/aliases/${alias}`)
      }
    } else {
      report('/localeNegotiation/aliases', 'not an object')
    }
  }

  const fallback = view.messageFallback
  if (closed(fallback, ['specification', 'mappings'], '/messageFallback')) {
    specification(fallback.specification, '/messageFallback/specification')
    if (isObject(fallback.mappings)) {
      for (const [source, candidates] of Object.entries(fallback.mappings)) {
        const path = `/messageFallback/mappings/${source}`
        if (!Array.isArray(candidates)) {
          report(path, 'not an array')
          continue
        }
        candidates.forEach((candidate, index) => {
          if (typeof candidate === 'string') {
            text(candidate, `${path}/${index}`)
          } else if (closed(candidate, ['kind'], `${path}/${index}`)) {
            if (candidate.kind !== 'intent-source-locale') {
              report(`${path}/${index}/kind`, 'not intent-source-locale')
            }
          }
        })
      }
    } else {
      report('/messageFallback/mappings', 'not an object')
    }
  }

  const coverage = view.coverage
  if (closed(coverage, ['specification', 'vocabulary', 'defaultMode', 'decisions'], '/coverage')) {
    specification(coverage.specification, '/coverage/specification')
    const vocabulary = coverage.vocabulary
    if (closed(vocabulary, ['identity', 'revision', 'semanticDigest'], '/coverage/vocabulary')) {
      text(vocabulary.identity, '/coverage/vocabulary/identity')
      text(vocabulary.revision, '/coverage/vocabulary/revision')
      semanticDigest(vocabulary.semanticDigest, '/coverage/vocabulary/semanticDigest')
    }
    if (!MODES.has(coverage.defaultMode)) {
      report('/coverage/defaultMode', 'not a coverage mode')
    }
    if (isObject(coverage.decisions)) {
      const rows = Object.keys(coverage.decisions).sort(compareText)
      if (!sameList(rows, requested)) {
        report('/coverage/decisions', 'its rows are not the requested locales')
      }
      let surfaces = null
      for (const [locale, row] of Object.entries(coverage.decisions)) {
        const path = `/coverage/decisions/${locale}`
        if (!isObject(row)) {
          report(path, 'not an object')
          continue
        }
        const classes = Object.keys(row).sort(compareText)
        surfaces ??= classes
        if (!sameList(classes, surfaces)) {
          report(path, 'its surface classes are not the other rows')
        }
        for (const [surface, mode] of Object.entries(row)) {
          if (!MODES.has(mode)) {
            report(`${path}/${surface}`, 'not a coverage mode')
          }
        }
      }
    } else {
      report('/coverage/decisions', 'not an object')
    }
  }

  const required = view.requiredPolicies
  const members = [...Object.keys(REQUIRED_POLICIES), 'projectProfileResolution']
  if (closed(required, members, '/requiredPolicies')) {
    for (const [slot, kind] of Object.entries(REQUIRED_POLICIES)) {
      reference(required[slot], kind, `/requiredPolicies/${slot}`)
    }
    const path = '/requiredPolicies/projectProfileResolution'
    if (isObject(required.projectProfileResolution)) {
      const bounds = []
      flatten(required.projectProfileResolution, '', bounds)
      const names = bounds.map(([name]) => name).sort(compareText)
      if (!sameList(names, [...profile.resourceBounds].sort(compareText))) {
        report(path, "its bounds are not the Profile Specification's resourceBounds")
      }
      for (const [name, value] of bounds) {
        if (typeof value !== 'string' || !BOUND.test(value) || BigInt(value) > LARGEST) {
          report(`${path}/${name}`, 'not a bound as its decimal string')
        }
      }
    } else {
      report(path, 'not an object')
    }
  }

  const optional = view.optionalPolicies
  if (closed(optional, Object.keys(OPTIONAL_POLICIES), '/optionalPolicies')) {
    for (const [slot, kind] of Object.entries(OPTIONAL_POLICIES)) {
      if (optional[slot] !== null) {
        reference(optional[slot], kind, `/optionalPolicies/${slot}`)
      }
    }
  }

  const targets = isObject(view.targets) ? view.targets : {}
  if (!isObject(view.targets) || Object.keys(targets).length === 0) {
    report('/targets', 'not a non-empty object')
  }
  for (const [target, entry] of Object.entries(targets)) {
    const path = `/targets/${target}`
    const fields = [
      'profile',
      'requestedLocales',
      'defaultRequestedLocale',
      'effectiveDefaultRequestedLocale'
    ]
    if (!closed(entry, fields, path)) {
      continue
    }
    reference(entry.profile, 'target-profile', `${path}/profile`)
    const subset = set(entry.requestedLocales, `${path}/requestedLocales`)
      ? entry.requestedLocales
      : []
    if (!subset.every(locale => requested.includes(locale))) {
      report(`${path}/requestedLocales`, 'not a subset of the requested locales')
    }
    if (entry.defaultRequestedLocale !== null && !subset.includes(entry.defaultRequestedLocale)) {
      report(`${path}/defaultRequestedLocale`, "not one of the target's locales")
    }
    const effective = entry.defaultRequestedLocale ?? projectDefault
    if (entry.effectiveDefaultRequestedLocale !== effective) {
      report(
        `${path}/effectiveDefaultRequestedLocale`,
        `${entry.effectiveDefaultRequestedLocale}, not ${effective}`
      )
    }
    if (!subset.includes(entry.effectiveDefaultRequestedLocale)) {
      report(`${path}/effectiveDefaultRequestedLocale`, "not one of the target's locales")
    }
  }

  const groups = isObject(view.deploymentGroups) ? view.deploymentGroups : {}
  if (!isObject(view.deploymentGroups) || Object.keys(groups).length === 0) {
    report('/deploymentGroups', 'not a non-empty object')
  }
  const placed = new Map()
  for (const [group, entry] of Object.entries(groups)) {
    const path = `/deploymentGroups/${group}`
    if (!closed(entry, ['members', 'hydrationRelations'], path)) {
      continue
    }
    const members = set(entry.members, `${path}/members`) ? entry.members : []
    for (const member of members) {
      if (!Object.hasOwn(targets, member)) {
        report(`${path}/members`, `${member} is no target`)
      }
      if (placed.has(member)) {
        report(`${path}/members`, `${member} is also in ${placed.get(member)}`)
      }
      placed.set(member, group)
    }
    const relations = entry.hydrationRelations
    if (!Array.isArray(relations)) {
      report(`${path}/hydrationRelations`, 'not an array')
      continue
    }
    relations.forEach((relation, index) => {
      const at = `${path}/hydrationRelations/${index}`
      if (!closed(relation, ['server', 'client'], at)) {
        return
      }
      for (const end of ['server', 'client']) {
        if (!members.includes(relation[end])) {
          report(`${at}/${end}`, 'not a member of its group')
        }
      }
      const before = relations[index - 1]
      if (index > 0 && isText(before?.server) && isText(before?.client)) {
        const order =
          compareText(before.server, relation.server) || compareText(before.client, relation.client)
        if (order >= 0) {
          report(at, 'not after its predecessor by server, then client')
        }
      }
    })
  }
  for (const target of Object.keys(targets)) {
    if (!placed.has(target)) {
      report('/deploymentGroups', `${target} is in no group`)
    }
  }

  const delivery = view.delivery
  if (closed(delivery, ['graph', 'placement', 'mode'], '/delivery')) {
    specification(delivery.graph, '/delivery/graph')
    specification(delivery.placement, '/delivery/placement')
    if (!PLACEMENT_MODES.has(delivery.mode)) {
      report('/delivery/mode', 'not the placement mode revision "0" admits')
    }
  }
}

/**
 * Check every view's shape and digest, and that distinct views have distinct
 * digests.
 *
 * @param vectors - The vectors document.
 * @param profile - The committed Profile Specification.
 * @param fail - Records one failed check.
 */
function checkViews(vectors, profile, fail) {
  const bytes = new Map()
  const digests = new Map()
  for (const { id, view, semanticDigest } of vectors.views) {
    checkView(id, view, profile, fail)
    let encoded
    try {
      encoded = encode(view).toString('hex')
    } catch (error) {
      fail(`${id}: the view has no canonical encoding: ${error.message}`)
      continue
    }
    if (bytes.has(encoded)) {
      fail(`${id}: the same view as ${bytes.get(encoded)}`)
    }
    bytes.set(encoded, id)
    const produced = digest(DOMAIN, view)
    if (produced !== semanticDigest) {
      fail(`${id}:\n  committed ${semanticDigest}\n  produced  ${produced}`)
    }
    if (digests.has(produced)) {
      fail(`${id}: the same digest as ${digests.get(produced)}`)
    }
    digests.set(produced, id)
  }
}

/**
 * Check that every case names a view, that every view is named by the case
 * that has its ID, and that some view is shared by several cases.
 *
 * @param vectors - The vectors document.
 * @param fail - Records one failed check.
 */
function checkCases(vectors, fail) {
  const views = new Set()
  for (const { id } of vectors.views) {
    if (views.has(id)) {
      fail(`${id}: two views have this ID`)
    }
    views.add(id)
  }
  const cases = new Map()
  for (const { id, view } of vectors.cases) {
    if (cases.has(id)) {
      fail(`${id}: two cases have this ID`)
    }
    cases.set(id, view)
    if (!views.has(view)) {
      fail(`${id}: names no view, ${view}`)
    }
  }
  for (const id of views) {
    if (cases.get(id) !== id) {
      fail(`${id}: the case with this ID does not name this view`)
    }
  }
  if (cases.size === views.size) {
    fail('no view is shared by several cases, so no case is another declaration of a profile')
  }
}

const vectors = JSON.parse(readFileSync(vectorsPath, 'utf8'))
const profile = JSON.parse(readFileSync(specificationPath, 'utf8'))
let failures = 0
const fail = message => {
  console.error(message)
  failures += 1
}
if (vectors.domain !== DOMAIN) {
  fail('the vectors do not use the digest domain design 017 registers')
}
checkViews(vectors, profile, fail)
checkCases(vectors, fail)

if (failures > 0) {
  console.error(`\n${failures} profile view vector check(s) failed`)
  process.exit(1)
}
console.log(
  `${vectors.views.length} views and ${vectors.cases.length} cases agree with an independent implementation`
)
