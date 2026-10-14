/**
 * Check the corpus gate's committed registry against this implementation.
 *
 * The Rust crate runs CLDR 48.2.0's locale canonicalization corpus through
 * the committed revision "0" artifact and writes one registry entry per
 * case. This script shares no code with it: it hashes the vendored corpus,
 * reads its sets and cases itself, projects each case, decides from the
 * committed conformance payload's validity groups whether the input is in the
 * admitted domain, and derives every disposition and count again from the
 * answers the registry records. It cannot run ICU4X, so the answers
 * themselves are the gate's; what it checks is that no case is missing,
 * moved, or explained by anything but the pinned data.
 */

import { createHash } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const crate = resolve(here, '../../../crates/intlify_locale_icu4x')
const CORPUS_PATH = 'common/testData/localeIdentifiers/localeCanonicalization.txt'
const corpusPath = resolve(crate, 'cldr', CORPUS_PATH)
const registryPath = resolve(crate, 'evidence/gap-registry.json')
const dataDirectory = resolve(crate, 'data')

/** The corpus's four sets, in their upstream order. */
const SETS = ['explicit', 'fromAliases', 'decanonicalized', 'withIrrelevants']

/** The validity statuses the layer admits on input. */
const ADMITTED = new Set(['regular', 'special', 'macroregion', 'unknown', 'deprecated'])

/** The dispositions, as the registry counts them. */
const DISPOSITIONS = {
  delegated: 'delegated',
  corrected: 'corrected',
  'outside-domain': 'outsideDomain',
  blocking: 'blocking'
}

/**
 * Read every case of the corpus, in its order.
 *
 * @param text - The corpus.
 * @returns The cases, or a description of the first line that is no case.
 */
function readCases(text) {
  const cases = []
  let set = -1
  const lines = text.split('\n')
  if (lines.at(-1) === '') {
    lines.pop()
  }
  for (const [index, line] of lines.entries()) {
    const number = index + 1
    const heading = line.startsWith('# ') ? SETS.indexOf(line.slice(2)) : -1
    if (heading >= 0) {
      if (heading !== set + 1) {
        return { problem: `line ${number}: set ${line.slice(2)} out of order` }
      }
      set = heading
      continue
    }
    if (line === '' || line.startsWith('#')) {
      continue
    }
    const fields = line.split('\t')
    if (set < 0 || fields.length !== 3 || fields[1] !== ';' || !fields[0] || !fields[2]) {
      return { problem: `line ${number} is no case` }
    }
    cases.push({ set: SETS[set], line: number, source: fields[0], expected: fields[2] })
  }
  if (set !== SETS.length - 1) {
    return { problem: 'the corpus ends before its last set' }
  }
  return { cases }
}

/**
 * Split a hyphen-separated identifier into its language identifier's
 * components, or report that it is none, under UTS #35's grammar.
 *
 * @param input - The projected input.
 * @returns The components, or null when the input is no locale identifier.
 */
function components(input) {
  const subtags = input.split('-')
  const language = subtags.shift() ?? ''
  if (!/^(?:[a-z]{2,3}|[a-z]{5,8})$/i.test(language)) {
    return null
  }
  const found = [{ component: 'language', field: 'language', code: language.toLowerCase() }]
  if (subtags.length > 0 && /^[a-z]{4}$/i.test(subtags[0])) {
    const script = subtags.shift().toLowerCase()
    found.push({
      component: 'script',
      field: 'script',
      code: script[0].toUpperCase() + script.slice(1)
    })
  }
  if (subtags.length > 0 && /^(?:[a-z]{2}|\d{3})$/i.test(subtags[0])) {
    found.push({ component: 'region', field: 'region', code: subtags.shift().toUpperCase() })
  }
  const variants = new Set()
  for (const subtag of subtags) {
    const variant = subtag.toLowerCase()
    if (!/^(?:[a-z\d]{5,8}|\d[a-z\d]{3})$/.test(variant) || variants.has(variant)) {
      return null
    }
    variants.add(variant)
    found.push({ component: 'variant', field: 'variant', code: variant })
  }
  return found
}

/**
 * Index the committed conformance payload's validity groups.
 *
 * @returns A map from field and code to its status.
 */
function validity() {
  const conformance = JSON.parse(readFileSync(resolve(dataDirectory, 'conformance.json'), 'utf8'))
  const statuses = new Map()
  for (const group of conformance.validity) {
    for (const code of group.codes) {
      statuses.set(`${group.field}:${code}`, group.status)
    }
  }
  return statuses
}

/**
 * Why an input is outside the admitted domain, from the pinned data alone.
 *
 * @param statuses - The validity index.
 * @param input - The projected input.
 * @returns The reason, or null when the input is in the domain.
 */
function outside(statuses, input) {
  const found = components(input)
  if (found === null) {
    return { reason: 'not-a-locale-identifier' }
  }
  for (const { component, field, code } of found) {
    const status = statuses.get(`${field}:${code}`) ?? 'absent'
    if (!ADMITTED.has(status)) {
      return { reason: 'component', component, code, status }
    }
  }
  return null
}

/**
 * The disposition the recorded answers give a case.
 *
 * @param reason - Why the input is outside the domain, or null.
 * @param normative - The projected result CLDR expects.
 * @param entry - The registry entry, with the answers it records.
 * @returns The disposition.
 */
