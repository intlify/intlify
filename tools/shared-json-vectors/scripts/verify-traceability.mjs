/**
 * Check the committed traceability report against the suite and design 015.
 *
 * The Rust crate writes the conformance suite's traceability report beside
 * the Suite Index. This script shares no code with it. It reads the report,
 * the suite, the committed registries and construction-admission body, the
 * Case Schema's boundary precedence, and design 015's Decision Log,
 * decision-class table, Coverage inventory, and conditional obligation
 * table, builds its own list of every subject of each kind, and checks that
 * each has exactly one record or one gap, that each record follows 015's
 * rules for its status (D15:3431-3444), that a record is mapped exactly
 * where its own reading of the suite finds evidence, and that the summary
 * counts each kind's records and gaps.
 */

import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { isDeepStrictEqual } from 'node:util'

import { repeatsMember } from './strict-json.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const repository = resolve(here, '../../..')
const root = resolve(repository, 'crates/intlify_config/conformance')
const DESIGN = 'design/015-intlify-project-profile-and-locale-policy-design.md'

const KINDS = [
  'decision',
  'check',
  'construction-rule',
  'input-admission-rule',
  'construction-reason',
  'boundary-kind',
  'safe-origin-kind',
  'finding-code',
  'finding-reason',
  'registry-member',
  'case',
  'handoff-vector',
  'conditional-obligation',
  'relation',
  'coverage-item',
  'benchmark-pair',
  'benchmark-case'
]
const NOT_INVENTORIED = ['benchmark-pair', 'benchmark-case']
const SELF = 'intlify_config'
const ARRAYS = {
  resolverConstructionCases: 'resolver-construction',
  resolverInputAdmissionCases: 'resolver-input-admission',
  resolverCases: 'resolver',
  handoffVectors: 'handoff-vector',
  conditionalObligations: 'conditional-obligation'
}

let failures = 0

/**
 * Compares two strings by their UTF-8 bytes.
 *
 * @param left - One string.
 * @param right - The other.
 * @returns Their order.
 */
function byBytes(left, right) {
  return Buffer.compare(Buffer.from(left), Buffer.from(right))
}

/**
 * Report one failure.
 *
 * @param message - What is wrong.
 */
function fail(message) {
  failures += 1
  console.error(message)
}

/**
 * Read a JSON file below the suite root strictly.
 *
 * @param path - The suite-root-relative path.
 * @returns The bytes and the parsed value.
 */
function read(path) {
  const bytes = readFileSync(resolve(root, path))
  const text = bytes.toString('utf8')
  if (repeatsMember(text)) {
    fail(`${path}: repeats a member`)
  }
  return { bytes, value: JSON.parse(text) }
}

/**
 * Every `path` member's string value at any depth of a value.
 *
 * @param value - The value.
 * @param paths - Where to collect them.
 * @returns The paths.
 */
function pathsIn(value, paths = []) {
  if (Array.isArray(value)) {
    value.forEach(item => pathsIn(item, paths))
  } else if (value !== null && typeof value === 'object') {
    for (const [name, member] of Object.entries(value)) {
      if (name === 'path' && typeof member === 'string') {
        paths.push(member)
      } else {
        pathsIn(member, paths)
      }
    }
  }
  return paths
}

/**
 * Every (code, reason) a value holds at any depth, as `code` and
 * `code/reason` strings.
 *
 * @param value - The value.
 * @param found - Where to collect them.
 * @returns The strings.
 */
function findingsIn(value, found = new Set()) {
  if (Array.isArray(value)) {
    value.forEach(item => findingsIn(item, found))
  } else if (value !== null && typeof value === 'object') {
    if (typeof value.code === 'string') {
      found.add(value.code)
      if (typeof value.reason === 'string') {
        found.add(`${value.code}/${value.reason}`)
      }
    }
    Object.values(value).forEach(member => findingsIn(member, found))
  }
  return found
}

// design 015's inventories.
const design = readFileSync(resolve(repository, DESIGN), 'utf8').split('\n')
const decisionClass = new Map()
for (const line of design) {
  const match = /^\| (implementation-measurement|non-resolver) \| (.*) \|$/.exec(line)
  for (const id of match ? match[2].split(', ') : []) {
    decisionClass.set(id, match[1])
  }
}
const decisions = design.flatMap(line => /^\| (015-\d{3}) \|/.exec(line)?.slice(1) ?? [])
const items = design.flatMap(line => /^- `(015-COV-\d{3})`/.exec(line)?.slice(1) ?? [])
const rows = new Map()
for (const line of design) {
  const match = /^\| `(015-(?:XV|AD|HV)-\d{3})` \|/.exec(line)
  if (match) {
    rows.set(match[1], line.match(/015-COV-\d{3}(?:\/[0-9a-z-]+)?/g) ?? [])
  }
}
const parts = new Set([...rows.values()].flat().filter(reference => reference.includes('/')))
const coverage = items.flatMap(item => {
  const split = [...parts].filter(part => part.startsWith(`${item}/`))
  return split.length > 0 ? split : [item]
})

