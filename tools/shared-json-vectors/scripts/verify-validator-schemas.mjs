/**
 * Check the committed validator schema vectors with a reading of their own.
 *
 * The Rust crate reads Draft 7 schema documents into nodes, names their
 * resources and anchors, resolves every `$ref` within the document, and refuses
 * a document with a problem; it writes what it reads from chosen documents.
 * This script shares no code with it: it walks Draft 7's subschema positions
 * itself, resolves references with the RFC 3986 and RFC 6901 module the
 * checkers share, and finds cycles of in-place applicators on its own. It
 * gathers every problem a document has rather than the first, and requires a
 * refused document to have exactly the one problem listed.
 *
 * `JSON.parse` keeps each number's text through its source access, since a
 * divisor's significant digits are counted from it. JavaScript objects order
 * names that look like array indices first, so the documents have none.
 */

import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import {
  evaluate,
  normalizeReference,
  parseReference,
  readFragment,
  recompose,
  resolveReference
} from './uri.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const vectorsPath = resolve(
  here,
  '../../../crates/intlify_config/fixtures/validator-schemas/vectors.json'
)

/** The one dialect a root may name. */
const DRAFT_7 = 'http://json-schema.org/draft-07/schema#'

/** The most significant digits a `multipleOf` divisor may have. */
const MAX_DIVISOR_DIGITS = 19

/** Draft 7's instance types. */
const TYPES = new Set(['array', 'boolean', 'integer', 'null', 'number', 'object', 'string'])

/** The keywords whose value is one subschema. */
const SINGLE = new Set([
  'additionalItems',
  'additionalProperties',
  'contains',
  'else',
  'if',
  'not',
  'propertyNames',
  'then'
])

/** The keywords whose value is an array of subschemas. */
const LISTS = new Set(['allOf', 'anyOf', 'oneOf'])

/** The keywords whose value is an object of subschemas, a dependency's array of names aside. */
const MAPS = new Set(['definitions', 'dependencies', 'patternProperties', 'properties'])

/** The keywords whose value is a non-negative integer. */
const COUNTS = new Set([
  'maxItems',
  'maxLength',
  'maxProperties',
  'minItems',
  'minLength',
  'minProperties'
])

/** The keywords whose value is a number. */
const BOUNDS = new Set(['exclusiveMaximum', 'exclusiveMinimum', 'maximum', 'minimum'])

const problems = []

/**
 * Record one disagreement.
 *
 * @param message - What disagrees.
 */
function problem(message) {
  problems.push(message)
}

/** A JSON number as its text spelled it. */
class JsonNumber {
  /**
   * Keep a number's text.
   *
   * @param source - The number's text.
   */
  constructor(source) {
    this.source = source
  }
}

/**
 * Parse JSON text, keeping each number's text.
 *
 * @param text - The JSON text.
 * @returns The value, with each number a `JsonNumber`.
 */
function parseJson(text) {
  return JSON.parse(text, (_name, value, context) => {
    if (typeof value !== 'number') {
      return value
    }
    if (context?.source === undefined) {
      throw new Error('JSON.parse does not give the source text of numbers')
    }
    return new JsonNumber(context.source)
  })
}

/**
 * Read a number's text as its sign, its significant digits, and the exponent
 * of the last of them.
 *
 * @param source - The number's text.
 * @returns The sign (-1, 0, or 1), the digits, and the exponent.
 */
function readNumber(source) {
  const [, minus, whole, fraction = '', written = '0'] =
    /^(-?)(0|[1-9]\d*)(?:\.(\d+))?(?:e([+-]?\d+))?$/i.exec(source)
  const significant = (whole + fraction).replace(/^0+/, '')
  if (significant === '') {
    return { sign: 0, digits: '', exponent: 0n }
  }
  const digits = significant.replace(/0+$/, '')
  return {
    sign: minus === '-' ? -1 : 1,
    digits,
    exponent: BigInt(written) - BigInt(fraction.length) + BigInt(significant.length - digits.length)
  }
}

/**
 * Whether a value is an object of JSON, not an array or a number.
 *
 * @param value - A parsed value.
 * @returns Whether it is an object.
 */
function isObject(value) {
  return (
    value !== null &&
    typeof value === 'object' &&
    !Array.isArray(value) &&
    !(value instanceof JsonNumber)
  )
}

/**
 * Whether a value is a schema: an object or a boolean.
 *
 * @param value - A parsed value.
 * @returns Whether it is a schema.
 */
function isSchema(value) {
  return typeof value === 'boolean' || isObject(value)
}

/**
 * Whether a value is a non-negative integer.
 *
 * @param value - A parsed value.
 * @returns Whether it is one.
 */
function isCount(value) {
  if (!(value instanceof JsonNumber)) {
    return false
  }
  const number = readNumber(value.source)
  return number.sign === 0 || (number.sign > 0 && number.exponent >= 0n)
}