function disposition(reason, normative, entry) {
  if (reason === null) {
    if (entry.layer?.canonical !== normative) {
      return 'blocking'
    }
    return entry.raw?.canonical === normative ? 'delegated' : 'corrected'
  }
  const rejection = reason.reason === 'component' ? reason.component : 'syntax'
  return entry.layer?.rejected === rejection ? 'outside-domain' : 'blocking'
}

/**
 * Compare two JSON values by their canonical spelling.
 *
 * @param left - One value.
 * @param right - The other value.
 * @returns Whether they are the same value.
 */
function same(left, right) {
  return JSON.stringify(left) === JSON.stringify(right)
}

/**
 * Check the registry against the vendored corpus and the committed artifact.
 *
 * @returns How many checks failed, and the number of cases checked.
 */
function checkRegistry() {
  let failures = 0
  const fail = message => {
    console.error(`gap registry: ${message}`)
    failures += 1
  }
  const bytes = readFileSync(corpusPath)
  const registry = JSON.parse(readFileSync(registryPath, 'utf8'))
  const manifest = JSON.parse(readFileSync(resolve(dataDirectory, 'manifest.json'), 'utf8'))
  const digest = `sha256:${createHash('sha256').update(bytes).digest('hex')}`
  if (registry.corpus.digest !== digest) {
    fail(`corpus digest\n  recorded ${registry.corpus.digest}\n  vendored ${digest}`)
  }
  if (registry.corpus.path !== CORPUS_PATH) {
    fail(`corpus path ${registry.corpus.path}`)
  }
  if (registry.corpus.cldr.revision !== manifest.generation.cldr.revision) {
    fail(`corpus release ${registry.corpus.cldr.revision}`)
  }
  for (const [name, recorded, committed] of [
    ['specification', registry.specification, manifest.specification],
    ['dataset identity', registry.dataset.identity, manifest.dataset.identity],
    ['semantic digest', registry.dataset.semanticDigest, manifest.dataset.semanticDigest],
    ['artifact integrity', registry.artifactIntegrity, manifest.integrityDigest]
  ]) {
    if (!same(recorded, committed)) {
      fail(`${name} is not the committed artifact's`)
    }
  }
  const read = readCases(bytes.toString('utf8'))
  if (read.problem) {
    fail(read.problem)
    return { failures, cases: 0 }
  }
  const { cases } = read
  if (registry.cases.length !== cases.length) {
    fail(`${registry.cases.length} entries for ${cases.length} cases`)
  }
  const statuses = validity()
  const counts = Object.fromEntries(
    SETS.map(set => [set, { delegated: 0, corrected: 0, outsideDomain: 0, blocking: 0 }])
  )
  const identities = new Set()
  for (const [index, upstream] of cases.entries()) {
    const entry = registry.cases[index] ?? {}
    const id = `${upstream.set}/${upstream.source}`
    if (identities.has(id)) {
      fail(`${id} is given twice`)
    }
    identities.add(id)
    const input = upstream.source.replaceAll('_', '-')
    const normative = upstream.expected.replaceAll('_', '-')
    if (
      entry.case !== id ||
      entry.line !== String(upstream.line) ||
      entry.expected !== upstream.expected ||
      entry.input !== input ||
      entry.normative !== normative
    ) {
      fail(`entry ${index} is not ${id} at line ${upstream.line}`)
      continue
    }
    const reason = outside(statuses, input)
    if (!same(entry.outside ?? null, reason)) {
      fail(`${id}: recorded ${JSON.stringify(entry.outside)}, data gives ${JSON.stringify(reason)}`)
    }
    const derived = disposition(reason, normative, entry)
    if (entry.disposition !== derived) {
      fail(`${id}: recorded ${entry.disposition}, the answers give ${derived}`)
    }
    counts[upstream.set][DISPOSITIONS[derived]] += 1
  }
  const total = { delegated: 0, corrected: 0, outsideDomain: 0, blocking: 0 }
  for (const [index, set] of SETS.entries()) {
    const expected = {
      set,
      cases: '0',
      delegated: '0',
      corrected: '0',
      outsideDomain: '0',
      blocking: '0'
    }
    let sum = 0
    for (const key of Object.keys(total)) {
      total[key] += counts[set][key]
      sum += counts[set][key]
      expected[key] = String(counts[set][key])
    }
    expected.cases = String(sum)
    if (!same(registry.sets[index], expected)) {
      fail(`set ${set}: recorded ${JSON.stringify(registry.sets[index])}`)
    }
  }
  const outcome = { cases: String(cases.length) }
  for (const [key, count] of Object.entries(total)) {
    outcome[key] = String(count)
  }
  outcome.admitted = total.blocking === 0
  if (!same(registry.outcome, outcome)) {
    fail(
      `outcome: recorded ${JSON.stringify(registry.outcome)}, derived ${JSON.stringify(outcome)}`
    )
  }
  if (total.blocking > 0) {
    fail(`${total.blocking} blocking case(s) refuse the adapter`)
  }
  return { failures, cases: cases.length }
}

const { failures, cases } = checkRegistry()
if (failures > 0) {
  console.error(`\n${failures} corpus gate check(s) failed`)
  process.exit(1)
}
console.log(
  `${cases} corpus cases agree with an independent projection and domain, and none is blocking`
)