// The suite.
const index = read('suite-index-v0.json').value
const manifests = []
for (const [array, kindClass] of Object.entries(ARRAYS)) {
  for (const entry of index[array] ?? []) {
    manifests.push({ array, kindClass, value: read(entry.path).value })
  }
}
/**
 * The IDs of the manifests whose traceability lists an ID under a member.
 *
 * @param member - `decisionIds`, `ruleIds`, or `checkIds`.
 * @param id - The ID.
 * @returns The manifest IDs.
 */
function tracing(member, id) {
  return manifests
    .filter(manifest => manifest.value.traceability?.[member]?.includes(id))
    .map(manifest => manifest.value.id)
}
/**
 * The IDs of one array's manifests that satisfy a predicate.
 *
 * @param array - The Suite Index array.
 * @param predicate - The predicate on a manifest.
 * @returns The manifest IDs.
 */
function casesOf(array, predicate) {
  return manifests
    .filter(manifest => manifest.array === array && predicate(manifest.value))
    .map(manifest => manifest.value.id)
}

// What the report should hold: each subject's evidence, or none for a gap.
const expected = new Map(KINDS.map(kind => [kind, new Map()]))
/**
 * Expect a subject, mapped by this evidence when it has any.
 *
 * @param kind - The subject kind.
 * @param id - The subject ID.
 * @param evidence - The evidence IDs.
 */
function expect(kind, id, evidence) {
  expected.get(kind).set(id, [...new Set(evidence)].sort(byBytes))
}
for (const id of decisions) {
  expect('decision', id, tracing('decisionIds', id))
}
for (const row of read('registries/check-v0.json').value.rows) {
  expect('check', row.checkId, tracing('checkIds', row.checkId))
}
const body = read('construction/construction-admission-v0.json').value
for (const row of body.failurePrecedence) {
  const rule = `015.construction.${row.component}.${row.check}`
  expect('construction-rule', rule, tracing('ruleIds', rule))
  expect(
    'construction-reason',
    row.reason,
    casesOf(
      'resolverConstructionCases',
      manifest => manifest.expected?.failureReason === row.reason
    )
  )
}
const precedence = read('schemas/case-v0.schema.json').value.failurePrecedence
for (const [kind, kindRows] of Object.entries(precedence)) {
  if (kind === 'resolver-construction') {
    continue
  }
  for (const reason of new Set(kindRows.map(row => row.reason))) {
    const rule = `015.input-admission.${kind}.${reason}`
    expect('input-admission-rule', rule, tracing('ruleIds', rule))
  }
  const cases = casesOf('resolverInputAdmissionCases', manifest => manifest.boundaryKind === kind)
  const admitted = flag =>
    manifests.some(
      manifest => cases.includes(manifest.value.id) && manifest.value.expected?.admitted === flag
    )
  expect('boundary-kind', kind, admitted(true) && admitted(false) ? cases : [])
}
const fixtures = manifests
  .filter(manifest => ['resolverCases', 'resolverInputAdmissionCases'].includes(manifest.array))
  .flatMap(manifest =>
    pathsIn(manifest.value).map(path => ({
      id: manifest.value.id,
      text: read(path).bytes.toString('utf8')
    }))
  )
for (const row of read('registries/safe-origin-kind-v0.json').value.rows) {
  expect(
    'safe-origin-kind',
    row.id,
    fixtures.filter(fixture => fixture.text.includes(`"${row.id}"`)).map(fixture => fixture.id)
  )
}
const findings = manifests
  .filter(manifest => manifest.array === 'resolverCases')
  .map(manifest => ({
    id: manifest.value.id,
    found: findingsIn(
      pathsIn(manifest.value.entries)
        .filter(path => path.endsWith('-expected.json'))
        .map(path => read(path).value)
    )
  }))
for (const row of read('registries/finding-v0.json').value.rows) {
  const by = key => findings.filter(finding => finding.found.has(key)).map(finding => finding.id)
  expect('finding-code', row.code, by(row.code))
  for (const reason of row.allowedReasons ?? []) {
    expect('finding-reason', `${row.code}/${reason}`, by(`${row.code}/${reason}`))
  }
}
const constructing = casesOf(
  'resolverConstructionCases',
  manifest => manifest.expected?.constructed === true
)
for (const role of Object.keys(read('registries/package-v0.json').value.members)) {
  expect('registry-member', role, constructing)
}
const covered = new Map()
for (const manifest of manifests) {
  const { array, value } = manifest
  if (array === 'handoffVectors' || array === 'conditionalObligations') {
    expect(ARRAYS[array], value.id, [value.id])
    continue
  }
  expect('case', value.id, Object.values(value.traceability ?? {}).flat())
  for (const relation of value.relations ?? []) {
    expect('relation', `${value.id}/${relation.kind}/${relation.case}`, [value.id])
    if (typeof relation.class === 'string') {
      covered.set(relation.class, [...(covered.get(relation.class) ?? []), value.id])
    }
  }
}
const obligations = new Map(
  manifests
    .filter(manifest => manifest.array === 'conditionalObligations')
    .map(manifest => [manifest.value.id, manifest.value])
)
const notApplicable = new Map()
for (const [row, named] of rows) {
  for (const subject of named) {
    if (obligations.get(row)?.activationState === 'inactive' && !notApplicable.has(subject)) {
      notApplicable.set(subject, obligations.get(row))
    }
  }
}
for (const subject of coverage) {
  expect(
    'coverage-item',
    subject,
    notApplicable.has(subject) ? [notApplicable.get(subject).id] : (covered.get(subject) ?? [])
  )
}