/**
 * Whether a value is an array of strings, none repeated, as the meta-schema's
 * `stringArray` is.
 *
 * @param value - A parsed value.
 * @returns Whether it is one.
 */
function isNames(value) {
  return (
    Array.isArray(value) &&
    value.every(item => typeof item === 'string') &&
    new Set(value).size === value.length
  )
}

/**
 * Whether a pattern is valid ECMA-262 with the `u` flag. The subset's
 * boundary is checked by the pattern vectors; the documents here use plain
 * patterns or malformed ones.
 *
 * @param source - The pattern.
 * @returns Whether it compiles.
 */
function compiles(source) {
  try {
    void new RegExp(source, 'u')
    return true
  } catch {
    return false
  }
}

/**
 * A JSON Pointer's reference token for a name.
 *
 * @param name - The name.
 * @returns The token.
 */
function escape(name) {
  return name.replaceAll('~', '~0').replaceAll('/', '~1')
}

/**
 * The subschemas of a schema object, each with the end of its JSON Pointer, in
 * the object's order.
 *
 * @param object - A schema object.
 * @returns Pairs of pointer ends and subschemas.
 */
function children(object) {
  const found = []
  for (const [keyword, value] of Object.entries(object)) {
    const at = `/${escape(keyword)}`
    if (SINGLE.has(keyword) || (keyword === 'items' && !Array.isArray(value))) {
      found.push([at, value])
    } else if ((LISTS.has(keyword) || keyword === 'items') && Array.isArray(value)) {
      value.forEach((item, index) => found.push([`${at}/${index}`, item]))
    } else if (MAPS.has(keyword) && isObject(value)) {
      for (const [name, member] of Object.entries(value)) {
        found.push([`${at}/${escape(name)}`, member])
      }
    }
  }
  return found.filter(([, value]) => isSchema(value))
}

/**
 * Count the schema nodes and `$ref` occurrences of any JSON.
 *
 * @param document - A parsed value.
 * @returns The two counts.
 */
function census(document) {
  let nodes = 0
  let references = 0
  const pending = [document]
  while (pending.length > 0) {
    const value = pending.pop()
    if (typeof value === 'boolean') {
      nodes += 1
    } else if (isObject(value)) {
      nodes += 1
      references += Object.hasOwn(value, '$ref') ? 1 : 0
      pending.push(...children(value).map(([, child]) => child))
    }
  }
  return { nodes, references }
}

/**
 * The resource a reference's components name, as a comparable text.
 *
 * @param reference - Components, normalized.
 * @returns The key.
 */
function resourceKey(reference) {
  return JSON.stringify([
    reference.scheme ?? null,
    reference.authority ?? null,
    reference.path,
    reference.query ?? null
  ])
}

/**
 * The problem a keyword's value has, if any: its shape, a pattern that does
 * not compile, or a divisor out of bounds.
 *
 * @param keyword - The keyword.
 * @param value - Its value.
 * @returns The problem's name, or null.
 */
function keywordProblem(keyword, value) {
  if (keyword === 'type') {
    // An array of types is not empty and names no type twice.
    const names = typeof value === 'string' ? [value] : value
    return Array.isArray(names) &&
      names.length > 0 &&
      names.every(name => TYPES.has(name)) &&
      new Set(names).size === names.length
      ? null
      : 'keyword'
  }
  if (keyword === 'multipleOf') {
    if (!(value instanceof JsonNumber)) {
      return 'keyword'
    }
    const number = readNumber(value.source)
    return number.sign <= 0 || number.digits.length > MAX_DIVISOR_DIGITS ? 'divisor' : null
  }
  if (BOUNDS.has(keyword)) {
    return value instanceof JsonNumber ? null : 'keyword'
  }
  if (COUNTS.has(keyword)) {
    return isCount(value) ? null : 'keyword'
  }
  if (keyword === 'pattern') {
    if (typeof value !== 'string') {
      return 'keyword'
    }
    return compiles(value) ? null : 'pattern'
  }
  if (keyword === 'enum') {
    return Array.isArray(value) ? null : 'keyword'
  }
  if (keyword === 'uniqueItems') {
    return typeof value === 'boolean' ? null : 'keyword'
  }
  if (keyword === 'required') {
    return isNames(value) ? null : 'keyword'
  }
  if (keyword === 'items' && Array.isArray(value)) {
    return value.length > 0 && value.every(isSchema) ? null : 'keyword'
  }
  if (SINGLE.has(keyword) || keyword === 'items') {
    return isSchema(value) ? null : 'keyword'
  }
  if (LISTS.has(keyword)) {
    return Array.isArray(value) && value.length > 0 && value.every(isSchema) ? null : 'keyword'
  }
  if (MAPS.has(keyword)) {
    if (!isObject(value)) {
      return 'keyword'
    }
    for (const [name, member] of Object.entries(value)) {
      if (keyword === 'patternProperties' && !compiles(name)) {
        return 'pattern'
      }
      const shaped =
        keyword === 'dependencies' && Array.isArray(member) ? isNames(member) : isSchema(member)
      if (!shaped) {
        return 'keyword'
      }
    }
  }
  return null
}

