/**
 * Check the committed validator value vectors with an implementation of their own.
 *
 * The Rust crate reads JSON numbers as exact decimals, keys JSON values for
 * equality, splits, resolves, and normalizes URI references under RFC 3986,
 * and reads the JSON Pointers that URI fragments spell under RFC 6901, and it
 * writes its readings of chosen inputs. This script shares no code with it: it
 * compares numbers as BigInt fractions, decides `multipleOf` by modular
 * exponentiation, compares JSON values on its own, reads RFC 3986's grammar
 * with its own expressions and Node's `net.isIPv6`, resolves by RFC 3986's
 * algorithm as the RFC writes it, and decodes fragments with
 * `decodeURIComponent`. It also carries its own transcription of the examples
 * RFC 3986 section 5.4 and RFC 6901 list, so a reading both get wrong in the
 * same way is still caught where the RFCs speak.
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
  '../../../crates/intlify_config/fixtures/validator-values/vectors.json'
)

/** The most significant digits a `multipleOf` divisor may have. */
const MAX_DIVISOR_DIGITS = 19

/** The largest count, which every larger count reads as. */
const MAX_COUNT = (1n << 64n) - 1n

/** Past this many places apart, two numbers are compared by their leading places. */
const SCALE_LIMIT = 100_000n

/** RFC 3986 section 5.4's base. */
const RFC_3986_BASE = 'http://a/b/c/d;p?q'

/** RFC 3986 sections 5.4.1 and 5.4.2: each reference with its target, strict. */
const RFC_3986_EXAMPLES = [
  ['g:h', 'g:h'],
  ['g', 'http://a/b/c/g'],
  ['./g', 'http://a/b/c/g'],
  ['g/', 'http://a/b/c/g/'],
  ['/g', 'http://a/g'],
  ['//g', 'http://g'],
  ['?y', 'http://a/b/c/d;p?y'],
  ['g?y', 'http://a/b/c/g?y'],
  ['#s', 'http://a/b/c/d;p?q#s'],
  ['g#s', 'http://a/b/c/g#s'],
  ['g?y#s', 'http://a/b/c/g?y#s'],
  [';x', 'http://a/b/c/;x'],
  ['g;x', 'http://a/b/c/g;x'],
  ['g;x?y#s', 'http://a/b/c/g;x?y#s'],
  ['', 'http://a/b/c/d;p?q'],
  ['.', 'http://a/b/c/'],
  ['./', 'http://a/b/c/'],
  ['..', 'http://a/b/'],
  ['../', 'http://a/b/'],
  ['../g', 'http://a/b/g'],
  ['../..', 'http://a/'],
  ['../../', 'http://a/'],
  ['../../g', 'http://a/g'],
  ['../../../g', 'http://a/g'],
  ['../../../../g', 'http://a/g'],
  ['/./g', 'http://a/g'],
  ['/../g', 'http://a/g'],
  ['g.', 'http://a/b/c/g.'],
  ['.g', 'http://a/b/c/.g'],
  ['g..', 'http://a/b/c/g..'],
  ['..g', 'http://a/b/c/..g'],
  ['./../g', 'http://a/b/g'],
  ['./g/.', 'http://a/b/c/g/'],
  ['g/./h', 'http://a/b/c/g/h'],
  ['g/../h', 'http://a/b/c/h'],
  ['g;x=1/./y', 'http://a/b/c/g;x=1/y'],
  ['g;x=1/../y', 'http://a/b/c/y'],
  ['g?y/./x', 'http://a/b/c/g?y/./x'],
  ['g?y/../x', 'http://a/b/c/g?y/../x'],
  ['g#s/./x', 'http://a/b/c/g#s/./x'],
  ['g#s/../x', 'http://a/b/c/g#s/../x'],
  ['http:g', 'http:g']
]

/** RFC 6901 section 5's document. */
const RFC_6901_DOCUMENT = {
  foo: ['bar', 'baz'],
  '': 0,
  'a/b': 1,
  'c%d': 2,
  'e^f': 3,
  'g|h': 4,
  'i\\j': 5,
  'k"l': 6,
  ' ': 7,
  'm~n': 8
}

/** RFC 6901 section 6: each fragment with the value it names. */
const RFC_6901_FRAGMENTS = [
  ['', RFC_6901_DOCUMENT],
  ['/foo', ['bar', 'baz']],
  ['/foo/0', 'bar'],
  ['/', 0],
  ['/a~1b', 1],
  ['/c%25d', 2],
  ['/e%5Ef', 3],
  ['/g%7Ch', 4],
  ['/i%5Cj', 5],
  ['/k%22l', 6],
  ['/%20', 7],
  ['/m~0n', 8]
]