// The report.
const report = read('traceability-v0.json').value
const seen = new Map(KINDS.map(kind => [kind, new Map()]))
for (const record of report.records ?? []) {
  const { kind, id } = record.subject ?? {}
  const of = seen.get(kind)
  if (of === undefined || of.has(id)) {
    fail(`record ${kind}/${id}: is of no kind or repeats a subject`)
    continue
  }
  of.set(id, record)
  const want = expected.get(kind).get(id)
  const evidence = [...(record.evidence ?? [])].sort(byBytes)
  const status = record.verificationStatus
  const classes = record.classes ?? []
  if (
    want === undefined ||
    want.length === 0 ||
    !isDeepStrictEqual(evidence, want) ||
    classes.length === 0
  ) {
    fail(`record ${kind}/${id}: does not have the evidence the suite gives it`)
  }
  if (Object.hasOwn(record, 'decisionClass') !== (kind === 'decision')) {
    fail(`record ${kind}/${id}: a decision's record alone names its class`)
  }
  if (
    kind === 'decision' &&
    record.decisionClass !== (decisionClass.get(id) ?? 'resolver-semantic')
  ) {
    fail(`record ${kind}/${id}: does not have its class from 015's table`)
  }
  if (kind === 'handoff-vector') {
    const owner = manifests.find(manifest => manifest.value.id === id)?.value.verificationOwner
    if (
      status !== 'owned-elsewhere' ||
      record.reason !== 'verified-by-owner' ||
      record.verificationOwner !== owner
    ) {
      fail(`record ${kind}/${id}: a vector is verified by its owner`)
    }
  } else if (
    kind === 'conditional-obligation' ||
    (kind === 'coverage-item' && notApplicable.has(id))
  ) {
    const obligation =
      kind === 'conditional-obligation' ? obligations.get(id) : notApplicable.get(id)
    if (
      status !== 'not-applicable' ||
      record.reason !== obligation?.notApplicableReason ||
      record.verificationOwner !== obligation?.verificationOwner
    ) {
      fail(`record ${kind}/${id}: is not applicable with its obligation's reason and owner`)
    }
  } else if (status !== 'mapped' || record.reason !== null || record.verificationOwner !== SELF) {
    fail(`record ${kind}/${id}: is not mapped by this suite`)
  }
}
for (const gap of report.gaps ?? []) {
  const of = seen.get(gap.kind)
  if (of === undefined || of.has(gap.id)) {
    fail(`gap ${gap.kind}/${gap.id}: is of no kind or repeats a subject`)
    continue
  }
  of.set(gap.id, null)
  if (expected.get(gap.kind).get(gap.id)?.length !== 0) {
    fail(`gap ${gap.kind}/${gap.id}: has evidence or is no subject`)
  }
}
const summary = report.summary ?? []
if (
  !isDeepStrictEqual(
    summary.map(row => row.kind),
    KINDS
  )
) {
  fail("summary: does not list 015's subject kinds in order")
}
for (const row of summary) {
  if (NOT_INVENTORIED.includes(row.kind)) {
    if (
      !isDeepStrictEqual(row, { kind: row.kind, inventoried: false }) ||
      seen.get(row.kind).size > 0
    ) {
      fail(`summary ${row.kind}: is not inventoried yet`)
    }
    continue
  }
  const held = [...seen.get(row.kind).values()]
  const count = status => held.filter(record => record?.verificationStatus === status).length
  const want = {
    kind: row.kind,
    inventoried: true,
    subjects: expected.get(row.kind).size,
    mapped: count('mapped'),
    notApplicable: count('not-applicable'),
    ownedElsewhere: count('owned-elsewhere'),
    gaps: held.filter(record => record === null).length
  }
  if (!isDeepStrictEqual(row, want) || held.length !== expected.get(row.kind).size) {
    fail(`summary ${row.kind}: does not count every subject once`)
  }
}

if (failures > 0) {
  process.exit(1)
}
const counted = summary.filter(row => row.inventoried)
console.log(
  `traceability report agrees: ${counted.map(row => `${row.subjects} ${row.kind}`).join(', ')}; ${counted.reduce((gaps, row) => gaps + row.gaps, 0)} gaps`
)