/**
 * Read a schema document, gathering every problem it has.
 *
 * @param document - The parsed document.
 * @param defaultBase - The base of a root that names no resource.
 * @returns The problems, and what the document reads to.
 */
function read(document, defaultBase) {
  const found = []
  if (!isSchema(document)) {
    return { found: ['notASchema'] }
  }
  const nodes = []
  const byLocation = new Map()
  const resources = []
  const resourceIndex = new Map()
  const anchors = []
  const anchorIndex = new Map()
  const base0 = normalizeReference(parseReference(defaultBase))

  /**
   * Name a resource, unless it is named already.
   *
   * @param resource - Its components, normalized and without a fragment.
   * @param index - The node it names.
   */
  const register = (resource, index) => {
    const key = resourceKey(resource)
    if (resourceIndex.has(key)) {
      found.push('duplicateIdentifier')
      return
    }
    resourceIndex.set(key, index)
    resources.push({ uri: recompose(resource), node: index })
  }

  /**
   * Read an `$id` against a base.
   *
   * @param identifier - The `$id`'s value.
   * @param base - The base it is read against.
   * @param index - Its node.
   * @returns The base of the node's subschemas.
   */
  const identify = (identifier, base, index) => {
    if (typeof identifier !== 'string') {
      found.push('identifier')
      return base
    }
    if (identifier.startsWith('#')) {
      const name = identifier.slice(1)
      if (!/^[a-z][\w.:-]*$/i.test(name)) {
        found.push('identifier')
        return base
      }
      const key = `${resourceKey(base)}#${name}`
      if (anchorIndex.has(key)) {
        found.push('duplicateIdentifier')
      } else {
        anchorIndex.set(key, index)
        anchors.push({ resource: recompose(base), name, node: index })
      }
      return base
    }
    const reference = parseReference(identifier)
    if (reference === null) {
      found.push('identifier')
      return base
    }
    const target = normalizeReference(resolveReference(reference, base))
    if (target.fragment !== undefined && target.fragment !== '') {
      found.push('identifier')
      return base
    }
    const resource = { ...target, fragment: undefined }
    register(resource, index)
    return resource
  }

  /**
   * Visit a node and the subschemas below it, in document order.
   *
   * @param value - The node's value.
   * @param location - Its JSON Pointer.
   * @param parentBase - The base it is in.
   */
  const visit = (value, location, parentBase) => {
    const index = nodes.length
    const node = { value, location, base: parentBase }
    nodes.push(node)
    byLocation.set(location, index)
    let base = parentBase
    if (isObject(value) && !Object.hasOwn(value, '$ref')) {
      if (Object.hasOwn(value, '$schema') && (index !== 0 || value.$schema !== DRAFT_7)) {
        found.push('dialect')
      }
      if (Object.hasOwn(value, '$id')) {
        base = identify(value.$id, parentBase, index)
      }
    }
    if (index === 0 && resources.length === 0) {
      register(base0, 0)
    }
    if (isObject(value)) {
      for (const [end, child] of children(value)) {
        visit(child, `${location}${end}`, base)
      }
    }
  }
  visit(document, '', base0)

  for (const node of nodes) {
    if (isObject(node.value) && !Object.hasOwn(node.value, '$ref')) {
      for (const [keyword, value] of Object.entries(node.value)) {
        const named = keywordProblem(keyword, value)
        if (named !== null) {
          found.push(named)
        }
      }
    }
  }

  /**
   * The node a `$ref` names, or undefined.
   *
   * @param text - The `$ref`'s value.
   * @param base - The base it is resolved against.
   * @returns The node's index.
   */
  const target = (text, base) => {
    const reference = typeof text === 'string' ? parseReference(text) : null
    if (reference === null) {
      return undefined
    }
    const resolved = normalizeReference(resolveReference(reference, base))
    const resource = resourceIndex.get(resourceKey(resolved))
    if (resource === undefined) {
      return undefined
    }
    const fragment = resolved.fragment ?? ''
    if (fragment === '') {
      return resource
    }
    if (!fragment.startsWith('/')) {
      return anchorIndex.get(`${resourceKey(resolved)}#${fragment}`)
    }
    const tokens = readFragment(fragment)
    if (tokens === null || !evaluate(nodes[resource].value, tokens).found) {
      return undefined
    }
    const location = nodes[resource].location + tokens.map(token => `/${escape(token)}`).join('')
    return byLocation.get(location)
  }

  const references = []
  const targets = new Map()
  for (const [index, node] of nodes.entries()) {
    if (isObject(node.value) && Object.hasOwn(node.value, '$ref')) {
      const named = target(node.value.$ref, node.base)
      if (named === undefined) {
        found.push('reference')
      } else {
        targets.set(index, named)
        references.push({ from: node.location, to: nodes[named].location })
      }
    }
  }

  /**
   * The nodes a node applies to the same instance.
   *
   * @param index - The node.
   * @returns Their indices.
   */
  const inPlace = index => {
    const { value, location } = nodes[index]
    if (!isObject(value)) {
      return []
    }
    if (Object.hasOwn(value, '$ref')) {
      return targets.has(index) ? [targets.get(index)] : []
    }
    const at = []
    for (const keyword of LISTS) {
      if (Array.isArray(value[keyword])) {
        value[keyword].forEach((_item, position) => at.push(`${location}/${keyword}/${position}`))
      }
    }
    for (const keyword of ['not', 'if']) {
      if (Object.hasOwn(value, keyword)) {
        at.push(`${location}/${keyword}`)
      }
    }
    if (Object.hasOwn(value, 'if')) {
      for (const keyword of ['then', 'else']) {
        if (Object.hasOwn(value, keyword)) {
          at.push(`${location}/${keyword}`)
        }
      }
    }
    if (isObject(value.dependencies)) {
      for (const [name, member] of Object.entries(value.dependencies)) {
        if (isSchema(member)) {
          at.push(`${location}/dependencies/${escape(name)}`)
        }
      }
    }
    return at.map(place => byLocation.get(place)).filter(place => place !== undefined)
  }

  const state = new Map()
  /**
   * Whether a cycle of in-place applicators runs through a node.
   *
   * @param index - The node.
   * @returns Whether one is found.
   */
  const cycleFrom = index => {
    state.set(index, 'open')
    for (const next of inPlace(index)) {
      if (state.get(next) === 'open' || (!state.has(next) && cycleFrom(next))) {
        return true
      }
    }
    state.set(index, 'done')
    return false
  }
  if (nodes.some((_node, index) => !state.has(index) && cycleFrom(index))) {
    found.push('cycle')
  }

  return {
    found,
    reading: {
      nodes: nodes.map(node => node.location),
      resources: resources.map(({ uri, node }) => ({ uri, node: nodes[node].location })),
      anchors: anchors.map(({ resource, name, node }) => ({
        resource,
        name,
        node: nodes[node].location
      })),
      references
    }
  }
}