const problems = []

/**
 * Record one disagreement.
 *
 * @param message - What disagrees.
 */
function problem(message) {
  problems.push(message)
}

/**
 * Read a JSON number as `sign * digits * 10^exponent`, the digits without a
 * leading or trailing zero.
 *
 * @param text - The number's text.
 * @returns The number, or null when the text is not a JSON number.
 */
function readNumber(text) {
  const match = /^(-?)(0|[1-9]\d*)(?:\.(\d+))?(?:e([+-]?\d+))?$/i.exec(text)
  if (match === null) {
    return null
  }
  const [, minus, whole, fraction = '', written = '0'] = match
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
 * The canonical text of a number: the sign, the digits, and the exponent when
 * it is not zero.
 *
 * @param number - A number as `readNumber` reads it.
 * @returns The text.
 */
function canonical(number) {
  if (number.sign === 0) {
    return '0'
  }
  const sign = number.sign < 0 ? '-' : ''
  return number.exponent === 0n
    ? `${sign}${number.digits}`
    : `${sign}${number.digits}e${number.exponent}`
}

/**
 * Compare two numbers as fractions of BigInts, or, when they are too many
 * places apart to scale, by their leading places.
 *
 * @param left - A number.
 * @param right - Another.
 * @returns -1, 0, or 1.
 */
function compare(left, right) {
  if (left.sign !== right.sign) {
    return Math.sign(left.sign - right.sign)
  }
  if (left.sign === 0) {
    return 0
  }
  const shift = left.exponent - right.exponent
  let magnitude
  if (shift > SCALE_LIMIT || shift < -SCALE_LIMIT) {
    const leftTop = left.exponent + BigInt(left.digits.length)
    const rightTop = right.exponent + BigInt(right.digits.length)
    if (leftTop === rightTop) {
      throw new Error(`cannot compare ${canonical(left)} with ${canonical(right)}`)
    }
    magnitude = leftTop > rightTop ? 1 : -1
  } else {
    const scaledLeft = BigInt(left.digits) * 10n ** (shift > 0n ? shift : 0n)
    const scaledRight = BigInt(right.digits) * 10n ** (shift < 0n ? -shift : 0n)
    magnitude = scaledLeft === scaledRight ? 0 : scaledLeft > scaledRight ? 1 : -1
  }
  return left.sign * magnitude
}

/**
 * Whether a number has no fractional part: its digits are divisible by the
 * power of ten its exponent divides them by.
 *
 * @param number - A number.
 * @returns Whether it is an integer.
 */
function isInteger(number) {
  if (number.sign === 0 || number.exponent >= 0n) {
    return true
  }
  if (-number.exponent > BigInt(number.digits.length)) {
    return false
  }
  return BigInt(number.digits) % 10n ** -number.exponent === 0n
}

/**
 * A number as a count of the length keywords.
 *
 * @param number - A number.
 * @returns The count as decimal text, at most 2^64 - 1, or null when the
 *   number is not a non-negative integer.
 */
function count(number) {
  if (number.sign < 0 || !isInteger(number)) {
    return null
  }
  if (number.sign === 0) {
    return '0'
  }
  if (number.exponent > 20n) {
    return MAX_COUNT.toString()
  }
  const value = BigInt(number.digits) * 10n ** number.exponent
  return (value > MAX_COUNT ? MAX_COUNT : value).toString()
}

/**
 * Why a number is not a `multipleOf` divisor.
 *
 * @param number - A number.
 * @returns The failure, or null for a divisor.
 */
function divisorFailure(number) {
  if (number.sign <= 0) {
    return 'notPositive'
  }
  return number.digits.length > MAX_DIVISOR_DIGITS ? 'digits' : null
}

/**
 * `base^exponent mod modulus` by repeated squaring.
 *
 * @param base - The base.
 * @param exponent - A non-negative exponent of any size.
 * @param modulus - A positive modulus.
 * @returns The power's remainder.
 */
function power(base, exponent, modulus) {
  let result = 1n % modulus
  let square = base % modulus
  for (let rest = exponent; rest > 0n; rest >>= 1n) {
    if ((rest & 1n) === 1n) {
      result = (result * square) % modulus
    }
    square = (square * square) % modulus
  }
  return result
}

/**
 * Whether dividing a number by a divisor leaves an integer.
 *
 * @param number - A number.
 * @param divisor - A positive number.
 * @returns Whether the quotient is an integer.
 */
function isMultiple(number, divisor) {
  if (number.sign === 0) {
    return true
  }
  const digits = BigInt(number.digits)
  const by = BigInt(divisor.digits)
  const shift = number.exponent - divisor.exponent
  if (shift >= 0n) {
    return ((digits % by) * power(10n, shift, by)) % by === 0n
  }
  // The quotient is digits / (by * 10^-shift), and digits is below 10^length.
  if (-shift > BigInt(number.digits.length)) {
    return false
  }
  return digits % (by * 10n ** -shift) === 0n
}

/** A JSON number as its text spelled it. */
class JsonNumber {
  /**
   * Read a number's text.
   *
   * @param source - The number's text.
   */
  constructor(source) {
    this.number = readNumber(source)
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
 * Whether two JSON values are equal: numbers by value, arrays item by item,
 * objects by their members in any order, and the rest as they are.
 *
 * @param left - A value.
 * @param right - Another.
 * @returns Whether they are equal.
 */
function equal(left, right) {
  if (left instanceof JsonNumber || right instanceof JsonNumber) {
    return (
      left instanceof JsonNumber &&
      right instanceof JsonNumber &&
      compare(left.number, right.number) === 0
    )
  }
  if (Array.isArray(left) || Array.isArray(right)) {
    return (
      Array.isArray(left) &&
      Array.isArray(right) &&
      left.length === right.length &&
      left.every((item, index) => equal(item, right[index]))
    )
  }
  if (left !== null && right !== null && typeof left === 'object' && typeof right === 'object') {
    const names = Object.keys(left)
    return (
      names.length === Object.keys(right).length &&
      names.every(name => Object.hasOwn(right, name) && equal(left[name], right[name]))
    )
  }
  return left === right
}

const vectors = JSON.parse(readFileSync(vectorsPath, 'utf8'))

// Numbers.
const { numbers } = vectors
for (const vector of numbers.parsed) {
  const number = readNumber(vector.text)
  const expected = number === null ? null : canonical(number)
  if (vector.decimal !== expected) {
    problem(`${JSON.stringify(vector.text)} reads to ${expected}, not ${vector.decimal}`)
  }
}
const values = numbers.values.map(text => readNumber(text))
values.forEach((value, index) => {
  if (value === null) {
    problem(`${JSON.stringify(numbers.values[index])} is not a JSON number`)
  }
})
const marks = { '-1': '<', 0: '=', 1: '>' }
if (numbers.order.length !== values.length ** 2) {
  problem(`${numbers.order.length} orderings for ${values.length} numbers`)
}
values.forEach((left, i) => {
  values.forEach((right, j) => {
    const mark = marks[compare(left, right)]
    if (numbers.order[i * values.length + j] !== mark) {
      problem(`${numbers.values[i]} ${mark} ${numbers.values[j]}`)
    }
  })
})
values.forEach((value, index) => {
  const text = numbers.values[index]
  if ((numbers.integers[index] === '1') !== isInteger(value)) {
    problem(`${text} is ${isInteger(value) ? '' : 'not '}an integer`)
  }
  if (numbers.counts[index] !== count(value)) {
    problem(`${text} counts ${count(value)}, not ${numbers.counts[index]}`)
  }
})
const divisors = []
for (const vector of numbers.divisors) {
  const divisor = readNumber(vector.text)
  const failure = divisorFailure(divisor)
  if (vector.failure !== failure) {
    problem(`${vector.text} as a divisor fails with ${failure}, not ${vector.failure}`)
  }
  if (failure === null) {
    divisors.push([vector.text, divisor])
  }
}
if (numbers.multiples.length !== values.length * divisors.length) {
  problem(`${numbers.multiples.length} multiples for ${values.length * divisors.length} pairs`)
}
values.forEach((value, i) => {
  divisors.forEach(([text, divisor], j) => {
    const multiple = isMultiple(value, divisor)
    if ((numbers.multiples[i * divisors.length + j] === '1') !== multiple) {
      problem(`${numbers.values[i]} is ${multiple ? '' : 'not '}a multiple of ${text}`)
    }
  })
})

// Equality.
const classes = vectors.equality.classes.map(members => members.map(text => parseJson(text)))
classes.forEach((members, i) => {
  members.forEach((member, k) => {
    if (!equal(members[0], member)) {
      problem(`${vectors.equality.classes[i][k]} is not equal to ${vectors.equality.classes[i][0]}`)
    }
  })
  classes.slice(i + 1).forEach((others, offset) => {
    if (equal(members[0], others[0])) {
      problem(
        `${vectors.equality.classes[i][0]} equals ${vectors.equality.classes[i + 1 + offset][0]}`
      )
    }
  })
})
for (const vector of vectors.equality.uniqueItems) {
  const items = parseJson(vector.items)
  const unique = items.every((item, i) => items.slice(i + 1).every(other => !equal(item, other)))
  if (vector.unique !== unique) {
    problem(`${vector.items} is ${unique ? '' : 'not '}unique`)
  }
}

// URI references.
const { references } = vectors
const FIELDS = ['scheme', 'authority', 'path', 'query', 'fragment']
for (const vector of references.parsed) {
  const parsed = parseReference(vector.text)
  const name = JSON.stringify(vector.text)
  if ((parsed === null) !== (vector.components === null)) {
    problem(`${name} is ${parsed === null ? 'not ' : ''}a URI reference`)
  } else if (parsed !== null) {
    for (const field of FIELDS) {
      if ((parsed[field] ?? null) !== vector.components[field]) {
        problem(`${name}: the ${field} is ${parsed[field]}, not ${vector.components[field]}`)
      }
    }
  }
}
const rfcBase = parseReference(RFC_3986_BASE)
for (const [text, target] of RFC_3986_EXAMPLES) {
  const resolved = recompose(resolveReference(parseReference(text), rfcBase))
  if (resolved !== target) {
    problem(
      `this checker resolves ${JSON.stringify(text)} to ${resolved}, not RFC 3986's ${target}`
    )
  }
}
if (references.exampleBase !== RFC_3986_BASE) {
  problem(`the examples resolve against ${references.exampleBase}`)
}
if (references.examples.length !== RFC_3986_EXAMPLES.length) {
  problem(`${references.examples.length} examples, not ${RFC_3986_EXAMPLES.length}`)
}
references.examples.forEach((vector, index) => {
  const [text, target] = RFC_3986_EXAMPLES[index] ?? []
  if (vector.reference !== text || vector.target !== target) {
    problem(`example ${index}: ${vector.reference} resolves to ${vector.target}, not ${target}`)
  }
})
references.bases.forEach((baseText, i) => {
  const base = parseReference(baseText)
  references.references.forEach((text, j) => {
    const target = recompose(resolveReference(parseReference(text), base))
    if (references.targets[i]?.[j] !== target) {
      problem(
        `${JSON.stringify(text)} against ${baseText} is ${target}, not ${references.targets[i]?.[j]}`
      )
    }
  })
})
for (const vector of references.normalized) {
  const normalized = recompose(normalizeReference(parseReference(vector.text)))
  if (vector.normalized !== normalized) {
    problem(`${JSON.stringify(vector.text)} normalizes to ${normalized}, not ${vector.normalized}`)
  }
}

// JSON Pointers.
const { pointers } = vectors
for (const [name, value] of Object.entries(RFC_6901_DOCUMENT)) {
  if (!equal(pointers.document[name], value)) {
    problem(`the document's ${JSON.stringify(name)} is not RFC 6901's`)
  }
}
for (const [fragment, value] of RFC_6901_FRAGMENTS) {
  const vector = pointers.fragments.find(candidate => candidate.fragment === fragment)
  const named =
    fragment === ''
      ? Object.entries(RFC_6901_DOCUMENT).every(([member, item]) =>
          equal(vector?.value?.[member], item)
        )
      : equal(vector?.value, value)
  if (vector?.found !== true || !named) {
    problem(`RFC 6901's ${JSON.stringify(fragment)} does not name what the RFC lists`)
  }
}
for (const vector of pointers.fragments) {
  const name = JSON.stringify(vector.fragment)
  const tokens = readFragment(vector.fragment)
  if (tokens === null || vector.tokens === null) {
    if (tokens !== vector.tokens) {
      problem(`${name} reads to ${JSON.stringify(tokens)}, not ${JSON.stringify(vector.tokens)}`)
    }
    continue
  }
  if (!equal(tokens, vector.tokens)) {
    problem(`${name} reads to ${JSON.stringify(tokens)}, not ${JSON.stringify(vector.tokens)}`)
  }
  const { found, value } = evaluate(pointers.document, tokens)
  if (vector.found !== found || (found && !equal(vector.value, value))) {
    problem(`${name} names ${found ? JSON.stringify(value) : 'nothing'}`)
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
  `validator value vectors agree: ${numbers.parsed.length} number texts, ` +
    `${values.length ** 2} orderings, ${values.length * divisors.length} multiples, ` +
    `${classes.length} equality classes, ${vectors.equality.uniqueItems.length} arrays, ` +
    `${references.parsed.length} reference texts, ${references.examples.length} RFC 3986 examples, ` +
    `${references.bases.length * references.references.length} resolutions, ` +
    `${references.normalized.length} normalizations, ${pointers.fragments.length} fragments`
)