const vectors = JSON.parse(readFileSync(vectorsPath, 'utf8'))
const failures = new Set()
let readCount = 0
for (const vector of vectors.documents) {
  const name = vector.name
  const document = parseJson(vector.text)
  const counted = census(document)
  if (counted.nodes !== vector.census.nodes || counted.references !== vector.census.references) {
    problem(
      `${name}: counts ${counted.nodes} nodes and ${counted.references} references, ` +
        `not ${vector.census.nodes} and ${vector.census.references}`
    )
  }
  const { found, reading } = read(document, vectors.defaultBase)
  if (vector.failure === null) {
    readCount += 1
    if (found.length > 0) {
      problem(`${name}: has ${found.join(', ')}`)
      continue
    }
    for (const part of ['nodes', 'resources', 'anchors', 'references']) {
      if (JSON.stringify(reading[part]) !== JSON.stringify(vector.reading[part])) {
        problem(
          `${name}: ${part} are ${JSON.stringify(reading[part])}, not ${JSON.stringify(vector.reading[part])}`
        )
      }
    }
  } else {
    failures.add(vector.failure)
    if (found.length !== 1 || found[0] !== vector.failure) {
      problem(`${name}: has ${found.join(', ') || 'no problem'}, not only ${vector.failure}`)
    }
  }
}

const meta = vectors.documents.find(vector => vector.name === 'meta-schema')
if (meta?.census.nodes !== 70 || meta?.census.references !== 29) {
  problem('the meta-schema does not have its 70 nodes and 29 references')
}
for (const failure of [
  'notASchema',
  'dialect',
  'identifier',
  'duplicateIdentifier',
  'keyword',
  'pattern',
  'divisor',
  'reference',
  'cycle'
]) {
  if (!failures.has(failure)) {
    problem(`no refused document fails with ${failure}`)
  }
}

if (problems.length > 0) {
  for (const message of problems) {
    console.error(message)
  }
  console.error(`${problems.length} disagreements`)
  process.exit(1)
}

console.log(
  `validator schema vectors agree: ${readCount} documents read and ` +
    `${vectors.documents.length - readCount} refused, ${failures.size} kinds of failure`
)
